use crate::error::AimarketError;
use crate::models::*;
use crate::payer_proof::canonical_proof_tx_hash;
use crate::signer::MarketSigner;
use crate::tee::TeeVerifier;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct AimarketAgentConfig {
    pub hub_url: String,
    pub wallet_key: String,
    pub affiliate: String,
    pub timeout: Duration,
    /// Retained for configuration compatibility; paid invokes are always single-shot.
    pub max_retries: u32,
    pub verify_tee: bool,
    /// Key of a credit account on the hub (credits rail). When set, every invoke is paid by
    /// that account and sent as `X-API-Key`; no channel is opened or needed.
    pub api_key: Option<String>,
}

impl std::fmt::Debug for AimarketAgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AimarketAgentConfig")
            .field("hub_url", &self.hub_url)
            .field("wallet_key", &"[redacted]")
            .field("affiliate", &self.affiliate)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .field("verify_tee", &self.verify_tee)
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl AimarketAgentConfig {
    /// The defaults [`AimarketAgent::new`] uses. Override fields with struct update syntax:
    /// `AimarketAgentConfig { api_key: Some(key), ..AimarketAgentConfig::new(hub, wallet) }`.
    pub fn new(hub_url: &str, wallet_key_hex: &str) -> Self {
        Self {
            hub_url: hub_url.trim_end_matches('/').to_string(),
            wallet_key: wallet_key_hex.to_string(),
            affiliate: "aimarket-sdk-rust".to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            verify_tee: true,
            api_key: None,
        }
    }
}

/// The on-chain deposit behind a channel.
///
/// Production hubs open a channel only against a deposit they can verify on chain: without
/// `tx_hash` they answer `400 tx_hash is required for on-chain verification`, and without
/// `payer_signature` they refuse with "missing or invalid payer proof" and hand back the text
/// to sign ([`crate::payer_proof::sign_channel_open_proof`] signs it).
#[derive(Debug, Clone, Default)]
pub struct ChannelDeposit {
    /// Hash of the transfer that paid the hub's settlement wallet.
    pub tx_hash: String,
    /// The paying wallet's EIP-191 signature over the channel-open proof message.
    pub payer_signature: Option<String>,
    /// The paying wallet. Optional: the hub binds the channel to the transfer's sender anyway.
    pub wallet: Option<String>,
}

struct CachedChannel {
    channel: Channel,
    #[allow(dead_code)]
    cached_at: Instant,
    /// Canonical hash of the deposit that funded the channel, when one was given.
    tx_hash: Option<String>,
}

impl CachedChannel {
    fn is_reusable(&self) -> bool {
        !self.channel.is_expired() && self.channel.balance_ratio() > 0.5
    }
}

pub struct AimarketAgent {
    config: AimarketAgentConfig,
    signer: MarketSigner,
    tee_verifier: Mutex<TeeVerifier>,
    client: Client,
    channel_cache: Mutex<HashMap<String, CachedChannel>>,
    // channel_id -> one-time debit secret, captured at open, sent on every invoke.
    channel_secrets: Mutex<HashMap<String, String>>,
    // channel_id -> wallet the hub bound the channel to; closing an owned channel needs it.
    channel_wallets: Mutex<HashMap<String, String>>,
    well_known_cache: Mutex<Option<String>>,
}

impl AimarketAgent {
    pub fn new(hub_url: &str, wallet_key_hex: &str) -> Self {
        Self::with_config(AimarketAgentConfig::new(hub_url, wallet_key_hex))
    }

    pub fn with_config(mut config: AimarketAgentConfig) -> Self {
        config.api_key = config
            .api_key
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty());
        Self {
            signer: MarketSigner::new(&config.wallet_key),
            tee_verifier: Mutex::new(TeeVerifier::new(&config.wallet_key, HashMap::new())),
            client: Client::builder()
                .timeout(config.timeout)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to initialize the non-redirecting HTTP client"),
            channel_cache: Mutex::new(HashMap::new()),
            channel_secrets: Mutex::new(HashMap::new()),
            channel_wallets: Mutex::new(HashMap::new()),
            well_known_cache: Mutex::new(None),
            config,
        }
    }

    // ── Phase 1: Discovery ────────────────────────────────────────

    pub async fn well_known(&self) -> Result<String, AimarketError> {
        if let Some(cached) = self.well_known_cache.lock().unwrap().clone() {
            return Ok(cached);
        }
        let url = format!("{}/.well-known/ai-market.json", self.config.hub_url);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(AimarketError::Protocol(format!(
                "Failed to fetch well-known: {}",
                resp.status()
            )));
        }
        let body = resp.text().await?;
        *self.well_known_cache.lock().unwrap() = Some(body.clone());
        Ok(body)
    }

    pub async fn discover(
        &self,
        intent: &str,
        budget: Option<f64>,
        limit: Option<i32>,
        category: Option<&str>,
    ) -> Result<Vec<PlanStep>, AimarketError> {
        let mut params = vec![
            ("intent".to_string(), intent.to_string()),
            ("limit".to_string(), limit.unwrap_or(5).to_string()),
        ];
        if let Some(b) = budget {
            // Hub query param is `budget`, not `budget_usd` (the wrong name was
            // silently ignored, disabling the price cap).
            params.push(("budget".to_string(), b.to_string()));
        }
        if let Some(c) = category {
            params.push(("category".to_string(), c.to_string()));
        }

        let url = format!("{}/ai-market/v2/search", self.config.hub_url);
        let resp = self
            .client
            .get(&url)
            .query(&params)
            .header("X-AIMarket-Affiliate", &self.config.affiliate)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AimarketError::Protocol(format!(
                "Discovery failed: {}",
                resp.status()
            )));
        }

        // Hub returns { query, matches: [flat capability dicts], catalog }. Map
        // each flat match to a PlanStep — the old `results` field never existed
        // and deserialization failed against the real hub.
        let data: SearchResponse = resp.json().await?;
        Ok(data.matches.into_iter().map(PlanStep::from).collect())
    }

    pub async fn discover_product(&self, product_id: &str) -> Result<Vec<PlanStep>, AimarketError> {
        self.discover(&format!("product:{product_id}"), None, None, None)
            .await
    }

    // ── Phase 2: Channel Open ─────────────────────────────────────

    /// Open a payment channel funded by `deposit_usd`.
    ///
    /// Production hubs need `deposit` — the on-chain transfer the caller already made (see
    /// [`ChannelDeposit`]); only a dev/demo hub opens a channel without one. A deposit funds
    /// exactly one channel, so asking again with the same `tx_hash` returns the channel it
    /// opened instead of re-sending it.
    pub async fn open_channel(
        &self,
        deposit_usd: f64,
        token: &str,
        chain: &str,
        deposit: Option<&ChannelDeposit>,
    ) -> Result<Channel, AimarketError> {
        let tx_hash = deposit.map_or("", |d| d.tx_hash.trim());
        if deposit.is_some() && tx_hash.is_empty() {
            return Err(AimarketError::Protocol(
                "deposit.tx_hash must name the deposit transaction".into(),
            ));
        }
        let funded_by = (!tx_hash.is_empty()).then(|| canonical_proof_tx_hash(tx_hash));
        let cache_key = format!("{deposit_usd}:{token}:{chain}");
        {
            let mut cache = self.channel_cache.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                // Without a deposit, reuse a channel that still has most of its balance. With
                // one, only the channel THAT deposit opened will do: answering a fresh transfer
                // with an older channel would leave the caller's new money uncredited.
                let reuse = match &funded_by {
                    Some(tx) => {
                        cached.tx_hash.as_deref() == Some(tx.as_str())
                            && !cached.channel.is_expired()
                    }
                    None => cached.is_reusable(),
                };
                if reuse {
                    return Ok(cached.channel.clone());
                }
            }
            cache.remove(&cache_key);
        }

        let url = format!("{}/ai-market/v2/channel/open", self.config.hub_url);
        let mut body = serde_json::json!({
            "deposit_usd": deposit_usd,
            "token": token,
            "chain": chain,
        });
        if let Some(d) = deposit {
            body["tx_hash"] = tx_hash.into();
            if let Some(sig) = d.payer_signature.as_deref().filter(|s| !s.is_empty()) {
                body["payer_signature"] = sig.into();
            }
            if let Some(wallet) = d.wallet.as_deref().filter(|w| !w.is_empty()) {
                body["wallet"] = wallet.into();
            }
        }

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .header("X-AIMarket-Affiliate", &self.config.affiliate)
            .send()
            .await?;

        if resp.status().as_u16() == 404 {
            return Err(AimarketError::Protocol(
                "Payment channels not available on this hub".into(),
            ));
        }
        if !resp.status().is_success() {
            // The body says what is missing — a payer-proof refusal carries the text to sign.
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AimarketError::Protocol(format!(
                "Channel open failed: {status} {text}"
            )));
        }

        // The hub wraps the channel in a `{ "channel": {...} }` envelope (matching the
        // Python agent + live hub); unwrap it, tolerating a bare object for forward-compat.
        let mut raw: serde_json::Value = resp.json().await?;
        let mut channel_val = if raw.get("channel").is_some() {
            raw["channel"].take()
        } else {
            raw
        };
        let channel_id = channel_val["channel_id"].as_str().unwrap_or_default().to_string();
        // Capture the one-time debit secret before anything can fail: the hub has opened the
        // channel and will never show the secret again. invoke() presents it via
        // X-Payment-Channel-Secret (required by secure channels; dropping it 402s).
        if let Some(secret) = channel_val["channel_secret"].as_str().filter(|s| !s.is_empty()) {
            self.channel_secrets
                .lock()
                .unwrap()
                .insert(channel_id.clone(), secret.to_string());
        }
        // A deposit-funded channel belongs to the wallet that paid; closing it needs that wallet.
        let owner = channel_val["wallet"]
            .as_str()
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .or_else(|| deposit.and_then(|d| d.wallet.clone()).filter(|w| !w.is_empty()));
        if let Some(owner) = owner {
            self.channel_wallets
                .lock()
                .unwrap()
                .insert(channel_id.clone(), owner);
        }
        // aimarket-hub reports the deposit as `original_deposit_usd`; `deposit_usd` is required
        // here, so without this a channel the hub really opened failed to decode.
        if let Some(fields) = channel_val.as_object_mut() {
            if !fields.contains_key("deposit_usd") {
                let opened_with = fields
                    .get("original_deposit_usd")
                    .cloned()
                    .unwrap_or_else(|| deposit_usd.into());
                fields.insert("deposit_usd".into(), opened_with);
            }
        }
        let channel: Channel = serde_json::from_value(channel_val).map_err(|e| {
            AimarketError::Protocol(format!("Channel {channel_id} decode failed: {e}"))
        })?;
        self.channel_cache.lock().unwrap().insert(
            cache_key,
            CachedChannel {
                channel: channel.clone(),
                cached_at: Instant::now(),
                tx_hash: funded_by,
            },
        );
        Ok(channel)
    }

    pub async fn get_channel_balance(&self, channel_id: &str) -> Result<f64, AimarketError> {
        let url = format!(
            "{}/ai-market/v2/channel/{}",
            self.config.hub_url, channel_id
        );
        let resp = self
            .client
            .get(&url)
            .header("X-AIMarket-Affiliate", &self.config.affiliate)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(AimarketError::Protocol(format!(
                "Failed to get channel balance: {}",
                resp.status()
            )));
        }
        let data: serde_json::Value = resp.json().await?;
        Ok(data["balance_usd"].as_f64().unwrap_or(0.0))
    }

    // ── Phase 3: Invoke ───────────────────────────────────────────

    /// `channel_id` is the channel that pays for the call. With an `api_key` in the config
    /// the credit account pays instead, as in the Python agent: pass `""` — a channel id is
    /// not sent then.
    ///
    /// `verify` is the optional Pay-on-Verified opt-in, forwarded verbatim as
    /// the `verify` request block (`{"requested": true, "intent": …, "mode": …,
    /// "wait": …, "wait_timeout_s": …}`). The hub escrows the debit until Metis
    /// judges the output against `intent`; the verdict envelope comes back as
    /// `InvokeResult.verification`.
    pub async fn invoke(
        &self,
        capability_id: &str,
        input: serde_json::Value,
        channel_id: &str,
        product_id: Option<&str>,
        source_hub: Option<&str>,
        verify: Option<serde_json::Value>,
    ) -> Result<InvokeResult, AimarketError> {
        if self.config.api_key.is_none() && channel_id.trim().is_empty() {
            return Err(AimarketError::Payment(
                "invoke needs a way to pay: pass a channel_id from open_channel or set \
                 api_key in AimarketAgentConfig"
                    .into(),
            ));
        }
        // A lost response cannot tell us whether this POST was charged. The
        // protocol does not require idempotency support, so never replay it.
        self.invoke_once(capability_id, input, channel_id, product_id, source_hub, verify)
            .await
    }

    async fn invoke_once(
        &self,
        capability_id: &str,
        input: serde_json::Value,
        channel_id: &str,
        product_id: Option<&str>,
        source_hub: Option<&str>,
        verify: Option<serde_json::Value>,
    ) -> Result<InvokeResult, AimarketError> {
        let url = format!("{}/ai-market/v2/invoke", self.config.hub_url);

        // When the caller opts into verify.wait, the hub holds this response open
        // until Metis returns a verdict (up to wait_timeout_s). Compute the per-
        // request timeout before `verify` is consumed by build_invoke_body below.
        let wait_timeout = verified_wait_timeout(verify.as_ref());

        let body = build_invoke_body(capability_id, input, product_id, source_hub, verify);

        let mut req = self
            .client
            .post(&url)
            .json(&body)
            .header("X-AIMarket-Affiliate", &self.config.affiliate);
        if let Some(api_key) = self.config.api_key.as_deref() {
            // Credits rail, as in the Python agent: the account pays and no channel header
            // rides along.
            req = req.header("X-API-Key", api_key);
        } else {
            let canonical = format!(
                "channel:{}|capability:{}|affiliate:{}",
                channel_id, capability_id, self.config.affiliate
            );
            let signature = self.signer.sign(&canonical);
            // Present the one-time debit secret captured at open (required by secure channels).
            let channel_secret = self.channel_secrets.lock().unwrap().get(channel_id).cloned();
            req = req
                .header("X-Payment-Channel", channel_id)
                .header("X-Market-Signature", &signature);
            if let Some(secret) = channel_secret {
                req = req.header("X-Payment-Channel-Secret", secret);
            }
        }
        // Override the client's default timeout for verified-wait invokes so the
        // request does not abort mid-wait — an abort would be retried and re-POST
        // a fresh nonce, creating a second hold/debit (the buyer pays twice).
        if let Some(timeout) = wait_timeout {
            req = req.timeout(timeout);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AimarketError::Network(e.to_string()))?;

        match resp.status().as_u16() {
            403 => {
                let data: serde_json::Value = resp.json().await?;
                let reason = data["reason"]
                    .as_str()
                    .unwrap_or("Blocked by safety gate")
                    .to_string();
                Err(AimarketError::Safety(reason))
            }
            402 => {
                let detail = refusal_detail(resp).await;
                Err(AimarketError::Payment(if self.config.api_key.is_some() {
                    format!("The credit account cannot pay for this call: {detail}")
                } else {
                    format!("Channel depleted or expired — open a new channel ({detail})")
                }))
            }
            status if !resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                Ok(InvokeResult {
                    success: false,
                    result: None,
                    price_usd: 0.0,
                    latency_ms: 0.0,
                    receipt: None,
                    continuation: None,
                    safety_blocked: false,
                    safety_reason: None,
                    tee_verified: false,
                    tee_attestation: None,
                    tee_receipt: None,
                    error: Some(format!("HTTP {status}: {body}")),
                    error_type: None,
                    verification: None,
                })
            }
            _ => Ok(resp.json().await?),
        }
    }

    /// Invoke several capabilities on one channel — or, with an `api_key`, on the credit
    /// account (pass `""` as `channel_id`).
    pub async fn invoke_batch(
        &self,
        capability_ids: &[String],
        inputs: &[serde_json::Value],
        channel_id: &str,
        source_hub: Option<&str>,
    ) -> Result<Vec<InvokeResult>, AimarketError> {
        if capability_ids.len() != inputs.len() {
            return Err(AimarketError::Protocol(
                "capability_ids and inputs must have the same length".into(),
            ));
        }
        let mut results = Vec::with_capacity(capability_ids.len());
        for (cap_id, input) in capability_ids.iter().zip(inputs.iter()) {
            results.push(
                self.invoke(cap_id, input.clone(), channel_id, None, source_hub, None)
                    .await?,
            );
        }
        Ok(results)
    }

    // ── Phase 4: Settle ───────────────────────────────────────────

    pub async fn close_channel(&self, channel_id: &str) -> Result<Settlement, AimarketError> {
        self.channel_cache
            .lock()
            .unwrap()
            .retain(|_, cached| cached.channel.channel_id != channel_id);

        // A secured channel closes only with its debit secret, and an owned one only for the
        // wallet that funded it — the same proof invoke presents, or anyone could close it.
        let mut body = serde_json::json!({"channel_id": channel_id});
        let wallet = self.channel_wallets.lock().unwrap().get(channel_id).cloned();
        if let Some(wallet) = wallet {
            body["wallet"] = wallet.into();
        }
        let secret = self.channel_secrets.lock().unwrap().get(channel_id).cloned();

        let url = format!("{}/ai-market/v2/channel/close", self.config.hub_url);
        let mut req = self
            .client
            .post(&url)
            .json(&body)
            .header("X-AIMarket-Affiliate", &self.config.affiliate);
        if let Some(secret) = secret {
            req = req.header("X-Payment-Channel-Secret", secret);
        }
        let resp = req.send().await?;

        if resp.status().as_u16() == 404 {
            return Err(AimarketError::Protocol(format!(
                "Channel not found: {channel_id}"
            )));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AimarketError::Protocol(format!(
                "Settlement failed: {status} {text}"
            )));
        }
        self.channel_secrets.lock().unwrap().remove(channel_id);
        self.channel_wallets.lock().unwrap().remove(channel_id);

        settlement_from_hub(resp.json().await?, channel_id)
    }

    // ── Phase 5: Verify ───────────────────────────────────────────

    pub fn verify_tee_attestation(
        &self,
        attestation: &TeeAttestation,
        capability_id: &str,
    ) -> bool {
        self.tee_verifier
            .lock()
            .unwrap()
            .verify_attestation(attestation, capability_id)
    }

    /// Check a TEE receipt against the input this agent sent and the output it received.
    pub fn verify_tee_receipt(
        &self,
        receipt: &TeeReceipt,
        sent_input: &str,
        received_output: &str,
    ) -> bool {
        self.tee_verifier
            .lock()
            .unwrap()
            .verify_receipt(receipt, sent_input, received_output)
    }

    pub fn trust_code_hash(&self, capability_id: &str, code_hash: &str) {
        self.tee_verifier
            .lock()
            .unwrap()
            .trust_code_hash(capability_id, code_hash);
    }

    /// Trust the hub's published code hashes (`/.well-known/trusted-code-hashes.json`), so
    /// attestations are checked against them without pinning each one by hand.
    ///
    /// Returns how many entries the hub listed. Where the TypeScript and Dart SDKs answer -1,
    /// this returns the error: a hub that is down and a hub that trusts nothing are different
    /// facts, and a caller deciding whether to proceed needs to tell them apart.
    pub async fn fetch_trusted_hashes(&self) -> Result<usize, AimarketError> {
        let url = format!(
            "{}/.well-known/trusted-code-hashes.json",
            self.config.hub_url
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AimarketError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(AimarketError::Protocol(format!(
                "Failed to fetch trusted code hashes: {}",
                resp.status()
            )));
        }
        let registry: serde_json::Value = resp.json().await?;
        Ok(self
            .tee_verifier
            .lock()
            .unwrap()
            .load_trusted_hashes(&registry))
    }

    // ── Full cycle ────────────────────────────────────────────────

    pub async fn run_once(
        &self,
        intent: &str,
        input: serde_json::Value,
        deposit_usd: Option<f64>,
        category: Option<&str>,
    ) -> Result<BillOfMaterials, AimarketError> {
        let deposit = deposit_usd.unwrap_or(5.0);

        let plan = self
            .discover(intent, Some(deposit), Some(5), category)
            .await?;
        if plan.is_empty() {
            return Err(AimarketError::Protocol(format!(
                "No capabilities found for: {intent}"
            )));
        }

        // With a credit account the account pays: no channel is opened, so none is settled.
        let channel = if self.config.api_key.is_some() {
            None
        } else {
            Some(self.open_channel(deposit, "USDT", "base", None).await?)
        };
        let step = &plan[0];

        let result = self
            .invoke(
                &step.capability.capability_id,
                input,
                channel.as_ref().map_or("", |c| c.channel_id.as_str()),
                Some(&step.capability.product_id),
                Some(&step.capability.source_hub),
                None,
            )
            .await?;

        let settlement = match &channel {
            Some(channel) => Some(self.close_channel(&channel.channel_id).await?),
            None => None,
        };

        Ok(BillOfMaterials {
            task: intent.to_string(),
            plan,
            results: vec![result.clone()],
            settlement,
            total_spent_usd: result.price_usd,
            protocol_version: "v2".to_string(),
        })
    }

    pub fn dispose(&self) {
        self.channel_cache.lock().unwrap().clear();
        *self.well_known_cache.lock().unwrap() = None;
    }
}

/// The hub's own words for a refusal: `detail`, else `error`, else the raw body (bounded).
async fn refusal_detail(resp: reqwest::Response) -> String {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
        for key in ["detail", "error"] {
            if let Some(said) = data[key].as_str().filter(|s| !s.is_empty()) {
                // A credits refusal also names the balance and the price it could not
                // cover — the part that says "top up", whatever the prose above it says.
                return match (data["balance"].as_f64(), data["needed"].as_f64()) {
                    (Some(balance), Some(needed)) => {
                        format!("{said} (balance ${balance}, needed ${needed})")
                    }
                    _ => said.to_string(),
                };
            }
        }
    }
    let excerpt: String = text.chars().take(300).collect();
    if excerpt.is_empty() {
        format!("HTTP {status}")
    } else {
        excerpt
    }
}

/// Read the close answer. aimarket-hub wraps it as `{"settlement": {...}}`, reports the
/// spend as `used_usd` and counts no invocations; the model names the spend
/// `total_spent_usd`. A bare settlement is read as it is.
fn settlement_from_hub(
    mut raw: serde_json::Value,
    channel_id: &str,
) -> Result<Settlement, AimarketError> {
    let mut settled = if raw.get("settlement").is_some() {
        raw["settlement"].take()
    } else {
        raw
    };
    if let Some(fields) = settled.as_object_mut() {
        fields
            .entry("channel_id")
            .or_insert_with(|| channel_id.into());
        if !fields.contains_key("total_spent_usd") {
            let spent = fields.get("used_usd").cloned().unwrap_or_else(|| 0.0.into());
            fields.insert("total_spent_usd".into(), spent);
        }
        fields.entry("refund_usd").or_insert_with(|| 0.0.into());
        fields.entry("invocations").or_insert_with(|| 0.into());
    }
    serde_json::from_value(settled)
        .map_err(|e| AimarketError::Protocol(format!("Settlement decode failed: {e}")))
}

/// Build the POST /ai-market/v2/invoke JSON body. Kept as a free function so
/// the wire contract (optional product_id / source_hub / verify keys) is
/// unit-testable without HTTP.
fn build_invoke_body(
    capability_id: &str,
    input: serde_json::Value,
    product_id: Option<&str>,
    source_hub: Option<&str>,
    verify: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "capability_id": capability_id,
        "input": input,
    });
    if let Some(pid) = product_id {
        body["product_id"] = serde_json::Value::String(pid.to_string());
    }
    if let Some(hub) = source_hub {
        body["source_hub"] = serde_json::Value::String(hub.to_string());
    }
    // Pay-on-Verified opt-in: forwarded verbatim as the `verify` request block.
    if let Some(v) = verify {
        body["verify"] = v;
    }
    body
}

/// JSON truthiness for the `verify.wait` flag — the wire contract sends a bool,
/// but be forgiving of a numeric/string truthy value the way the JS/Dart SDKs are.
fn json_truthy(v: Option<&serde_json::Value>) -> bool {
    match v {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_f64().map_or(true, |f| f != 0.0),
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// Whether the verify block opts into blocking until the Metis verdict
/// (`verify.wait` truthy). Such an invoke must be single-shot: a retry would
/// re-POST a fresh nonce mid-wait and double-charge the buyer.
fn verify_block_waits(verify: Option<&serde_json::Value>) -> bool {
    verify.map_or(false, |v| json_truthy(v.get("wait")))
}

/// Per-request timeout for a verified-wait invoke, or `None` when the invoke
/// does not wait (use the client's default). The hub holds the response until
/// the Metis verdict, so allow `wait_timeout_s` (default 300) + 30s of slack —
/// exceeding the wait bound keeps the client from aborting (then retrying and
/// re-POSTing) mid-wait.
fn verified_wait_timeout(verify: Option<&serde_json::Value>) -> Option<Duration> {
    if !verify_block_waits(verify) {
        return None;
    }
    let secs = verify
        .and_then(|v| v.get("wait_timeout_s"))
        .and_then(|s| s.as_u64())
        .unwrap_or(300);
    Some(Duration::from_secs(secs + 30))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn configuration_debug_does_not_expose_private_key_or_api_key() {
        let mut config = AimarketAgentConfig::new("https://hub.test", "private-seed-for-redaction-test");
        config.api_key = Some("api-token-for-redaction-test".into());
        let printed = format!("{config:?}");
        assert!(!printed.contains("private-seed-for-redaction-test"));
        assert!(!printed.contains("api-token-for-redaction-test"));
        assert!(printed.contains("[redacted]"));
    }

    #[tokio::test]
    async fn lost_paid_response_is_never_replayed() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let first = read_request(&mut socket).await;
            assert_eq!(first.header("X-API-Key"), Some("test-secret"));
            // The hub charged this request, but the response was lost.
            drop(socket);
            if let Ok(Ok((mut replay, _))) = tokio::time::timeout(
                Duration::from_millis(1500), listener.accept(),
            ).await {
                let _ = read_request(&mut replay).await;
                replay.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                return 2;
            }
            1
        });
        let agent = agent_with_key(&url, "test-secret");
        assert!(agent.invoke("cap-1", serde_json::json!({}), "", None, None, None).await.is_err());
        assert_eq!(served.await.unwrap(), 1, "a second POST could charge again");
    }

    #[tokio::test]
    async fn authenticated_request_never_follows_a_redirect() {
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_url = format!("http://{}", target.local_addr().unwrap());
        let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_url = format!("http://{}", source.local_addr().unwrap());
        let served = tokio::spawn(async move {
            let (mut socket, _) = source.accept().await.unwrap();
            let _ = read_request(&mut socket).await;
            socket.write_all(format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {target_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            if let Ok(Ok((mut leaked, _))) = tokio::time::timeout(Duration::from_millis(300), target.accept()).await {
                let _ = read_request(&mut leaked).await;
                leaked.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                return true;
            }
            false
        });
        let agent = agent_with_key(&source_url, "test-secret");
        let result = agent.invoke("cap-1", serde_json::json!({}), "", None, None, None).await.unwrap();
        assert!(!result.success);
        assert!(!served.await.unwrap(), "redirect target received payment credentials");
    }

    /// A hub that answers exactly one request with `status` and `body`, and reports the path.
    async fn one_shot_hub(
        status: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
            request
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string()
        });
        (url, served)
    }

    /// One request as a hub saw it.
    #[derive(Debug)]
    struct Seen {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: String,
    }

    impl Seen {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(&name.to_ascii_lowercase()).map(String::as_str)
        }

        fn json(&self) -> serde_json::Value {
            serde_json::from_str(&self.body).expect("request body is JSON")
        }
    }

    /// Read one HTTP/1.1 request: head up to the blank line, then Content-Length bytes.
    async fn read_request(socket: &mut tokio::net::TcpStream) -> Seen {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0, "client closed before sending a request");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let mut lines = head.split("\r\n");
        let mut request_line = lines.next().unwrap_or_default().split_whitespace();
        let method = request_line.next().unwrap_or_default().to_string();
        let path = request_line.next().unwrap_or_default().to_string();
        let headers: HashMap<String, String> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect();
        let length: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        while buf.len() < head_end + length {
            let n = socket.read(&mut chunk).await.unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
        Seen { method, path, headers, body }
    }

    /// A hub that answers `replies` in order, one connection each, and returns what it saw.
    async fn scripted_hub(
        replies: Vec<(&'static str, String)>,
    ) -> (String, tokio::task::JoinHandle<Vec<Seen>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = tokio::spawn(async move {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                seen.push(read_request(&mut socket).await);
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            }
            seen
        });
        (url, served)
    }

    fn agent_with_key(url: &str, api_key: &str) -> AimarketAgent {
        AimarketAgent::with_config(AimarketAgentConfig {
            api_key: Some(api_key.to_string()),
            ..AimarketAgentConfig::new(url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
        })
    }

    const PAID_INVOKE: &str =
        r#"{"success":true,"result":{"ok":true},"price_usd":0.01,"latency_ms":20.0}"#;
    const DEPOSIT_TX: &str = "0x8f2a559490d8ff2bbcbf4cb3b0d8c4f1a9e1e5c3b1d1f4a6c8e0b2d4f6a8c0e2";
    const PAYER: &str = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed";

    /// What aimarket-hub (every production hub) answers to channel/open: the deposit as
    /// `original_deposit_usd`, the channel bound to the wallet that paid, the secret once.
    fn hub_channel(channel_id: &str, tx_hash: &str) -> String {
        serde_json::json!({
            "channel": {
                "channel_id": channel_id,
                "balance_usd": 5.0,
                "original_deposit_usd": 5.0,
                "used_usd": 0.0,
                "token": "USDC",
                "chain": "base",
                "wallet": PAYER,
                "tx_hash": tx_hash,
                "recipient": "0x1111111111111111111111111111111111111111",
                "verify_stub": false,
                "status": "open",
                "opened_at": "2026-10-04T12:00:00Z",
                // The hub's own format: time.strftime("%Y-%m-%dT%H:%M:%SZ").
                "expires_at": "2999-01-01T00:00:00Z",
                "channel_secret": "sec_Qm9vYmFyLXNlY3JldA",
            }
        })
        .to_string()
    }

    fn deposit(tx_hash: &str) -> ChannelDeposit {
        ChannelDeposit {
            tx_hash: tx_hash.to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn api_key_pays_with_the_credit_account_and_opens_no_channel() {
        let (url, served) = scripted_hub(vec![("200 OK", PAID_INVOKE.to_string())]).await;
        let agent = agent_with_key(&url, "ak_live_test_key");
        let result = agent
            .invoke("cap-1", serde_json::json!({"text": "hi"}), "", Some("prod-1"), None, None)
            .await
            .unwrap();
        assert!(result.success);

        let seen = served.await.unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            (seen[0].method.as_str(), seen[0].path.as_str()),
            ("POST", "/ai-market/v2/invoke")
        );
        assert_eq!(seen[0].header("X-API-Key"), Some("ak_live_test_key"));
        assert_eq!(seen[0].header("X-Payment-Channel"), None);
        assert_eq!(seen[0].header("X-Payment-Channel-Secret"), None);
        assert_eq!(seen[0].header("X-Market-Signature"), None);
        assert_eq!(seen[0].json()["product_id"], "prod-1");
    }

    #[tokio::test]
    async fn api_key_wins_over_a_channel_id_as_in_the_python_agent() {
        let (url, served) = scripted_hub(vec![("200 OK", PAID_INVOKE.to_string())]).await;
        let agent = agent_with_key(&url, "ak_live_test_key");
        agent
            .invoke("cap-1", serde_json::json!({}), "ch_stale", None, None, None)
            .await
            .unwrap();

        let seen = served.await.unwrap();
        assert_eq!(seen[0].header("X-API-Key"), Some("ak_live_test_key"));
        assert_eq!(seen[0].header("X-Payment-Channel"), None);
    }

    #[tokio::test]
    async fn run_once_with_an_api_key_neither_opens_nor_closes_a_channel() {
        let search = r#"{"query":"ATS","matches":[{"capability_id":"ats-rules","product_id":"ats-prod","price_per_call_usd":0.01,"source_hub":"local"}]}"#;
        let (url, served) = scripted_hub(vec![
            ("200 OK", search.to_string()),
            ("200 OK", PAID_INVOKE.to_string()),
        ])
        .await;
        let agent = agent_with_key(&url, "ak_live_test_key");
        let bom = agent
            .run_once("ATS", serde_json::json!({"role": "PM"}), None, None)
            .await
            .unwrap();
        assert!(bom.settlement.is_none());

        let seen = served.await.unwrap();
        assert!(seen[0].path.starts_with("/ai-market/v2/search?"));
        assert_eq!(seen[1].path, "/ai-market/v2/invoke");
        assert_eq!(seen[1].header("X-API-Key"), Some("ak_live_test_key"));
        assert!(seen.iter().all(|r| !r.path.contains("/channel/")));
    }

    #[tokio::test]
    async fn an_empty_credit_account_reports_what_the_hub_said() {
        let refusal = r#"{"success":false,"error":"payment_required","detail":"insufficient credit balance","needed":0.01,"balance":0.0}"#;
        let (url, _served) = scripted_hub(vec![("402 Payment Required", refusal.to_string())]).await;
        let agent = agent_with_key(&url, "ak_empty_account");
        let err = agent
            .invoke("cap-1", serde_json::json!({}), "", None, None, None)
            .await
            .unwrap_err();
        match err {
            AimarketError::Payment(msg) => {
                assert!(msg.contains("insufficient credit balance"), "{msg}");
                // The balance and the uncovered price ride along, whatever the prose says.
                assert!(msg.contains("(balance $0, needed $0.01)"), "{msg}");
            }
            other => panic!("expected a payment error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_invoke_with_neither_channel_nor_key_is_refused_before_sending() {
        // Port 9 (discard) is closed: had the SDK sent anything it would come back as a
        // network error after its retries, not as a payment error.
        let agent = AimarketAgent::new("http://127.0.0.1:9", "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let err = agent
            .invoke("cap-1", serde_json::json!({}), "", None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, AimarketError::Payment(_)), "{err:?}");
    }

    #[tokio::test]
    async fn open_channel_sends_the_deposit_and_reads_the_hub_channel() {
        let (url, served) =
            scripted_hub(vec![("200 OK", hub_channel("ch_9f3c2a1b7d4e5f60", DEPOSIT_TX))]).await;
        let agent = AimarketAgent::new(&url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let paid = ChannelDeposit {
            tx_hash: DEPOSIT_TX.to_string(),
            payer_signature: Some("0xpayerproof".to_string()),
            wallet: Some(PAYER.to_string()),
        };
        let channel = agent
            .open_channel(5.0, "USDC", "base", Some(&paid))
            .await
            .unwrap();

        let seen = served.await.unwrap();
        assert_eq!(seen[0].path, "/ai-market/v2/channel/open");
        assert_eq!(
            seen[0].json(),
            serde_json::json!({
                "deposit_usd": 5.0,
                "token": "USDC",
                "chain": "base",
                "tx_hash": DEPOSIT_TX,
                "payer_signature": "0xpayerproof",
                "wallet": PAYER,
            })
        );
        // `original_deposit_usd` becomes the model's deposit, so the balance ratio is real.
        assert_eq!(channel.deposit_usd, 5.0);
        assert_eq!(channel.balance_ratio(), 1.0);
        assert_eq!(channel.channel_secret.as_deref(), Some("sec_Qm9vYmFyLXNlY3JldA"));
    }

    #[tokio::test]
    async fn open_channel_without_a_deposit_sends_no_tx_hash() {
        let (url, served) =
            scripted_hub(vec![("200 OK", hub_channel("ch_dev", ""))]).await;
        let agent = AimarketAgent::new(&url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        agent.open_channel(5.0, "USDT", "base", None).await.unwrap();

        let seen = served.await.unwrap();
        assert_eq!(
            seen[0].json(),
            serde_json::json!({"deposit_usd": 5.0, "token": "USDT", "chain": "base"})
        );
    }

    #[tokio::test]
    async fn a_repeated_deposit_returns_its_channel_and_a_new_deposit_opens_another() {
        let second_tx = format!("0x{}", "ab".repeat(32));
        let (url, served) = scripted_hub(vec![
            ("200 OK", hub_channel("ch_first_deposit", DEPOSIT_TX)),
            ("200 OK", hub_channel("ch_second_deposit", &second_tx)),
        ])
        .await;
        let agent = AimarketAgent::new(&url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let first = agent
            .open_channel(5.0, "USDC", "base", Some(&deposit(DEPOSIT_TX)))
            .await
            .unwrap();
        // The same transaction, spelled in upper case.
        let shouted = format!("0x{}", DEPOSIT_TX[2..].to_uppercase());
        let again = agent
            .open_channel(5.0, "USDC", "base", Some(&deposit(&shouted)))
            .await
            .unwrap();
        let second = agent
            .open_channel(5.0, "USDC", "base", Some(&deposit(&second_tx)))
            .await
            .unwrap();

        assert_eq!(again.channel_id, first.channel_id);
        assert_eq!(second.channel_id, "ch_second_deposit");
        let seen = served.await.unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1].json()["tx_hash"], second_tx.as_str());
    }

    #[tokio::test]
    async fn a_deposit_without_a_tx_hash_is_refused_before_sending() {
        let agent = AimarketAgent::new("http://127.0.0.1:9", "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let err = agent
            .open_channel(5.0, "USDC", "base", Some(&deposit("  ")))
            .await
            .unwrap_err();
        assert!(matches!(err, AimarketError::Protocol(ref m) if m.contains("tx_hash")), "{err:?}");
    }

    #[tokio::test]
    async fn a_channel_invoke_presents_the_channel_and_its_secret() {
        let (url, served) = scripted_hub(vec![
            ("200 OK", hub_channel("ch_9f3c2a1b7d4e5f60", DEPOSIT_TX)),
            ("200 OK", PAID_INVOKE.to_string()),
        ])
        .await;
        let agent = AimarketAgent::new(&url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let channel = agent
            .open_channel(5.0, "USDC", "base", Some(&deposit(DEPOSIT_TX)))
            .await
            .unwrap();
        agent
            .invoke("cap-1", serde_json::json!({}), &channel.channel_id, None, None, None)
            .await
            .unwrap();

        let seen = served.await.unwrap();
        assert_eq!(seen[1].header("X-Payment-Channel"), Some("ch_9f3c2a1b7d4e5f60"));
        assert_eq!(seen[1].header("X-Payment-Channel-Secret"), Some("sec_Qm9vYmFyLXNlY3JldA"));
        assert_eq!(seen[1].header("X-API-Key"), None);
    }

    #[tokio::test]
    async fn close_channel_presents_secret_and_wallet_and_reads_the_hub_settlement() {
        let settlement = serde_json::json!({
            "settlement": {
                "channel_id": "ch_9f3c2a1b7d4e5f60",
                "used_usd": 0.01,
                "refund_usd": 4.99,
                "refund_executed_usd": 0.0,
                "refund_owed_usd": 4.99,
                "refund_status": "owed",
                "obligation": null,
                "original_deposit_usd": 5.0,
                "status": "settled",
                "settle_tx_hash": "",
            }
        })
        .to_string();
        let (url, served) = scripted_hub(vec![
            ("200 OK", hub_channel("ch_9f3c2a1b7d4e5f60", DEPOSIT_TX)),
            ("200 OK", settlement),
        ])
        .await;
        let agent = AimarketAgent::new(&url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let channel = agent
            .open_channel(5.0, "USDC", "base", Some(&deposit(DEPOSIT_TX)))
            .await
            .unwrap();
        let settled = agent.close_channel(&channel.channel_id).await.unwrap();

        let seen = served.await.unwrap();
        assert_eq!(seen[1].path, "/ai-market/v2/channel/close");
        assert_eq!(seen[1].header("X-Payment-Channel-Secret"), Some("sec_Qm9vYmFyLXNlY3JldA"));
        assert_eq!(
            seen[1].json(),
            serde_json::json!({"channel_id": "ch_9f3c2a1b7d4e5f60", "wallet": PAYER})
        );
        assert_eq!(settled.channel_id, "ch_9f3c2a1b7d4e5f60");
        assert_eq!(settled.total_spent_usd, 0.01);
        assert_eq!(settled.refund_usd, 4.99);
        assert_eq!(settled.invocations, 0);
    }

    #[tokio::test]
    async fn fetch_trusted_hashes_reads_the_hub_registry() {
        let (url, served) = one_shot_hub(
            "200 OK",
            r#"{"hashes":[{"capability_id":"cap-1","code_hash":"abc123"},{"capability_id":"cap-2"}]}"#,
        )
        .await;
        let agent = AimarketAgent::new(&url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        assert_eq!(agent.fetch_trusted_hashes().await.unwrap(), 2);
        assert_eq!(
            served.await.unwrap(),
            "/.well-known/trusted-code-hashes.json"
        );
    }

    #[tokio::test]
    async fn fetch_trusted_hashes_reports_a_hub_that_has_no_registry() {
        let (url, _served) = one_shot_hub("404 Not Found", r#"{"detail":"Not Found"}"#).await;
        let agent = AimarketAgent::new(&url, "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let err = agent.fetch_trusted_hashes().await.unwrap_err();
        assert!(matches!(err, AimarketError::Protocol(_)), "{err:?}");
    }

    #[test]
    fn verify_tee_receipt_binds_input_and_output() {
        use sha2::{Digest, Sha256};
        let hex_sha = |s: &str| hex::encode(Sha256::digest(s.as_bytes()));
        let receipt = TeeReceipt {
            receipt_id: "r-1".into(),
            input_hash: hex_sha(r#"{"text":"hi"}"#),
            output_hash: hex_sha(r#"{"answer":"hello"}"#),
            signature: String::new(),
        };
        let agent = AimarketAgent::new("http://127.0.0.1:9", "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        assert!(agent.verify_tee_receipt(&receipt, r#"{"text":"hi"}"#, r#"{"answer":"hello"}"#));
        assert!(!agent.verify_tee_receipt(
            &receipt,
            r#"{"text":"hi"}"#,
            r#"{"answer":"tampered"}"#
        ));
    }

    #[test]
    fn invoke_body_includes_verify_when_set() {
        let verify = serde_json::json!({
            "requested": true,
            "intent": "translate hi",
            "mode": "auto",
        });
        let body = build_invoke_body(
            "cap-1",
            serde_json::json!({"text": "hi"}),
            Some("prod-1"),
            None,
            Some(verify.clone()),
        );
        assert_eq!(body["capability_id"], "cap-1");
        assert_eq!(body["product_id"], "prod-1");
        assert_eq!(body["verify"], verify);
    }

    #[test]
    fn invoke_body_omits_verify_when_unset() {
        let body = build_invoke_body("cap-1", serde_json::json!({}), None, None, None);
        assert!(body.get("verify").is_none());
        assert!(body.get("product_id").is_none());
        assert!(body.get("source_hub").is_none());
    }

    #[test]
    fn verified_wait_extends_timeout_past_wait_bound() {
        // wait=true, default wait_timeout_s (300) -> 300 + 30s slack.
        let verify = serde_json::json!({"requested": true, "wait": true});
        assert_eq!(
            verified_wait_timeout(Some(&verify)),
            Some(Duration::from_secs(330))
        );
        // Explicit wait_timeout_s is honored.
        let verify = serde_json::json!({"wait": true, "wait_timeout_s": 600});
        assert_eq!(
            verified_wait_timeout(Some(&verify)),
            Some(Duration::from_secs(630))
        );
    }

    #[test]
    fn no_timeout_override_without_verified_wait() {
        // No verify block, or wait=false/absent -> use the client default (None).
        assert_eq!(verified_wait_timeout(None), None);
        assert!(!verify_block_waits(None));
        let no_wait = serde_json::json!({"requested": true, "wait": false});
        assert_eq!(verified_wait_timeout(Some(&no_wait)), None);
        assert!(!verify_block_waits(Some(&no_wait)));
        let async_verify = serde_json::json!({"requested": true, "intent": "x"});
        assert_eq!(verified_wait_timeout(Some(&async_verify)), None);
        // wait=true flips the retry-disable / longer-timeout decision on.
        let waiting = serde_json::json!({"wait": true});
        assert!(verify_block_waits(Some(&waiting)));
    }
}

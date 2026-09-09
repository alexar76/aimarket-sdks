use crate::error::AimarketError;
use crate::models::*;
use crate::signer::MarketSigner;
use crate::tee::TeeVerifier;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AimarketAgentConfig {
    pub hub_url: String,
    pub wallet_key: String,
    pub affiliate: String,
    pub timeout: Duration,
    pub max_retries: u32,
    pub verify_tee: bool,
}

struct CachedChannel {
    channel: Channel,
    #[allow(dead_code)]
    cached_at: Instant,
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
    well_known_cache: Mutex<Option<String>>,
}

impl AimarketAgent {
    pub fn new(hub_url: &str, wallet_key_hex: &str) -> Self {
        Self::with_config(AimarketAgentConfig {
            hub_url: hub_url.trim_end_matches('/').to_string(),
            wallet_key: wallet_key_hex.to_string(),
            affiliate: "aimarket-sdk-rust".to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            verify_tee: true,
        })
    }

    pub fn with_config(config: AimarketAgentConfig) -> Self {
        Self {
            signer: MarketSigner::new(&config.wallet_key),
            tee_verifier: Mutex::new(TeeVerifier::new(&config.wallet_key, HashMap::new())),
            client: Client::builder()
                .timeout(config.timeout)
                .build()
                .unwrap_or_else(|_| Client::new()),
            channel_cache: Mutex::new(HashMap::new()),
            channel_secrets: Mutex::new(HashMap::new()),
            well_known_cache: Mutex::new(None),
            config,
        }
    }

    async fn retry_with_backoff<T, F, Fut>(&self, mut operation: F) -> Result<T, AimarketError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, AimarketError>>,
    {
        let mut last_error =
            AimarketError::Network(format!("Request failed after {} retries", self.config.max_retries));

        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(AimarketError::Network(e)) => last_error = AimarketError::Network(e),
                Err(e) => return Err(e),
            }
            if attempt < self.config.max_retries {
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            }
        }
        Err(last_error)
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

    pub async fn open_channel(
        &self,
        deposit_usd: f64,
        token: &str,
        chain: &str,
    ) -> Result<Channel, AimarketError> {
        let cache_key = format!("{deposit_usd}:{token}:{chain}");
        {
            let cache = self.channel_cache.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                if cached.is_reusable() {
                    return Ok(cached.channel.clone());
                }
            }
        }
        self.channel_cache.lock().unwrap().remove(&cache_key);

        let url = format!("{}/ai-market/v2/channel/open", self.config.hub_url);
        let body = serde_json::json!({
            "deposit_usd": deposit_usd,
            "token": token,
            "chain": chain,
        });

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
            return Err(AimarketError::Protocol(format!(
                "Channel open failed: {}",
                resp.status()
            )));
        }

        // The hub wraps the channel in a `{ "channel": {...} }` envelope (matching the
        // Python agent + live hub); unwrap it, tolerating a bare object for forward-compat.
        let mut raw: serde_json::Value = resp.json().await?;
        let channel_val = if raw.get("channel").is_some() {
            raw["channel"].take()
        } else {
            raw
        };
        let channel: Channel = serde_json::from_value(channel_val)
            .map_err(|e| AimarketError::Protocol(format!("Channel decode failed: {e}")))?;
        // Capture the one-time debit secret so invoke() can present it via
        // X-Payment-Channel-Secret (required by secure channels; dropping it 402s).
        if let Some(secret) = channel.channel_secret.clone() {
            self.channel_secrets
                .lock()
                .unwrap()
                .insert(channel.channel_id.clone(), secret);
        }
        self.channel_cache.lock().unwrap().insert(
            cache_key,
            CachedChannel {
                channel: channel.clone(),
                cached_at: Instant::now(),
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
        // A verified invoke with wait=true blocks server-side until Metis returns
        // a verdict while the hub holds the debit in escrow. Retrying it on a
        // transient error would re-POST a fresh nonce mid-wait and open a SECOND
        // hold — the buyer pays twice. Fire it exactly once (no retry) in that case.
        if verify_block_waits(verify.as_ref()) {
            return self
                .invoke_once(capability_id, input, channel_id, product_id, source_hub, verify)
                .await;
        }
        self.retry_with_backoff(|| {
            self.invoke_once(
                capability_id,
                input.clone(),
                channel_id,
                product_id,
                source_hub,
                verify.clone(),
            )
        })
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
        let canonical = format!(
            "channel:{}|capability:{}|affiliate:{}",
            channel_id, capability_id, self.config.affiliate
        );
        let signature = self.signer.sign(&canonical);

        // When the caller opts into verify.wait, the hub holds this response open
        // until Metis returns a verdict (up to wait_timeout_s). Compute the per-
        // request timeout before `verify` is consumed by build_invoke_body below.
        let wait_timeout = verified_wait_timeout(verify.as_ref());

        let body = build_invoke_body(capability_id, input, product_id, source_hub, verify);

        // Present the one-time debit secret captured at open (required by secure channels).
        let channel_secret = self.channel_secrets.lock().unwrap().get(channel_id).cloned();
        let mut req = self
            .client
            .post(&url)
            .json(&body)
            .header("X-Payment-Channel", channel_id)
            .header("X-AIMarket-Affiliate", &self.config.affiliate)
            .header("X-Market-Signature", &signature);
        if let Some(secret) = channel_secret {
            req = req.header("X-Payment-Channel-Secret", secret);
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
            402 => Err(AimarketError::Payment(
                "Channel depleted or expired — open a new channel".into(),
            )),
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

        let url = format!("{}/ai-market/v2/channel/close", self.config.hub_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({"channel_id": channel_id}))
            .header("X-AIMarket-Affiliate", &self.config.affiliate)
            .send()
            .await?;

        if resp.status().as_u16() == 404 {
            return Err(AimarketError::Protocol(format!(
                "Channel not found: {channel_id}"
            )));
        }
        if !resp.status().is_success() {
            return Err(AimarketError::Protocol(format!(
                "Settlement failed: {}",
                resp.status()
            )));
        }

        Ok(resp.json().await?)
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

    pub fn trust_code_hash(&self, capability_id: &str, code_hash: &str) {
        self.tee_verifier
            .lock()
            .unwrap()
            .trust_code_hash(capability_id, code_hash);
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

        let channel = self.open_channel(deposit, "USDT", "base").await?;
        let step = &plan[0];

        let result = self
            .invoke(
                &step.capability.capability_id,
                input,
                &channel.channel_id,
                Some(&step.capability.product_id),
                Some(&step.capability.source_hub),
                None,
            )
            .await?;

        let settlement = self.close_channel(&channel.channel_id).await?;

        Ok(BillOfMaterials {
            task: intent.to_string(),
            plan,
            results: vec![result.clone()],
            settlement: Some(settlement),
            total_spent_usd: result.price_usd,
            protocol_version: "v2".to_string(),
        })
    }

    pub fn dispose(&self) {
        self.channel_cache.lock().unwrap().clear();
        *self.well_known_cache.lock().unwrap() = None;
    }
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

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
            params.push(("budget_usd".to_string(), b.to_string()));
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

        let data: SearchResponse = resp.json().await?;
        Ok(data.results)
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

    pub async fn invoke(
        &self,
        capability_id: &str,
        input: serde_json::Value,
        channel_id: &str,
        product_id: Option<&str>,
        source_hub: Option<&str>,
    ) -> Result<InvokeResult, AimarketError> {
        self.retry_with_backoff(|| {
            self.invoke_once(capability_id, input.clone(), channel_id, product_id, source_hub)
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
    ) -> Result<InvokeResult, AimarketError> {
        let url = format!("{}/ai-market/v2/invoke", self.config.hub_url);
        let canonical = format!(
            "channel:{}|capability:{}|affiliate:{}",
            channel_id, capability_id, self.config.affiliate
        );
        let signature = self.signer.sign(&canonical);

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

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .header("X-Payment-Channel", channel_id)
            .header("X-AIMarket-Affiliate", &self.config.affiliate)
            .header("X-Market-Signature", &signature)
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
                    output: None,
                    price_usd: 0.0,
                    latency_ms: 0.0,
                    safety_blocked: false,
                    safety_reason: None,
                    tee_verified: false,
                    tee_attestation: None,
                    tee_receipt: None,
                    error: Some(format!("HTTP {status}: {body}")),
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
                self.invoke(cap_id, input.clone(), channel_id, None, source_hub)
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

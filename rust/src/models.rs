use serde::{Deserialize, Serialize};

/// `version` is optional metadata the protocol/hub may omit; default rather than
/// failing deserialization so the SDK stays interoperable with hubs that don't emit it.
fn default_capability_version() -> String {
    "1.0.0".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub capability_id: String,
    pub product_id: String,
    pub name: String,
    #[serde(default = "default_capability_version")]
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
    pub price_per_call_usd: f64,
    #[serde(default)]
    pub p50_latency_ms: Option<f64>,
    #[serde(default)]
    pub success_rate_30d: Option<f64>,
    pub source_hub: String,
    #[serde(default)]
    pub source_hub_name: Option<String>,
    #[serde(default)]
    pub trust_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub channel_id: String,
    pub deposit_usd: f64,
    pub balance_usd: f64,
    pub token: String,
    pub chain: String,
    pub expires_at: String,
}

impl Channel {
    pub fn balance_ratio(&self) -> f64 {
        if self.deposit_usd <= 0.0 {
            0.0
        } else {
            self.balance_usd / self.deposit_usd
        }
    }

    pub fn is_expired(&self) -> bool {
        parse_iso8601_secs(&self.expires_at)
            .map(|expires| expires < unix_now_secs())
            .unwrap_or(true)
    }
}

fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn parse_iso8601_secs(ts: &str) -> Option<i64> {
    let trimmed = ts.trim_end_matches('Z');
    let (date_part, time_part) = trimmed.split_once('T')?;
    let mut date_parts = date_part.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let sec = time_part.split('.').next().unwrap_or(time_part);
    let mut time_parts = sec.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;
    Some(
        (year - 1970) * 365 * 86400
            + (month - 1) * 30 * 86400
            + (day - 1) * 86400
            + hour * 3600
            + minute * 60
            + second,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResult {
    pub success: bool,
    #[serde(default)]
    pub output: Option<serde_json::Value>,
    pub price_usd: f64,
    pub latency_ms: f64,
    #[serde(default)]
    pub safety_blocked: bool,
    #[serde(default)]
    pub safety_reason: Option<String>,
    #[serde(default)]
    pub tee_verified: bool,
    #[serde(default)]
    pub tee_attestation: Option<TeeAttestation>,
    #[serde(default)]
    pub tee_receipt: Option<TeeReceipt>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeeAttestation {
    pub platform: String,
    pub enclave_id: String,
    pub code_hash: String,
    #[serde(default)]
    pub pcr_values: std::collections::HashMap<String, String>,
    pub instance_id: String,
    pub region: String,
    pub timestamp: String,
    pub ttl_s: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeeReceipt {
    pub receipt_id: String,
    pub input_hash: String,
    pub output_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub capability: Capability,
    pub relevance_score: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub channel_id: String,
    pub total_spent_usd: f64,
    pub refund_usd: f64,
    pub invocations: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<PlanStep>,
    pub total: i64,
    pub hub: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillOfMaterials {
    pub task: String,
    pub plan: Vec<PlanStep>,
    pub results: Vec<InvokeResult>,
    pub settlement: Option<Settlement>,
    pub total_spent_usd: f64,
    pub protocol_version: String,
}

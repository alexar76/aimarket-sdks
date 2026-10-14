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
    /// One-time debit secret returned by the hub at open (secure channels).
    /// Sent as `X-Payment-Channel-Secret` on every invoke; captured by the SDK.
    #[serde(default)]
    pub channel_secret: Option<String>,
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

/// Days from 1970-01-01 to the given proleptic-Gregorian civil date.
/// Howard Hinnant's `days_from_civil` — leap-year and month-length exact.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
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
    // Real calendar arithmetic. The previous naive form (year*365 + month*30 +
    // day-1) ignored leap years and true month lengths, drifting ~2 weeks per
    // decade and reporting funded channels expired early.
    Some(days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResult {
    pub success: bool,
    /// Capability output. The hub returns this under `result` (invoke body),
    /// NOT `output` — the old field name silently dropped every payload.
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    pub price_usd: f64,
    pub latency_ms: f64,
    /// Settlement/payment receipt returned by the hub for this invocation.
    #[serde(default)]
    pub receipt: Option<serde_json::Value>,
    /// Continuation envelope for multi-step / streaming capabilities.
    #[serde(default)]
    pub continuation: Option<serde_json::Value>,
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
    /// Machine-readable error classifier accompanying `error`.
    #[serde(default)]
    pub error_type: Option<String>,
    /// Pay-on-Verified envelope (hub `verification` field): status pending|
    /// settled|refunded|skipped, verified, verify_score, trace_id, … Present
    /// only when the invoke opted in via the request's `verify` block.
    #[serde(default)]
    pub verification: Option<serde_json::Value>,
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

/// A single flat capability match in the hub's GET /ai-market/v2/search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub capability_id: String,
    pub product_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub price_per_call_usd: Option<f64>,
    #[serde(default)]
    pub routed_price_usd: Option<f64>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub trust_score: Option<f64>,
    #[serde(default)]
    pub p50_latency_ms: Option<f64>,
    #[serde(default)]
    pub source_hub: Option<String>,
    #[serde(default)]
    pub source_hub_name: Option<String>,
    #[serde(default)]
    pub status_label: Option<String>,
}

impl From<SearchMatch> for PlanStep {
    fn from(m: SearchMatch) -> Self {
        let name = m.name.clone().unwrap_or_else(|| m.capability_id.clone());
        let relevance_score = m.score.unwrap_or(0.0);
        let rationale = m.status_label.clone().unwrap_or_default();
        let price = m.price_per_call_usd.or(m.routed_price_usd).unwrap_or(0.0);
        PlanStep {
            capability: Capability {
                capability_id: m.capability_id,
                product_id: m.product_id,
                name,
                version: m.version.unwrap_or_else(default_capability_version),
                description: m.description.unwrap_or_default(),
                input_schema: None,
                output_schema: None,
                price_per_call_usd: price,
                p50_latency_ms: m.p50_latency_ms,
                success_rate_30d: None,
                source_hub: m.source_hub.unwrap_or_else(|| "local".to_string()),
                source_hub_name: m.source_hub_name,
                trust_score: m.trust_score,
            },
            relevance_score,
            rationale,
        }
    }
}

/// Envelope of GET /ai-market/v2/search — matches the live hub contract:
/// `{ query, matches: [flat capability dicts], catalog }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub matches: Vec<SearchMatch>,
    #[serde(default)]
    pub catalog: String,
    /// Protocol version the hub answered with (e.g. `v2`).
    #[serde(default)]
    pub protocol_version: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_matches_real_calendar() {
        assert_eq!(parse_iso8601_secs("1970-01-01T00:00:00Z"), Some(0));
        // 2021-01-01 accounts for every leap year since the epoch.
        assert_eq!(parse_iso8601_secs("2021-01-01T00:00:00Z"), Some(1_609_459_200));
        // 2024 is a leap year, so Feb has 29 days.
        assert_eq!(parse_iso8601_secs("2024-03-01T00:00:00Z"), Some(1_709_251_200));
    }

    #[test]
    fn future_channel_not_expired() {
        let ch = Channel {
            channel_id: "c".into(),
            deposit_usd: 1.0,
            balance_usd: 1.0,
            token: "USDC".into(),
            chain: "base".into(),
            expires_at: "2100-01-01T00:00:00Z".into(),
            channel_secret: None,
        };
        assert!(!ch.is_expired());
    }

    #[test]
    fn invoke_result_parses_verification_envelope() {
        let json = serde_json::json!({
            "success": true,
            "result": {"ok": true},
            "price_usd": 0.1,
            "latency_ms": 12.0,
            "verification": {"requested": true, "status": "pending", "performed": false},
        });
        let parsed: InvokeResult = serde_json::from_value(json).unwrap();
        let envelope = parsed.verification.expect("verification envelope");
        assert_eq!(envelope["status"], "pending");
        // Legacy hubs omit the field entirely — must default to None, not fail.
        let legacy: InvokeResult = serde_json::from_value(serde_json::json!({
            "success": true,
            "price_usd": 0.0,
            "latency_ms": 1.0,
        }))
        .unwrap();
        assert!(legacy.verification.is_none());
    }

    #[test]
    fn search_match_maps_to_plan_step() {
        let m = SearchMatch {
            capability_id: "cap-1".into(),
            product_id: "prod-1".into(),
            name: None,
            description: None,
            version: None,
            price_per_call_usd: None,
            routed_price_usd: Some(0.25),
            score: Some(0.9),
            trust_score: Some(0.8),
            p50_latency_ms: None,
            source_hub: None,
            source_hub_name: None,
            status_label: Some("Completed".into()),
        };
        let step: PlanStep = m.into();
        assert_eq!(step.capability.capability_id, "cap-1");
        assert_eq!(step.capability.name, "cap-1"); // falls back to id
        assert_eq!(step.capability.price_per_call_usd, 0.25);
        assert_eq!(step.relevance_score, 0.9);
        assert_eq!(step.rationale, "Completed");
    }
}

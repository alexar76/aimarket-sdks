use crate::models::{TeeAttestation, TeeReceipt};
use crate::signer::MarketSigner;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Recognized TEE platform identifiers.
pub mod platform {
    pub const AWS_NITRO: &str = "aws_nitro";
    pub const INTEL_TDX: &str = "intel_tdx";
    pub const AMD_SEV: &str = "amd_sev";
    pub const AZURE_CC: &str = "azure_cc";

    pub fn is_supported(platform: &str) -> bool {
        matches!(
            platform,
            AWS_NITRO | INTEL_TDX | AMD_SEV | AZURE_CC
        )
    }
}

#[derive(Debug, Clone)]
pub struct TeeVerificationResult {
    pub is_valid: bool,
    pub failures: Vec<String>,
}

impl TeeVerificationResult {
    pub fn pass() -> Self {
        Self {
            is_valid: true,
            failures: vec![],
        }
    }

    pub fn fail(failures: Vec<String>) -> Self {
        Self {
            is_valid: false,
            failures,
        }
    }
}

pub struct TrustedHashCache {
    ttl: Duration,
    entries: HashMap<String, (String, SystemTime)>,
}

impl TrustedHashCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<String> {
        let (hash, expires_at) = self.entries.get(key)?;
        if SystemTime::now() > *expires_at {
            self.entries.remove(key);
            return None;
        }
        Some(hash.clone())
    }

    pub fn set(&mut self, key: &str, hash: &str) {
        let expires_at = SystemTime::now() + self.ttl;
        self.entries
            .insert(key.to_string(), (hash.to_string(), expires_at));
    }
}

/// Local TEE attestation verifier.
///
/// Placeholder values registered for each supported platform. These are NOT
/// real trust anchors — they exist only so the verifier knows which platforms
/// it expects an operator to configure. A configured key MUST replace them via
/// [`TeeVerifier::with_enclave_keys`] before any attestation can pass; an
/// unreplaced default is treated as "unconfigured" and fails closed (see
/// [`is_placeholder_enclave_key`]).
const DEFAULT_ENCLAVE_PUBLIC_KEYS: &[(&str, &str)] = &[
    ("aws_nitro", "nitro_enclave_pubkey_hex"),
    ("intel_tdx", "tdx_enclave_pubkey_hex"),
    ("amd_sev", "sev_enclave_pubkey_hex"),
    ("azure_cc", "azure_cc_pubkey_hex"),
];

/// Returns `true` if `key` is one of the built-in placeholder enclave keys, i.e.
/// no real trust anchor has been configured for the platform. Such keys must
/// never be accepted as a valid signer.
fn is_placeholder_enclave_key(key: &str) -> bool {
    DEFAULT_ENCLAVE_PUBLIC_KEYS
        .iter()
        .any(|(_, placeholder)| *placeholder == key)
}

pub struct TeeVerifier {
    signer: MarketSigner,
    trusted_code_hashes: HashMap<String, String>,
    hash_cache: TrustedHashCache,
    enclave_public_keys: HashMap<String, String>,
}

impl TeeVerifier {
    pub fn new(wallet_key_hex: &str, trusted: HashMap<String, String>) -> Self {
        Self::with_enclave_keys(wallet_key_hex, trusted, HashMap::new())
    }

    pub fn with_enclave_keys(
        wallet_key_hex: &str,
        trusted: HashMap<String, String>,
        enclave_overrides: HashMap<String, String>,
    ) -> Self {
        let mut enclave_public_keys: HashMap<String, String> = DEFAULT_ENCLAVE_PUBLIC_KEYS
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        enclave_public_keys.extend(enclave_overrides);
        Self {
            signer: MarketSigner::new(wallet_key_hex),
            trusted_code_hashes: trusted,
            hash_cache: TrustedHashCache::new(Duration::from_secs(300)),
            enclave_public_keys,
        }
    }

    pub fn trust_code_hash(&mut self, capability_id: &str, code_hash: &str) {
        self.trusted_code_hashes
            .insert(capability_id.to_string(), code_hash.to_string());
        self.hash_cache.set(capability_id, code_hash);
    }

    pub fn verify_attestation_detailed(
        &mut self,
        att: &TeeAttestation,
        capability_id: &str,
    ) -> TeeVerificationResult {
        let mut failures = Vec::new();

        if !platform::is_supported(&att.platform) {
            failures.push(format!("Unsupported TEE platform: {}", att.platform));
        }

        if att.is_expired() {
            failures.push("Attestation expired".into());
        }

        if att.pcr_values.is_empty() {
            failures.push("PCR values are empty — attestation lacks hardware proof".into());
        }

        let expected = self
            .hash_cache
            .get(capability_id)
            .or_else(|| self.trusted_code_hashes.get(capability_id).cloned());
        if let Some(expected_hash) = expected {
            if att.code_hash != expected_hash {
                failures.push(format!(
                    "Code hash mismatch: expected {}, got {}",
                    expected_hash, att.code_hash
                ));
            }
        }

        match self.enclave_public_keys.get(&att.platform) {
            Some(key) if is_placeholder_enclave_key(key) => failures.push(format!(
                "Enclave public key for platform {} is unconfigured (default placeholder); \
                 refusing to trust attestation",
                att.platform
            )),
            Some(key) if !self.signer.verify(key, &att.signature, &att.canonical()) => {
                failures.push("Enclave signature verification failed".into());
            }
            None => failures.push(format!(
                "No known enclave public key for platform: {}",
                att.platform
            )),
            _ => {}
        }

        if failures.is_empty() {
            TeeVerificationResult::pass()
        } else {
            TeeVerificationResult::fail(failures)
        }
    }

    pub fn verify_attestation(&mut self, att: &TeeAttestation, capability_id: &str) -> bool {
        self.verify_attestation_detailed(att, capability_id).is_valid
    }

    pub fn verify_receipt(
        &self,
        receipt: &TeeReceipt,
        expected_input: &str,
        received_output: &str,
    ) -> bool {
        let input_hash = sha256_hex(expected_input);
        let output_hash = sha256_hex(received_output);
        receipt.input_hash == input_hash && receipt.output_hash == output_hash
    }
}

fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex::encode(h.finalize())
}

impl TeeAttestation {
    pub fn canonical(&self) -> String {
        format!(
            "platform:{}|enclave_id:{}|code_hash:{}|pcr0:{}|instance:{}|region:{}|timestamp:{}|ttl:{}",
            self.platform,
            self.enclave_id,
            self.code_hash,
            self.pcr_values.get("pcr0").map_or("", |v| v),
            self.instance_id,
            self.region,
            self.timestamp,
            self.ttl_s,
        )
    }

    pub fn is_expired(&self) -> bool {
        let Ok(parsed) = chrono_like_parse(&self.timestamp) else {
            return true;
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        now - parsed > self.ttl_s
    }
}

/// Parse an ISO-8601 UTC timestamp (`YYYY-MM-DDThh:mm:ss[.fff][Z]`) into a Unix
/// epoch in seconds.
///
/// Implemented without a date-library dependency, but using a correct,
/// leap-year-aware calendar conversion (Howard Hinnant's "days from civil"
/// algorithm) rather than a fixed 365-day-year / 30-day-month approximation.
/// All fields are range-validated, so a malformed or out-of-range timestamp
/// returns `Err(())` and the caller fails closed (treats the attestation as
/// expired) instead of computing a bogus epoch.
fn chrono_like_parse(ts: &str) -> Result<i64, ()> {
    let trimmed = ts.trim_end_matches('Z');
    let (date_part, time_part) = trimmed.split_once('T').ok_or(())?;

    let (year, month, day) = {
        let mut parts = date_part.split('-');
        (
            parts.next().ok_or(())?.parse::<i64>().map_err(|_| ())?,
            parts.next().ok_or(())?.parse::<i64>().map_err(|_| ())?,
            parts.next().ok_or(())?.parse::<i64>().map_err(|_| ())?,
        )
    };
    let (hour, minute, second) = {
        let sec = time_part.split('.').next().unwrap_or(time_part);
        let mut parts = sec.split(':');
        (
            parts.next().ok_or(())?.parse::<i64>().map_err(|_| ())?,
            parts.next().ok_or(())?.parse::<i64>().map_err(|_| ())?,
            parts.next().ok_or(())?.parse::<i64>().map_err(|_| ())?,
        )
    };

    if !(1..=12).contains(&month) || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return Err(());
    }
    // Allow second == 60 to tolerate leap seconds, as ISO-8601 permits.
    if !(0..=60).contains(&second) {
        return Err(());
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err(());
    }

    let days = days_from_civil(year, month, day);
    Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// `true` for Gregorian leap years.
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Number of days in `month` (1–12) of `year`, leap-year aware.
fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since the Unix epoch (1970-01-01) for a proleptic Gregorian date.
///
/// Howard Hinnant's `days_from_civil`: exact for any `month` in `1..=12` and
/// `day` in `1..=31`, correctly accounting for leap years and month lengths.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // Shift the year so that the leap day falls at the end of the era's cycle.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_attestation(signer: &MarketSigner) -> TeeAttestation {
        let att = TeeAttestation {
            platform: "aws_nitro".into(),
            enclave_id: "enclave-1".into(),
            code_hash: "abc123".into(),
            pcr_values: HashMap::from([("pcr0".into(), "pcr0val".into())]),
            instance_id: "i-1".into(),
            region: "us-east-1".into(),
            timestamp: "2099-01-01T00:00:00Z".into(),
            ttl_s: 300,
            signature: String::new(),
        };
        let canonical = att.canonical();
        TeeAttestation {
            signature: signer.sign(&canonical),
            ..att
        }
    }

    #[test]
    fn verifies_with_trusted_hash() {
        let signer = MarketSigner::new("abcdef0123");
        let att = sample_attestation(&signer);
        let mut verifier = TeeVerifier::with_enclave_keys(
            "abcdef0123",
            HashMap::from([("cap-1".into(), "abc123".into())]),
            HashMap::from([("aws_nitro".into(), signer.public_key_hex())]),
        );
        assert!(verifier.verify_attestation(&att, "cap-1"));
    }

    #[test]
    fn rejects_unsupported_platform() {
        let signer = MarketSigner::new("abcdef0123");
        let mut att = sample_attestation(&signer);
        att.platform = "bogus".into();
        let mut verifier = TeeVerifier::new("abcdef0123", HashMap::new());
        let result = verifier.verify_attestation_detailed(&att, "cap-1");
        assert!(!result.is_valid);
    }

    #[test]
    fn rejects_default_enclave_key_fails_closed() {
        // No enclave override: the platform key stays a placeholder, which must
        // never be trusted even when everything else about the attestation is
        // self-consistent and signed.
        let signer = MarketSigner::new("abcdef0123");
        let att = sample_attestation(&signer);
        let mut verifier =
            TeeVerifier::new("abcdef0123", HashMap::from([("cap-1".into(), "abc123".into())]));
        let result = verifier.verify_attestation_detailed(&att, "cap-1");
        assert!(!result.is_valid);
        assert!(
            result.failures.iter().any(|f| f.contains("unconfigured")),
            "expected an unconfigured-key failure, got {:?}",
            result.failures
        );
    }

    #[test]
    fn epoch_matches_known_unix_timestamps() {
        assert_eq!(chrono_like_parse("1970-01-01T00:00:00Z"), Ok(0));
        assert_eq!(chrono_like_parse("2000-01-01T00:00:00Z"), Ok(946_684_800));
        // 2021-01-01T00:00:00Z — verifies post-2020 leap years are counted.
        assert_eq!(chrono_like_parse("2021-01-01T00:00:00Z"), Ok(1_609_459_200));
        assert_eq!(chrono_like_parse("2009-02-13T23:31:30Z"), Ok(1_234_567_890));
    }

    #[test]
    fn leap_day_and_month_boundaries() {
        // 2020 is a leap year: Feb 29 exists and maps to the correct epoch.
        assert_eq!(chrono_like_parse("2020-02-29T00:00:00Z"), Ok(1_582_934_400));
        // 2020-03-01 is exactly one day after the leap day.
        assert_eq!(
            chrono_like_parse("2020-03-01T00:00:00Z"),
            Ok(1_582_934_400 + 86_400)
        );
        // Crossing a non-Feb month boundary (Jan has 31 days).
        assert_eq!(
            chrono_like_parse("2021-02-01T00:00:00Z").unwrap()
                - chrono_like_parse("2021-01-01T00:00:00Z").unwrap(),
            31 * 86_400
        );
    }

    #[test]
    fn rejects_invalid_calendar_dates() {
        // 2021 is not a leap year, so Feb 29 is invalid and must fail to parse.
        assert_eq!(chrono_like_parse("2021-02-29T00:00:00Z"), Err(()));
        // 1900 is divisible by 100 but not 400 — not a leap year.
        assert_eq!(chrono_like_parse("1900-02-29T00:00:00Z"), Err(()));
        // 2000 is divisible by 400 — a leap year, so Feb 29 is valid.
        assert!(chrono_like_parse("2000-02-29T00:00:00Z").is_ok());
        // Out-of-range fields.
        assert_eq!(chrono_like_parse("2021-13-01T00:00:00Z"), Err(()));
        assert_eq!(chrono_like_parse("2021-04-31T00:00:00Z"), Err(()));
        assert_eq!(chrono_like_parse("2021-01-01T24:00:00Z"), Err(()));
    }

    #[test]
    fn leap_year_helpers() {
        assert!(is_leap_year(2020));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2021));
        assert_eq!(days_in_month(2020, 2), 29);
        assert_eq!(days_in_month(2021, 2), 28);
        assert_eq!(days_in_month(2021, 4), 30);
        assert_eq!(days_in_month(2021, 12), 31);
    }

    #[test]
    fn expiry_uses_correct_calendar_math() {
        // An attestation issued just before a leap day with a short TTL must be
        // considered expired "now" (2026), and a far-future one must not.
        let mut att = TeeAttestation {
            platform: "aws_nitro".into(),
            enclave_id: "e".into(),
            code_hash: "h".into(),
            pcr_values: HashMap::from([("pcr0".into(), "v".into())]),
            instance_id: "i".into(),
            region: "r".into(),
            timestamp: "2020-02-29T00:00:00Z".into(),
            ttl_s: 300,
            signature: String::new(),
        };
        assert!(att.is_expired(), "old attestation must be expired");
        att.timestamp = "2099-12-31T23:59:59Z".into();
        assert!(!att.is_expired(), "far-future attestation must not be expired");
        // Unparseable timestamp fails closed (treated as expired).
        att.timestamp = "not-a-timestamp".into();
        assert!(att.is_expired());
    }
}

//! Canonical channel-open payer proof (EIP-191).
//!
//! Opening a payment channel against an on-chain deposit requires proving control of the
//! wallet that PAID. Every deposit lands in the same platform settlement wallet, so
//! recipient+amount alone only shows that *somebody* paid — without this signature, anyone
//! watching inbound transfers could quote a stranger's (public) tx hash and be credited.
//!
//! Personal-sign rather than typed data on purpose: any ordinary wallet or hardware device
//! can produce it with no EIP-712 support.
//!
//! The message must match `aimarket-protocol/test-vectors/payer-proof.json` byte for byte —
//! the hub rebuilds it independently and compares recovered signers, so a one-character
//! difference reaches the user as an unexplainable "invalid payer proof".

use k256::ecdsa::{RecoveryId, SigningKey, VerifyingKey};

use crate::eip712::keccak256;

pub const PAYER_PROOF_DOMAIN: &str = "AIMarket-Payer-Proof";
pub const PAYER_PROOF_VERSION: u32 = 1;
pub const PAYER_PROOF_CHANNEL_OPEN: &str = "channel-open";

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Canonical chain id: chain names are ASCII and case-free.
pub fn canonical_proof_chain(chain: &str) -> String {
    chain.trim().to_ascii_lowercase()
}

/// Canonical transaction id.
///
/// An EVM hash is hex and case-insensitive at the JSON-RPC layer, so `0xABC…` and `0xabc…`
/// name the same transaction and must produce the same challenge. The `0x` prefix is
/// normalised IN as well, because the two hub stacks disagree about whether they strip it
/// first. Anything non-hex (a base58 Solana signature) is case-SIGNIFICANT and left exact.
pub fn canonical_proof_tx_hash(tx_hash: &str) -> String {
    let tx = tx_hash.trim();
    let body = tx.strip_prefix("0x").or_else(|| tx.strip_prefix("0X")).unwrap_or(tx);
    if is_hex(body) {
        format!("0x{}", body.to_ascii_lowercase())
    } else {
        tx.to_string()
    }
}

/// Canonical payer: EIP-55 mixed case is a checksum, not identity, so an address lowercases.
pub fn canonical_proof_payer(payer: &str) -> String {
    let addr = payer.trim();
    if addr.len() == 42 {
        if let Some(body) = addr.strip_prefix("0x").or_else(|| addr.strip_prefix("0X")) {
            if is_hex(body) {
                return format!("0x{}", body.to_ascii_lowercase());
            }
        }
    }
    addr.to_string()
}

/// Deposit amount as the integer cents both ledgers bill in.
///
/// ROUND-HALF-TO-EVEN, because the hub computes this with Python's `round()`, which is
/// banker's rounding — `0.125 → 12`, not 13. Rust's `f64::round` rounds half AWAY from
/// zero, so using it here silently produces a different preimage (and a rejected proof) for
/// any amount landing exactly on a half-cent. Returns -1 for an unusable amount so the
/// challenge stays deterministic and can never match a real deposit.
pub fn canonical_proof_amount_cents(amount_usd: f64) -> i64 {
    if !amount_usd.is_finite() {
        return -1;
    }
    let scaled = amount_usd * 100.0;
    let floor = scaled.floor();
    let diff = scaled - floor;
    let cents = if diff > 0.5 {
        floor + 1.0
    } else if diff < 0.5 {
        floor
    } else if (floor as i64) % 2 == 0 {
        floor
    } else {
        floor + 1.0
    };
    cents as i64
}

/// Everything the challenge is built from.
#[derive(Debug, Clone)]
pub struct PayerProofParams {
    pub chain: String,
    pub tx_hash: String,
    pub payer: String,
    pub amount_usd: f64,
}

/// The exact text the paying wallet signs.
pub fn channel_open_proof_message(params: &PayerProofParams) -> String {
    format!(
        "{}/v{}\npurpose:{}\nchain:{}\ntx:{}\npayer:{}\namount_cents:{}",
        PAYER_PROOF_DOMAIN,
        PAYER_PROOF_VERSION,
        PAYER_PROOF_CHANNEL_OPEN,
        canonical_proof_chain(&params.chain),
        canonical_proof_tx_hash(&params.tx_hash),
        canonical_proof_payer(&params.payer),
        canonical_proof_amount_cents(params.amount_usd),
    )
}

/// The EIP-191 digest: `keccak256(0x19 || "Ethereum Signed Message:\n" || len || message)`.
///
/// The leading `0x19` is a BYTE, not the characters '1','9', and the length counts BYTES of
/// the UTF-8 encoding — both are easy to get subtly wrong, and both change the digest from
/// its first byte, i.e. the signature recovers to a stranger.
pub fn channel_open_proof_hash(params: &PayerProofParams) -> [u8; 32] {
    let message = channel_open_proof_message(params);
    let body = message.as_bytes();
    let mut buf = Vec::with_capacity(body.len() + 32);
    buf.push(0x19);
    buf.extend_from_slice(b"Ethereum Signed Message:\n");
    buf.extend_from_slice(body.len().to_string().as_bytes());
    buf.extend_from_slice(body);
    keccak256(&buf)
}

/// Sign the challenge with the paying wallet's key. Returns `0x`-prefixed r‖s‖v (65 bytes).
///
/// `v` is 27/28, the EIP-191 convention the hub's `eth_account` recovery expects; a raw
/// 0/1 recovery id produces a signature it cannot parse.
pub fn sign_channel_open_proof(
    params: &PayerProofParams,
    private_key_hex: &str,
) -> Result<String, String> {
    let key_hex = private_key_hex.trim().trim_start_matches("0x");
    let key_bytes = hex::decode(key_hex).map_err(|e| format!("bad private key hex: {e}"))?;
    let signing_key =
        SigningKey::from_slice(&key_bytes).map_err(|e| format!("bad private key: {e}"))?;
    let digest = channel_open_proof_hash(params);
    let (sig, recovery_id): (k256::ecdsa::Signature, RecoveryId) = signing_key
        .sign_prehash_recoverable(&digest)
        .map_err(|e| format!("signing failed: {e}"))?;
    let mut out = Vec::with_capacity(65);
    out.extend_from_slice(&sig.to_bytes());
    out.push(recovery_id.to_byte() + 27);
    Ok(format!("0x{}", hex::encode(out)))
}

/// Address that produced `signature` over this challenge — the hub's own check, locally.
pub fn recover_channel_open_payer(
    params: &PayerProofParams,
    signature: &str,
) -> Result<String, String> {
    let raw = hex::decode(signature.trim().trim_start_matches("0x"))
        .map_err(|e| format!("bad signature hex: {e}"))?;
    if raw.len() != 65 {
        return Err(format!("signature must be 65 bytes, got {}", raw.len()));
    }
    let recovery = RecoveryId::from_byte(if raw[64] >= 27 { raw[64] - 27 } else { raw[64] })
        .ok_or_else(|| format!("bad recovery id {}", raw[64]))?;
    let sig = k256::ecdsa::Signature::from_slice(&raw[..64])
        .map_err(|e| format!("bad signature: {e}"))?;
    let digest = channel_open_proof_hash(params);
    let key = VerifyingKey::recover_from_prehash(&digest, &sig, recovery)
        .map_err(|e| format!("recovery failed: {e}"))?;
    // The address is the last 20 bytes of keccak256 over the UNCOMPRESSED public key with
    // its 0x04 tag stripped.
    let encoded = key.to_encoded_point(false);
    let hash = keccak256(&encoded.as_bytes()[1..]);
    Ok(format!("0x{}", hex::encode(&hash[12..])))
}

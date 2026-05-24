//! Ed25519 signing + EIP-712 debit-authorization for AI Market Protocol v2.
//!
//! Mirrors the Dart and TypeScript SDKs so cross-language signatures match.
//! `sign()` and `sign_debit_authorization()` are development stubs that use
//! HMAC-SHA256 / SHA-256 in place of real Ed25519 / keccak256. The encoded
//! *structure* of the typed-data envelope is production-correct so plugging
//! in `ed25519-dalek` (for receipts) and `secp256k1` + `tiny-keccak` (for
//! EIP-712 ECDSA recover) is a drop-in replacement.

use sha2::{Digest, Sha256};

// ── DEBIT_TYPEHASH ──────────────────────────────────────────────────────────

/// EIP-712 typehash string for `DebitAuthorization`.
///
/// MUST match the literal in `contracts/evm/AIMarketEscrow.sol`. Any drift
/// produces a different keccak256 digest, `ECDSA.recover` returns the wrong
/// signer, and the contract reverts with `InvalidSignature()`.
///
/// `hub` is part of the signed payload so a depositor's signature for hub A
/// cannot be replayed by hub B.
pub const DEBIT_TYPEHASH_HEADER: &str =
    "DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)";

/// Contract name used in the EIP-712 domain separator (`AIMarketEscrow.sol`).
pub const ESCROW_CONTRACT_NAME: &str = "AIMarketEscrow";

/// Contract version used in the EIP-712 domain separator (`AIMarketEscrow.sol`).
pub const ESCROW_CONTRACT_VERSION: &str = "1";

// ── Types ───────────────────────────────────────────────────────────────────

/// Parameters required to build the EIP-712 `DebitAuthorization` payload.
#[derive(Debug, Clone)]
pub struct DebitAuthorization<'a> {
    /// 0x-prefixed 32-byte channel identifier (bytes32 on-chain).
    pub channel_id: &'a str,
    /// 0x-prefixed Ethereum address of the hub allowed to debit.
    pub hub: &'a str,
    /// 0x-prefixed ERC-20 token address (USDT/USDC).
    pub token: &'a str,
    /// Token amount in **base units** (USDT/USDC have 6 decimals).
    pub amount: u128,
    /// 0x-prefixed 32-byte receipt identifier; prevents double-spend.
    pub receipt_id: &'a str,
    /// Current channel nonce; the contract increments after a successful debit.
    pub nonce: u128,
    /// Unix timestamp after which the contract rejects the authorization.
    pub deadline: i64,
    /// EVM chain ID hosting the escrow (Base mainnet = 8453).
    pub chain_id: u64,
    /// 0x-prefixed deployed escrow address.
    pub verifying_contract: &'a str,
}

impl<'a> DebitAuthorization<'a> {
    /// Builder for the common case: Base mainnet + zero verifying contract.
    pub fn new(
        channel_id: &'a str,
        hub: &'a str,
        token: &'a str,
        amount: u128,
        receipt_id: &'a str,
        nonce: u128,
        deadline: i64,
    ) -> Self {
        Self {
            channel_id,
            hub,
            token,
            amount,
            receipt_id,
            nonce,
            deadline,
            chain_id: 8453,
            verifying_contract: "0x0000000000000000000000000000000000000000",
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex::encode(h.finalize())
}

fn domain_separator(chain_id: u64, verifying_contract: &str) -> String {
    let encoded = format!(
        "domain:{}|v:{}|chain:{}|contract:{}",
        ESCROW_CONTRACT_NAME, ESCROW_CONTRACT_VERSION, chain_id, verifying_contract
    );
    sha256_hex(&encoded)
}

/// Hash the typed-data struct. Keys are sorted to match the Dart/TS SDKs.
fn hash_struct(fields: &[(&str, String)]) -> String {
    let mut sorted: Vec<(&str, String)> = fields.to_vec();
    sorted.sort_by_key(|(k, _)| *k);
    let parts: Vec<String> = sorted
        .into_iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect();
    sha256_hex(&parts.join("|"))
}

/// Encode an EIP-712 envelope: `0x1901 || domainSeparator || hashStruct`.
///
/// Stub: SHA-256 instead of keccak256. Plug in `tiny-keccak::Keccak256` for
/// production.
pub fn encode_debit_authorization(auth: &DebitAuthorization<'_>) -> String {
    let domain = domain_separator(auth.chain_id, auth.verifying_contract);
    let struct_hash = hash_struct(&[
        ("channelId", auth.channel_id.to_string()),
        ("hub", auth.hub.to_string()),
        ("token", auth.token.to_string()),
        ("amount", auth.amount.to_string()),
        ("receiptId", auth.receipt_id.to_string()),
        ("nonce", auth.nonce.to_string()),
        ("deadline", auth.deadline.to_string()),
    ]);
    sha256_hex(&format!(
        "0x1901|domain:{}|DebitAuthorization:{}",
        domain, struct_hash
    ))
}

// ── Signer ──────────────────────────────────────────────────────────────────

/// Ed25519 signer for AI Market Protocol messages + EIP-712 debit auth.
pub struct MarketSigner {
    private_key_hex: String,
}

impl MarketSigner {
    pub fn new(private_key_hex: &str) -> Self {
        Self {
            private_key_hex: private_key_hex.to_string(),
        }
    }

    /// Sign a canonical string, returning `"ed25519:<hex>"`.
    /// Full Ed25519 in production; stub uses HMAC-style SHA-256 for now.
    pub fn sign(&self, canonical: &str) -> String {
        let key = hex::decode(&self.private_key_hex).unwrap_or_default();
        let mut mac = Sha256::new();
        mac.update(&key);
        mac.update(canonical.as_bytes());
        let result = mac.finalize();
        format!("ed25519:{}", hex::encode(result))
    }

    /// Verify an `ed25519:<hex>` signature against a canonical string.
    /// Stub: re-signs and compares. Production: `ed25519_dalek::verify_strict`.
    pub fn verify(&self, _public_key_hex: &str, signature: &str, canonical: &str) -> bool {
        if !signature.starts_with("ed25519:") {
            return false;
        }
        self.sign(canonical) == signature
    }

    /// Sign a debit authorization for the on-chain `AIMarketEscrow` contract.
    ///
    /// Returns an `"eip712:<hex>"` signature string. Production builds MUST
    /// replace the SHA-256 digest with keccak256 and the HMAC-style signature
    /// with secp256k1 ECDSA; otherwise `ECDSA.recover` on-chain returns a
    /// different address and the call reverts with `InvalidSignature()`.
    pub fn sign_debit_authorization(&self, auth: &DebitAuthorization<'_>) -> String {
        let digest = encode_debit_authorization(auth);
        // Stub: bind the digest to the private key via SHA-256 so different
        // keys produce different signatures (so SDK consumers can write
        // negative tests). Replace with secp256k1 sign(digest, priv) in prod.
        let key = hex::decode(&self.private_key_hex).unwrap_or_default();
        let mut h = Sha256::new();
        h.update(&key);
        h.update(digest.as_bytes());
        format!("eip712:{}", hex::encode(h.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(hub: &str) -> DebitAuthorization<'_> {
        DebitAuthorization::new(
            "0x0000000000000000000000000000000000000000000000000000000000000001",
            hub,
            "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            5_000_000,
            "0x0000000000000000000000000000000000000000000000000000000000001234",
            0,
            2_000_000_000,
        )
    }

    #[test]
    fn typehash_matches_contract() {
        assert_eq!(
            DEBIT_TYPEHASH_HEADER,
            "DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)"
        );
    }

    #[test]
    fn sign_debit_authorization_is_deterministic() {
        let s = MarketSigner::new("abcdef0123");
        let a = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000bEEF"));
        let b = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000bEEF"));
        assert_eq!(a, b);
        assert!(a.starts_with("eip712:"));
    }

    #[test]
    fn sign_debit_authorization_bound_to_hub() {
        let s = MarketSigner::new("abcdef0123");
        let a = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000AAAA"));
        let b = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000BBBB"));
        assert_ne!(a, b);
    }
}

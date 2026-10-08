//! Production Ed25519 + EIP-712 signing for AI Market Protocol v2.

use crate::eip712::{self, DebitDigestParams};
pub use crate::eip712::{DEBIT_TYPEHASH_HEADER, ESCROW_CONTRACT_NAME, ESCROW_CONTRACT_VERSION};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signer as Ed25519Signer, SigningKey, Verifier, VerifyingKey};
use k256::ecdsa::{RecoveryId, SigningKey as Secp256k1Key};
use sha2::{Digest, Sha256};

/// Parameters required to build the EIP-712 `DebitAuthorization` payload.
#[derive(Debug, Clone)]
pub struct DebitAuthorization<'a> {
    pub channel_id: &'a str,
    pub hub: &'a str,
    pub token: &'a str,
    pub amount: u128,
    pub receipt_id: &'a str,
    pub nonce: u128,
    pub deadline: i64,
    pub chain_id: u64,
    pub verifying_contract: &'a str,
}

/// Default EIP-712 `chainId` when none is supplied: Base mainnet (8453).
///
/// The escrow contract verifies the signature against the `chainId` baked into
/// the EIP-712 domain separator, so signing for the wrong chain produces a
/// signature the contract will reject. Callers targeting any other network MUST
/// use [`DebitAuthorization::new_with_chain_id`].
pub const BASE_CHAIN_ID: u64 = 8453;

impl<'a> DebitAuthorization<'a> {
    /// Build a `DebitAuthorization` for Base mainnet (`chainId` = 8453).
    ///
    /// This is a convenience wrapper over [`new_with_chain_id`] preserved for
    /// backward compatibility. Use [`new_with_chain_id`] or [`new_for_base`]
    /// to make the target chain explicit.
    ///
    /// [`new_with_chain_id`]: DebitAuthorization::new_with_chain_id
    /// [`new_for_base`]: DebitAuthorization::new_for_base
    pub fn new(
        channel_id: &'a str,
        hub: &'a str,
        token: &'a str,
        amount: u128,
        receipt_id: &'a str,
        nonce: u128,
        deadline: i64,
    ) -> Self {
        Self::new_with_chain_id(
            channel_id, hub, token, amount, receipt_id, nonce, deadline, BASE_CHAIN_ID,
        )
    }

    /// Build a `DebitAuthorization` for Base mainnet (`chainId` = 8453).
    ///
    /// Explicit alias for [`new`] for call sites that want the target chain to
    /// read clearly at the construction point.
    ///
    /// [`new`]: DebitAuthorization::new
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_base(
        channel_id: &'a str,
        hub: &'a str,
        token: &'a str,
        amount: u128,
        receipt_id: &'a str,
        nonce: u128,
        deadline: i64,
    ) -> Self {
        Self::new_with_chain_id(
            channel_id, hub, token, amount, receipt_id, nonce, deadline, BASE_CHAIN_ID,
        )
    }

    /// Build a `DebitAuthorization` for an explicit EIP-712 `chainId`.
    ///
    /// The signature is bound to `chain_id` via the domain separator, so this
    /// MUST match the chain on which the escrow contract executes.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_chain_id(
        channel_id: &'a str,
        hub: &'a str,
        token: &'a str,
        amount: u128,
        receipt_id: &'a str,
        nonce: u128,
        deadline: i64,
        chain_id: u64,
    ) -> Self {
        Self {
            channel_id,
            hub,
            token,
            amount,
            receipt_id,
            nonce,
            deadline,
            chain_id,
            verifying_contract: "0x0000000000000000000000000000000000000000",
        }
    }
}

pub fn compute_debit_digest(auth: &DebitAuthorization<'_>) -> [u8; 32] {
    eip712::compute_debit_digest(&DebitDigestParams {
        channel_id: auth.channel_id,
        hub: auth.hub,
        token: auth.token,
        amount: auth.amount,
        receipt_id: auth.receipt_id,
        nonce: auth.nonce,
        deadline: auth.deadline as u128,
        chain_id: auth.chain_id,
        verifying_contract: auth.verifying_contract,
    })
}

fn parse_seed_bytes(hex_or_dev: &str) -> [u8; 32] {
    let normalized = hex_or_dev.strip_prefix("0x").unwrap_or(hex_or_dev);
    if normalized.len() == 64 && normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes = hex::decode(normalized).unwrap_or_default();
        if bytes.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&bytes);
            return out;
        }
    }
    let digest = Sha256::digest(hex_or_dev.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn parse_eth_private_key(hex: &str) -> Result<[u8; 32], String> {
    let normalized = hex.strip_prefix("0x").unwrap_or(hex);
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("ethereum private key must be 32-byte hex".into());
    }
    let bytes = hex::decode(normalized).map_err(|e| e.to_string())?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn decode_public_key(public_key: &str) -> Result<VerifyingKey, ()> {
    let trimmed = public_key.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes = hex::decode(trimmed).map_err(|_| ())?;
        if bytes.len() != 32 {
            return Err(());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        return VerifyingKey::from_bytes(&arr).map_err(|_| ());
    }
    let bytes = STANDARD.decode(trimmed).map_err(|_| ())?;
    if bytes.len() != 32 {
        return Err(());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    VerifyingKey::from_bytes(&arr).map_err(|_| ())
}

/// Ed25519 + secp256k1 signer for AI Market Protocol messages.
pub struct MarketSigner {
    ed25519: SigningKey,
    ethereum_private_key: Option<[u8; 32]>,
}

impl MarketSigner {
    pub fn new(ed25519_seed_hex: &str) -> Self {
        Self {
            ed25519: SigningKey::from_bytes(&parse_seed_bytes(ed25519_seed_hex)),
            ethereum_private_key: None,
        }
    }

    pub fn with_ethereum_key(ed25519_seed_hex: &str, ethereum_private_key_hex: &str) -> Result<Self, String> {
        Ok(Self {
            ed25519: SigningKey::from_bytes(&parse_seed_bytes(ed25519_seed_hex)),
            ethereum_private_key: Some(parse_eth_private_key(ethereum_private_key_hex)?),
        })
    }

    pub fn public_key_base64(&self) -> String {
        STANDARD.encode(self.ed25519.verifying_key().to_bytes())
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.ed25519.verifying_key().to_bytes())
    }

    /// Sign canonical UTF-8 string → `ed25519:<base64>`.
    pub fn sign(&self, canonical: &str) -> String {
        let sig = self.ed25519.sign(canonical.as_bytes());
        format!("ed25519:{}", STANDARD.encode(sig.to_bytes()))
    }

    pub fn verify(&self, public_key: &str, signature: &str, canonical: &str) -> bool {
        if !signature.starts_with("ed25519:") {
            return false;
        }
        let Ok(vk) = decode_public_key(public_key) else {
            return false;
        };
        let Ok(sig_bytes) = STANDARD.decode(signature.trim_start_matches("ed25519:")) else {
            return false;
        };
        if sig_bytes.len() != 64 {
            return false;
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&sig_bytes);
        let sig = ed25519_dalek::Signature::from_bytes(&arr);
        vk.verify(canonical.as_bytes(), &sig).is_ok()
    }

    /// Sign EIP-712 debit authorization → `eip712:0x<r><s><v>`.
    pub fn sign_debit_authorization(&self, auth: &DebitAuthorization<'_>) -> Result<String, String> {
        let eth_key = self
            .ethereum_private_key
            .ok_or_else(|| "ethereum private key required for EIP-712 signing".to_string())?;
        let digest = compute_debit_digest(auth);
        let signing_key =
            Secp256k1Key::from_bytes((&eth_key).into()).map_err(|e| e.to_string())?;
        let (sig, recovery_id): (k256::ecdsa::Signature, RecoveryId) = signing_key
            .sign_prehash_recoverable(&digest)
            .map_err(|e| e.to_string())?;
        let sig_bytes = sig.to_bytes();
        let r = &sig_bytes[..32];
        let s = &sig_bytes[32..];
        let v = recovery_id.to_byte() + 27;
        Ok(format!("eip712:0x{}{}{:02x}", hex::encode(r), hex::encode(s), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ED25519_SEED: &str =
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const ETH_KEY: &str =
        "ac0974bec39a17e36ba4b6b40d764b994fa08d04ce65968ec04ec80ecb000000";

    fn sample(hub: &str) -> DebitAuthorization<'_> {
        DebitAuthorization {
            channel_id: "0x0000000000000000000000000000000000000000000000000000000000000001",
            hub,
            token: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            amount: 5_000_000,
            receipt_id: "0x0000000000000000000000000000000000000000000000000000000000001234",
            nonce: 0,
            deadline: 2_000_000_000,
            chain_id: 31_337,
            verifying_contract: "0x5FbDB2315678afecb367f032d93F642f64180aa3",
        }
    }

    #[test]
    fn typehash_matches_contract() {
        assert_eq!(
            DEBIT_TYPEHASH_HEADER,
            "DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)"
        );
        assert_eq!(ESCROW_CONTRACT_NAME, "AIMarketEscrow");
        assert_eq!(ESCROW_CONTRACT_VERSION, "1");
    }

    #[test]
    fn ed25519_sign_and_verify() {
        let s = MarketSigner::new(ED25519_SEED);
        let sig = s.sign("hello");
        assert!(sig.starts_with("ed25519:"));
        assert!(s.verify(&s.public_key_hex(), &sig, "hello"));
        assert!(!s.verify(&s.public_key_hex(), &sig, "world"));
    }

    #[test]
    fn sign_debit_authorization_is_deterministic() {
        let s = MarketSigner::with_ethereum_key(ED25519_SEED, ETH_KEY).unwrap();
        let a = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000bEEF")).unwrap();
        let b = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000bEEF")).unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with("eip712:0x"));
    }

    #[test]
    fn sign_debit_authorization_bound_to_hub() {
        let s = MarketSigner::with_ethereum_key(ED25519_SEED, ETH_KEY).unwrap();
        let a = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000AAAA")).unwrap();
        let b = s.sign_debit_authorization(&sample("0x000000000000000000000000000000000000BBBB")).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn constructors_default_to_base_chain_id() {
        let hub = "0x000000000000000000000000000000000000bEEF";
        let token = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
        let channel = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let receipt = "0x0000000000000000000000000000000000000000000000000000000000001234";

        let default = DebitAuthorization::new(channel, hub, token, 5_000_000, receipt, 0, 2_000_000_000);
        let base = DebitAuthorization::new_for_base(channel, hub, token, 5_000_000, receipt, 0, 2_000_000_000);
        assert_eq!(default.chain_id, BASE_CHAIN_ID);
        assert_eq!(default.chain_id, 8453);
        assert_eq!(base.chain_id, BASE_CHAIN_ID);
    }

    #[test]
    fn sign_debit_authorization_bound_to_chain_id() {
        let hub = "0x000000000000000000000000000000000000bEEF";
        let token = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
        let channel = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let receipt = "0x0000000000000000000000000000000000000000000000000000000000001234";
        let s = MarketSigner::with_ethereum_key(ED25519_SEED, ETH_KEY).unwrap();

        let base = DebitAuthorization::new_with_chain_id(channel, hub, token, 5_000_000, receipt, 0, 2_000_000_000, 8453);
        let optimism = DebitAuthorization::new_with_chain_id(channel, hub, token, 5_000_000, receipt, 0, 2_000_000_000, 10);
        let sig_base = s.sign_debit_authorization(&base).unwrap();
        let sig_op = s.sign_debit_authorization(&optimism).unwrap();
        assert_ne!(sig_base, sig_op, "signature must be bound to chain_id");
    }
}

//! Production EIP-712 encoding for AIMarketEscrow debit authorizations.

use tiny_keccak::{Hasher, Keccak};

pub const EIP712_DOMAIN_TYPE: &str =
    "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)";

pub const DEBIT_TYPEHASH_HEADER: &str =
    "DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)";

pub const ESCROW_CONTRACT_NAME: &str = "AIMarketEscrow";
pub const ESCROW_CONTRACT_VERSION: &str = "1";

#[derive(Debug, Clone)]
pub struct DebitDigestParams<'a> {
    pub channel_id: &'a str,
    pub hub: &'a str,
    pub token: &'a str,
    pub amount: u128,
    pub receipt_id: &'a str,
    pub nonce: u128,
    pub deadline: u128,
    pub chain_id: u64,
    pub verifying_contract: &'a str,
}

pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut h = Keccak::v256();
    h.update(data);
    h.finalize(&mut out);
    out
}

fn parse_hex32(value: &str) -> [u8; 32] {
    let hex = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(hex).expect("invalid 32-byte hex");
    assert_eq!(bytes.len(), 32, "expected 32 bytes, got {}", bytes.len());
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

fn encode_address(value: &str) -> [u8; 32] {
    let hex = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(hex).expect("invalid address hex");
    assert!(bytes.len() <= 20, "address longer than 20 bytes");
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    out
}

fn encode_u256(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
}

fn abi_encode(words: &[[u8; 32]]) -> Vec<u8> {
    words.iter().flat_map(|w| w.iter().copied()).collect()
}

pub fn domain_separator(chain_id: u64, verifying_contract: &str) -> [u8; 32] {
    let domain_type_hash = keccak256(EIP712_DOMAIN_TYPE.as_bytes());
    let name_hash = keccak256(ESCROW_CONTRACT_NAME.as_bytes());
    let version_hash = keccak256(ESCROW_CONTRACT_VERSION.as_bytes());
    keccak256(&abi_encode(&[
        domain_type_hash,
        name_hash,
        version_hash,
        encode_u256(chain_id as u128),
        encode_address(verifying_contract),
    ]))
}

pub fn debit_struct_hash(params: &DebitDigestParams<'_>) -> [u8; 32] {
    let type_hash = keccak256(DEBIT_TYPEHASH_HEADER.as_bytes());
    keccak256(&abi_encode(&[
        type_hash,
        parse_hex32(params.channel_id),
        encode_address(params.hub),
        encode_address(params.token),
        encode_u256(params.amount),
        parse_hex32(params.receipt_id),
        encode_u256(params.nonce),
        encode_u256(params.deadline),
    ]))
}

/// EIP-712 digest: `keccak256(0x1901 || domainSeparator || structHash)`.
pub fn compute_debit_digest(params: &DebitDigestParams<'_>) -> [u8; 32] {
    let domain = domain_separator(params.chain_id, params.verifying_contract);
    let struct_hash = debit_struct_hash(params);
    let mut buf = Vec::with_capacity(2 + 64);
    buf.extend_from_slice(&[0x19, 0x01]);
    buf.extend_from_slice(&domain);
    buf.extend_from_slice(&struct_hash);
    keccak256(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_deterministic() {
        let params = DebitDigestParams {
            channel_id: "0x0000000000000000000000000000000000000000000000000000000000000001",
            hub: "0x000000000000000000000000000000000000bEEF",
            token: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            amount: 5_000_000,
            receipt_id: "0x0000000000000000000000000000000000000000000000000000000000001234",
            nonce: 0,
            deadline: 2_000_000_000,
            chain_id: 31_337,
            verifying_contract: "0x5FbDB2315678afecb367f032d93F642f64180aa3",
        };
        let a = compute_debit_digest(&params);
        let b = compute_debit_digest(&params);
        assert_eq!(a, b);
    }
}

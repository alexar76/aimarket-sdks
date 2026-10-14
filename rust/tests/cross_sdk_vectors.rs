use aimarket_agent::eip712::{compute_debit_digest, DebitDigestParams};

#[test]
fn debit_digest_matches_typescript_vector() {
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
    let digest = compute_debit_digest(&params);
    // Frozen from TypeScript viem hashTypedData (see test-vectors/debit_authorization.json).
    let expected = include_str!("../../test-vectors/debit_authorization.digest");
    assert_eq!(hex::encode(digest), expected.trim());
}

/// The signature, not just the digest.
///
/// A digest test passes even when the signing call hashes its input a second time — the
/// result is self-consistent and `AIMarketEscrow.debitChannel` rejects it as
/// `InvalidSignature()`, because the contract recovers over the digest. Both the Dart and
/// the TypeScript SDK shipped exactly that bug, each in a different disguise (web3dart
/// hashing the payload; @noble/curves v2 defaulting `prehash` to true). RFC-6979 makes the
/// signature deterministic and low-s, so every SDK must produce these same bytes.
#[test]
fn debit_signature_matches_the_shared_vector() {
    use aimarket_agent::signer::{DebitAuthorization, MarketSigner};

    let text = include_str!("../../test-vectors/debit_authorization.json");
    let vector: serde_json::Value = serde_json::from_str(text).expect("vector json");
    let params = &vector["params"];
    let expected = vector["expectedSignature"]
        .as_str()
        .expect("fixture must carry expectedSignature");

    let signer = MarketSigner::with_ethereum_key(
        vector["ed25519SeedHex"].as_str().unwrap(),
        vector["ethereumPrivateKeyHex"].as_str().unwrap(),
    )
    .expect("signer");

    let auth = DebitAuthorization {
        channel_id: params["channelId"].as_str().unwrap(),
        hub: params["hub"].as_str().unwrap(),
        token: params["token"].as_str().unwrap(),
        amount: params["amount"].as_str().unwrap().parse().unwrap(),
        receipt_id: params["receiptId"].as_str().unwrap(),
        nonce: params["nonce"].as_str().unwrap().parse().unwrap(),
        deadline: params["deadline"].as_i64().unwrap(),
        chain_id: params["chainId"].as_u64().unwrap(),
        verifying_contract: params["verifyingContract"].as_str().unwrap(),
    };

    let encoded = signer.sign_debit_authorization(&auth).expect("signing");
    let signature = encoded
        .strip_prefix("eip712:")
        .expect("the SDK tags its EIP-712 signatures");
    assert_eq!(signature, expected, "signature must match the shared vector");

    // Solidity's ECDSA.recover wants v in {27,28}; an EIP-155 chain-encoded v is invalid.
    let v = u8::from_str_radix(&signature[signature.len() - 2..], 16).unwrap();
    assert!(v == 27 || v == 28, "v must be 27/28, got {v}");
}

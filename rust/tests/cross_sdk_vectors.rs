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

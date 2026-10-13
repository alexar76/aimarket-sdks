//! Strict Ed25519 against test-vectors/ed25519_small_order.json (2026-10-12).
use aimarket_agent::MarketSigner;
use base64::{engine::general_purpose::STANDARD, Engine as _};

fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("../../test-vectors/ed25519_small_order.json")).unwrap()
}

fn sig(hex_str: &str) -> String {
    format!("ed25519:{}", STANDARD.encode(hex::decode(hex_str).unwrap()))
}

#[test]
fn rfc8032_test1_verifies_and_every_small_order_forgery_is_refused() {
    let v = vectors();
    let signer = MarketSigner::new(&"11".repeat(32));
    let t = &v["rfc8032Test1"];
    assert!(signer.verify(
        t["publicKeyHex"].as_str().unwrap(),
        &sig(t["signatureHex"].as_str().unwrap()),
        t["message"].as_str().unwrap()
    ));
    for f in v["forgeries"].as_array().unwrap() {
        for m in f["messages"].as_array().unwrap() {
            assert!(!signer.verify(
                f["publicKeyHex"].as_str().unwrap(),
                &sig(f["signatureHex"].as_str().unwrap()),
                m.as_str().unwrap()
            ), "forgery accepted: {}", f["name"]);
        }
    }
}

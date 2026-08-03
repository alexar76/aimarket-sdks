//! The channel-open payer proof, bound to the shared protocol vector.
//!
//! The hub rebuilds this message independently and compares recovered signers, so the
//! vector is the contract: these tests assert against its bytes rather than against our own
//! implementation agreeing with itself.

use std::fs;
use std::path::PathBuf;

use aimarket_agent::payer_proof::{
    canonical_proof_amount_cents, channel_open_proof_message, recover_channel_open_payer,
    sign_channel_open_proof, PayerProofParams,
};
use serde_json::Value;

fn vector() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../aimarket-protocol/test-vectors/payer-proof.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing {} ({e}) — the cross-SDK contract cannot be checked",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("vector is not valid JSON")
}

fn params(case: &Value) -> PayerProofParams {
    PayerProofParams {
        chain: case["chain"].as_str().unwrap().to_string(),
        tx_hash: case["tx_hash"].as_str().unwrap().to_string(),
        payer: case["payer"].as_str().unwrap().to_string(),
        amount_usd: case["amount_usd"].as_f64().unwrap(),
    }
}

#[test]
fn reproduces_every_canonical_message() {
    let v = vector();
    let cases = v["cases"].as_array().expect("cases");
    assert!(cases.len() > 4, "the vector should carry several cases");
    for case in cases {
        let expected = case["message"].as_str().unwrap();
        assert_eq!(
            channel_open_proof_message(&params(case)),
            expected,
            "message mismatch for case {}",
            case["name"]
        );
    }
}

#[test]
fn converts_amounts_the_way_the_hub_does() {
    let v = vector();
    for case in v["cases"].as_array().unwrap() {
        assert_eq!(
            canonical_proof_amount_cents(case["amount_usd"].as_f64().unwrap()),
            case["amount_cents"].as_i64().unwrap(),
            "cents mismatch for case {}",
            case["name"]
        );
    }
}

#[test]
fn rounds_a_half_cent_to_even_not_away_from_zero() {
    // (0.125_f64 * 100.0).round() is 13; the hub's Python round() gives 12, so the naive
    // rule produces a valid-looking signature the hub refuses.
    assert_eq!(canonical_proof_amount_cents(0.125), 12);
    assert_eq!(canonical_proof_amount_cents(0.135), 14);
    assert_eq!((0.125_f64 * 100.0).round() as i64, 13, "the trap, spelled out");
}

#[test]
fn refuses_an_unusable_amount_deterministically() {
    assert_eq!(canonical_proof_amount_cents(f64::NAN), -1);
    assert_eq!(canonical_proof_amount_cents(f64::INFINITY), -1);
}

#[test]
fn signs_exactly_what_the_vector_signed() {
    let v = vector();
    let key = v["test_key"].as_str().unwrap();
    let signed: Vec<&Value> = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c.get("signature").is_some())
        .collect();
    assert!(!signed.is_empty(), "the vector should carry signed cases");
    for case in signed {
        let signature = sign_channel_open_proof(&params(case), key).expect("signing");
        assert_eq!(
            signature,
            case["signature"].as_str().unwrap(),
            "signature must match the vector byte for byte (case {})",
            case["name"]
        );
    }
}

#[test]
fn the_signature_recovers_to_the_paying_wallet() {
    let v = vector();
    let key = v["test_key"].as_str().unwrap();
    let expected = v["test_address"].as_str().unwrap().to_ascii_lowercase();
    let case = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c.get("signature").is_some())
        .unwrap();
    let p = params(case);
    let signature = sign_channel_open_proof(&p, key).unwrap();
    assert_eq!(
        recover_channel_open_payer(&p, &signature).unwrap(),
        expected
    );
}

#[test]
fn a_different_wallet_does_not_recover_as_the_payer() {
    let v = vector();
    let case = v["cases"].as_array().unwrap()[0].clone();
    let p = params(&case);
    let other = format!("0x{}", "77".repeat(32));
    let signature = sign_channel_open_proof(&p, &other).unwrap();
    assert_ne!(
        recover_channel_open_payer(&p, &signature).unwrap(),
        v["test_address"].as_str().unwrap().to_ascii_lowercase()
    );
}

#[test]
fn malformed_signatures_are_errors_not_panics() {
    let v = vector();
    let p = params(&v["cases"].as_array().unwrap()[0]);
    for bad in ["", "0x", "0xzz", "0x1234", &"0x".to_string()] {
        assert!(recover_channel_open_payer(&p, bad).is_err(), "accepted {bad:?}");
    }
}

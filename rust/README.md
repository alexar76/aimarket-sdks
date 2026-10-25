# aimarket-agent

AI Market Protocol v2 consumer SDK for **Rust** — Tauri desktop apps and native CLI tools.

Discover, pay for, and invoke AI capabilities from the decentralized marketplace, with production cryptography:

- **Ed25519** (`ed25519:<base64>`) for canonical hub / invoke signatures
- **EIP-712** (`keccak256` + secp256k1, `eip712:0x<r><s><v>`) for on-chain channel debits

Part of the [AIMarket SDKs](https://github.com/alexar76/aimarket-sdks) (Dart · TypeScript · Rust) — all three ship the same version and the same model shapes, enforced by an ecosystem parity guard in CI.

## Install

Requires Rust 1.87 or newer for the patched K256/ECDSA cryptography dependencies.

```toml
# Cargo.toml — published on crates.io:
[dependencies]
aimarket-agent = "0.3"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"

# Local dev against the monorepo (optional):
#   aimarket-agent = { path = "../aimarket-sdks/rust" }
```

## Quick start

`AimarketAgent` pays one of two ways, as the Python `aimarket-agent` does: with a
credit-account key when one is set, otherwise from a payment channel.

### Pay with a credit account

```rust
use aimarket_agent::{AimarketAgent, AimarketAgentConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A key from POST /ai-market/v2/accounts on a hub with open signup, or one its
    // operator issued. The account pays: no channel, no deposit.
    let agent = AimarketAgent::with_config(AimarketAgentConfig {
        api_key: Some(std::env::var("AIMARKET_API_KEY")?),
        ..AimarketAgentConfig::new("https://modelmarket.dev", &load_your_wallet_key())
    });

    // Discover capabilities for an intent — returns a ranked Vec<PlanStep>.
    let plan = agent
        .discover("ATS scoring rules for fintech roles", Some(1.0), Some(5), None)
        .await?;
    let cap = &plan[0].capability;

    let result = agent
        .invoke(
            &cap.capability_id,
            json!({ "target_role": "Senior PM", "industry": "fintech" }),
            "",                     // no channel: the account pays
            Some(&cap.product_id),  // the hub requires it
            Some(&cap.source_hub),  // and this, for a federated capability
            // Optional Pay-on-Verified opt-in, e.g.
            // Some(json!({ "requested": true, "intent": "score the resume" })).
            None,
        )
        .await?;

    println!("Output: {:?} · cost ${}", result.result, result.price_usd);
    Ok(())
}
```

With `api_key` set, every invoke sends `X-API-Key` and no channel headers, and `run_once`
neither opens nor closes a channel. A 402 (`AimarketError::Payment`) carries the hub's reason
and the account's `balance` against the `needed` price.

### Pay from a channel

Without a key, invokes are paid from a channel. A production hub opens one only against an
on-chain deposit it can verify: transfer the deposit first, then pass its transaction hash
and the paying wallet's signature over the payer-proof challenge.

```rust
use aimarket_agent::payer_proof::{sign_channel_open_proof, PayerProofParams};
use aimarket_agent::ChannelDeposit;

let payer_signature = sign_channel_open_proof(
    &PayerProofParams {
        chain: "base".into(),
        tx_hash: deposit_tx_hash.clone(),
        payer: paying_wallet_address.clone(),
        amount_usd: 5.0,
    },
    &paying_wallet_key,
)?;
let agent = AimarketAgent::new("https://modelmarket.dev", &load_your_wallet_key());
let deposit = ChannelDeposit {
    tx_hash: deposit_tx_hash,
    payer_signature: Some(payer_signature),
    ..Default::default()
};
let channel = agent.open_channel(5.0, "USDC", "base", Some(&deposit)).await?;
let result = agent
    .invoke(&cap.capability_id, input, &channel.channel_id, Some(&cap.product_id), None, None)
    .await?;
agent.close_channel(&channel.channel_id).await?; // presents the channel secret and paying wallet
```

Without `tx_hash` a production hub answers `400 tx_hash is required for on-chain
verification`; without `payer_signature` it refuses with "missing or invalid payer proof" and
returns the text to sign. Only a dev/demo hub opens a channel with no deposit (`None`). A
deposit funds exactly one channel, so `open_channel` with the same `tx_hash` returns the
channel it opened.

## Links

- Ecosystem & live demos: <https://ecosystem.modelmarket.dev/>
- Protocol spec & schemas: <https://github.com/alexar76/aimarket-protocol>

## License

MIT

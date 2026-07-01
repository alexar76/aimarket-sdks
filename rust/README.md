# aimarket-agent

AI Market Protocol v2 consumer SDK for **Rust** — Tauri desktop apps and native CLI tools.

Discover, pay for, and invoke AI capabilities from the decentralized marketplace, with production cryptography:

- **Ed25519** (`ed25519:<base64>`) for canonical hub / invoke signatures
- **EIP-712** (`keccak256` + secp256k1, `eip712:0x<r><s><v>`) for on-chain channel debits

Part of the [AIMarket SDKs](https://github.com/alexar76/aimarket-sdks) (Dart · TypeScript · Rust) — all three ship the same version and the same model shapes, enforced by an ecosystem parity guard in CI.

## Install

```toml
# Cargo.toml — published on crates.io:
[dependencies]
aimarket-agent = "0.1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"

# Local dev against the monorepo (optional):
#   aimarket-agent = { path = "../aimarket-sdks/rust" }
```

## Quick start

```rust
use aimarket_agent::AimarketAgent;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let agent = AimarketAgent::new("https://hub.aicom.io", &load_your_wallet_key());

    // Discover capabilities for an intent — returns a ranked Vec<PlanStep>.
    let plan = agent
        .discover("ATS scoring rules for fintech roles", Some(1.0), Some(5), None)
        .await?;

    // Open a $5 channel (good for ~50 calls), then invoke the best match.
    let channel = agent.open_channel(5.0, "USDT", "base").await?;
    let result = agent
        .invoke(
            &plan[0].capability.capability_id,
            json!({ "target_role": "Senior PM", "industry": "fintech" }),
            &channel.channel_id,
            None,
            None,
        )
        .await?;

    println!("Output: {:?} · cost ${} · TEE {}", result.output, result.price_usd, result.tee_verified);
    agent.close_channel(&channel.channel_id).await?;
    Ok(())
}
```

## Links

- Ecosystem & live demos: <https://modeldev.modelmarket.dev>
- Protocol spec & schemas: <https://github.com/alexar76/aimarket-protocol>

## License

MIT

<!-- aicom-mirror-notice -->
> **📖 Read-only mirror.** `aimarket-sdks` is published from the canonical AI-Factory monorepo.
> **Pull requests are not accepted** — any commit pushed here is overwritten by
> `scripts/mirror_satellites.sh` on the next sync.
> 🐞 Found a bug or have a request? Please **[open an issue](https://github.com/alexar76/aimarket-sdks/issues)**.

# aimarket-sdks

<!-- aicom-readme-badges -->
<p align="center">
  <a href="https://github.com/alexar76/aimarket-sdks/actions/workflows/ci.yml"><img src="https://raw.githubusercontent.com/alexar76/aimarket-sdks/main/docs/badges/ci.svg" alt="CI" /></a>
  <a href="https://github.com/alexar76/aimarket-sdks/releases"><img src="https://raw.githubusercontent.com/alexar76/aimarket-sdks/main/docs/badges/release.svg" alt="Release" /></a>
  <a href="https://raw.githubusercontent.com/alexar76/aimarket-sdks/main/docs/badges/coverage.svg"><img src="https://raw.githubusercontent.com/alexar76/aimarket-sdks/main/docs/badges/coverage.svg" alt="Test coverage" /></a>
  <a href="https://github.com/alexar76/aimarket-sdks/blob/main/LICENSE"><img src="https://raw.githubusercontent.com/alexar76/aimarket-sdks/main/docs/badges/license.svg" alt="License: Apache-2.0" /></a>
</p>
<!-- /aicom-readme-badges -->











> **Ecosystem:** [AICOM overview & live demos](https://modeldev.modelmarket.dev) · **Oracles:** [oracles.modelmarket.dev](https://oracles.modelmarket.dev) · [GitHub](https://github.com/alexar76/oracles) · **SDK package version:** `0.2.0` — live on [npm](https://www.npmjs.com/package/@aimarket/agent) · [crates.io](https://crates.io/crates/aimarket-agent) · [pub.dev](https://pub.dev/packages/aimarket_agent) · **Community:** [Discord · Pollux](https://discord.gg/aimarket) · [Telegram · Castor](https://t.me/just_for_agents) · **Version policy:** [`docs/sdk-version-policy.md`](../docs/sdk-version-policy.md) (Python SDK is separate at **2.1.x** on PyPI)

Language-native SDKs for the [AIMarket Protocol v2](https://github.com/alexar76/aimarket-protocol).  
Use these to embed marketplace economy into **desktop apps, mobile apps, and servers**.

## SDKs

| Language | Directory | Install | Target |
|---|---|---|---|
| **Dart** | `/dart` | [`dart pub add aimarket_agent`](https://pub.dev/packages/aimarket_agent) | Flutter desktop (macOS/Windows/Linux), Dart servers |
| **TypeScript** | `/typescript` | [`npm install @aimarket/agent`](https://www.npmjs.com/package/@aimarket/agent) | Electron, Node.js servers, web apps |
| **Rust** | `/rust` | [`aimarket-agent = "0.2.0"`](https://crates.io/crates/aimarket-agent) | Tauri, native CLI tools |

> **Python?** The consumer SDK for Python is the separate [`aimarket-agent`](https://pypi.org/project/aimarket-agent/) package — `pip install aimarket-agent`.

All three SDKs use **production cryptography**:

- **Ed25519** (`ed25519:<base64>`) for canonical hub / invoke signatures — matches `aimarket_hub.signing.Signer`
- **EIP-712** (`keccak256` + **secp256k1**, `eip712:0x<r><s><v>`) for `AIMarketEscrow.debitChannel` — matches `contracts/evm/src/AIMarketEscrow.sol`

Cross-language test vectors live in [`test-vectors/debit_authorization.json`](test-vectors/debit_authorization.json). Refresh with:

```bash
cd typescript && npm run build && node ../scripts/verify_test_vectors.mjs
```

## What this gives you

```
Your App                    AI Market Hub Network
┌──────────┐               ┌─────────────────┐
│ discover │──────────────▶│ /.well-known/   │
│  intent  │               │   ai-market.json│
│          │               └────────┬────────┘
│  plan    │◀───────────────────────┘
│          │               ┌─────────────────┐
│  open    │──────────────▶│ channel/open    │
│  channel │               │ (pre-funded)    │
│          │◀──────────────│                 │
│          │               └────────┬────────┘
│  invoke  │──────────────▶│ invoke          │
│  (TEE    │               │ (safety-gated)  │
│  verified)◀──────────────│                 │
│          │               └────────┬────────┘
│  settle  │──────────────▶│ channel/close   │
└──────────┘               └─────────────────┘
```

## Quick Start (Dart)

```yaml
# pubspec.yaml — published on pub.dev:
dependencies:
  aimarket_agent: ^0.2.0
# or run:  dart pub add aimarket_agent
#
# Local dev against the monorepo (optional):
#   aimarket_agent:
#     git: { url: https://github.com/alexar76/aimarket-sdks, path: dart }
```

```dart
import 'package:aimarket_agent/aimarket_agent.dart';

void main() async {
  final agent = AimarketAgent(
    hubUrl: 'https://modelmarket.dev',
    walletKey: loadYourWalletKey(),
  );

  // Discover career-related capabilities
  final capabilities = await agent.discover(
    intent: 'ATS scoring rules for fintech roles',
    budget: 1.00,
    limit: 5,
  );

  // Open a $5 channel (good for ~50 calls)
  final channel = await agent.openChannel(5.00);

  // Invoke the best match
  final result = await agent.invoke(
    capabilityId: capabilities.first.id,
    input: {'target_role': 'Senior PM', 'industry': 'fintech'},
    channelId: channel.id,
  );

  print('Score: ${result.output}');
  print('Cost: \$${result.priceUsd}');
  print('TEE verified: ${result.teeVerified}');

  await agent.closeChannel(channel.id);
}
```

## Quick Start (TypeScript)

```bash
# Published on npm:
npm install @aimarket/agent

# Local dev against the monorepo (optional):
#   git clone https://github.com/alexar76/aimarket-sdks
#   cd aimarket-sdks/typescript && npm install && npm run build
```

```ts
import { AimarketAgent } from '@aimarket/agent';

const agent = new AimarketAgent({
  hubUrl: 'https://modelmarket.dev',
  walletKey: loadYourWalletKey(),
});

// Discover capabilities for an intent — returns a ranked PlanStep[].
const plan = await agent.discover({
  intent: 'ATS scoring rules for fintech roles',
  budget: 1.0,
  limit: 5,
});

// Open a $5 channel (good for ~50 calls).
const channel = await agent.openChannel(5.0);

// Invoke the best match (TEE-verified by default).
const result = await agent.invoke({
  capabilityId: plan[0].capability.capability_id,
  input: { target_role: 'Senior PM', industry: 'fintech' },
  channelId: channel.channel_id,
});

console.log('Output:', result.output);
console.log(`Cost: $${result.price_usd}`);
console.log('TEE verified:', result.tee_verified);

await agent.closeChannel(channel.channel_id);
```

## Quick Start (Rust)

```toml
# Cargo.toml — published on crates.io:
[dependencies]
aimarket-agent = "0.2.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"

# Local dev against the monorepo (optional):
#   aimarket-agent = { path = "../aimarket-sdks/rust" }
```

```rust
use aimarket_agent::AimarketAgent;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let agent = AimarketAgent::new("https://modelmarket.dev", &load_your_wallet_key());

    // Discover capabilities for an intent — returns a ranked Vec<PlanStep>.
    let plan = agent
        .discover("ATS scoring rules for fintech roles", Some(1.0), Some(5), None)
        .await?;

    // Open a $5 channel (good for ~50 calls).
    let channel = agent.open_channel(5.0, "USDT", "base").await?;

    // Invoke the best match (TEE-verified by default).
    let result = agent
        .invoke(
            &plan[0].capability.capability_id,
            json!({ "target_role": "Senior PM", "industry": "fintech" }),
            &channel.channel_id,
            None,
            None,
        )
        .await?;

    println!("Output: {:?}", result.output);
    println!("Cost: ${}", result.price_usd);
    println!("TEE verified: {}", result.tee_verified);

    agent.close_channel(&channel.channel_id).await?;
    Ok(())
}
```

> All three Quick Starts drive the **same 5-phase cycle** (discover → open → invoke → settle) against the same hub. The three SDKs are held at identical versions and matching model shapes by an ecosystem parity guard that runs in CI on every change.

## Architecture

Each language SDK implements the same 5-phase consumer cycle:

1. **Discovery** — `GET /.well-known/ai-market.json` → `GET /ai-market/v2/search`
2. **Channel Open** — `POST /ai-market/v2/channel/open` with deposit
3. **Invoke** — `POST /ai-market/v2/invoke` with payment channel header
4. **Settle** — `POST /ai-market/v2/channel/close`
5. **Verify** — Local TEE attestation check (code hash + signature)

The protocol is universal JSON/HTTP. SDKs are thin wrappers — the real logic is in the hub.

## Demo

- **Live:** https://modelmarket.dev/
- **Docs:** https://github.com/alexar76/aimarket-sdks/blob/main/README.md

## Related repos

| Repo | Role |
|------|------|
| [aimarket-protocol](https://github.com/alexar76/aimarket-protocol) | Normative v2 spec |
| [aimarket-hub](https://github.com/alexar76/aimarket-hub) | Reference hub |
| [aimarket-agent](https://github.com/alexar76/aimarket-agent) | Python consumer SDK (2.x line) |
| [aimarket-widget](https://github.com/alexar76/aimarket-widget) | Embeddable UI |
| [dioscuri](https://github.com/alexar76/dioscuri) | Twin community agents — MNEMOSYNE Q&A |

## Community

The [DIOSCURI](https://github.com/alexar76/dioscuri) twins answer questions from synced GitHub docs.

| Channel | Twin | Best for |
|---------|------|----------|
| [Discord](https://discord.gg/aimarket) | Pollux | Help, ideas, show-and-tell |
| [Telegram](https://t.me/just_for_agents) | Castor | Releases, digests, quick news |

**Ecosystem map:** [Alien Monitor](https://magic-ai-factory.com/monitor/) · [AICOM](https://magic-ai-factory.com)

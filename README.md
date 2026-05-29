<!-- aicom-mirror-notice -->
> **Mirror — read-only.**
> The canonical source for `aimarket-sdks` lives in the AI-Factory monorepo.
> Open issues and PRs at `Superowner/aicom`; commits pushed here are
> overwritten by `scripts/mirror_satellites.sh` on the next sync run.
> See `docs/repository-canonical-policy.md` for the policy.

# aimarket-sdks

[![CI](https://github.com/alexar76/aimarket-sdks/actions/workflows/ci.yml/badge.svg)](https://github.com/alexar76/aimarket-sdks/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/alexar76/aimarket-sdks?include_prereleases&label=release)](https://github.com/alexar76/aimarket-sdks/releases)

> **Ecosystem:** [AICOM overview & live demos](https://alexar76.github.io/aicom/) · **SDK package version:** `0.1.0` (Dart / TS / Rust)

Language-native SDKs for the [AI Market Protocol v2](https://github.com/alexar76/aimarket-protocol).  
Use these to embed marketplace economy into **desktop apps, mobile apps, and servers**.

## SDKs

| Language | Directory | Status | Target |
|---|---|---|---|
| **Dart** | `/dart` | Alpha | Flutter desktop (macOS/Windows/Linux), Dart servers |
| **TypeScript** | `/typescript` | Stub | Electron, Node.js servers, web apps |
| **Rust** | `/rust` | Stub | Tauri, native CLI tools |

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
# pubspec.yaml
dependencies:
  aimarket_agent:
    git:
      url: https://github.com/alexar76/aimarket-sdks
      path: dart
```

```dart
import 'package:aimarket_agent/aimarket_agent.dart';

void main() async {
  final agent = AimarketAgent(
    hubUrl: 'https://hub.aicom.io',
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

## Architecture

Each language SDK implements the same 5-phase consumer cycle:

1. **Discovery** — `GET /.well-known/ai-market.json` → `GET /ai-market/v2/search`
2. **Channel Open** — `POST /ai-market/v2/channel/open` with deposit
3. **Invoke** — `POST /ai-market/v2/invoke` with payment channel header
4. **Settle** — `POST /ai-market/v2/channel/close`
5. **Verify** — Local TEE attestation check (code hash + signature)

The protocol is universal JSON/HTTP. SDKs are thin wrappers — the real logic is in the hub.

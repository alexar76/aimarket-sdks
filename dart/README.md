# aimarket_agent

AI Market Protocol v2 consumer SDK for **Dart / Flutter** — desktop (macOS/Windows/Linux) and Dart servers.

Discover, pay for, and invoke AI capabilities from the decentralized marketplace, with production cryptography:

- **Ed25519** (`ed25519:<base64>`) for canonical hub / invoke signatures
- **EIP-712** (`keccak256` + secp256k1, `eip712:0x<r><s><v>`) for on-chain channel debits

Part of the [AIMarket SDKs](https://github.com/alexar76/aimarket-sdks) (Dart · TypeScript · Rust) — all three ship the same version and the same model shapes, enforced by an ecosystem parity guard in CI.

## Install

```yaml
# pubspec.yaml — published on pub.dev:
dependencies:
  aimarket_agent: ^0.1.0
# or run:  dart pub add aimarket_agent
#
# Local dev against the monorepo (optional):
#   aimarket_agent:
#     git: { url: https://github.com/alexar76/aimarket-sdks, path: dart }
```

## Quick start

```dart
import 'package:aimarket_agent/aimarket_agent.dart';

void main() async {
  final agent = AimarketAgent(
    hubUrl: 'https://hub.aicom.io',
    walletKey: loadYourWalletKey(),
  );

  // Discover capabilities for an intent.
  final capabilities = await agent.discover(
    intent: 'ATS scoring rules for fintech roles',
    budget: 1.00,
    limit: 5,
  );

  // Open a $5 channel (good for ~50 calls), then invoke the best match.
  final channel = await agent.openChannel(5.00);
  final result = await agent.invoke(
    capabilityId: capabilities.first.id,
    input: {'target_role': 'Senior PM', 'industry': 'fintech'},
    channelId: channel.id,
  );

  print('Output: ${result.output} · cost \$${result.priceUsd} · TEE ${result.teeVerified}');
  await agent.closeChannel(channel.id);
}
```

## Links

- Ecosystem & live demos: <https://modeldev.modelmarket.dev>
- Protocol spec & schemas: <https://github.com/alexar76/aimarket-protocol>

## License

MIT

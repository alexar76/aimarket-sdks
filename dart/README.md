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
  aimarket_agent: ^0.3.0
# or run:  dart pub add aimarket_agent
#
# Local dev against the monorepo (optional):
#   aimarket_agent:
#     git: { url: https://github.com/alexar76/aimarket-sdks, path: dart }
```

## Quick start

`AimarketAgent` pays one of two ways, as the Python `aimarket-agent` does: with a
credit-account key when one is set, otherwise from a payment channel.

### Pay with a credit account

```dart
import 'dart:io';

import 'package:aimarket_agent/aimarket_agent.dart';

void main() async {
  // A key from POST /ai-market/v2/accounts on a hub with open signup, or one its
  // operator issued. The account pays: no channel, no deposit.
  final agent = AimarketAgent(
    hubUrl: 'https://modelmarket.dev',
    walletKey: loadYourWalletKey(),
    apiKey: Platform.environment['AIMARKET_API_KEY'],
  );

  // Discover capabilities for an intent.
  final plan = await agent.discover(
    intent: 'ATS scoring rules for fintech roles',
    budget: 1.00,
    limit: 5,
  );
  final cap = plan.first.capability;

  final result = await agent.invoke(
    capabilityId: cap.id,
    productId: cap.productId, // the hub requires it
    sourceHub: cap.sourceHub, // and this, for a federated capability
    input: {'target_role': 'Senior PM', 'industry': 'fintech'},
  );

  print('Output: ${result.result} · cost \$${result.priceUsd}');
  agent.dispose();
}
```

With `apiKey` set, every invoke sends `X-API-Key` and no channel headers, and `runOnce`
neither opens nor closes a channel. A 402 (`AimarketPaymentException`) carries the hub's
reason and the account's `balance` against the `needed` price.

### Pay from a channel

Without a key, invokes are paid from a channel. A production hub opens one only against an
on-chain deposit it can verify: transfer the deposit first, then pass its transaction hash
and the paying wallet's signature over the payer-proof challenge.

```dart
final payerSignature = signChannelOpenProof(
  privateKeyHex: payingWalletKey,
  chain: 'base',
  txHash: depositTxHash,
  payer: payingWalletAddress,
  amountUsd: 5.00,
);
final agent = AimarketAgent(hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey());
final channel = await agent.openChannel(
  5.00,
  token: 'USDC',
  txHash: depositTxHash,
  payerSignature: payerSignature,
);
final result = await agent.invoke(
  capabilityId: cap.id,
  productId: cap.productId,
  input: {'target_role': 'Senior PM', 'industry': 'fintech'},
  channelId: channel.id,
);
await agent.closeChannel(channel.id); // presents the channel secret and paying wallet
```

Without `txHash` a production hub answers `400 tx_hash is required for on-chain
verification`; without `payerSignature` it refuses with "missing or invalid payer proof" and
returns the text to sign. Only a dev/demo hub opens a channel with no deposit. A deposit funds
exactly one channel, so `openChannel` with the same `txHash` returns the channel it opened.

## Links

- Ecosystem & live demos: <https://ecosystem.modelmarket.dev/>
- Protocol spec & schemas: <https://github.com/alexar76/aimarket-protocol>

## License

MIT

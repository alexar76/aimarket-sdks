# @aimarket/agent

AI Market Protocol v2 consumer SDK for **TypeScript** — Electron, Node.js servers, and web apps.

Discover, pay for, and invoke AI capabilities from the decentralized marketplace, with production cryptography:

- **Ed25519** (`ed25519:<base64>`) for canonical hub / invoke signatures
- **EIP-712** (`keccak256` + secp256k1, `eip712:0x<r><s><v>`) for on-chain channel debits

Part of the [AIMarket SDKs](https://github.com/alexar76/aimarket-sdks) (Dart · TypeScript · Rust) — all three ship the same version and the same model shapes, enforced by an ecosystem parity guard in CI.

## Install

```bash
# Published on npm (v0.1.1+ — CommonJS require() on Node 20/22):
npm install @aimarket/agent

# ESM / TypeScript:
import { AimarketAgent } from '@aimarket/agent';

# CommonJS:
const { AimarketAgent } = require('@aimarket/agent');
```

## Quick start

`market` is exported by the published **0.2.2** package (`npm install @aimarket/agent`).
To run the example below from this **0.3.0** source tree instead, build the checkout first:

```bash
git clone https://github.com/alexar76/aimarket-sdks.git
cd aimarket-sdks/typescript
npm ci
npm run build
```

Or [download a runnable consumer project](https://play.modelmarket.dev/start?role=consumer&stack=typescript).

The shortest safe path needs no wallet or private key. It searches the live Independent
federation, applies budget/trust/latency limits again in the client, invokes exactly once,
and verifies the returned Ed25519 + ML-DSA-65 receipt against the live Hub identity:

```ts
import { market } from './dist/index.js';

const run = await market.run({
  intent: 'audit this agent trace for unsafe tool use',
  input: { trace: 'An agent reads a public weather report and returns its source.' },
});

console.log(run.invocation.result);
console.log(run.trusted); // true only after pinned Hub receipt verification
```

The zero-config call uses the Hub's bounded free trial. For repeated calls, pass either
`payment: { kind: 'credits', apiKey }` or
`payment: { kind: 'channel', channelId, channelSecret }`. Credentials are header-only,
never placed in URLs or request bodies, and a possibly charged invoke is never retried.

### Pay with a credit account

`AimarketAgent` pays one of two ways, as the Python `aimarket-agent` does: with a
credit-account key when one is set, otherwise from a payment channel.

```ts
import { AimarketAgent } from '@aimarket/agent';

// A key from POST /ai-market/v2/accounts on a hub with open signup, or one its
// operator issued. The account pays: no channel, no deposit.
const agent = new AimarketAgent({
  hubUrl: 'https://modelmarket.dev',
  walletKey: loadYourWalletKey(),
  apiKey: process.env.AIMARKET_API_KEY,
});

// Discover capabilities for an intent — returns a ranked PlanStep[].
const plan = await agent.discover({ intent: 'ATS scoring rules for fintech roles', budget: 1.0 });
const cap = plan[0].capability;

const result = await agent.invoke({
  capabilityId: cap.capability_id,
  productId: cap.product_id, // the hub requires it
  sourceHub: cap.source_hub, // and this, for a federated capability
  input: { target_role: 'Senior PM', industry: 'fintech' },
});
console.log('Output:', result.result, '· cost $', result.price_usd);
```

With `apiKey` set, every invoke sends `X-API-Key` and no channel headers, and `runOnce`
neither opens nor closes a channel. A 402 (`AimarketPaymentException`) carries the hub's
reason and the account's `balance` against the `needed` price.

### Pay from a channel

Without a key, invokes are paid from a channel. A production hub opens one only against an
on-chain deposit it can verify: transfer the deposit first, then pass its transaction hash
and the paying wallet's signature over the payer-proof challenge.

```ts
import { AimarketAgent, signChannelOpenProof } from '@aimarket/agent';
import { privateKeyToAccount } from 'viem/accounts';

const payer = privateKeyToAccount(payingWalletKey);
const payerSignature = await signChannelOpenProof(
  { chain: 'base', txHash: depositTxHash, payer: payer.address, amountUsd: 5 },
  (message) => payer.signMessage({ message }),
);

const agent = new AimarketAgent({ hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey() });
const channel = await agent.openChannel(5.0, 'USDC', 'base', { txHash: depositTxHash, payerSignature });
const result = await agent.invoke({
  capabilityId: cap.capability_id,
  productId: cap.product_id,
  input: { target_role: 'Senior PM', industry: 'fintech' },
  channelId: channel.channel_id,
});
await agent.closeChannel(channel.channel_id); // presents the channel secret and paying wallet
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

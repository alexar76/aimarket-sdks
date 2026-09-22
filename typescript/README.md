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

The shortest safe path needs no wallet or private key. It searches the live Independent
federation, applies budget/trust/latency limits again in the client, invokes exactly once,
and verifies the returned Ed25519 + ML-DSA-65 receipt against the live Hub identity:

```ts
import { market } from '@aimarket/agent';

const run = await market.run({
  intent: 'audit this agent trace for unsafe tool use',
  input: { trace },
});

console.log(run.invocation.result);
console.log(run.trusted); // true only after pinned Hub receipt verification
```

The zero-config call uses the Hub's bounded free trial. For repeated calls, pass either
`payment: { kind: 'credits', apiKey }` or
`payment: { kind: 'channel', channelId, channelSecret }`. Credentials are header-only,
never placed in URLs or request bodies, and a possibly charged invoke is never retried.

### Full channel lifecycle

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

// Open a $5 channel (good for ~50 calls), then invoke the best match.
const channel = await agent.openChannel(5.0);
const result = await agent.invoke({
  capabilityId: plan[0].capability.capability_id,
  input: { target_role: 'Senior PM', industry: 'fintech' },
  channelId: channel.channel_id,
});

console.log('Output:', result.result, '· cost $', result.price_usd, '· TEE', result.tee_verified);
await agent.closeChannel(channel.channel_id);
```

## Links

- Ecosystem & live demos: <https://modeldev.modelmarket.dev>
- Protocol spec & schemas: <https://github.com/alexar76/aimarket-protocol>

## License

MIT

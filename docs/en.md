# aimarket SDKs — canonical umbrella guide (Dart / TypeScript / Rust)

Language-native consumer SDKs for the **AIMarket Protocol v2** — a JSON/HTTP marketplace where your app can **discover**, **pay for**, and **invoke** AI capabilities served by independent hubs. The three SDKs documented here (Dart, TypeScript, Rust) ship the **same 5-phase cycle**, the **same model shapes**, and the **same Ed25519 signing**, held in lock-step by an ecosystem parity guard in CI. A fourth, architecturally different SDK exists for Python — see the [Python guide](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md).

> **Reference hub:** [modelmarket.dev](https://modelmarket.dev) · **Factory:** [magic-ai-factory.com](https://magic-ai-factory.com) · **Oracles:** [oracles.modelmarket.dev](https://oracles.modelmarket.dev) · **Version policy:** [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md)

This is **the flagship SDK guide**. It is technical, table-heavy, and example-first. Every signature, header, port, and address below is taken from the live source in `aimarket-sdks/`.

---

## 1. What this is

The AIMarket Protocol v2 is a universal JSON/HTTP marketplace for AI capabilities. Sellers publish capabilities behind hubs; consumers discover them by **intent**, open a **pre-funded payment channel**, **invoke** the capability (paying per call from the channel), and **settle** to refund whatever they did not spend. Capabilities can run inside a **TEE** (trusted execution enclave), and the protocol lets you verify that cryptographically before and after a call. Hubs **federate**, so one hub can route an invocation to a capability hosted on another.

The four SDKs:

| SDK | Package | Target runtimes |
|-----|---------|-----------------|
| **Dart** | `aimarket_agent` | Flutter desktop (macOS/Windows/Linux), Dart servers |
| **TypeScript** | `@aimarket/agent` | Electron, Node.js servers, web apps |
| **Rust** | `aimarket-agent` (crate) | Tauri, native CLI tools |
| **Python** | `aimarket-agent` (PyPI) | LangChain / server agents / CLI — *separate, stateless, no wallet* → [Python guide](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) |

Live URLs you will reference:

- **Reference hub (consumer entry point):** `https://modelmarket.dev` — AIMarket Protocol v2. Local dev: `http://localhost:9083`.
- **Factory app / site:** `https://magic-ai-factory.com` — where capabilities are built and published.
- **Oracles portal:** `https://oracles.modelmarket.dev` — seventeen live verifiable-math oracles, all reachable as capabilities.

Use `https://modelmarket.dev` as the canonical `hubUrl` in your own code unless you are pointing at a self-hosted or federated hub.

---

## 2. Choose your SDK + install

| Runtime | Package | Install | Version line |
|---------|---------|---------|--------------|
| **Flutter / Dart** | `aimarket_agent` | `dart pub add aimarket_agent` | **0.1.x** |
| **Electron / Node.js** | `@aimarket/agent` | `npm install @aimarket/agent` | **0.1.x** |
| **Tauri / Rust** | `aimarket-agent` (crate) | `cargo add aimarket-agent` | **0.1.x** |
| **Server / LangChain / CLI** | `aimarket-agent` (PyPI) | `pip install aimarket-agent` → [Python guide](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) | **2.1.x** |

> **Two version lines, on purpose.** Dart / TypeScript / Rust are the lock-stepped cross-platform family at **0.1.x**; the Python agent is an older, separate package at **2.1.x** on PyPI. Both target AIMarket Protocol v2. See [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md) — do not "fix" the mismatch.

### Registry install (recommended)

```yaml
# Dart — pubspec.yaml (published on pub.dev)
dependencies:
  aimarket_agent: ^0.1.0
# or:  dart pub add aimarket_agent
```

```bash
# TypeScript — published on npm
npm install @aimarket/agent
```

```toml
# Rust — Cargo.toml (published on crates.io)
[dependencies]
aimarket-agent = "0.1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
# or:  cargo add aimarket-agent
```

### Monorepo / git install (local dev)

```yaml
# Dart — path or git dependency
dependencies:
  aimarket_agent:
    path: ../aimarket-sdks/dart
  # or:
  # aimarket_agent:
  #   git: { url: https://github.com/alexar76/aimarket-sdks, path: dart }
```

```bash
# TypeScript — build from the monorepo
git clone https://github.com/alexar76/aimarket-sdks
cd aimarket-sdks/typescript && npm install && npm run build
```

```toml
# Rust — path dependency on the monorepo crate
aimarket-agent = { path = "../aimarket-sdks/rust" }
```

---

## 3. Authentication — the Ed25519 truth

**The wallet key these SDKs sign with is an Ed25519 seed, not an Ethereum/secp256k1 private key.** This is the single most important thing to get right.

- `walletKey` is a **64-character hex string = a 32-byte Ed25519 seed**. The signer turns it into an Ed25519 keypair.
- If the string is **not** 64-char hex, it is SHA-256'd into a 32-byte seed (a dev fallback — never use this in production).
- Every **invoke** request is signed over the canonical string `channel:<id>|capability:<id>|affiliate:<affil>`, producing a signature of the form `ed25519:<base64>`, sent as the header `X-Market-Signature`.

The Dart signer (`dart/lib/src/signer.dart`) makes all three points explicit:

```dart
// signedHeaders() — what every invoke sends:
final canonical = 'channel:$channelId|capability:$capabilityId|affiliate:$affiliate';
return {
  'X-Payment-Channel': channelId,
  'X-AIMarket-Affiliate': affiliate,
  'X-Market-Signature': signCanonical(canonical),  // 'ed25519:<base64>'
};
```

```dart
// _parseSeedBytes() — 64-char hex => raw 32-byte seed; otherwise SHA-256(input).
final normalized = hexOrDev.startsWith('0x') ? hexOrDev.substring(2) : hexOrDev;
if (normalized.length == 64 && RegExp(r'^[0-9a-fA-F]+$').hasMatch(normalized)) {
  // use the hex bytes directly as the Ed25519 seed
}
// else: sha256(input)  ← dev fallback only
```

### Loading the key securely (never hardcode)

Pull the seed from the platform keychain / secure storage at runtime. Do not commit it, do not bake it into your binary.

```dart
// Dart / Flutter — e.g. flutter_secure_storage
final walletKey = await secureStorage.read(key: 'aimarket_wallet_seed');
final agent = AimarketAgent(hubUrl: 'https://modelmarket.dev', walletKey: walletKey!);
```

```ts
// TypeScript / Electron — e.g. safeStorage / keytar
const walletKey = await loadWalletSeedFromKeychain();
const agent = new AimarketAgent({ hubUrl: 'https://modelmarket.dev', walletKey });
```

```rust
// Rust / Tauri — e.g. the OS keyring
let wallet_key = load_wallet_seed_from_keyring()?;
let agent = AimarketAgent::new("https://modelmarket.dev", &wallet_key);
```

### The optional secondary key (EIP-712 / secp256k1)

There is a **second, optional** key used only for **on-chain channel debits** against `AIMarketEscrow.debitChannel` — an EIP-712 signature over a secp256k1 key (`eip712:0x<r><s><v>`). In Dart it is `MarketSigner(ethereumPrivateKeyHex: ...)`, consumed by `signDebitAuthorization(...)`. **Most consumers never need it** — the channel/invoke/settle flow runs entirely on the Ed25519 seed. Only reach for it if you are settling channels directly on-chain.

> Do **not** call `walletKey` an "EVM/Ethereum private key." It is an Ed25519 seed. The secp256k1/EIP-712 key is a distinct, optional input.

---

## 4. The 5-phase value lifecycle

Every SDK implements the same five phases against the same hub endpoints.

| # | Phase | Hub v2 endpoint | Method (Dart shown) | What it does |
|---|-------|-----------------|---------------------|--------------|
| 1 | **Discovery** | `GET /.well-known/ai-market.json` → `GET /ai-market/v2/search` | `discover(...)` | find capabilities matching an intent + budget |
| 2 | **Channel** | `POST /ai-market/v2/channel/open` | `openChannel(depositUsd, token, chain)` | open a pre-funded payment channel |
| 3 | **Invoke** | `POST /ai-market/v2/invoke` (headers `X-Payment-Channel`, `X-Market-Signature`) | `invoke(...)` | call a capability, pay from the channel |
| 4 | **Settle** | `POST /ai-market/v2/channel/close` | `closeChannel(channelId)` | close the channel, refund the unspent balance |
| 5 | **Verify** | *(local)* | `verifyTeeAttestation(...)` / `verifyTeeReceipt(...)` | TEE attestation before send + receipt after |

`runOnce(...)` is the convenience method that runs all five and returns a `BillOfMaterials`.

### Method surface (per language)

| Phase | Dart | TypeScript | Rust |
|-------|------|------------|------|
| Discover | `discover({intent, budget?, limit=5, category?})` | `discover({ intent, budget?, limit?, category? })` | `discover(intent, budget, limit, category)` |
| Channel | `openChannel(depositUsd, {token='USDT', chain='base'})` | `openChannel(depositUsd, token='USDT', chain='base')` | `open_channel(deposit_usd, token, chain)` |
| Invoke | `invoke({capabilityId, input, channelId, sourceHub?, productId?, verifyTee=true, attestation?})` | `invoke({ capabilityId, input, channelId, productId?, sourceHub?, verifyTee?, attestation? })` | `invoke(capability_id, input, channel_id, product_id, source_hub)` |
| Settle | `closeChannel(channelId)` | `closeChannel(channelId)` | `close_channel(channel_id)` |
| Full cycle | `runOnce({intent, input, depositUsd=5.00, category?})` | `runOnce({ intent, input, depositUsd?, category? })` | `run_once(intent, input, deposit_usd, category)` |

> **Result field naming differs by language idiom.** Dart uses camelCase (`result.priceUsd`, `result.teeVerified`, `result.teeReceipt`); TypeScript and Rust use snake_case (`result.price_usd`, `result.tee_verified`, `result.tee_receipt`). The wire JSON is snake_case throughout. Plan steps: Dart exposes `step.capability.id`; TS/Rust expose `step.capability.capability_id`.

### One-shot: `runOnce(...)`

```dart
// Dart
final agent = AimarketAgent(hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey());
final bom = await agent.runOnce(
  intent: 'pdf summarization',
  input: {'text': '...'},
  depositUsd: 5.00,
  category: 'productivity',
);
print('task=${bom.task} protocol=${bom.protocolVersion} spent=\$${bom.totalSpentUsd}');
agent.dispose();
```

```ts
// TypeScript
const agent = new AimarketAgent({ hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey() });
const bom = await agent.runOnce({
  intent: 'pdf summarization',
  input: { text: '...' },
  depositUsd: 5.0,
  category: 'productivity',
});
console.log(`task=${bom.task} protocol=${bom.protocol_version} spent=$${bom.total_spent_usd}`);
agent.dispose();
```

```rust
// Rust
use aimarket_agent::AimarketAgent;
use serde_json::json;

let agent = AimarketAgent::new("https://modelmarket.dev", &load_your_wallet_key());
let bom = agent
    .run_once("pdf summarization", json!({ "text": "..." }), Some(5.00), Some("productivity"))
    .await?;
println!("task={} protocol={} spent=${}", bom.task, bom.protocol_version, bom.total_spent_usd);
agent.dispose();
```

### Manual cycle: Discovery → Channel → Invoke → Settle

```dart
// Dart
final agent = AimarketAgent(hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey());

final plan    = await agent.discover(intent: 'pdf summarization', budget: 10.00, category: 'productivity');
final channel = await agent.openChannel(5.00);                       // token: 'USDT', chain: 'base'
final result  = await agent.invoke(
  capabilityId: plan.first.capability.id,
  input: {'text': '...'},
  channelId: channel.id,
  verifyTee: true,
);
print('ok=${result.success} cost=\$${result.priceUsd} tee=${result.teeVerified}');

final settlement = await agent.closeChannel(channel.id);
print('spent=\$${settlement.totalSpentUsd} refund=\$${settlement.refundUsd}');
agent.dispose();
```

```ts
// TypeScript
const agent = new AimarketAgent({ hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey() });

const plan    = await agent.discover({ intent: 'pdf summarization', budget: 10.0, category: 'productivity' });
const channel = await agent.openChannel(5.0);                        // token 'USDT', chain 'base'
const result  = await agent.invoke({
  capabilityId: plan[0].capability.capability_id,
  input: { text: '...' },
  channelId: channel.channel_id,
  verifyTee: true,
});
console.log(`ok=${result.success} cost=$${result.price_usd} tee=${result.tee_verified}`);

const settlement = await agent.closeChannel(channel.channel_id);
console.log(`spent=$${settlement.total_spent_usd} refund=$${settlement.refund_usd}`);
agent.dispose();
```

```rust
// Rust
use aimarket_agent::AimarketAgent;
use serde_json::json;

let agent = AimarketAgent::new("https://modelmarket.dev", &load_your_wallet_key());

let plan    = agent.discover("pdf summarization", Some(10.0), Some(5), Some("productivity")).await?;
let channel = agent.open_channel(5.0, "USDT", "base").await?;
let result  = agent
    .invoke(&plan[0].capability.capability_id, json!({ "text": "..." }), &channel.channel_id, None, None)
    .await?;
println!("ok={} cost=${} tee={}", result.success, result.price_usd, result.tee_verified);

let settlement = agent.close_channel(&channel.channel_id).await?;
println!("spent=${} refund=${}", settlement.total_spent_usd, settlement.refund_usd);
agent.dispose();
```

> **Defaults to know:** `openChannel` defaults to `token='USDT'`, `chain='base'`. `runOnce` defaults `depositUsd=5.00`. Per-request `timeout` is 30s and `maxRetries` is 3 (only network/timeout errors retry, with 1s/2s/4s backoff; 402/403 and protocol errors propagate immediately). The default affiliate is per-language: `aimarket-sdk-dart`, `aimarket-sdk-ts`, `aimarket-sdk-rust`.

---

## 5. TEE verification

If a capability runs in a trusted enclave, you can prove it — **before** you send data, and **after** you receive output. The pattern is: register the expected code hash, verify the attestation, abort if it fails, then verify the receipt.

```dart
// Dart — register a known-good code hash, then verify the attestation BEFORE sending
agent.trustCodeHash('cap-id', 'sha256:a1b2c3d4...');

if (!agent.verifyTeeAttestation(attestation, 'cap-id')) {
  // The capability is NOT running the trusted code in a real enclave — abort.
  return;
}

final result = await agent.invoke(
  capabilityId: 'cap-id',
  input: input,
  channelId: channel.id,
  verifyTee: false,   // already verified the attestation above
);

// Verify the receipt AFTER: proves the output came out of the attested enclave.
if (result.teeReceipt != null &&
    !agent.verifyTeeReceipt(result.teeReceipt!, json.encode(input), json.encode(result.output))) {
  // The output may NOT have been produced in the attested enclave — treat as untrusted.
}
```

```ts
// TypeScript
agent.trustCodeHash('cap-id', 'sha256:a1b2c3d4...');

if (!agent.verifyTeeAttestation(attestation, 'cap-id')) return; // abort — not a verified enclave

const result = await agent.invoke({
  capabilityId: 'cap-id',
  input,
  channelId: channel.channel_id,
  verifyTee: false,
});

if (result.tee_receipt &&
    !agent.verifyTeeReceipt(result.tee_receipt, JSON.stringify(input), JSON.stringify(result.output))) {
  // output not verifiably produced in the attested enclave
}
```

```rust
// Rust — note: the Rust agent exposes verify_tee_attestation + trust_code_hash.
agent.trust_code_hash("cap-id", "sha256:a1b2c3d4...");

if !agent.verify_tee_attestation(&attestation, "cap-id") {
    // abort — not a verified enclave
    return Ok(());
}
let result = agent.invoke("cap-id", input, &channel.channel_id, None, None).await?;
```

Two complementary ways to verify, plus a built-in pre-flight:

- **`verifyTeeAttestation(attestation, capabilityId)`** — before send. Checks the enclave's code hash and signature against your trusted set. Returns `false` if the enclave can't be trusted.
- **`verifyTeeReceipt(receipt, sentInput, receivedOutput)`** *(Dart / TypeScript)* — after receive. Proves the output was produced in the attested enclave from the exact input you sent.
- **Built-in pre-check:** when you pass an `attestation` to `invoke(..., verifyTee: true)`, the SDK verifies it **before transmitting any input** and **fails closed** — a bad attestation aborts the call (Dart/TS raise the safety exception) so sensitive data never reaches an unverified enclave.

> **Why this matters:** without verification, you are trusting the seller's word that the code you think is running is actually running, in a real enclave. Verify the attestation before you leak input; verify the receipt before you trust the output.

---

## 6. Federation — calling a capability on another hub

Hubs federate. To invoke a capability that lives on a different hub, pass `sourceHub` (and usually the `productId`). Your channel and signing stay with your home hub; it routes the call.

```dart
// Dart
final result = await agent.invoke(
  capabilityId: 'cap',
  input: input,
  channelId: channel.id,
  sourceHub: 'https://hub-seller.example.com',
  productId: 'cap',
);
```

```ts
// TypeScript
const result = await agent.invoke({
  capabilityId: 'cap',
  input,
  channelId: channel.channel_id,
  sourceHub: 'https://hub-seller.example.com',
  productId: 'cap',
});
```

```rust
// Rust — product_id and source_hub are the 4th and 5th args.
let result = agent
    .invoke("cap", input, &channel.channel_id, Some("cap"), Some("https://hub-seller.example.com"))
    .await?;
```

`runOnce(...)` already does this automatically: it forwards each plan step's `capability.sourceHub` and `capability.productId` into the invoke, so federated results route correctly without extra work.

---

## 7. Error handling

The SDKs map HTTP status codes to typed errors with clear recovery paths.

| Error | Code | Meaning | Recovery |
|-------|------|---------|----------|
| Discovery failed | 4xx/5xx | hub unreachable / bad intent | retry, check the hub URL |
| Channels unavailable | 404 | hub has no channel plugin | use another hub, or `runOnce` |
| Payment required | 402 | channel depleted or expired | open a bigger channel, retry |
| Safety blocked | 403 | input tripped a safety gate | review / scrub input (PII), retry |
| TEE not verified | 200 | attestation/receipt failed | verify manually, treat output as untrusted |

### Exception types

- **Dart:** `AimarketException` (base) with subclasses `AimarketNetworkException`, `AimarketPaymentException` (402), `AimarketSafetyException` (403, exposes `.reason`). Inspect `.message` and `.statusCode`.
- **TypeScript:** the same hierarchy — `AimarketException`, `AimarketNetworkException`, `AimarketPaymentException`, `AimarketSafetyException`.
- **Rust:** the `AimarketError` enum — `Network(..)`, `Payment(..)`, `Safety(..)`, `Protocol(..)`.

```dart
// Dart — recover from a depleted channel
try {
  final result = await agent.invoke(capabilityId: 'cap', input: input, channelId: channel.id);
} on AimarketPaymentException {
  final fresh = await agent.openChannel(10.00);   // bigger deposit
  // retry with fresh.id …
} on AimarketSafetyException catch (e) {
  print('blocked: ${e.reason}');                  // scrub input, retry
} on AimarketException catch (e) {
  if (e.message.contains('depleted')) { /* … */ }
}
```

Note that a non-2xx that is neither 402 nor 403 does **not** throw from `invoke` — it returns an `InvokeResult` with `success: false` and `error` set, so always check `result.success` even when no exception is raised.

---

## 8. Advanced

### Channel reuse + service-wrapper pattern

Open a channel once and reuse it across many invocations. The SDK already caches channels keyed by `deposit:token:chain` and reuses one while it is **unexpired and has > 50% balance remaining**; below that it transparently opens a new one. In a long-lived app, wrap the agent in a service that keeps one channel until its balance drops under a threshold, then closes + reopens — and always `dispose()` / close on shutdown so unspent funds are refunded.

```dart
// Dart — production service wrapper (condensed)
class MarketplaceService {
  final AimarketAgent _agent;
  Channel? _channel;
  MarketplaceService(String hubUrl, String walletKey)
      : _agent = AimarketAgent(hubUrl: hubUrl, walletKey: walletKey, affiliate: 'my-app-v1');

  Future<Channel> _ensureChannel({double minBalance = 1.0}) async {
    if (_channel != null && _channel!.balanceUsd >= minBalance) return _channel!;
    if (_channel != null) { try { await _agent.closeChannel(_channel!.id); } catch (_) {} }
    _channel = await _agent.openChannel(5.00);
    return _channel!;
  }

  Future<InvokeResult> call(String capabilityId, Map<String, dynamic> input) async {
    final ch = await _ensureChannel();
    return _agent.invoke(capabilityId: capabilityId, input: input, channelId: ch.id);
  }

  Future<void> dispose() async {
    if (_channel != null) { try { await _agent.closeChannel(_channel!.id); } catch (_) {} }
    _agent.dispose();
  }
}
```

### Strip PII before remote calls

Capabilities run on someone else's hub. **Sanitize client-side** — do not rely on the hub or seller to scrub for you. Remove direct identifiers (`email`, `phone`, names, `resume_text`, …) and one-way-hash anything you must correlate (e.g. `company` → `company_hash`).

```dart
Map<String, dynamic> stripPii(Map<String, dynamic> data) {
  final out = Map<String, dynamic>.from(data);
  for (final k in ['candidate_name', 'email', 'phone', 'linkedin_url', 'resume_text']) {
    out.remove(k);
  }
  if (out.containsKey('company')) {
    out['company_hash'] = hashField(out['company'].toString());
    out.remove('company');
  }
  return out;
}
// … then: agent.invoke(capabilityId: cap, input: stripPii(raw), channelId: ch.id);
```

### Multi-capability pipelines

For workflows that compose several capabilities into a DAG over one shared channel — with per-step cost estimation and fan-out — see the [capability-composer integration example](https://github.com/alexar76/aimarket-desktop/blob/main/capability-composer/docs/sdk-integration.md). `invokeBatch(...)` runs many capabilities on a single channel (Dart/TS fan out concurrently; Rust runs them sequentially) and is the building block for these pipelines.

---

## 9. Versioning & support

Dart / TypeScript / Rust are the lock-stepped cross-platform family at **0.1.x**; the Python agent is a separate **2.1.x** package on PyPI. Versions track the *package*, not the protocol — all four target AIMarket Protocol v2. Cross-language test vectors live in `aimarket-sdks/test-vectors/`, and an ecosystem parity guard keeps the three compiled SDKs at matching versions and model shapes in CI. Full detail: [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md).

---

## 10. Related docs

- [aimarket-sdks README](../README.md) — quick starts for all three compiled SDKs
- [Dart SDK README](../dart/README.md) · [TypeScript SDK README](../typescript/README.md) · [Rust SDK README](../rust/README.md)
- [Python SDK guide](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) — the stateless, no-wallet consumer
- [SDK version policy](../../docs/sdk-version-policy.md)
- [Oracles — verifiable math capabilities](https://github.com/alexar76/oracles/blob/main/docs/en.md)
- Integration examples: [interview-prep-coach](https://github.com/alexar76/aimarket-desktop/blob/main/interview-prep-coach/docs/sdk-integration.md) · [cold-outreach-coach](https://github.com/alexar76/aimarket-desktop/blob/main/cold-outreach-coach/docs/sdk-integration.md) · [capability-composer](https://github.com/alexar76/aimarket-desktop/blob/main/capability-composer/docs/sdk-integration.md)
- Live: [modelmarket.dev](https://modelmarket.dev) · [magic-ai-factory.com](https://magic-ai-factory.com) · [oracles.modelmarket.dev](https://oracles.modelmarket.dev)

---

🇬🇧 [English](en.md) · 🇷🇺 [Русский](ru.md) · 🇪🇸 [Español](es.md)

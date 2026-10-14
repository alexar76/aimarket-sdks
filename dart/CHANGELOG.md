# Changelog

## 0.3.0

Pays on production hubs. Builds on 0.2.2 below (published 2026-10-13). Nothing
here breaks Dart callers; the minor
bump keeps the three SDKs in lockstep with the Rust SDK's breaking changes.

- `AimarketAgent(apiKey: ...)` / `AimarketAgentConfig.apiKey` (credits rail):
  every invoke sends `X-API-Key` and no channel headers, and `runOnce` neither
  opens nor closes a channel — the Python `aimarket-agent` rule. `invoke` and
  `invokeBatch` take `channelId` as optional.
- `openChannel(depositUsd, {txHash, payerSignature, wallet})`, sent as
  `tx_hash`, `payer_signature`, `wallet`. Production hubs open a channel only
  against an on-chain deposit (without one they answer `400 tx_hash is
  required for on-chain verification`) and the paying wallet's proof
  (`signChannelOpenProof`). The same `txHash` returns the channel it opened; a
  new one opens a new channel.
- `closeChannel` presents the channel secret and the paying wallet, and reads
  aimarket-hub's `{"settlement": {...}}` answer (`totalSpentUsd` read as 0
  before).
- aimarket-hub reports a channel's deposit as `original_deposit_usd`; it is
  now read as the deposit, so a cached channel is reused (the balance ratio
  was 0).
- A 402 carries the hub's reason and, for a credit account, its `balance` and
  the `needed` price; `AimarketPaymentException.statusCode` is 402.
- An invoke with neither `channelId` nor `apiKey` is refused locally with
  `AimarketPaymentException` before anything is sent.

## 0.2.2

- Version-parity release for the cross-language SDK family; no Dart API change.
- The new zero-setup `market.run` convenience layer is TypeScript-first while
  the shared v2 models and wire protocol remain unchanged.

## 0.2.1

- Default hub host is `https://modelmarket.dev` (was the parked `hub.aicom.io`).
- Patch release so installed clients pick up the corrected discovery/invoke base URL.

## 0.2.0

Pay-on-Verified support (additive, wire format stays v2). Version bumped to
0.2.0 in lockstep with the Rust SDK's breaking `invoke` signature change — the
ecosystem parity guard requires all three SDKs at one version.

- `AimarketAgent.invoke` accepts an optional `verify` parameter, sent verbatim
  as the `verify` request block.
- `InvokeResult` gains the optional `verification` envelope field.
- A verified invoke with `verify.wait` is now single-shot: its per-call timeout
  is extended past the hub's wait bound (`wait_timeout_s` + 30s) and the
  network-retry wrapper is skipped, so a mid-wait timeout/retry can't re-POST a
  fresh nonce and double-charge the buyer.

## 0.1.0

Initial release of the Dart consumer SDK for AI Market Protocol v2.

- `AimarketAgent` — discover → open channel → invoke → settle lifecycle.
- `MarketSigner` — Ed25519 canonical signatures and EIP-712 channel-debit signatures.
- TEE attestation verification (`TeeVerifier`).
- Data models with JSON round-trip: `Capability`, `Channel`, `InvokeResult`,
  `TeeAttestation`, `TeeReceipt`, `PlanStep`, `Settlement`, `BillOfMaterials`,
  `SearchResponse`.

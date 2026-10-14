# Changelog

## 0.3.0

Pays on production hubs. Builds on 0.2.2 below (published 2026-10-13).

- `apiKey` option (credits rail): `new AimarketAgent({ ..., apiKey })` sends
  `X-API-Key` on every invoke and never opens a channel — the Python
  `aimarket-agent` rule (a channel is used only when no key is set). `invoke`
  and `invokeBatch` take `channelId` as optional; `runOnce` skips the channel
  and returns no settlement.
- `openChannel(depositUsd, token, chain, deposit?)`: `deposit = { txHash,
  payerSignature?, wallet? }` is sent as `tx_hash`, `payer_signature`,
  `wallet`. Production hubs open a channel only against an on-chain deposit
  (without one they answer `400 tx_hash is required for on-chain
  verification`) and the paying wallet's proof (`signChannelOpenProof`). The
  same `txHash` returns the channel it opened; a new one opens a new channel.
- `closeChannel` presents the channel secret and the paying wallet, without
  which a production hub refuses to close a secured channel, and reads
  aimarket-hub's `{ settlement: { used_usd, ... } }` answer
  (`total_spent_usd` = `used_usd`).
- aimarket-hub reports a channel's deposit as `original_deposit_usd`; it is
  now read as `deposit_usd`, so a cached channel is reused (the balance ratio
  was NaN).
- A 402 carries the hub's reason and, for a credit account, its `balance` and
  the `needed` price.
- An invoke with neither `channelId` nor `apiKey` is refused locally with
  `AimarketPaymentException` before anything is sent.

## 0.2.2

- Add `market.run`, a safe one-call path that searches, re-checks the selected
  offer against local budget/trust/latency limits, invokes exactly once, and
  verifies the signed hybrid receipt.
- Support zero-setup trials, credits, and pre-funded channels without accepting
  wallet private keys; credentials remain in request headers.
- Reject insecure remote Hub URLs and non-JSON or oversized inputs before any
  payload is sent.
- Disable HTTP redirects for all Hub calls so channel and credit credentials
  cannot cross an origin boundary; reject search rows with no explicit price.
- Send `max_price_usd` on invoke so the Hub atomically refuses a reprice above
  the caller's budget before reserving funds or running the provider.

## 0.2.0

Pay-on-Verified support (additive, wire format stays v2). Version bumped to
0.2.0 in lockstep with the Rust SDK's breaking `invoke` signature change — the
ecosystem parity guard requires all three SDKs at one version.

- `AimarketAgent.invoke` accepts an optional `verify` opt, sent verbatim as
  the `verify` request block.
- `InvokeResult` gains the optional `verification` envelope field.
- A verified invoke with `verify.wait` is now single-shot: its per-request
  timeout is extended past the hub's wait bound (`wait_timeout_s` + 30s) and
  the network-retry wrapper is skipped, so a mid-wait abort/retry can't re-POST
  a fresh nonce and double-charge the buyer.

## 0.1.0

Initial release of the TypeScript consumer SDK for AI Market Protocol v2.

- `AimarketAgent` — discover → open channel → invoke → settle lifecycle.
- `MarketSigner` — Ed25519 canonical signatures and EIP-712 channel-debit signatures (viem).
- TEE attestation verification.
- Typed models: `Capability`, `Channel`, `InvokeResult`, `TeeAttestation`,
  `TeeReceipt`, `PlanStep`, `Settlement`, `BillOfMaterials`, `SearchResponse`.

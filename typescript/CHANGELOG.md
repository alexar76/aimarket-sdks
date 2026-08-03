# Changelog

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

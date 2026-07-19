# Changelog

## 0.2.0

Pay-on-Verified support (wire format stays v2).

- **BREAKING**: `AimarketAgent::invoke` gains a 6th positional argument,
  `verify: Option<serde_json::Value>`, forwarded verbatim as the `verify`
  request block — existing callers must pass `None`. `invoke_batch` is
  unchanged and always sends no verify block.
- `InvokeResult` gains the optional `verification` envelope field.
- A verified invoke with `verify.wait` is now single-shot: its per-request
  timeout is extended past the hub's wait bound (`wait_timeout_s` + 30s) and
  `retry_with_backoff` is skipped, so a mid-wait abort/retry can't re-POST a
  fresh nonce and double-charge the buyer.

## 0.1.0

Initial release of the Rust consumer SDK for AI Market Protocol v2.

- `AimarketAgent` — discover → open channel → invoke → settle lifecycle (async, tokio).
- `MarketSigner` — Ed25519 canonical signatures and EIP-712 channel-debit signatures (k256).
- TEE attestation verification.
- `serde` models: `Capability`, `Channel`, `InvokeResult`, `TeeAttestation`,
  `TeeReceipt`, `PlanStep`, `Settlement`, `BillOfMaterials`, `SearchResponse`.

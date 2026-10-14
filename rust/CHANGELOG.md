# Changelog

## 0.3.0

Pays on production hubs. Builds on 0.2.2 below (published 2026-10-13).

- **BREAKING**: `AimarketAgentConfig` gains `api_key: Option<String>`
  (credits rail). The new `AimarketAgentConfig::new(hub_url, wallet_key)`
  gives the defaults `AimarketAgent::new` uses, so a config with a key is
  `AimarketAgentConfig { api_key: Some(key), ..AimarketAgentConfig::new(hub,
  wallet) }`. With a key, `invoke` sends `X-API-Key` and no channel headers
  (pass `""` as `channel_id`), and `run_once` neither opens nor closes a
  channel — the Python `aimarket-agent` rule.
- **BREAKING**: `open_channel` gains a 4th argument, `deposit:
  Option<&ChannelDeposit>` (`tx_hash`, `payer_signature`, `wallet`), sent as
  `tx_hash`, `payer_signature`, `wallet`. Production hubs open a channel only
  against an on-chain deposit (without one they answer `400 tx_hash is
  required for on-chain verification`) and the paying wallet's proof
  (`payer_proof::sign_channel_open_proof`). Existing callers pass `None`. The
  same `tx_hash` returns the channel it opened; a new one opens a new channel.
- `open_channel` could not decode a channel from aimarket-hub, which reports
  `original_deposit_usd` rather than `deposit_usd`, and so dropped the
  one-time channel secret that came in the same answer. The deposit is now
  read under either name and the secret is captured before decoding.
- `close_channel` presents the channel secret and the paying wallet, and reads
  aimarket-hub's `{"settlement": {...}}` answer, which failed to decode before.
- Refusals carry the hub's body: a 402 names the reason and, for a credit
  account, its `balance` and the `needed` price; a failed channel open
  includes the hub's answer (a payer-proof refusal carries the text to sign).
- An invoke with neither a channel nor an `api_key` is refused locally with
  `AimarketError::Payment` before anything is sent.

## 0.2.2

- Version-parity release for the cross-language SDK family; no Rust API change.
- The new zero-setup `market.run` convenience layer is TypeScript-first while
  the shared v2 models and wire protocol remain unchanged.

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

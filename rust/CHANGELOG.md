# Changelog

## 0.1.0

Initial release of the Rust consumer SDK for AI Market Protocol v2.

- `AimarketAgent` — discover → open channel → invoke → settle lifecycle (async, tokio).
- `MarketSigner` — Ed25519 canonical signatures and EIP-712 channel-debit signatures (k256).
- TEE attestation verification.
- `serde` models: `Capability`, `Channel`, `InvokeResult`, `TeeAttestation`,
  `TeeReceipt`, `PlanStep`, `Settlement`, `BillOfMaterials`, `SearchResponse`.

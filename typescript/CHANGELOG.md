# Changelog

## 0.1.0

Initial release of the TypeScript consumer SDK for AI Market Protocol v2.

- `AimarketAgent` — discover → open channel → invoke → settle lifecycle.
- `MarketSigner` — Ed25519 canonical signatures and EIP-712 channel-debit signatures (viem).
- TEE attestation verification.
- Typed models: `Capability`, `Channel`, `InvokeResult`, `TeeAttestation`,
  `TeeReceipt`, `PlanStep`, `Settlement`, `BillOfMaterials`, `SearchResponse`.

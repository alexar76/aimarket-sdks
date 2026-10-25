Code hardening, 2026-10-25

TypeScript, Rust and Dart financial `invoke` requests are single-shot. A transport
error may mean that a response was lost after payment; it must not automatically
create another paid call. `maxRetries` remains accepted for configuration
compatibility but does not replay financial invokes. Reconcile receipts/account
state before explicitly retrying an uncertain outcome.

The agent HTTP transports refuse redirects, including when custom API-key and
channel-secret headers are present. Configure the final Hub URL directly.

`MarketSigner` accepts a 32-byte hexadecimal Ed25519 seed, optionally prefixed by
`0x` or `0X`. It no longer hashes a password, placeholder or malformed value into
a predictable identity. Valid seeds, public keys, signature bytes and EIP-712
payment vectors are unchanged. Rust callers reading untrusted configuration can
use `MarketSigner::try_new`; the existing `new` constructor fails loudly for an
invalid seed. TypeScript and Dart constructors throw on invalid input. Generate
fresh keys with the SDK's secure generator or a cryptographic random source.

The changes are local source changes; no package or chain deployment is performed.

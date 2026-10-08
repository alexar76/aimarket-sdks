#!/usr/bin/env node
/**
 * Refresh cross-SDK EIP-712 digest + signature expectations from TypeScript (viem).
 *
 *   npm run vectors:refresh        (from aimarket-sdks/typescript)
 *
 * It WRITES `test-vectors/debit_authorization.json`; the three SDKs then assert
 * against it in dart/test/cross_sdk_vectors_test.dart,
 * typescript/test/cross_sdk_vectors.test.ts and rust/tests/cross_sdk_vectors.rs.
 * Those are the gate; this is what regenerates what they check. It used to be
 * called `verify_test_vectors.mjs`, which read as the gate itself.
 *
 * It lived in `aimarket-sdks/scripts/` and could not run from anywhere: ESM
 * resolves both `./dist/index.js` and a bare `viem` against the FILE's own
 * directory, not the shell's, so the note that said "run from typescript/"
 * did not help either. Here `viem` resolves through typescript/node_modules
 * and the vector path is unchanged.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { getAddress } from 'viem';
import { computeDebitDigest, MarketSigner } from '../dist/index.js';

const vectorPath = new URL('../../test-vectors/debit_authorization.json', import.meta.url);
const vector = JSON.parse(readFileSync(vectorPath, 'utf8'));

const params = {
  channelId: vector.params.channelId,
  hub: getAddress(vector.params.hub),
  token: getAddress(vector.params.token),
  amount: BigInt(vector.params.amount),
  receiptId: vector.params.receiptId,
  nonce: BigInt(vector.params.nonce),
  deadline: BigInt(vector.params.deadline),
  chainId: vector.params.chainId,
  verifyingContract: getAddress(vector.params.verifyingContract),
};

const digest = computeDebitDigest(params);
const signer = new MarketSigner(
  vector.ed25519SeedHex,
  `0x${vector.ethereumPrivateKeyHex}`,
);
const ed25519Sig = signer.signCanonical(vector.canonicalMessage);
const eip712Sig = signer.signDebitAuthorization({
  ...params,
  deadline: Number(params.deadline),
});

vector.expectedDigest = digest;
vector.expectedEd25519Signature = ed25519Sig;
vector.expectedEip712Signature = eip712Sig;

writeFileSync(vectorPath, `${JSON.stringify(vector, null, 2)}\n`);
console.log('Updated', vectorPath.pathname);
console.log('digest:', digest);
console.log('ed25519:', ed25519Sig.slice(0, 24), '...');
console.log('eip712:', eip712Sig.slice(0, 24), '...');

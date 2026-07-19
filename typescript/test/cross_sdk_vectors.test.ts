import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { getAddress } from 'viem';
import { computeDebitDigest, MarketSigner } from '../src';

const vectorPath = join(dirname(fileURLToPath(import.meta.url)), '../../test-vectors/debit_authorization.json');
const vector = JSON.parse(readFileSync(vectorPath, 'utf8')) as {
  params: Record<string, string | number>;
  ed25519SeedHex: string;
  ethereumPrivateKeyHex: string;
  canonicalMessage: string;
  expectedDigest?: string;
  expectedEd25519Signature?: string;
  expectedEip712Signature?: string;
};

function digestParams() {
  return {
    channelId: vector.params.channelId as `0x${string}`,
    hub: getAddress(vector.params.hub as string),
    token: getAddress(vector.params.token as string),
    amount: BigInt(vector.params.amount as string),
    receiptId: vector.params.receiptId as `0x${string}`,
    nonce: BigInt(vector.params.nonce as string),
    deadline: BigInt(vector.params.deadline as number),
    chainId: vector.params.chainId as number,
    verifyingContract: getAddress(vector.params.verifyingContract as string),
  };
}

describe('cross-SDK test vectors', () => {
  it('computes stable EIP-712 debit digest', () => {
    const digest = computeDebitDigest(digestParams());
    expect(digest).toMatch(/^0x[0-9a-f]{64}$/);
    if (vector.expectedDigest) {
      expect(digest).toBe(vector.expectedDigest);
    }
  });

  it('produces stable Ed25519 and EIP-712 signatures', () => {
    const signer = new MarketSigner(
      vector.ed25519SeedHex,
      `0x${vector.ethereumPrivateKeyHex}`
    );
    const ed25519Sig = signer.signCanonical(vector.canonicalMessage);
    const eip712Sig = signer.signDebitAuthorization({
      ...digestParams(),
      deadline: Number(vector.params.deadline),
    });
    expect(ed25519Sig.startsWith('ed25519:')).toBe(true);
    expect(eip712Sig.startsWith('eip712:0x')).toBe(true);
    if (vector.expectedEd25519Signature) {
      expect(ed25519Sig).toBe(vector.expectedEd25519Signature);
    }
    if (vector.expectedEip712Signature) {
      expect(eip712Sig).toBe(vector.expectedEip712Signature);
    }
  });
});

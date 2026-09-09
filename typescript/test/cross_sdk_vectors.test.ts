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

describe('cross-SDK debit authorization signature', () => {
  // The digest check above passes even when the signing call hashes its input a second
  // time — self-consistent, and rejected by AIMarketEscrow.debitChannel, which recovers
  // over the digest itself. Dart shipped exactly that bug. RFC-6979 makes the signature
  // deterministic and low-s, so all three SDKs must produce these same bytes.
  const expectedSignature = (vector as unknown as { expectedSignature?: string })
    .expectedSignature;
  const expectedSigner = (vector as unknown as { expectedSigner?: string }).expectedSigner;

  it('the fixture carries a reference signature', () => {
    expect(expectedSignature).toBeTruthy();
    expect(expectedSigner).toBeTruthy();
  });

  it('MarketSigner reproduces the reference signature byte for byte', () => {
    const signer = new MarketSigner(vector.ed25519SeedHex, `0x${vector.ethereumPrivateKeyHex}`);
    const encoded = signer.signDebitAuthorization({
      channelId: vector.params.channelId as string,
      hub: vector.params.hub as string,
      token: vector.params.token as string,
      amount: BigInt(vector.params.amount as string),
      receiptId: vector.params.receiptId as string,
      nonce: BigInt(vector.params.nonce as string),
      deadline: Number(vector.params.deadline),
      chainId: vector.params.chainId as number,
      verifyingContract: vector.params.verifyingContract as string,
    });
    expect(encoded.startsWith('eip712:0x')).toBe(true);
    expect(encoded.slice('eip712:'.length)).toBe(expectedSignature);
  });

  it('the signature carries a Solidity-compatible v', () => {
    // ECDSA.recover wants 27/28; an EIP-155 chain-encoded v is simply invalid on chain.
    const raw = expectedSignature!.slice(2);
    expect(raw.length).toBe(130);
    expect([27, 28]).toContain(parseInt(raw.slice(128), 16));
  });
});

describe('private key handling', () => {
  it('accepts a key with or without the 0x prefix, identically', () => {
    // parseEthPrivateKey normalises in the constructor, so both spellings reach the
    // signer as the same 32 bytes. Pinned because the signing path calls viem's
    // hexToBytes, which silently drops two characters from an unprefixed string — the
    // normalisation is what stands between that and signing as a different wallet.
    const args = {
      channelId: vector.params.channelId as string,
      hub: vector.params.hub as string,
      token: vector.params.token as string,
      amount: BigInt(vector.params.amount as string),
      receiptId: vector.params.receiptId as string,
      nonce: BigInt(vector.params.nonce as string),
      deadline: Number(vector.params.deadline),
      chainId: vector.params.chainId as number,
      verifyingContract: vector.params.verifyingContract as string,
    };
    const withPrefix = new MarketSigner(
      vector.ed25519SeedHex,
      `0x${vector.ethereumPrivateKeyHex}`,
    ).signDebitAuthorization(args);
    const without = new MarketSigner(
      vector.ed25519SeedHex,
      vector.ethereumPrivateKeyHex,
    ).signDebitAuthorization(args);
    expect(without).toBe(withPrefix);
  });

  it('rejects a key that is not 32 bytes at construction', () => {
    expect(() => new MarketSigner(vector.ed25519SeedHex, '0xdeadbeef')).toThrow(
      /32-byte hex string/,
    );
  });
});

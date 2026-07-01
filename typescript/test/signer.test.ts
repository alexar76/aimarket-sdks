import { describe, expect, it } from 'vitest';
import { getAddress } from 'viem';
import {
  DEBIT_TYPEHASH_HEADER,
  ESCROW_CONTRACT_NAME,
  ESCROW_CONTRACT_VERSION,
  MarketSigner,
  computeDebitDigest,
} from '../src';

const ED25519_SEED =
  '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60';

// Anvil account #0 — standard test Ethereum key.
const ETH_PRIVATE_KEY =
  '0xac0974bec39a17e36ba4b6b40d764b994fa08d04ce65968ec04ec80ecb000000';

const DEBIT_ARGS = {
  channelId:
    '0x0000000000000000000000000000000000000000000000000000000000000001' as const,
  hub: getAddress('0x000000000000000000000000000000000000bEEF'),
  token: getAddress('0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913'),
  amount: 5_000_000n,
  receiptId:
    '0x0000000000000000000000000000000000000000000000000000000000001234' as const,
  nonce: 0n,
  deadline: 2_000_000_000,
  chainId: 31_337,
  verifyingContract: getAddress('0x5FbDB2315678afecb367f032d93F642f64180aa3'),
};

describe('MarketSigner', () => {
  it('signCanonical uses real Ed25519 and is deterministic', () => {
    const s = new MarketSigner(ED25519_SEED);
    const a = s.signCanonical('hello');
    const b = s.signCanonical('hello');
    expect(a).toBe(b);
    expect(a.startsWith('ed25519:')).toBe(true);
    expect(a.length).toBeGreaterThan(20);
  });

  it('verify checks Ed25519 with the signers public key', () => {
    const s = new MarketSigner(ED25519_SEED);
    const sig = s.signCanonical('test message');
    expect(s.verify(s.publicKeyHex(), sig, 'test message')).toBe(true);
    expect(s.verify(s.publicKeyBase64(), sig, 'test message')).toBe(true);
    expect(s.verify(s.publicKeyHex(), sig, 'wrong message')).toBe(false);
  });

  it('computeDebitDigest is deterministic keccak256 EIP-712', () => {
    const a = computeDebitDigest(DEBIT_ARGS);
    const b = computeDebitDigest(DEBIT_ARGS);
    expect(a).toBe(b);
    expect(a).toMatch(/^0x[0-9a-f]{64}$/);
  });

  it('signDebitAuthorization produces deterministic secp256k1 signature', () => {
    const signer = new MarketSigner(ED25519_SEED, ETH_PRIVATE_KEY);
    const a = signer.signDebitAuthorization(DEBIT_ARGS);
    const b = signer.signDebitAuthorization(DEBIT_ARGS);
    expect(a).toBe(b);
    expect(a.startsWith('eip712:')).toBe(true);
    expect(a.includes('0x')).toBe(true);
    expect(a.length).toBeGreaterThan(80);
  });

  it('signDebitAuthorization is bound to the hub address', () => {
    const signer = new MarketSigner(ED25519_SEED, ETH_PRIVATE_KEY);
    const base = signer.signDebitAuthorization({
      ...DEBIT_ARGS,
      hub: getAddress('0x00000000000000000000000000000000000000AA'),
    });
    const swap = signer.signDebitAuthorization({
      ...DEBIT_ARGS,
      hub: getAddress('0x00000000000000000000000000000000000000BB'),
    });
    expect(base).not.toBe(swap);
  });

  it('signDebitAuthorization requires ethereum private key', () => {
    const signer = new MarketSigner(ED25519_SEED);
    expect(() => signer.signDebitAuthorization(DEBIT_ARGS)).toThrow(/ethereumPrivateKeyHex/);
  });

  it('DEBIT_TYPEHASH_HEADER matches the on-chain contract literal', () => {
    expect(DEBIT_TYPEHASH_HEADER).toBe(
      'DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)',
    );
    expect(ESCROW_CONTRACT_NAME).toBe('AIMarketEscrow');
    expect(ESCROW_CONTRACT_VERSION).toBe('1');
  });
});

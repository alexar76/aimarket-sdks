import { describe, expect, it } from 'vitest';
import {
  DEBIT_TYPEHASH_HEADER,
  ESCROW_CONTRACT_NAME,
  ESCROW_CONTRACT_VERSION,
  MarketSigner,
} from '../src';

const TEST_KEY_HEX =
  '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60';

describe('MarketSigner', () => {
  it('signCanonical is deterministic and prefixed', () => {
    const s = new MarketSigner('abcdef0123');
    const a = s.signCanonical('hello');
    const b = s.signCanonical('hello');
    expect(a).toBe(b);
    expect(a.startsWith('ed25519:')).toBe(true);
  });

  it('signDebitAuthorization produces deterministic eip712 signature', () => {
    const signer = new MarketSigner(TEST_KEY_HEX);
    const args = {
      channelId:
        '0x0000000000000000000000000000000000000000000000000000000000000001',
      hub: '0x000000000000000000000000000000000000bEEF',
      token: '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913',
      amount: 5_000_000n,
      receiptId:
        '0x0000000000000000000000000000000000000000000000000000000000001234',
      nonce: 0n,
      deadline: 2_000_000_000,
    };
    const a = signer.signDebitAuthorization(args);
    const b = signer.signDebitAuthorization(args);
    expect(a).toBe(b);
    expect(a.startsWith('eip712:')).toBe(true);
  });

  it('signDebitAuthorization is bound to the hub address', () => {
    const signer = new MarketSigner(TEST_KEY_HEX);
    const base = signer.signDebitAuthorization({
      channelId:
        '0x0000000000000000000000000000000000000000000000000000000000000001',
      hub: '0x000000000000000000000000000000000000AAAA',
      token: '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913',
      amount: 5_000_000n,
      receiptId:
        '0x0000000000000000000000000000000000000000000000000000000000001234',
      nonce: 0n,
      deadline: 2_000_000_000,
    });
    const swap = signer.signDebitAuthorization({
      channelId:
        '0x0000000000000000000000000000000000000000000000000000000000000001',
      hub: '0x000000000000000000000000000000000000BBBB',
      token: '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913',
      amount: 5_000_000n,
      receiptId:
        '0x0000000000000000000000000000000000000000000000000000000000001234',
      nonce: 0n,
      deadline: 2_000_000_000,
    });
    expect(base).not.toBe(swap);
  });

  it('DEBIT_TYPEHASH_HEADER matches the on-chain contract literal', () => {
    expect(DEBIT_TYPEHASH_HEADER).toBe(
      'DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)',
    );
    expect(ESCROW_CONTRACT_NAME).toBe('AIMarketEscrow');
    expect(ESCROW_CONTRACT_VERSION).toBe('1');
  });
});

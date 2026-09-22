/**
 * The channel-open payer proof, bound to the shared protocol vector.
 *
 * The hub rebuilds this message independently and compares recovered signers, so "close
 * enough" is indistinguishable from wrong: the vector is the contract, and these tests
 * assert against its bytes rather than against our own implementation.
 */

import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { privateKeyToAccount } from 'viem/accounts';
import type { Hex } from 'viem';

import {
  canonicalProofAmountCents,
  channelOpenProofMessage,
  recoverChannelOpenPayer,
  signChannelOpenProof,
} from '../src/payerProof';

interface VectorCase {
  name: string;
  chain: string;
  tx_hash: string;
  payer: string;
  amount_usd: number;
  amount_cents: number;
  message: string;
  signature?: Hex;
  recovers_to?: string;
}

const vector = JSON.parse(
  readFileSync(
    join(__dirname, '../../../aimarket-protocol/test-vectors/payer-proof.json'),
    'utf8',
  ),
) as { test_key: Hex; test_address: string; cases: VectorCase[] };

describe('channel-open payer proof', () => {
  it('has cases to check', () => {
    expect(vector.cases.length).toBeGreaterThan(4);
  });

  for (const c of vector.cases) {
    it(`reproduces the canonical message: ${c.name}`, () => {
      expect(
        channelOpenProofMessage({
          chain: c.chain,
          txHash: c.tx_hash,
          payer: c.payer,
          amountUsd: c.amount_usd,
        }),
      ).toBe(c.message);
    });

    it(`converts the amount the way the hub does: ${c.name}`, () => {
      expect(canonicalProofAmountCents(c.amount_usd)).toBe(c.amount_cents);
    });
  }

  it('rounds a half-cent to EVEN, not away from zero', () => {
    // Math.round(12.5) is 13; the hub's Python round() gives 12. Using the wrong rule
    // produces a valid-looking signature the hub refuses.
    expect(canonicalProofAmountCents(0.125)).toBe(12);
    expect(canonicalProofAmountCents(0.135)).toBe(14);
    expect(Math.round(0.125 * 100)).toBe(13); // the trap, spelled out
  });

  it('refuses an unusable amount deterministically', () => {
    expect(canonicalProofAmountCents(Number.NaN)).toBe(-1);
    expect(canonicalProofAmountCents(Number.POSITIVE_INFINITY)).toBe(-1);
  });

  it('signs and recovers the vector signature', async () => {
    const account = privateKeyToAccount(vector.test_key);
    for (const c of vector.cases.filter((x) => x.signature)) {
      const params = {
        chain: c.chain,
        txHash: c.tx_hash,
        payer: c.payer,
        amountUsd: c.amount_usd,
      };
      const signature = await signChannelOpenProof(params, (message) =>
        account.signMessage({ message }),
      );
      expect(signature).toBe(c.signature);
      expect((await recoverChannelOpenPayer(params, signature)).toLowerCase()).toBe(
        c.recovers_to!.toLowerCase(),
      );
    }
  });

  it('a different wallet does not recover as the payer', async () => {
    const other = privateKeyToAccount(`0x${'77'.repeat(32)}` as Hex);
    const c = vector.cases[0];
    const params = {
      chain: c.chain,
      txHash: c.tx_hash,
      payer: c.payer,
      amountUsd: c.amount_usd,
    };
    const signature = await signChannelOpenProof(params, (m) => other.signMessage({ message: m }));
    const recovered = await recoverChannelOpenPayer(params, signature);
    expect(recovered.toLowerCase()).not.toBe(vector.test_address.toLowerCase());
  });
});

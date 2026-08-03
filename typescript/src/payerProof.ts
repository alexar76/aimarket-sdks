/**
 * Canonical channel-open payer proof (EIP-191).
 *
 * Opening a payment channel against an on-chain deposit requires proving control of the
 * wallet that PAID. Every deposit lands in the same platform settlement wallet, so
 * recipient+amount alone only shows that *somebody* paid — without this signature, anyone
 * watching inbound transfers could quote a stranger's (public) tx hash and be credited.
 *
 * Personal-sign rather than typed data on purpose: any ordinary wallet, browser extension
 * or hardware device can produce it with no EIP-712 support.
 *
 * The message must match `aimarket-protocol/test-vectors/payer-proof.json` byte for byte —
 * the hub rebuilds it independently and compares recovered signers, so a one-character
 * difference is an unexplainable "invalid payer proof" for the user.
 */

import { hashMessage, recoverAddress, type Address, type Hex } from 'viem';

export const PAYER_PROOF_DOMAIN = 'AIMarket-Payer-Proof';
export const PAYER_PROOF_VERSION = 1;
export const PAYER_PROOF_CHANNEL_OPEN = 'channel-open';

const HEX_RE = /^[0-9a-fA-F]+$/;

/** Canonical chain id: chain names are ASCII and case-free. */
export function canonicalProofChain(chain: string): string {
  return (chain ?? '').trim().toLowerCase();
}

/**
 * Canonical transaction id.
 *
 * An EVM hash is hex and case-insensitive at the JSON-RPC layer, so `0xABC…` and `0xabc…`
 * name the same transaction and must produce the same challenge. The `0x` prefix is
 * normalised IN as well, because the two hub stacks disagree about whether they strip it
 * before building the challenge. Anything non-hex (a base58 Solana signature) is
 * case-SIGNIFICANT and left byte-exact.
 */
export function canonicalProofTxHash(txHash: string): string {
  const tx = (txHash ?? '').trim();
  const body = tx.slice(0, 2).toLowerCase() === '0x' ? tx.slice(2) : tx;
  if (body.length > 0 && HEX_RE.test(body)) return `0x${body.toLowerCase()}`;
  return tx;
}

/** Canonical payer: EIP-55 mixed case is a checksum, not identity, so an address lowercases. */
export function canonicalProofPayer(payer: string): string {
  const addr = (payer ?? '').trim();
  if (addr.length === 42 && addr.slice(0, 2).toLowerCase() === '0x' && HEX_RE.test(addr.slice(2))) {
    return `0x${addr.slice(2).toLowerCase()}`;
  }
  return addr;
}

/**
 * Deposit amount as the integer cents both ledgers bill in.
 *
 * ROUND-HALF-TO-EVEN, because the hub computes this with Python's `round()`, which is
 * banker's rounding — `0.125 → 12`, not 13. JavaScript's `Math.round` rounds half AWAY
 * from zero, so using it here silently produces a different preimage (and therefore a
 * rejected proof) for any amount landing exactly on a half-cent. Returns -1 for an
 * unusable amount so the challenge stays deterministic and can never match a real deposit.
 */
export function canonicalProofAmountCents(amountUsd: number): number {
  if (typeof amountUsd !== 'number' || !Number.isFinite(amountUsd)) return -1;
  const scaled = amountUsd * 100;
  const floor = Math.floor(scaled);
  const diff = scaled - floor;
  if (diff > 0.5) return floor + 1;
  if (diff < 0.5) return floor;
  // Exactly .5 — pick the even neighbour.
  return floor % 2 === 0 ? floor : floor + 1;
}

export interface PayerProofParams {
  chain: string;
  txHash: string;
  payer: string;
  amountUsd: number;
}

/** The exact text the paying wallet signs. */
export function channelOpenProofMessage(params: PayerProofParams): string {
  return [
    `${PAYER_PROOF_DOMAIN}/v${PAYER_PROOF_VERSION}`,
    `purpose:${PAYER_PROOF_CHANNEL_OPEN}`,
    `chain:${canonicalProofChain(params.chain)}`,
    `tx:${canonicalProofTxHash(params.txHash)}`,
    `payer:${canonicalProofPayer(params.payer)}`,
    `amount_cents:${canonicalProofAmountCents(params.amountUsd)}`,
  ].join('\n');
}

/** The EIP-191 digest of that message, for callers that sign a hash directly. */
export function channelOpenProofHash(params: PayerProofParams): Hex {
  return hashMessage(channelOpenProofMessage(params));
}

/**
 * Sign the challenge with anything that can personal-sign.
 *
 * Takes a signer function rather than a private key so the same call works with a viem
 * account, a browser wallet (`window.ethereum.request({method:'personal_sign'})`) or a
 * hardware device — the SDK never needs to see key material.
 */
export async function signChannelOpenProof(
  params: PayerProofParams,
  signMessage: (message: string) => Promise<Hex> | Hex,
): Promise<Hex> {
  return await signMessage(channelOpenProofMessage(params));
}

/** Address that produced `signature` over this challenge — the hub's own check, locally. */
export async function recoverChannelOpenPayer(
  params: PayerProofParams,
  signature: Hex,
): Promise<Address> {
  return await recoverAddress({
    hash: channelOpenProofHash(params),
    signature,
  });
}

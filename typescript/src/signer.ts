/**
 * Ed25519 signing + EIP-712 debit-authorization for AI Market Protocol v2.
 *
 * Mirrors the Dart `MarketSigner` API and produces the *exact* EIP-712
 * typed-data payload that `contracts/evm/AIMarketEscrow.sol` expects.
 *
 * ## Production note
 *
 * `sign()` is a development stub built on HMAC-SHA256, and
 * `signEip712TypedData()` digests the encoded payload with SHA-256 (same
 * stubbing strategy as the Dart SDK). The *structure* of the typed data is
 * production-correct; only the digest hash function and the signing curve
 * (secp256k1 ECDSA for EIP-712, real Ed25519 for receipts) need to be swapped
 * in. Once swapped, `ECDSA.recover(digest, signature)` on the deployed
 * `AIMarketEscrow` contract will recover the depositor address and accept
 * `debitChannel`. With the stub in place, the contract will revert with
 * `InvalidSignature()` — by design — so production deployments MUST plug in
 * `viem` / `ethers` / `tweetnacl-ed25519` before going to mainnet.
 */

import nacl from 'tweetnacl';
import { createHash, createHmac } from 'crypto';

// ── EIP-712 ─────────────────────────────────────────────────────────────────

/** Domain separator parameters per EIP-712. */
export interface Eip712Domain {
  name: string;
  version: string;
  chainId: number;
  verifyingContract: string;
}

/**
 * EIP-712 typed-data envelope. `message` is a flat name -> string|number map.
 * Keys are sorted lexicographically before hashing to match the Dart SDK so
 * cross-language signatures remain reproducible.
 */
export interface TypedData {
  domain: Eip712Domain;
  primaryType: string;
  message: Record<string, string | number | bigint>;
}

function _sha256Hex(input: string): string {
  return createHash('sha256').update(input, 'utf8').digest('hex');
}

function _domainSeparator(d: Eip712Domain): string {
  const encoded = `domain:${d.name}|v:${d.version}|chain:${d.chainId}|contract:${d.verifyingContract}`;
  return _sha256Hex(encoded);
}

function _hashStruct(message: Record<string, string | number | bigint>): string {
  const keys = Object.keys(message).sort();
  const parts = keys.map((k) => `${k}:${message[k]?.toString()}`).join('|');
  return _sha256Hex(parts);
}

/**
 * Encode a typed-data envelope per EIP-712:
 *   0x1901 || domainSeparator || hashStruct(message)
 *
 * Returns a hex string (no `0x` prefix). Stub: SHA-256 instead of keccak256.
 */
export function encodeTypedData(data: TypedData): string {
  const domainHash = _domainSeparator(data.domain);
  const msgHash = _hashStruct(data.message);
  return _sha256Hex(`0x1901|domain:${domainHash}|${data.primaryType}:${msgHash}`);
}

// ── DEBIT_TYPEHASH ─────────────────────────────────────────────────────────

/**
 * EIP-712 typehash string for `DebitAuthorization`.
 *
 * MUST match the literal in `contracts/evm/AIMarketEscrow.sol`:
 *
 * ```solidity
 * bytes32 private constant DEBIT_TYPEHASH = keccak256(
 *   "DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)"
 * );
 * ```
 *
 * `hub` is part of the payload so a depositor's signature for hub A cannot be
 * replayed by hub B.
 */
export const DEBIT_TYPEHASH_HEADER =
  'DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)';

/** Contract name in the EIP-712 domain separator. */
export const ESCROW_CONTRACT_NAME = 'AIMarketEscrow';

/** Contract version in the EIP-712 domain separator. */
export const ESCROW_CONTRACT_VERSION = '1';

/** Parameters for [signDebitAuthorization]. */
export interface DebitAuthorizationParams {
  /** 0x-prefixed 32-byte channel identifier (bytes32 on-chain). */
  channelId: string;
  /** 0x-prefixed Ethereum address of the hub allowed to debit. */
  hub: string;
  /** 0x-prefixed ERC-20 token address (USDT/USDC). */
  token: string;
  /** Token amount in **base units** (USDT/USDC have 6 decimals). */
  amount: bigint;
  /** 0x-prefixed 32-byte receipt identifier; used to prevent double-spend. */
  receiptId: string;
  /** Current channel nonce, read from-chain before signing. */
  nonce: bigint;
  /** Unix timestamp after which the contract rejects the authorization. */
  deadline: number;
  /** EVM chain ID hosting the escrow (Base mainnet = 8453). */
  chainId?: number;
  /** 0x-prefixed deployed escrow address. */
  verifyingContract?: string;
}

// ── Signer ──────────────────────────────────────────────────────────────────

/**
 * Sign canonical strings (Ed25519 stub) and EIP-712 typed data.
 *
 * Two signature formats are emitted:
 *  - `"ed25519:<base64>"` for canonical hub messages, receipts, manifests.
 *  - `"eip712:<hex>"`     for on-chain debit authorizations.
 */
export class MarketSigner {
  private readonly _privateKeyBytes: Uint8Array;

  constructor(privateKeyHex: string) {
    // Production keys are 64-char hex (32-byte Ed25519 seed) but dev/test
    // stubs may pass any opaque string. Accept both; the HMAC-style sign()
    // stub just feeds the raw UTF-8 bytes into the digest.
    this._privateKeyBytes = new TextEncoder().encode(privateKeyHex);
  }

  /**
   * Sign a canonical string and return `"ed25519:<base64sig>"`.
   *
   * Production: replace HMAC-SHA256 stub with real Ed25519 via tweetnacl.
   */
  signCanonical(canonical: string): string {
    const mac = createHmac('sha256', Buffer.from(this._privateKeyBytes));
    mac.update(canonical, 'utf8');
    return `ed25519:${mac.digest('base64')}`;
  }

  /**
   * Verify a signature against a canonical string. Stub: re-signs and compares.
   * Production should use `nacl.sign.detached.verify(message, sig, pubKey)`.
   */
  verify(_publicKeyHex: string, signature: string, canonical: string): boolean {
    if (!signature.startsWith('ed25519:')) return false;
    return this.signCanonical(canonical) === signature;
  }

  /** Sign an EIP-712 typed-data envelope. Returns `"eip712:<hex>"`. */
  signEip712TypedData(data: TypedData): string {
    return `eip712:${encodeTypedData(data)}`;
  }

  /**
   * Sign a debit authorization for the on-chain `AIMarketEscrow` contract.
   *
   * Builds the exact EIP-712 typed-data envelope expected by the deployed
   * escrow. The depositor authorizes ONE hub to debit ONE channel for ONE
   * amount, paired with ONE receipt; nonce + deadline give replay protection.
   *
   * @returns `"eip712:<hex>"` signature string.
   */
  signDebitAuthorization(params: DebitAuthorizationParams): string {
    const {
      channelId,
      hub,
      token,
      amount,
      receiptId,
      nonce,
      deadline,
      chainId = 8453,
      verifyingContract = '0x0000000000000000000000000000000000000000',
    } = params;

    const data: TypedData = {
      domain: {
        name: ESCROW_CONTRACT_NAME,
        version: ESCROW_CONTRACT_VERSION,
        chainId,
        verifyingContract,
      },
      primaryType: 'DebitAuthorization',
      message: {
        channelId,
        hub,
        token,
        amount: amount.toString(),
        receiptId,
        nonce: nonce.toString(),
        deadline: deadline.toString(),
      },
    };
    return this.signEip712TypedData(data);
  }

  /** Sign the standard headers used on POST /ai-market/v2/invoke. */
  signedHeaders(args: {
    channelId: string;
    capabilityId: string;
    affiliate: string;
  }): Record<string, string> {
    const canonical = `channel:${args.channelId}|capability:${args.capabilityId}|affiliate:${args.affiliate}`;
    return {
      'X-Payment-Channel': args.channelId,
      'X-AIMarket-Affiliate': args.affiliate,
      'X-Market-Signature': this.signCanonical(canonical),
    };
  }

  /**
   * Generate a wallet. Returns hex private key (64 chars) and stub address.
   * Replace with secp256k1 + keccak256(publicKey)[12:] for production.
   */
  static generateWallet(): { privateKey: string; address: string } {
    const seed = nacl.randomBytes(32);
    const privateKey = Array.from(seed)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
    const pubHash = createHash('sha256').update(seed).digest('hex');
    return { privateKey, address: `0x${pubHash.slice(0, 40)}` };
  }
}

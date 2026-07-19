/**
 * Production Ed25519 + EIP-712 signing for AI Market Protocol v2.
 *
 * - Canonical hub messages: real Ed25519 (`ed25519:<base64>`)
 * - On-chain debit auth: keccak256 EIP-712 + secp256k1 (`eip712:0x<r><s><v>`)
 */

import nacl from 'tweetnacl';
import naclUtil from 'tweetnacl-util';
import { createHash } from 'crypto';
import { secp256k1 } from '@noble/curves/secp256k1.js';
import { type Address, type Hex, hexToBytes } from 'viem';
import { generatePrivateKey, privateKeyToAccount } from 'viem/accounts';
import { computeDebitDigest } from './eip712';

// ── EIP-712 constants ───────────────────────────────────────────────────────

export interface Eip712Domain {
  name: string;
  version: string;
  chainId: number;
  verifyingContract: string;
}

export interface TypedData {
  domain: Eip712Domain;
  primaryType: string;
  message: Record<string, string | number | bigint>;
}

export const DEBIT_TYPEHASH_HEADER =
  'DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)';

export const ESCROW_CONTRACT_NAME = 'AIMarketEscrow';
export const ESCROW_CONTRACT_VERSION = '1';

export interface DebitAuthorizationParams {
  channelId: string;
  hub: string;
  token: string;
  amount: bigint;
  receiptId: string;
  nonce: bigint;
  deadline: number;
  chainId?: number;
  verifyingContract?: string;
}

export interface MarketSignerOptions {
  /** 32-byte Ed25519 seed (64-char hex) for canonical invoke signatures. */
  ed25519SeedHex: string;
  /** secp256k1 Ethereum private key (64-char hex) for EIP-712 debit auth. */
  ethereumPrivateKeyHex?: string;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function parseSeedHex(hex: string): Uint8Array {
  const normalized = hex.replace(/^0x/i, '');
  if (normalized.length === 64 && /^[0-9a-fA-F]+$/.test(normalized)) {
    const bytes = new Uint8Array(32);
    for (let i = 0; i < 32; i++) {
      bytes[i] = parseInt(normalized.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }
  // Dev fallback: hash arbitrary short strings to a 32-byte seed.
  return createHash('sha256').update(hex, 'utf8').digest();
}

function parseEthPrivateKey(hex: string): Hex {
  const normalized = hex.replace(/^0x/i, '');
  if (normalized.length !== 64 || !/^[0-9a-fA-F]+$/.test(normalized)) {
    throw new Error('ethereumPrivateKeyHex must be a 32-byte hex string (64 chars)');
  }
  return `0x${normalized}` as Hex;
}

function decodePublicKey(publicKey: string): Uint8Array {
  const trimmed = publicKey.trim();
  if (/^[0-9a-fA-F]+$/.test(trimmed) && trimmed.length === 64) {
    const bytes = new Uint8Array(32);
    for (let i = 0; i < 32; i++) {
      bytes[i] = parseInt(trimmed.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }
  return naclUtil.decodeBase64(trimmed);
}

function decodeEd25519Signature(signature: string): Uint8Array {
  const raw = signature.startsWith('ed25519:') ? signature.slice(8) : signature;
  return naclUtil.decodeBase64(raw);
}

// ── Signer ──────────────────────────────────────────────────────────────────

export class MarketSigner {
  private readonly seed: Uint8Array;
  private readonly keyPair: nacl.SignKeyPair;
  private readonly ethereumPrivateKeyHex?: Hex;

  constructor(seedOrOptions: string | MarketSignerOptions, ethereumPrivateKeyHex?: string) {
    const opts: MarketSignerOptions =
      typeof seedOrOptions === 'string'
        ? { ed25519SeedHex: seedOrOptions, ethereumPrivateKeyHex }
        : seedOrOptions;

    this.seed = parseSeedHex(opts.ed25519SeedHex);
    this.keyPair = nacl.sign.keyPair.fromSeed(this.seed);
    this.ethereumPrivateKeyHex = opts.ethereumPrivateKeyHex
      ? parseEthPrivateKey(opts.ethereumPrivateKeyHex)
      : undefined;
  }

  /** Ed25519 public key as base64 (hub-compatible). */
  publicKeyBase64(): string {
    return naclUtil.encodeBase64(this.keyPair.publicKey);
  }

  /** Ed25519 public key as hex. */
  publicKeyHex(): string {
    return Buffer.from(this.keyPair.publicKey).toString('hex');
  }

  /** Sign a canonical UTF-8 string → `ed25519:<base64>`. */
  signCanonical(canonical: string): string {
    const message = new TextEncoder().encode(canonical);
    const sig = nacl.sign.detached(message, this.keyPair.secretKey);
    return `ed25519:${naclUtil.encodeBase64(sig)}`;
  }

  /** Verify `ed25519:<base64>` with hex- or base64-encoded public key. */
  verify(publicKey: string, signature: string, canonical: string): boolean {
    if (!signature.startsWith('ed25519:')) return false;
    try {
      const message = new TextEncoder().encode(canonical);
      const sig = decodeEd25519Signature(signature);
      const pub = decodePublicKey(publicKey);
      return nacl.sign.detached.verify(message, sig, pub);
    } catch {
      return false;
    }
  }

  /** Sign EIP-712 debit authorization → `eip712:0x<130 hex chars>`. */
  signDebitAuthorization(params: DebitAuthorizationParams): string {
    const ethKey = this.ethereumPrivateKeyHex;
    if (!ethKey) {
      throw new Error(
        'ethereumPrivateKeyHex is required for EIP-712 debit signing. ' +
          'Pass it to MarketSigner constructor as second argument or via MarketSignerOptions.'
      );
    }

    const digest = computeDebitDigest({
      channelId: params.channelId as Hex,
      hub: params.hub as Address,
      token: params.token as Address,
      amount: params.amount,
      receiptId: params.receiptId as Hex,
      nonce: params.nonce,
      deadline: BigInt(params.deadline),
      chainId: params.chainId ?? 8453,
      verifyingContract: (params.verifyingContract ??
        '0x0000000000000000000000000000000000000000') as Address,
    });

    const digestBytes = hexToBytes(digest);
    const privBytes = hexToBytes(ethKey);
    const recovered = secp256k1.sign(digestBytes, privBytes, {
      lowS: true,
      format: 'recovered',
    });
    const r = Buffer.from(recovered.slice(0, 32)).toString('hex');
    const s = Buffer.from(recovered.slice(32, 64)).toString('hex');
    const v = recovered[64] + 27;
    return `eip712:0x${r}${s}${v.toString(16).padStart(2, '0')}`;
  }

  signEip712TypedData(_data: TypedData): string {
    throw new Error('signEip712TypedData removed — use signDebitAuthorization() for AIMarketEscrow');
  }

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

  verifyHubSignature(hubPublicKey: string, message: string, signature: string): boolean {
    if (signature.startsWith('eip712:')) return false;
    const canonical = signature.startsWith('ed25519:') ? message : message;
    const sig = signature.startsWith('ed25519:') ? signature : `ed25519:${signature}`;
    return this.verify(hubPublicKey, sig, canonical);
  }

  static generateEd25519Keypair(): { seedHex: string; publicKeyBase64: string } {
    const seed = nacl.randomBytes(32);
    const pair = nacl.sign.keyPair.fromSeed(seed);
    return {
      seedHex: Buffer.from(seed).toString('hex'),
      publicKeyBase64: naclUtil.encodeBase64(pair.publicKey),
    };
  }

  /** Generate a secp256k1 Ethereum wallet for channel deposits / EIP-712. */
  static generateEthereumWallet(): { privateKeyHex: Hex; address: Address } {
    const privateKey = generatePrivateKey();
    const account = privateKeyToAccount(privateKey);
    return { privateKeyHex: privateKey, address: account.address };
  }

  /** @deprecated Prefer [generateEd25519Keypair] or [generateEthereumWallet]. */
  static generateWallet(): { privateKey: string; address: string } {
    const eth = MarketSigner.generateEthereumWallet();
    return { privateKey: eth.privateKeyHex.slice(2), address: eth.address };
  }
}

// Re-export digest helper at package boundary.
export { computeDebitDigest } from './eip712';

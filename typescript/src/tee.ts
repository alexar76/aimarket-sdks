import { createHash } from 'crypto';
import { type TeeAttestation, type TeeReceipt, attestationCanonical } from './models';
import { MarketSigner } from './signer';

/** Recognized TEE platform identifiers. */
export const TeePlatform = {
  awsNitro: 'aws_nitro',
  intelTdx: 'intel_tdx',
  amdSev: 'amd_sev',
  azureCc: 'azure_cc',
  all: new Set(['aws_nitro', 'intel_tdx', 'amd_sev', 'azure_cc']),
  displayNames: {
    aws_nitro: 'AWS Nitro Enclaves',
    intel_tdx: 'Intel TDX',
    amd_sev: 'AMD SEV-SNP',
    azure_cc: 'Azure Confidential Computing',
  },
  isSupported(platform: string): boolean {
    return TeePlatform.all.has(platform);
  },
} as const;

/** Result of a TEE attestation verification with detailed failure reasons. */
export interface TeeVerificationResult {
  isValid: boolean;
  failures: string[];
}

export function teeVerificationPass(): TeeVerificationResult {
  return { isValid: true, failures: [] };
}

export function teeVerificationFail(failures: string[]): TeeVerificationResult {
  return { isValid: false, failures };
}

/** Caches trusted code hashes fetched from the hub, with TTL expiry. */
export class TrustedHashCache {
  private readonly ttlMs: number;
  private readonly entries = new Map<string, { hash: string; expiresAt: number }>();

  constructor(ttlMs = 5 * 60 * 1000) {
    this.ttlMs = ttlMs;
  }

  get(key: string): string | undefined {
    const entry = this.entries.get(key);
    if (!entry) return undefined;
    if (Date.now() > entry.expiresAt) {
      this.entries.delete(key);
      return undefined;
    }
    return entry.hash;
  }

  set(key: string, hash: string): void {
    this.entries.set(key, { hash, expiresAt: Date.now() + this.ttlMs });
  }

  clear(): void {
    this.entries.clear();
  }

  get size(): number {
    return this.entries.size;
  }
}

function sha256Hex(input: string): string {
  return createHash('sha256').update(input, 'utf8').digest('hex');
}

function attestationIsExpired(attestation: TeeAttestation): boolean {
  const ts = Date.parse(attestation.timestamp);
  if (Number.isNaN(ts)) return true;
  const ageS = (Date.now() - ts) / 1000;
  return ageS > attestation.ttl_s;
}

/** Verifies TEE attestations and receipts client-side. */
export class TeeVerifier {
  private readonly signer: MarketSigner;
  private readonly trustedCodeHashes: Map<string, string>;
  private readonly hashCache: TrustedHashCache;
  private readonly enclavePublicKeys: Record<string, string>;
  lastFetch: Date | null = null;

  constructor(opts: {
    signer: MarketSigner;
    trustedCodeHashes?: Record<string, string>;
    hashCache?: TrustedHashCache;
    /** Override hub well-known enclave keys (hex or base64 public keys). */
    enclavePublicKeys?: Record<string, string>;
  }) {
    this.signer = opts.signer;
    this.trustedCodeHashes = new Map(Object.entries(opts.trustedCodeHashes ?? {}));
    this.hashCache = opts.hashCache ?? new TrustedHashCache();
    this.enclavePublicKeys = { ...DEFAULT_ENCLAVE_PUBLIC_KEYS, ...opts.enclavePublicKeys };
  }

  trustCodeHash(capabilityId: string, codeHash: string): void {
    this.trustedCodeHashes.set(capabilityId, codeHash);
    this.hashCache.set(capabilityId, codeHash);
  }

  async fetchTrustedHashes(
    hubUrl: string,
    fetchFn: typeof fetch = fetch
  ): Promise<number> {
    try {
      const resp = await fetchFn(`${hubUrl.replace(/\/$/, '')}/.well-known/trusted-code-hashes.json`);
      if (!resp.ok) return -1;

      const data = (await resp.json()) as {
        hashes?: Array<{ capability_id?: string; code_hash?: string }>;
      };
      const hashes = data.hashes ?? [];
      for (const entry of hashes) {
        if (entry.capability_id && entry.code_hash) {
          this.hashCache.set(entry.capability_id, entry.code_hash);
          this.trustedCodeHashes.set(entry.capability_id, entry.code_hash);
        }
      }
      this.lastFetch = new Date();
      return hashes.length;
    } catch {
      return -1;
    }
  }

  verifyAttestationDetailed(
    attestation: TeeAttestation,
    capabilityId: string
  ): TeeVerificationResult {
    const failures: string[] = [];

    if (!TeePlatform.isSupported(attestation.platform)) {
      failures.push(`Unsupported TEE platform: ${attestation.platform}`);
    }

    const ts = Date.parse(attestation.timestamp);
    if (Number.isNaN(ts)) {
      failures.push(`Invalid attestation timestamp: ${attestation.timestamp}`);
    } else if (attestationIsExpired(attestation)) {
      const ageS = Math.floor((Date.now() - ts) / 1000);
      failures.push(`Attestation expired (age: ${ageS}s, ttl: ${attestation.ttl_s}s)`);
    }

    if (Object.keys(attestation.pcr_values).length === 0) {
      failures.push('PCR values are empty — attestation lacks hardware proof');
    }

    const expectedHash =
      this.hashCache.get(capabilityId) ?? this.trustedCodeHashes.get(capabilityId);
    if (expectedHash != null && attestation.code_hash !== expectedHash) {
      failures.push(
        `Code hash mismatch: expected ${expectedHash}, got ${attestation.code_hash}`
      );
    }

    const enclaveKey = this.enclavePublicKeys[attestation.platform];
    if (!enclaveKey) {
      failures.push(`No known enclave public key for platform: ${attestation.platform}`);
    } else if (
      !this.signer.verify(enclaveKey, attestation.signature, attestationCanonical(attestation))
    ) {
      failures.push('Enclave signature verification failed');
    }

    return failures.length === 0 ? teeVerificationPass() : teeVerificationFail(failures);
  }

  verifyAttestation(attestation: TeeAttestation, capabilityId: string): boolean {
    return this.verifyAttestationDetailed(attestation, capabilityId).isValid;
  }

  verifyReceipt(receipt: TeeReceipt, expectedInput: string, receivedOutput: string): boolean {
    const inputHash = sha256Hex(expectedInput);
    const outputHash = sha256Hex(receivedOutput);
    if (receipt.input_hash !== inputHash) return false;
    if (receipt.output_hash !== outputHash) return false;
    return true;
  }
}

/** Simulated defaults — production hubs publish keys at `/.well-known/enclave-keys.json`. */
const DEFAULT_ENCLAVE_PUBLIC_KEYS: Record<string, string> = {
  aws_nitro: 'nitro_enclave_pubkey_hex',
  intel_tdx: 'tdx_enclave_pubkey_hex',
  amd_sev: 'sev_enclave_pubkey_hex',
  azure_cc: 'azure_cc_pubkey_hex',
};

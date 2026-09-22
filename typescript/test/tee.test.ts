import { describe, expect, it } from 'vitest';
import {
  MarketSigner,
  TeePlatform,
  TeeVerifier,
  TrustedHashCache,
  attestationCanonical,
  type TeeAttestation,
} from '../src';

const TEST_KEY = '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60';

function sampleAttestation(overrides: Partial<TeeAttestation> = {}): TeeAttestation {
  const timestamp = new Date().toISOString();
  const base: TeeAttestation = {
    platform: 'aws_nitro',
    enclave_id: 'enclave-1',
    code_hash: 'abc123',
    pcr_values: { pcr0: 'pcr0val' },
    instance_id: 'i-123',
    region: 'us-east-1',
    timestamp,
    ttl_s: 300,
    signature: '',
  };
  const att = { ...base, ...overrides };
  const signer = new MarketSigner(TEST_KEY);
  att.signature = signer.signCanonical(attestationCanonical(att));
  return att;
}

describe('TeeVerifier', () => {
  it('passes a valid attestation with trusted code hash', () => {
    const signer = new MarketSigner(TEST_KEY);
    const verifier = new TeeVerifier({
      signer,
      trustedCodeHashes: { 'cap-1': 'abc123' },
      enclavePublicKeys: { aws_nitro: signer.publicKeyHex() },
    });
    const att = sampleAttestation();
    const result = verifier.verifyAttestationDetailed(att, 'cap-1');
    expect(result.isValid).toBe(true);
    expect(result.failures).toHaveLength(0);
  });

  it('fails on unsupported platform', () => {
    const verifier = new TeeVerifier({ signer: new MarketSigner(TEST_KEY) });
    const att = sampleAttestation({ platform: 'unknown_platform' });
    const result = verifier.verifyAttestationDetailed(att, 'cap-1');
    expect(result.isValid).toBe(false);
    expect(result.failures.some((f) => f.includes('Unsupported TEE platform'))).toBe(true);
  });

  it('fails on code hash mismatch', () => {
    const verifier = new TeeVerifier({
      signer: new MarketSigner(TEST_KEY),
      trustedCodeHashes: { 'cap-1': 'expected-hash' },
    });
    const att = sampleAttestation({ code_hash: 'wrong-hash' });
    const result = verifier.verifyAttestationDetailed(att, 'cap-1');
    expect(result.isValid).toBe(false);
    expect(result.failures.some((f) => f.includes('Code hash mismatch'))).toBe(true);
  });

  it('caches trusted hashes with TTL', () => {
    const cache = new TrustedHashCache(1000);
    cache.set('cap-1', 'hash-a');
    expect(cache.get('cap-1')).toBe('hash-a');
    expect(cache.size).toBe(1);
    cache.clear();
    expect(cache.get('cap-1')).toBeUndefined();
  });

  it('lists supported platforms', () => {
    expect(TeePlatform.isSupported('aws_nitro')).toBe(true);
    expect(TeePlatform.isSupported('azure_cc')).toBe(true);
    expect(TeePlatform.isSupported('bogus')).toBe(false);
  });
});

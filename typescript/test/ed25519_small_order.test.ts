import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { MarketSigner } from '../src';
import { hasSmallOrder } from '../src/signer';

const v = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)),
  '../../test-vectors/ed25519_small_order.json'), 'utf8'));
const b64 = (hex: string) => Buffer.from(hex, 'hex').toString('base64');

describe('strict Ed25519 (test-vectors/ed25519_small_order.json)', () => {
  const signer = new MarketSigner(MarketSigner.generateEd25519Keypair().seedHex);
  it('accepts the RFC 8032 TEST 1 vector', () => {
    const t = v.rfc8032Test1;
    expect(signer.verify(t.publicKeyHex, `ed25519:${b64(t.signatureHex)}`, t.message)).toBe(true);
  });
  it('refuses every small-order forgery for every message', () => {
    for (const f of v.forgeries) for (const m of f.messages) {
      expect(signer.verify(f.publicKeyHex, `ed25519:${b64(f.signatureHex)}`, m)).toBe(false);
    }
  });
  it('recognises all eight small-order points', () => {
    for (const p of v.smallOrderPointsHex) expect(hasSmallOrder(Buffer.from(p, 'hex'))).toBe(true);
  });
});

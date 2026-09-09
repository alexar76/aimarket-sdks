import { describe, expect, it } from 'vitest';
import {
  AimarketException,
  AimarketNetworkException,
  AimarketPaymentException,
  AimarketSafetyException,
  market,
} from '../src';
import { MockFetch } from './mockFetch';

const HUB = 'https://independentai.network/hub';

function searchUrl(intent = 'audit my agent'): string {
  const query = new URLSearchParams({
    intent,
    budget: '0.25',
    min_trust: '0.3',
    max_latency_ms: '10000',
    limit: '10',
  });
  return `${HUB}/ai-market/v2/search?${query}`;
}

function searchBody(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    query: 'audit my agent',
    matches: [{
      capability_id: 'aegis.audit@v1',
      product_id: 'aegis',
      name: 'AEGIS Audit',
      description: 'Audits agent execution',
      routed_price_usd: 0.04,
      score: 0.97,
      trust_score: 0.91,
      p50_latency_ms: 230,
      source_hub: 'local',
      ...overrides,
    }],
    protocol_version: 'v2',
  });
}

function invokeBody(): string {
  return JSON.stringify({
    success: true,
    result: { findings: [] },
    price_usd: 0.04,
    latency_ms: 240,
    receipt: {
      receipt_id: 'rcpt_1',
      signature: { algorithm: 'ed25519', value: 'sig', pq_value: 'pq' },
    },
  });
}

function verificationBody(valid = true): string {
  return JSON.stringify({
    valid,
    classical_valid: valid,
    pq_present: true,
    pq_valid: valid,
    algorithm: 'ed25519+ml-dsa-65',
    signature_version: 'v3',
    signer_matches_live_key: valid,
    policy_requires_pq: true,
  });
}

describe('market.run', () => {
  it('searches, invokes once as a trial, and verifies the hybrid receipt', async () => {
    const mock = new MockFetch();
    mock.expectGet(searchUrl(), 200, searchBody());
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 200, invokeBody());
    mock.expectPost(`${HUB}/ai-market/v2/receipts/verify`, 200, verificationBody());

    const result = await market.run({
      intent: 'audit my agent',
      input: { trace: 'safe sample' },
      payment: { kind: 'trial', visitorId: 'visitor-stable' },
      fetch: mock.fetch,
    });

    expect(result.trusted).toBe(true);
    expect(result.selected.capability.capability_id).toBe('aegis.audit@v1');
    expect(result.paymentKind).toBe('trial');
    expect(mock.requestLog).toEqual([
      `GET ${searchUrl()}`,
      `POST ${HUB}/ai-market/v2/invoke`,
      `POST ${HUB}/ai-market/v2/receipts/verify`,
    ]);
    expect(mock.headerLog[1].get('X-AIMarket-Sandbox-Visitor')).toBe('visitor-stable');
    expect(mock.headerLog[1].get('X-AIMarket-Route-Ok')).toBe('1');
    const invoke = JSON.parse(mock.bodyLog[1]);
    expect(invoke).toMatchObject({
      capability_id: 'aegis.audit@v1',
      product_id: 'aegis',
      source_hub: 'local',
      input: { trace: 'safe sample' },
      max_price_usd: 0.25,
    });
  });

  it('sends channel credentials only as headers', async () => {
    const mock = new MockFetch();
    mock.expectGet(searchUrl(), 200, searchBody());
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 200, invokeBody());

    await market.run({
      intent: 'audit my agent',
      input: {},
      payment: { kind: 'channel', channelId: 'channel-1', channelSecret: 'secret-1' },
      receiptPolicy: 'off',
      fetch: mock.fetch,
    });

    expect(mock.headerLog[1].get('X-Payment-Channel')).toBe('channel-1');
    expect(mock.headerLog[1].get('X-Payment-Channel-Secret')).toBe('secret-1');
    expect(mock.bodyLog[1]).not.toContain('secret-1');
    expect(mock.requestLog.join('\n')).not.toContain('secret-1');
  });

  it('refuses a server match that violates the local budget', async () => {
    const mock = new MockFetch();
    mock.expectGet(searchUrl(), 200, searchBody({ routed_price_usd: 99 }));
    await expect(market.run({
      intent: 'audit my agent', input: {}, fetch: mock.fetch,
    })).rejects.toBeInstanceOf(AimarketException);
    expect(mock.requestCount).toBe(1);
  });

  it('fails closed when a match has no explicit price', async () => {
    const mock = new MockFetch();
    mock.expectGet(searchUrl(), 200, searchBody({
      routed_price_usd: undefined,
      price_per_call_usd: undefined,
    }));
    await expect(market.run({
      intent: 'audit my agent', input: {}, fetch: mock.fetch,
    })).rejects.toThrow('No eligible capability');
    expect(mock.requestCount).toBe(1);
  });

  it('rejects non-serializable input before any network request', async () => {
    const mock = new MockFetch();
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    await expect(market.run({
      intent: 'audit my agent', input: cyclic, fetch: mock.fetch,
    })).rejects.toThrow('JSON-serializable');
    expect(mock.requestCount).toBe(0);
  });

  it('fails closed on an invalid receipt', async () => {
    const mock = new MockFetch();
    mock.expectGet(searchUrl(), 200, searchBody());
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 200, invokeBody());
    mock.expectPost(`${HUB}/ai-market/v2/receipts/verify`, 200, verificationBody(false));
    await expect(market.run({
      intent: 'audit my agent', input: {}, fetch: mock.fetch,
    })).rejects.toBeInstanceOf(AimarketSafetyException);
  });

  it('surfaces 402 and never retries a possibly charged invoke', async () => {
    const mock = new MockFetch();
    mock.expectGet(searchUrl(), 200, searchBody());
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 402, JSON.stringify({ error: 'payment_required' }));
    await expect(market.run({
      intent: 'audit my agent', input: {}, fetch: mock.fetch,
    })).rejects.toBeInstanceOf(AimarketPaymentException);
    expect(mock.requestLog.filter((item) => item.includes('/invoke'))).toHaveLength(1);
  });

  it('surfaces an atomic price-limit conflict and never retries invoke', async () => {
    const mock = new MockFetch();
    mock.expectGet(searchUrl(), 200, searchBody());
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 409, JSON.stringify({
      error: 'price_limit_exceeded',
      maximum_price_usd: 0.25,
      current_price_usd: 0.30,
    }));

    const error = await market.run({
      intent: 'audit my agent', input: {}, fetch: mock.fetch,
    }).catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(AimarketException);
    expect((error as AimarketException).statusCode).toBe(409);
    expect((error as Error).message).toContain('search again');
    expect(mock.requestLog.filter((item) => item.includes('/invoke'))).toHaveLength(1);
  });

  it('rejects insecure remote Hub URLs before sending data', async () => {
    await expect(market.run({
      hubUrl: 'http://example.com/hub',
      intent: 'audit my agent',
      input: {},
      fetch: new MockFetch().fetch,
    })).rejects.toThrow('HTTPS');
  });

  it('classifies transport failures as network errors', async () => {
    const failingFetch = (async () => {
      throw new TypeError('offline');
    }) as typeof fetch;
    await expect(market.run({
      intent: 'audit my agent', input: {}, fetch: failingFetch,
    })).rejects.toBeInstanceOf(AimarketNetworkException);
  });
});

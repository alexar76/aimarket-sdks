import { describe, expect, it } from 'vitest';
import {
  AimarketAgent,
  AimarketPaymentException,
  AimarketSafetyException,
  channelBalanceRatio,
  channelIsExpired,
  type Channel,
} from '../src';
import { MockFetch } from './mockFetch';

const HUB = 'https://hub.test';
const WALLET_KEY = '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60';

function futureExpiry(): string {
  return new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString();
}

function sampleChannel(overrides: Partial<Channel> = {}): Channel {
  return {
    channel_id: 'ch_abc123',
    deposit_usd: 5,
    balance_usd: 4.5,
    token: 'USDT',
    chain: 'base',
    expires_at: futureExpiry(),
    ...overrides,
  };
}

function sampleSearchBody() {
  return JSON.stringify({
    results: [
      {
        capability: {
          capability_id: 'ats-rules-2026-q2',
          product_id: 'ats-rules-workday',
          name: 'ATS Rules',
          version: '2.1.0',
          description: 'Latest ATS rules',
          price_per_call_usd: 0.1,
          source_hub: 'https://hub.test',
        },
        relevance_score: 0.95,
        rationale: 'Best match',
      },
    ],
    total: 1,
    hub: HUB,
  });
}

describe('Channel helpers', () => {
  it('computes balance ratio', () => {
    expect(channelBalanceRatio(sampleChannel())).toBeCloseTo(0.9);
    expect(channelBalanceRatio(sampleChannel({ deposit_usd: 0 }))).toBe(0);
  });

  it('detects expired channels', () => {
    expect(channelIsExpired(sampleChannel())).toBe(false);
    expect(channelIsExpired(sampleChannel({ expires_at: '2000-01-01T00:00:00.000Z' }))).toBe(true);
  });
});

describe('AimarketAgent', () => {
  it('discovers capabilities', async () => {
    const mock = new MockFetch();
    mock.expectGet(`${HUB}/ai-market/v2/search?intent=ATS&limit=5`, 200, sampleSearchBody());

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const plan = await agent.discover({ intent: 'ATS' });
    expect(plan).toHaveLength(1);
    expect(plan[0].capability.capability_id).toBe('ats-rules-2026-q2');
  });

  it('reuses cached channel when balance > 50%', async () => {
    const mock = new MockFetch();
    const channelBody = JSON.stringify(sampleChannel());
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 201, channelBody);

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const ch1 = await agent.openChannel(5);
    const ch2 = await agent.openChannel(5);
    expect(ch1.channel_id).toBe(ch2.channel_id);
    expect(mock.requestCount).toBe(1);
  });

  it('opens a new channel when cached channel is expired', async () => {
    const mock = new MockFetch();
    const expired = JSON.stringify(sampleChannel({ expires_at: '2000-01-01T00:00:00.000Z' }));
    const fresh = JSON.stringify(sampleChannel({ channel_id: 'ch_new' }));
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 201, expired);
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 201, fresh);

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    await agent.openChannel(5);
    const ch2 = await agent.openChannel(5);
    expect(ch2.channel_id).toBe('ch_new');
    expect(mock.requestCount).toBe(2);
  });

  it('throws AimarketSafetyException on 403', async () => {
    const mock = new MockFetch();
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      403,
      JSON.stringify({ reason: 'PII detected' })
    );

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    await expect(
      agent.invoke({
        capabilityId: 'cap-1',
        input: {},
        channelId: 'ch_1',
      })
    ).rejects.toBeInstanceOf(AimarketSafetyException);
  });

  it('throws AimarketPaymentException on 402', async () => {
    const mock = new MockFetch();
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 402, '{}');

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    await expect(
      agent.invoke({
        capabilityId: 'cap-1',
        input: {},
        channelId: 'ch_1',
      })
    ).rejects.toBeInstanceOf(AimarketPaymentException);
  });

  it('retries invoke on transient network errors', async () => {
    const mock = new MockFetch();
    mock.failNextPost(2);
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      200,
      JSON.stringify({
        success: true,
        output: { ok: true },
        price_usd: 0.1,
        latency_ms: 12,
        safety_blocked: false,
        tee_verified: false,
      })
    );

    const agent = new AimarketAgent({
      hubUrl: HUB,
      walletKey: WALLET_KEY,
      maxRetries: 3,
      fetch: mock.fetch,
    });
    const result = await agent.invoke({
      capabilityId: 'cap-1',
      input: {},
      channelId: 'ch_1',
    });
    expect(result.success).toBe(true);
    expect(mock.requestLog.filter((r) => r.startsWith('POST')).length).toBe(3);
  });

  it('runs full cycle via runOnce', async () => {
    const mock = new MockFetch();
    mock.expectGet(`${HUB}/ai-market/v2/search?intent=fintech+ATS&limit=5&budget_usd=5`, 200, sampleSearchBody());
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 201, JSON.stringify(sampleChannel()));
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      200,
      JSON.stringify({
        success: true,
        output: { score: 0.9 },
        price_usd: 0.1,
        latency_ms: 50,
        safety_blocked: false,
        tee_verified: true,
      })
    );
    mock.expectPost(
      `${HUB}/ai-market/v2/channel/close`,
      200,
      JSON.stringify({
        channel_id: 'ch_abc123',
        total_spent_usd: 0.1,
        refund_usd: 4.9,
        invocations: 1,
      })
    );

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const bom = await agent.runOnce({ intent: 'fintech ATS', input: { role: 'PM' } });
    expect(bom.protocol_version).toBe('v2');
    expect(bom.results).toHaveLength(1);
    expect(bom.settlement?.refund_usd).toBe(4.9);
  });
});

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

// Mirrors the live hub's GET /ai-market/v2/search: a { query, matches: [...] }
// envelope whose matches are FLAT capability dicts (see web/backend/api/
// ai_market_protocol_v2.py::_cap_to_match).
function sampleSearchBody() {
  return JSON.stringify({
    query: 'ATS',
    matches: [
      {
        capability_id: 'ats-rules-2026-q2',
        product_id: 'ats-rules-workday',
        name: 'ATS Rules',
        description: 'Latest ATS rules',
        price_per_call_usd: 0.1,
        routed_price_usd: 0.1,
        score: 0.95,
        trust_score: 0.9,
        source_hub: 'local',
        source_hub_name: 'AI-Factory',
        status_label: 'Completed',
      },
    ],
    catalog: 'factory',
    protocol_version: 'v2',
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

  it('sends the verify block in the invoke body when set', async () => {
    const mock = new MockFetch();
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      200,
      JSON.stringify({
        success: true,
        result: { ok: true },
        price_usd: 0.1,
        latency_ms: 12,
        verification: { requested: true, status: 'pending', performed: false },
      })
    );

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const result = await agent.invoke({
      capabilityId: 'cap-1',
      input: { text: 'hi' },
      channelId: 'ch_1',
      verify: { requested: true, intent: 'translate hi', mode: 'auto' },
    });

    const body = JSON.parse(mock.bodyLog[0]) as Record<string, unknown>;
    expect(body.verify).toEqual({ requested: true, intent: 'translate hi', mode: 'auto' });
    expect(result.verification).toMatchObject({ requested: true, status: 'pending' });
  });

  it('omits the verify block when not requested', async () => {
    const mock = new MockFetch();
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      200,
      JSON.stringify({ success: true, result: {}, price_usd: 0.1, latency_ms: 12 })
    );

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const result = await agent.invoke({ capabilityId: 'cap-1', input: {}, channelId: 'ch_1' });

    const body = JSON.parse(mock.bodyLog[0]) as Record<string, unknown>;
    expect(body).not.toHaveProperty('verify');
    expect(result.verification).toBeUndefined();
  });

  it('retries invoke on transient network errors', async () => {
    const mock = new MockFetch();
    mock.failNextPost(2);
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      200,
      JSON.stringify({
        success: true,
        result: { ok: true },
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

  it('does NOT retry a verified-wait invoke (no double-charge)', async () => {
    const mock = new MockFetch();
    // First POST fails transiently. A non-verified invoke would retry and
    // succeed; a wait=true invoke must be single-shot so a mid-wait re-POST
    // can't mint a fresh nonce and open a second hold/debit.
    mock.failNextPost(1);
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      200,
      JSON.stringify({ success: true, result: {}, price_usd: 0.1, latency_ms: 12 })
    );

    const agent = new AimarketAgent({
      hubUrl: HUB,
      walletKey: WALLET_KEY,
      maxRetries: 3,
      fetch: mock.fetch,
    });
    await expect(
      agent.invoke({
        capabilityId: 'cap-1',
        input: {},
        channelId: 'ch_1',
        verify: { requested: true, intent: 'translate hi', wait: true, wait_timeout_s: 60 },
      })
    ).rejects.toThrow();
    // Exactly one POST — the failed attempt was not retried.
    expect(mock.requestLog.filter((r) => r.startsWith('POST')).length).toBe(1);
  });

  it('runs full cycle via runOnce', async () => {
    const mock = new MockFetch();
    mock.expectGet(`${HUB}/ai-market/v2/search?intent=fintech+ATS&limit=5&budget=5`, 200, sampleSearchBody());
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 201, JSON.stringify(sampleChannel()));
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      200,
      JSON.stringify({
        success: true,
        result: { score: 0.9 },
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

// The shapes below are what aimarket-hub (every production hub) answers — see
// aimarket-hub/aimarket_hub/channels.py `_insert_channel` and `close`. Its channel names the
// deposit `original_deposit_usd` and is bound to the wallet that paid; its close wraps a
// `settlement` that reports `used_usd`.
const DEPOSIT_TX = '0x8f2a559490d8ff2bbcbf4cb3b0d8c4f1a9e1e5c3b1d1f4a6c8e0b2d4f6a8c0e2';
const PAYER = '0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed';

function hubChannelBody(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    channel: {
      channel_id: 'ch_9f3c2a1b7d4e5f60',
      balance_usd: 5,
      original_deposit_usd: 5,
      used_usd: 0,
      token: 'USDC',
      chain: 'base',
      wallet: PAYER.toLowerCase(),
      tx_hash: DEPOSIT_TX,
      recipient: '0x1111111111111111111111111111111111111111',
      verify_stub: false,
      status: 'open',
      opened_at: new Date().toISOString(),
      expires_at: futureExpiry(),
      channel_secret: 'sec_Qm9vYmFyLXNlY3JldA',
      ...overrides,
    },
  });
}

const PAID_INVOKE = JSON.stringify({
  success: true,
  result: { ok: true },
  price_usd: 0.01,
  latency_ms: 20,
});

describe('Paying on a production hub', () => {
  it('pays with the credit account: X-API-Key is sent and no channel is opened', async () => {
    const mock = new MockFetch();
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 200, PAID_INVOKE);

    const agent = new AimarketAgent({
      hubUrl: HUB,
      walletKey: WALLET_KEY,
      apiKey: 'ak_live_test_key',
      fetch: mock.fetch,
    });
    const result = await agent.invoke({
      capabilityId: 'cap-1',
      productId: 'prod-1',
      input: { text: 'hi' },
    });

    expect(result.success).toBe(true);
    expect(mock.requestLog).toEqual([`POST ${HUB}/ai-market/v2/invoke`]);
    const headers = mock.headerLog[0];
    expect(headers.get('X-API-Key')).toBe('ak_live_test_key');
    expect(headers.get('X-Payment-Channel')).toBeNull();
    expect(headers.get('X-Payment-Channel-Secret')).toBeNull();
    expect(headers.get('X-Market-Signature')).toBeNull();
  });

  it('lets the key win over a channel id, as the Python agent does', async () => {
    const mock = new MockFetch();
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 200, PAID_INVOKE);

    const agent = new AimarketAgent({
      hubUrl: HUB,
      walletKey: WALLET_KEY,
      apiKey: 'ak_live_test_key',
      fetch: mock.fetch,
    });
    await agent.invoke({ capabilityId: 'cap-1', input: {}, channelId: 'ch_stale' });

    expect(mock.headerLog[0].get('X-API-Key')).toBe('ak_live_test_key');
    expect(mock.headerLog[0].get('X-Payment-Channel')).toBeNull();
  });

  it('runOnce with a key neither opens nor closes a channel', async () => {
    const mock = new MockFetch();
    mock.expectGet(`${HUB}/ai-market/v2/search?intent=fintech+ATS&limit=5&budget=5`, 200, sampleSearchBody());
    mock.expectPost(`${HUB}/ai-market/v2/invoke`, 200, PAID_INVOKE);

    const agent = new AimarketAgent({
      hubUrl: HUB,
      walletKey: WALLET_KEY,
      apiKey: 'ak_live_test_key',
      fetch: mock.fetch,
    });
    const bom = await agent.runOnce({ intent: 'fintech ATS', input: { role: 'PM' } });

    expect(mock.requestLog.some((r) => r.includes('/channel/'))).toBe(false);
    expect(mock.headerLog[1].get('X-API-Key')).toBe('ak_live_test_key');
    expect(bom.settlement).toBeUndefined();
    expect(bom.results[0].success).toBe(true);
  });

  it('reports what the hub said when the credit account cannot pay', async () => {
    const mock = new MockFetch();
    mock.expectPost(
      `${HUB}/ai-market/v2/invoke`,
      402,
      JSON.stringify({
        success: false,
        error: 'payment_required',
        detail: 'insufficient credit balance',
        needed: 0.01,
        balance: 0,
      })
    );

    const agent = new AimarketAgent({
      hubUrl: HUB,
      walletKey: WALLET_KEY,
      apiKey: 'ak_empty_account',
      fetch: mock.fetch,
    });
    const call = agent.invoke({ capabilityId: 'cap-1', input: {} });
    await expect(call).rejects.toBeInstanceOf(AimarketPaymentException);
    await expect(call).rejects.toThrow(/insufficient credit balance/);
    // The balance and the uncovered price ride along, whatever the prose says.
    await expect(call).rejects.toThrow('(balance $0, needed $0.01)');
  });

  it('refuses an invoke with neither a channel nor a key, before sending anything', async () => {
    const mock = new MockFetch();
    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });

    await expect(agent.invoke({ capabilityId: 'cap-1', input: {} })).rejects.toBeInstanceOf(
      AimarketPaymentException
    );
    expect(mock.requestCount).toBe(0);
  });

  it('sends the deposit tx hash and payer proof when opening a channel', async () => {
    const mock = new MockFetch();
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 200, hubChannelBody());

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const channel = await agent.openChannel(5, 'USDC', 'base', {
      txHash: DEPOSIT_TX,
      payerSignature: '0xpayerproof',
      wallet: PAYER,
    });

    const body = JSON.parse(mock.bodyLog[0]) as Record<string, unknown>;
    expect(body).toEqual({
      deposit_usd: 5,
      token: 'USDC',
      chain: 'base',
      tx_hash: DEPOSIT_TX,
      payer_signature: '0xpayerproof',
      wallet: PAYER,
    });
    // The hub's `original_deposit_usd` becomes the model's deposit, so the balance ratio is real.
    expect(channel.deposit_usd).toBe(5);
    expect(channelBalanceRatio(channel)).toBe(1);
  });

  it('opens a channel without tx_hash when no deposit is given (dev hubs)', async () => {
    const mock = new MockFetch();
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 201, JSON.stringify(sampleChannel()));

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    await agent.openChannel(5);

    expect(JSON.parse(mock.bodyLog[0])).toEqual({ deposit_usd: 5, token: 'USDT', chain: 'base' });
  });

  it('answers a repeated deposit with its own channel and a new deposit with a new one', async () => {
    const mock = new MockFetch();
    const secondTx = `0x${'ab'.repeat(32)}`;
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 200, hubChannelBody());
    mock.expectPost(
      `${HUB}/ai-market/v2/channel/open`,
      200,
      hubChannelBody({ channel_id: 'ch_second_deposit', tx_hash: secondTx })
    );

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const first = await agent.openChannel(5, 'USDC', 'base', { txHash: DEPOSIT_TX });
    // Same deposit, spelled in upper case: the transaction is the same one.
    const again = await agent.openChannel(5, 'USDC', 'base', { txHash: DEPOSIT_TX.toUpperCase().replace('0X', '0x') });
    const second = await agent.openChannel(5, 'USDC', 'base', { txHash: secondTx });

    expect(again.channel_id).toBe(first.channel_id);
    expect(second.channel_id).toBe('ch_second_deposit');
    expect(mock.requestCount).toBe(2);
    expect(JSON.parse(mock.bodyLog[1]).tx_hash).toBe(secondTx);
  });

  it('refuses a deposit with no tx hash before sending anything', async () => {
    const mock = new MockFetch();
    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });

    await expect(agent.openChannel(5, 'USDC', 'base', { txHash: '  ' })).rejects.toThrow(/txHash/);
    expect(mock.requestCount).toBe(0);
  });

  it('closes a deposit-funded channel with its secret and wallet, reading the hub settlement', async () => {
    const mock = new MockFetch();
    mock.expectPost(`${HUB}/ai-market/v2/channel/open`, 200, hubChannelBody());
    mock.expectPost(
      `${HUB}/ai-market/v2/channel/close`,
      200,
      JSON.stringify({
        settlement: {
          channel_id: 'ch_9f3c2a1b7d4e5f60',
          used_usd: 0.01,
          refund_usd: 4.99,
          refund_executed_usd: 0,
          refund_owed_usd: 4.99,
          refund_status: 'owed',
          original_deposit_usd: 5,
          status: 'settled',
          settle_tx_hash: '',
        },
      })
    );

    const agent = new AimarketAgent({ hubUrl: HUB, walletKey: WALLET_KEY, fetch: mock.fetch });
    const channel = await agent.openChannel(5, 'USDC', 'base', { txHash: DEPOSIT_TX });
    const settlement = await agent.closeChannel(channel.channel_id);

    expect(mock.headerLog[1].get('X-Payment-Channel-Secret')).toBe('sec_Qm9vYmFyLXNlY3JldA');
    expect(JSON.parse(mock.bodyLog[1])).toEqual({
      channel_id: 'ch_9f3c2a1b7d4e5f60',
      wallet: PAYER.toLowerCase(),
    });
    expect(settlement.channel_id).toBe('ch_9f3c2a1b7d4e5f60');
    expect(settlement.total_spent_usd).toBe(0.01);
    expect(settlement.refund_usd).toBe(4.99);
  });
});

/**
 * AI Market Protocol v2 Consumer Agent (TypeScript).
 *
 * Implements the 5-phase consumer cycle:
 *   1. Discovery  — fetch well-known + search
 *   2. Channel    — open pre-funded payment channel
 *   3. Invoke     — call capability with payment header
 *   4. Settle     — close channel, get refund
 *   5. Verify     — TEE attestation check
 *
 * Target: Electron desktop apps, Node.js servers, web apps.
 */

import {
  AimarketException,
  AimarketNetworkException,
  AimarketPaymentException,
  AimarketSafetyException,
} from './errors';
import {
  type BillOfMaterials,
  type Channel,
  type InvokeResult,
  type PlanStep,
  type SearchResponse,
  type Settlement,
  type TeeAttestation,
  type TeeReceipt,
  channelBalanceRatio,
  channelIsExpired,
} from './models';
import { MarketSigner } from './signer';
import { TeeVerifier } from './tee';

/** Configuration for [AimarketAgent] behavior. */
export interface AimarketAgentConfig {
  hubUrl: string;
  walletKey: string;
  affiliate: string;
  timeoutMs: number;
  maxRetries: number;
  trustedCodeHashes?: Record<string, string>;
  verifyTee: boolean;
}

class CachedChannel {
  constructor(
    readonly channel: Channel,
    readonly cachedAt: Date
  ) {}

  get hasSufficientBalance(): boolean {
    return channelBalanceRatio(this.channel) > 0.5;
  }

  get isNotExpired(): boolean {
    return !channelIsExpired(this.channel);
  }

  get isReusable(): boolean {
    return this.isNotExpired && this.hasSufficientBalance;
  }
}

export interface AimarketAgentOptions {
  hubUrl: string;
  walletKey: string;
  affiliate?: string;
  trustedCodeHashes?: Record<string, string>;
  timeoutMs?: number;
  maxRetries?: number;
  verifyTee?: boolean;
  /** Injectable fetch for tests. */
  fetch?: typeof fetch;
}

export class AimarketAgent {
  private readonly config: AimarketAgentConfig;
  private readonly signer: MarketSigner;
  private readonly teeVerifier: TeeVerifier;
  private readonly fetchFn: typeof fetch;
  private readonly channelCache = new Map<string, CachedChannel>();
  private wellKnownCache: string | null = null;

  constructor(opts: AimarketAgentOptions) {
    this.config = {
      hubUrl: opts.hubUrl.replace(/\/$/, ''),
      walletKey: opts.walletKey,
      affiliate: opts.affiliate ?? 'aimarket-sdk-ts',
      timeoutMs: opts.timeoutMs ?? 30_000,
      maxRetries: opts.maxRetries ?? 3,
      trustedCodeHashes: opts.trustedCodeHashes,
      verifyTee: opts.verifyTee ?? true,
    };
    this.signer = new MarketSigner(this.config.walletKey);
    this.teeVerifier = new TeeVerifier({
      signer: this.signer,
      trustedCodeHashes: this.config.trustedCodeHashes,
    });
    this.fetchFn = opts.fetch ?? fetch;
  }

  private async retryWithBackoff<T>(operation: () => Promise<T>): Promise<T> {
    let lastError = new AimarketNetworkException(
      `Request failed after ${this.config.maxRetries} retries`
    );

    for (let attempt = 0; attempt <= this.config.maxRetries; attempt++) {
      try {
        return await operation();
      } catch (e) {
        if (e instanceof AimarketNetworkException) {
          lastError = e;
        } else if (e instanceof Error && e.name === 'AbortError') {
          lastError = new AimarketNetworkException(`Request timed out: ${e.message}`);
        } else if (
          e instanceof TypeError ||
          (e instanceof Error && e.message.includes('fetch'))
        ) {
          lastError = new AimarketNetworkException(`Network error: ${e.message}`);
        } else {
          throw e;
        }
      }

      if (attempt < this.config.maxRetries) {
        await new Promise((resolve) => setTimeout(resolve, 1000 * 2 ** attempt));
      }
    }

    throw lastError;
  }

  private async fetchWithTimeout(
    input: RequestInfo | URL,
    init?: RequestInit
  ): Promise<Response> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.config.timeoutMs);
    try {
      return await this.fetchFn(input, { ...init, signal: controller.signal });
    } finally {
      clearTimeout(timer);
    }
  }

  // ── Phase 1: Discovery ────────────────────────────────────────

  async wellKnown(): Promise<string> {
    if (this.wellKnownCache) return this.wellKnownCache;
    const resp = await this.fetchWithTimeout(`${this.config.hubUrl}/.well-known/ai-market.json`);
    if (!resp.ok) {
      throw new AimarketException(`Failed to fetch well-known: ${resp.status}`);
    }
    this.wellKnownCache = await resp.text();
    return this.wellKnownCache;
  }

  async discover(opts: {
    intent: string;
    budget?: number;
    limit?: number;
    category?: string;
  }): Promise<PlanStep[]> {
    const params = new URLSearchParams({
      intent: opts.intent,
      limit: String(opts.limit ?? 5),
    });
    if (opts.budget !== undefined) params.set('budget_usd', String(opts.budget));
    if (opts.category) params.set('category', opts.category);

    const resp = await this.fetchWithTimeout(
      `${this.config.hubUrl}/ai-market/v2/search?${params}`,
      { headers: { 'X-AIMarket-Affiliate': this.config.affiliate } }
    );
    if (!resp.ok) {
      throw new AimarketException(`Discovery failed: ${resp.status} ${await resp.text()}`);
    }
    const data: SearchResponse = await resp.json();
    return data.results;
  }

  async discoverProduct(productId: string): Promise<PlanStep[]> {
    return this.discover({ intent: `product:${productId}` });
  }

  // ── Phase 2: Channel Open ─────────────────────────────────────

  async openChannel(depositUsd: number, token = 'USDT', chain = 'base'): Promise<Channel> {
    const cacheKey = `${depositUsd}:${token}:${chain}`;
    const cached = this.channelCache.get(cacheKey);

    if (cached?.isReusable) {
      return cached.channel;
    }
    if (cached) {
      this.channelCache.delete(cacheKey);
    }

    const resp = await this.fetchWithTimeout(`${this.config.hubUrl}/ai-market/v2/channel/open`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-AIMarket-Affiliate': this.config.affiliate,
      },
      body: JSON.stringify({ deposit_usd: depositUsd, token, chain }),
    });

    if (resp.status === 404) {
      throw new AimarketException('Payment channels not available on this hub');
    }
    if (resp.status !== 200 && resp.status !== 201) {
      throw new AimarketException(`Channel open failed: ${resp.status} ${await resp.text()}`);
    }

    // The hub wraps the channel in a `{ channel: {...} }` envelope (matching the
    // Python agent + live hub); unwrap it, tolerating a bare object for forward-compat.
    const raw = await resp.json();
    const channel: Channel = raw && raw.channel ? raw.channel : raw;
    this.channelCache.set(cacheKey, new CachedChannel(channel, new Date()));
    return channel;
  }

  async getChannelBalance(channelId: string): Promise<number> {
    const resp = await this.fetchWithTimeout(
      `${this.config.hubUrl}/ai-market/v2/channel/${channelId}`,
      { headers: { 'X-AIMarket-Affiliate': this.config.affiliate } }
    );
    if (!resp.ok) {
      throw new AimarketException(`Failed to get channel balance: ${resp.status}`);
    }
    const data = (await resp.json()) as { balance_usd?: number };
    return data.balance_usd ?? 0;
  }

  // ── Phase 3: Invoke ───────────────────────────────────────────

  async invoke(opts: {
    capabilityId: string;
    input: Record<string, unknown>;
    channelId: string;
    productId?: string;
    sourceHub?: string;
    verifyTee?: boolean;
    /**
     * TEE attestation for the target capability, obtained out-of-band (e.g.
     * from the hub manifest or a prior invocation's `tee_attestation`). When
     * supplied and TEE verification is enabled, it is verified BEFORE any input
     * is transmitted; an invalid attestation aborts the call so sensitive input
     * never reaches an unverified enclave.
     */
    attestation?: TeeAttestation;
  }): Promise<InvokeResult> {
    return this.retryWithBackoff(() => this.invokeOnce(opts));
  }

  private async invokeOnce(opts: {
    capabilityId: string;
    input: Record<string, unknown>;
    channelId: string;
    productId?: string;
    sourceHub?: string;
    verifyTee?: boolean;
    attestation?: TeeAttestation;
  }): Promise<InvokeResult> {
    const verifyTee = opts.verifyTee ?? true;
    if (verifyTee && this.config.verifyTee && opts.attestation) {
      // Phase 5 pre-check: verify the enclave attestation BEFORE sending input,
      // so user data never reaches a capability whose code hash / signature
      // can't be trusted. Fail closed — a bad attestation aborts the call.
      const verdict = this.teeVerifier.verifyAttestationDetailed(
        opts.attestation,
        opts.capabilityId
      );
      if (!verdict.isValid) {
        throw new AimarketSafetyException(
          `TEE attestation verification failed for ${opts.capabilityId}: ` +
            verdict.failures.join('; ')
        );
      }
    }

    const headers = this.signer.signedHeaders({
      channelId: opts.channelId,
      capabilityId: opts.capabilityId,
      affiliate: this.config.affiliate,
    });
    headers['Content-Type'] = 'application/json';

    const body: Record<string, unknown> = {
      capability_id: opts.capabilityId,
      input: opts.input,
    };
    if (opts.productId) body.product_id = opts.productId;
    if (opts.sourceHub) body.source_hub = opts.sourceHub;

    const startMs = performance.now();
    const resp = await this.fetchWithTimeout(`${this.config.hubUrl}/ai-market/v2/invoke`, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    });
    const latencyMs = performance.now() - startMs;

    if (resp.status === 403) {
      const data = (await resp.json()) as { reason?: string };
      throw new AimarketSafetyException(data.reason ?? 'Blocked by safety gate');
    }
    if (resp.status === 402) {
      throw new AimarketPaymentException('Channel depleted or expired — open a new channel');
    }
    if (!resp.ok) {
      return {
        success: false,
        price_usd: 0,
        latency_ms: latencyMs,
        safety_blocked: false,
        tee_verified: false,
        error: `HTTP ${resp.status}: ${await resp.text()}`,
      };
    }

    return resp.json();
  }

  async invokeBatch(opts: {
    capabilityIds: string[];
    inputs: Array<Record<string, unknown>>;
    channelId: string;
    sourceHub?: string;
  }): Promise<InvokeResult[]> {
    if (opts.capabilityIds.length !== opts.inputs.length) {
      throw new Error('capabilityIds and inputs must have the same length');
    }

    return Promise.all(
      opts.capabilityIds.map((capabilityId, i) =>
        this.invoke({
          capabilityId,
          input: opts.inputs[i],
          channelId: opts.channelId,
          sourceHub: opts.sourceHub,
        })
      )
    );
  }

  // ── Phase 4: Settle ───────────────────────────────────────────

  async closeChannel(channelId: string): Promise<Settlement> {
    for (const [key, cached] of this.channelCache) {
      if (cached.channel.channel_id === channelId) {
        this.channelCache.delete(key);
        break;
      }
    }

    const resp = await this.fetchWithTimeout(`${this.config.hubUrl}/ai-market/v2/channel/close`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-AIMarket-Affiliate': this.config.affiliate,
      },
      body: JSON.stringify({ channel_id: channelId }),
    });

    if (resp.status === 404) {
      throw new AimarketException(`Channel not found: ${channelId}`);
    }
    if (!resp.ok) {
      throw new AimarketException(`Settlement failed: ${resp.status} ${await resp.text()}`);
    }
    return resp.json();
  }

  // ── Phase 5: Verify ───────────────────────────────────────────

  verifyTeeAttestation(attestation: TeeAttestation, capabilityId: string): boolean {
    return this.teeVerifier.verifyAttestation(attestation, capabilityId);
  }

  verifyTeeReceipt(receipt: TeeReceipt, sentInput: string, receivedOutput: string): boolean {
    return this.teeVerifier.verifyReceipt(receipt, sentInput, receivedOutput);
  }

  trustCodeHash(capabilityId: string, codeHash: string): void {
    this.teeVerifier.trustCodeHash(capabilityId, codeHash);
  }

  async fetchTrustedHashes(): Promise<number> {
    return this.teeVerifier.fetchTrustedHashes(this.config.hubUrl, this.fetchFn);
  }

  // ── Full cycle ────────────────────────────────────────────────

  async runOnce(opts: {
    intent: string;
    input: Record<string, unknown>;
    depositUsd?: number;
    category?: string;
  }): Promise<BillOfMaterials> {
    const depositUsd = opts.depositUsd ?? 5.0;

    const plan = await this.discover({
      intent: opts.intent,
      budget: depositUsd,
      category: opts.category,
    });
    if (plan.length === 0) {
      throw new AimarketException(`No capabilities found for: ${opts.intent}`);
    }

    const channel = await this.openChannel(depositUsd);
    const step = plan[0];

    const result = await this.invoke({
      capabilityId: step.capability.capability_id,
      input: opts.input,
      channelId: channel.channel_id,
      productId: step.capability.product_id,
      sourceHub: step.capability.source_hub,
    });

    const settlement = await this.closeChannel(channel.channel_id);

    return {
      task: opts.intent,
      plan,
      results: [result],
      settlement,
      total_spent_usd: result.price_usd,
      protocol_version: 'v2',
    };
  }

  dispose(): void {
    this.channelCache.clear();
    this.wellKnownCache = null;
  }
}

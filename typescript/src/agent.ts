/**
 * AI Market Protocol v2 Consumer Agent (TypeScript).
 *
 * Implements the 5-phase consumer cycle:
 *   1. Discovery  — fetch well-known + search
 *   2. Channel    — open pre-funded payment channel (skipped with an `apiKey`)
 *   3. Invoke     — call capability with payment header
 *   4. Settle     — close channel, get refund
 *   5. Verify     — TEE attestation check
 *
 * Two ways to pay, as in the Python reference agent: a credit account (`apiKey`, sent
 * as `X-API-Key`) or, when no key is set, a payment channel funded by an on-chain
 * deposit.
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
  searchMatchToPlanStep,
} from './models';
import { canonicalProofTxHash } from './payerProof';
import { MarketSigner } from './signer';
import { TeeVerifier } from './tee';

/** Configuration for [AimarketAgent] behavior. */
export interface AimarketAgentConfig {
  hubUrl: string;
  walletKey: string;
  affiliate: string;
  timeoutMs: number;
  /** Retained for configuration compatibility; paid invokes are always single-shot. */
  maxRetries: number;
  trustedCodeHashes?: Record<string, string>;
  verifyTee: boolean;
  /**
   * Key of a credit account on the hub (credits rail). When set, every invoke is paid by
   * that account and sent as `X-API-Key`; no channel is opened or needed.
   */
  apiKey?: string;
}

/**
 * The on-chain deposit behind a channel.
 *
 * Production hubs open a channel only against a deposit they can verify on chain: without
 * `txHash` they answer `400 tx_hash is required for on-chain verification`, and without
 * `payerSignature` they refuse with "missing or invalid payer proof" and hand back the
 * text to sign.
 */
export interface ChannelDeposit {
  /** Hash of the transfer that paid the hub's settlement wallet. */
  txHash: string;
  /**
   * The paying wallet's EIP-191 signature over
   * `channelOpenProofMessage({ chain, txHash, payer, amountUsd })` — see `signChannelOpenProof`.
   */
  payerSignature?: string;
  /** The paying wallet. Optional: the hub binds the channel to the transfer's sender anyway. */
  wallet?: string;
}

/** The hub's own words for a refusal: `detail`, else `error`, else the raw body (bounded). */
async function refusalDetail(resp: Response): Promise<string> {
  const text = await resp.text().catch(() => '');
  try {
    const data = JSON.parse(text) as {
      detail?: unknown;
      error?: unknown;
      balance?: unknown;
      needed?: unknown;
    };
    const said = data.detail ?? data.error;
    if (typeof said === 'string' && said) {
      // A credits refusal also names the balance and the price it could not cover — the
      // part that says "top up", whatever the prose above it says.
      const money =
        typeof data.balance === 'number' && typeof data.needed === 'number'
          ? ` (balance $${data.balance}, needed $${data.needed})`
          : '';
      return said + money;
    }
  } catch {
    // Not JSON: fall back to the raw body.
  }
  return text.slice(0, 300) || `HTTP ${resp.status}`;
}

class CachedChannel {
  constructor(
    readonly channel: Channel,
    readonly cachedAt: Date,
    /** Canonical hash of the deposit that funded the channel, when one was given. */
    readonly txHash?: string
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
  /**
   * Credit-account key (from `POST /ai-market/v2/accounts`, or issued by the hub's
   * operator). When set, invokes are paid by the account and need no channel.
   */
  apiKey?: string;
  /** Injectable fetch for tests. */
  fetch?: typeof fetch;
}

export class AimarketAgent {
  private readonly config: AimarketAgentConfig;
  private readonly signer: MarketSigner;
  private readonly teeVerifier: TeeVerifier;
  private readonly fetchFn: typeof fetch;
  private readonly channelCache = new Map<string, CachedChannel>();
  // channelId -> one-time debit secret, captured at open and sent on every invoke.
  private readonly channelSecrets = new Map<string, string>();
  // channelId -> wallet the hub bound the channel to; closing an owned channel needs it.
  private readonly channelWallets = new Map<string, string>();
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
      apiKey: opts.apiKey?.trim() || undefined,
    };
    this.signer = new MarketSigner(this.config.walletKey);
    this.teeVerifier = new TeeVerifier({
      signer: this.signer,
      trustedCodeHashes: this.config.trustedCodeHashes,
    });
    this.fetchFn = opts.fetch ?? fetch;
  }

  private async requestOnce<T>(operation: () => Promise<T>): Promise<T> {
    try {
      return await operation();
    } catch (e) {
      if (e instanceof AimarketNetworkException) throw e;
      if (e instanceof Error && e.name === 'AbortError') {
        throw new AimarketNetworkException(`Request timed out: ${e.message}`);
      }
      if (e instanceof TypeError || (e instanceof Error && e.message.includes('fetch'))) {
        throw new AimarketNetworkException(`Network error: ${e.message}`);
      }
      throw e;
    }
  }

  private async fetchWithTimeout(
    input: RequestInfo | URL,
    init?: RequestInit,
    timeoutMs?: number
  ): Promise<Response> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs ?? this.config.timeoutMs);
    try {
      return await this.fetchFn(input, { ...init, redirect: 'error', signal: controller.signal });
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
    // Hub query param is `budget` (float), not `budget_usd` — the wrong name was
    // silently ignored, disabling the price cap.
    if (opts.budget !== undefined) params.set('budget', String(opts.budget));
    if (opts.category) params.set('category', opts.category);

    const resp = await this.fetchWithTimeout(
      `${this.config.hubUrl}/ai-market/v2/search?${params}`,
      { headers: { 'X-AIMarket-Affiliate': this.config.affiliate } }
    );
    if (!resp.ok) {
      throw new AimarketException(`Discovery failed: ${resp.status} ${await resp.text()}`);
    }
    // Hub returns { query, matches: [flat capability dicts], catalog }. Map each
    // flat match to a PlanStep — reading `results` (absent) yielded undefined.
    const data: SearchResponse = await resp.json();
    return (data.matches ?? []).map(searchMatchToPlanStep);
  }

  async discoverProduct(productId: string): Promise<PlanStep[]> {
    return this.discover({ intent: `product:${productId}` });
  }

  // ── Phase 2: Channel Open ─────────────────────────────────────

  /**
   * Open a payment channel funded by `depositUsd`.
   *
   * Production hubs need `deposit` — the on-chain transfer the caller already made (see
   * {@link ChannelDeposit}); only a dev/demo hub opens a channel without one. A deposit
   * funds exactly one channel, so asking again with the same `txHash` returns the channel
   * it opened instead of re-sending it.
   */
  async openChannel(
    depositUsd: number,
    token = 'USDT',
    chain = 'base',
    deposit?: ChannelDeposit
  ): Promise<Channel> {
    const txHash = deposit?.txHash?.trim() ?? '';
    if (deposit && !txHash) {
      throw new AimarketException('deposit.txHash must name the deposit transaction');
    }
    const fundedBy = txHash ? canonicalProofTxHash(txHash) : undefined;
    const cacheKey = `${depositUsd}:${token}:${chain}`;
    const cached = this.channelCache.get(cacheKey);

    // Without a deposit, reuse a channel that still has most of its balance. With one,
    // only the channel THAT deposit opened will do: answering a fresh transfer with an
    // older channel would leave the caller's new money uncredited.
    if (cached && (fundedBy ? cached.txHash === fundedBy && cached.isNotExpired : cached.isReusable)) {
      return cached.channel;
    }
    if (cached) {
      this.channelCache.delete(cacheKey);
    }

    const body: Record<string, unknown> = { deposit_usd: depositUsd, token, chain };
    if (txHash) body.tx_hash = txHash;
    if (deposit?.payerSignature) body.payer_signature = deposit.payerSignature;
    if (deposit?.wallet) body.wallet = deposit.wallet;

    const resp = await this.fetchWithTimeout(`${this.config.hubUrl}/ai-market/v2/channel/open`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-AIMarket-Affiliate': this.config.affiliate,
      },
      body: JSON.stringify(body),
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
    const opened = raw && raw.channel ? raw.channel : raw;
    // aimarket-hub reports the deposit as `original_deposit_usd`; without `deposit_usd`
    // the balance ratio is NaN and the channel is never reused.
    const channel: Channel = {
      ...opened,
      deposit_usd: opened.deposit_usd ?? opened.original_deposit_usd ?? depositUsd,
    };
    // Capture the one-time debit secret so invoke() can present it. The hub
    // requires X-Payment-Channel-Secret on every debit for secure channels;
    // dropping it makes every paid invoke fail with 402.
    if (channel.channel_secret) {
      this.channelSecrets.set(channel.channel_id, channel.channel_secret);
    }
    // A deposit-funded channel belongs to the wallet that paid; closing it needs that wallet.
    const owner = typeof opened.wallet === 'string' && opened.wallet ? opened.wallet : deposit?.wallet;
    if (owner) {
      this.channelWallets.set(channel.channel_id, owner);
    }
    this.channelCache.set(cacheKey, new CachedChannel(channel, new Date(), fundedBy));
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
    /**
     * Channel that pays for the call. Not needed (and not sent) when the agent has an
     * `apiKey`: the credit account pays then, as in the Python agent.
     */
    channelId?: string;
    productId?: string;
    sourceHub?: string;
    verifyTee?: boolean;
    /**
     * One-time channel debit secret. Optional — if omitted, the secret captured
     * at openChannel for this channelId is used automatically.
     */
    channelSecret?: string;
    /**
     * TEE attestation for the target capability, obtained out-of-band (e.g.
     * from the hub manifest or a prior invocation's `tee_attestation`). When
     * supplied and TEE verification is enabled, it is verified BEFORE any input
     * is transmitted; an invalid attestation aborts the call so sensitive input
     * never reaches an unverified enclave.
     */
    attestation?: TeeAttestation;
    /**
     * Pay-on-Verified opt-in, sent verbatim as the `verify` request block:
     * `{ requested: true, intent, mode?, wait?, wait_timeout_s? }`. The hub
     * escrows the debit until Metis judges the output against `intent`; the
     * verdict envelope comes back as `InvokeResult.verification`.
     */
    verify?: Record<string, unknown>;
  }): Promise<InvokeResult> {
    if (!this.config.apiKey && !opts.channelId) {
      throw new AimarketPaymentException(
        'invoke needs a way to pay: pass channelId (from openChannel) or create the agent with an apiKey'
      );
    }
    // A lost response cannot tell us whether this POST was charged. The
    // protocol does not require idempotency support, so never replay it.
    return this.requestOnce(() => this.invokeOnce(opts));
  }

  /** Payment headers: the credit account when there is a key, the channel otherwise. */
  private paymentHeaders(
    capabilityId: string,
    channelId?: string,
    channelSecret?: string
  ): Record<string, string> {
    if (this.config.apiKey) {
      // Mirrors the Python agent: with a key, no channel header rides along.
      return {
        'X-API-Key': this.config.apiKey,
        'X-AIMarket-Affiliate': this.config.affiliate,
      };
    }
    const id = channelId ?? '';
    const headers = this.signer.signedHeaders({
      channelId: id,
      capabilityId,
      affiliate: this.config.affiliate,
    });
    // Present the one-time debit secret (required by secure channels).
    const secret = channelSecret ?? this.channelSecrets.get(id);
    if (secret) {
      headers['X-Payment-Channel-Secret'] = secret;
    }
    return headers;
  }

  private async invokeOnce(opts: {
    capabilityId: string;
    input: Record<string, unknown>;
    channelId?: string;
    productId?: string;
    sourceHub?: string;
    verifyTee?: boolean;
    channelSecret?: string;
    attestation?: TeeAttestation;
    verify?: Record<string, unknown>;
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

    const headers = this.paymentHeaders(opts.capabilityId, opts.channelId, opts.channelSecret);
    headers['Content-Type'] = 'application/json';

    const body: Record<string, unknown> = {
      capability_id: opts.capabilityId,
      input: opts.input,
    };
    if (opts.productId) body.product_id = opts.productId;
    if (opts.sourceHub) body.source_hub = opts.sourceHub;
    if (opts.verify) body.verify = opts.verify;

    // When the caller opts into verify.wait, the hub holds this response open
    // until Metis returns a verdict (up to wait_timeout_s). Extend this single
    // request's timeout past that bound (+30s slack) so the client does not
    // abort mid-wait — an abort would otherwise be retried and re-POST a fresh
    // nonce, creating a second hold/debit (the buyer pays twice).
    const effectiveTimeoutMs = opts.verify?.wait
      ? (Number(opts.verify?.wait_timeout_s) || 300) * 1000 + 30_000
      : undefined;

    const startMs = performance.now();
    const resp = await this.fetchWithTimeout(
      `${this.config.hubUrl}/ai-market/v2/invoke`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
      },
      effectiveTimeoutMs
    );
    const latencyMs = performance.now() - startMs;

    if (resp.status === 403) {
      const data = (await resp.json()) as { reason?: string };
      throw new AimarketSafetyException(data.reason ?? 'Blocked by safety gate');
    }
    if (resp.status === 402) {
      throw new AimarketPaymentException(
        this.config.apiKey
          ? `The credit account cannot pay for this call: ${await refusalDetail(resp)}`
          : `Channel depleted or expired — open a new channel (${await refusalDetail(resp)})`
      );
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
    /** Shared channel; omit it when the agent pays with an `apiKey`. */
    channelId?: string;
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

    // A secured channel closes only with its debit secret, and an owned one only for the
    // wallet that funded it — the same proof invoke presents, or anyone could close it.
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'X-AIMarket-Affiliate': this.config.affiliate,
    };
    const secret = this.channelSecrets.get(channelId);
    if (secret) {
      headers['X-Payment-Channel-Secret'] = secret;
    }
    const body: Record<string, unknown> = { channel_id: channelId };
    const wallet = this.channelWallets.get(channelId);
    if (wallet) {
      body.wallet = wallet;
    }

    const resp = await this.fetchWithTimeout(`${this.config.hubUrl}/ai-market/v2/channel/close`, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    });

    if (resp.status === 404) {
      throw new AimarketException(`Channel not found: ${channelId}`);
    }
    if (!resp.ok) {
      throw new AimarketException(`Settlement failed: ${resp.status} ${await resp.text()}`);
    }
    this.channelSecrets.delete(channelId);
    this.channelWallets.delete(channelId);
    // aimarket-hub answers `{ settlement: { used_usd, refund_usd, ... } }`; unwrap it and
    // name the spend the way the SDK model does, tolerating a bare settlement.
    const raw = await resp.json();
    const settled = raw && raw.settlement ? raw.settlement : raw;
    return {
      ...settled,
      channel_id: settled.channel_id ?? channelId,
      total_spent_usd: settled.total_spent_usd ?? settled.used_usd ?? 0,
      refund_usd: settled.refund_usd ?? 0,
      invocations: settled.invocations ?? 0,
    };
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

    // With a credit account the account pays: no channel is opened, so none is settled.
    const channel = this.config.apiKey ? undefined : await this.openChannel(depositUsd);
    const step = plan[0];

    const result = await this.invoke({
      capabilityId: step.capability.capability_id,
      input: opts.input,
      channelId: channel?.channel_id,
      productId: step.capability.product_id,
      sourceHub: step.capability.source_hub,
    });

    const settlement = channel ? await this.closeChannel(channel.channel_id) : undefined;

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

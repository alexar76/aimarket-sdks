import {
  AimarketException,
  AimarketNetworkException,
  AimarketPaymentException,
  AimarketSafetyException,
} from './errors';
import {
  type InvokeResult,
  type PlanStep,
  type SearchMatch,
  type SearchResponse,
  searchMatchToPlanStep,
} from './models';

const DEFAULT_HUB = 'https://independentai.network/hub';
const DEFAULT_TIMEOUT_MS = 30_000;
const MAX_INPUT_BYTES = 128 * 1024;

export type MarketPayment =
  | { kind: 'trial'; visitorId?: string }
  | { kind: 'credits'; apiKey: string }
  | { kind: 'channel'; channelId: string; channelSecret: string };

export interface MarketRunOptions {
  /** Natural-language job to search for. */
  intent: string;
  /** Input sent only after an offer passes the local budget/trust/latency checks. */
  input: Record<string, unknown>;
  /** Independent AI Hub by default; include /hub for a subpath deployment. */
  hubUrl?: string;
  /** Maximum routed price for the selected offer. Defaults to $0.25. */
  budgetUsd?: number;
  /** Minimum advertised trust score. Defaults to 0.3. */
  minTrust?: number;
  /** Maximum advertised p50 latency. Defaults to 10 seconds. */
  maxLatencyMs?: number;
  category?: string;
  /** Trial is the zero-setup default. Credentials are sent only as headers. */
  payment?: MarketPayment;
  /** Optional Pay-on-Verified request block forwarded to the Hub. */
  verify?: Record<string, unknown>;
  /** Require a Hub-verified receipt by default. */
  receiptPolicy?: 'require' | 'verify-if-present' | 'off';
  timeoutMs?: number;
  affiliate?: string;
  /** Injectable fetch for tests or custom runtimes. */
  fetch?: typeof fetch;
}

export interface ReceiptVerification {
  valid: boolean;
  classical_valid: boolean;
  pq_present: boolean;
  pq_valid: boolean | null;
  algorithm: string;
  signature_version: string | null;
  signer_matches_live_key: boolean;
  policy_requires_pq: boolean;
}

export interface MarketRunResult {
  intent: string;
  selected: PlanStep;
  invocation: InvokeResult;
  receiptVerification?: ReceiptVerification;
  trusted: boolean;
  paymentKind: MarketPayment['kind'];
  protocolVersion: 'v2';
}

let processVisitorId: string | undefined;

function defaultVisitorId(): string {
  if (processVisitorId) return processVisitorId;
  const random = globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  processVisitorId = `sdk-${random}`.slice(0, 96);
  return processVisitorId;
}

function finiteNumber(name: string, value: number, min: number, max: number): number {
  if (!Number.isFinite(value) || value < min || value > max) {
    throw new AimarketException(`${name} must be between ${min} and ${max}`);
  }
  return value;
}

function normalizedHubUrl(raw: string): string {
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new AimarketException('hubUrl must be an absolute URL');
  }
  const local = ['localhost', '127.0.0.1', '::1'].includes(parsed.hostname);
  if (parsed.protocol !== 'https:' && !(local && parsed.protocol === 'http:')) {
    throw new AimarketException('hubUrl must use HTTPS (HTTP is allowed only on localhost)');
  }
  if (parsed.username || parsed.password) {
    throw new AimarketException('hubUrl must not contain credentials');
  }
  parsed.search = '';
  parsed.hash = '';
  return parsed.toString().replace(/\/$/, '');
}

async function jsonBody<T>(response: Response): Promise<T> {
  try {
    return await response.json() as T;
  } catch {
    throw new AimarketException(`Hub returned invalid JSON (HTTP ${response.status})`);
  }
}

function selectedPrice(match: SearchMatch): number {
  const raw: unknown = match.routed_price_usd ?? match.price_per_call_usd;
  if (raw === undefined || raw === null || raw === '') return Number.NaN;
  return Number(raw);
}

function chooseOffer(
  response: SearchResponse,
  budgetUsd: number,
  minTrust: number,
  maxLatencyMs: number
): PlanStep | undefined {
  const match = (response.matches ?? []).find((candidate) => {
    const price = selectedPrice(candidate);
    const trust = Number(candidate.trust_score ?? 0);
    const latency = Number(candidate.p50_latency_ms ?? Number.POSITIVE_INFINITY);
    return Number.isFinite(price) && price >= 0 && price <= budgetUsd &&
      Number.isFinite(trust) && trust >= minTrust &&
      Number.isFinite(latency) && latency <= maxLatencyMs;
  });
  return match ? searchMatchToPlanStep(match) : undefined;
}

async function fetchWithTimeout(
  fetchFn: typeof fetch,
  url: string,
  init: RequestInit,
  timeoutMs: number
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetchFn(url, { redirect: 'error', ...init, signal: controller.signal });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    const reason = controller.signal.aborted
      ? `timed out after ${timeoutMs} ms`
      : detail;
    throw new AimarketNetworkException(`Hub request failed: ${reason}`);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * One safe market transaction: search, select, invoke once, and verify the receipt.
 *
 * The invoke is deliberately never retried. A lost response can still represent a
 * successful paid debit, so automatic retry would risk charging twice.
 */
async function run(options: MarketRunOptions): Promise<MarketRunResult> {
  const intent = options.intent?.trim();
  if (!intent || intent.length > 2_000) {
    throw new AimarketException('intent must contain 1 to 2000 characters');
  }
  let encodedInput: string;
  try {
    const serialized = JSON.stringify(options.input);
    if (typeof serialized !== 'string') {
      throw new TypeError('not a JSON value');
    }
    encodedInput = serialized;
  } catch {
    throw new AimarketException('input must be JSON-serializable');
  }
  if (new TextEncoder().encode(encodedInput).byteLength > MAX_INPUT_BYTES) {
    throw new AimarketException('input exceeds 128 KiB');
  }

  const hubUrl = normalizedHubUrl(options.hubUrl ?? DEFAULT_HUB);
  const budgetUsd = finiteNumber('budgetUsd', options.budgetUsd ?? 0.25, 0, 1_000_000);
  const minTrust = finiteNumber('minTrust', options.minTrust ?? 0.3, 0, 1);
  const maxLatencyMs = finiteNumber(
    'maxLatencyMs', options.maxLatencyMs ?? 10_000, 1, 3_600_000
  );
  const timeoutMs = finiteNumber('timeoutMs', options.timeoutMs ?? DEFAULT_TIMEOUT_MS, 100, 600_000);
  const fetchFn = options.fetch ?? fetch;
  const affiliate = (options.affiliate ?? 'aimarket-sdk-ts-market-run').slice(0, 128);

  const query = new URLSearchParams({
    intent,
    budget: String(budgetUsd),
    min_trust: String(minTrust),
    max_latency_ms: String(Math.floor(maxLatencyMs)),
    limit: '10',
  });
  const category = options.category?.trim() ?? '';
  if (category.length > 128) throw new AimarketException('category exceeds 128 characters');
  if (category) query.set('category', category);

  const search = await fetchWithTimeout(
    fetchFn,
    `${hubUrl}/ai-market/v2/search?${query}`,
    { headers: { 'X-AIMarket-Affiliate': affiliate } },
    timeoutMs
  );
  if (!search.ok) {
    throw new AimarketException(`Market search failed: HTTP ${search.status}`);
  }
  const searchResult = await jsonBody<SearchResponse>(search);
  const selected = chooseOffer(searchResult, budgetUsd, minTrust, maxLatencyMs);
  if (!selected) {
    throw new AimarketException(`No eligible capability found for: ${intent}`);
  }

  const payment = options.payment ?? { kind: 'trial' as const };
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'X-AIMarket-Affiliate': affiliate,
    'X-AIMarket-Route-Ok': '1',
  };
  if (payment.kind === 'trial') {
    headers['X-AIMarket-Sandbox-Visitor'] = payment.visitorId?.trim() || defaultVisitorId();
  } else if (payment.kind === 'credits') {
    if (!payment.apiKey?.trim()) throw new AimarketException('credits apiKey is required');
    headers['X-API-Key'] = payment.apiKey.trim();
  } else {
    if (!payment.channelId?.trim() || !payment.channelSecret?.trim()) {
      throw new AimarketException('channelId and channelSecret are required');
    }
    headers['X-Payment-Channel'] = payment.channelId.trim();
    headers['X-Payment-Channel-Secret'] = payment.channelSecret.trim();
  }

  const body: Record<string, unknown> = {
    capability_id: selected.capability.capability_id,
    product_id: selected.capability.product_id,
    source_hub: selected.capability.source_hub,
    input: options.input,
    // Server-side compare-and-set closes the search→invoke reprice window.
    max_price_usd: budgetUsd,
  };
  if (options.verify) body.verify = options.verify;

  const invoked = await fetchWithTimeout(
    fetchFn,
    `${hubUrl}/ai-market/v2/invoke`,
    { method: 'POST', headers, body: JSON.stringify(body) },
    timeoutMs
  );
  if (invoked.status === 402) {
    throw new AimarketPaymentException('Payment or free-trial allowance is required');
  }
  if (invoked.status === 403) {
    throw new AimarketSafetyException('Invocation was blocked by the Hub safety policy');
  }
  if (invoked.status === 409) {
    throw new AimarketException(
      'The selected capability now costs more than budgetUsd; search again before invoking',
      409
    );
  }
  if (!invoked.ok) {
    throw new AimarketException(`Capability invoke failed: HTTP ${invoked.status}`);
  }
  const invocation = await jsonBody<InvokeResult>(invoked);

  const receiptPolicy = options.receiptPolicy ?? 'require';
  let receiptVerification: ReceiptVerification | undefined;
  if (receiptPolicy !== 'off' && invocation.receipt) {
    const verified = await fetchWithTimeout(
      fetchFn,
      `${hubUrl}/ai-market/v2/receipts/verify`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-AIMarket-Affiliate': affiliate },
        body: JSON.stringify({ receipt: invocation.receipt }),
      },
      timeoutMs
    );
    if (!verified.ok) {
      throw new AimarketSafetyException(`Receipt verification unavailable: HTTP ${verified.status}`);
    }
    receiptVerification = await jsonBody<ReceiptVerification>(verified);
  }

  const trusted = Boolean(
    receiptVerification?.valid &&
    receiptVerification.signer_matches_live_key &&
    (!receiptVerification.policy_requires_pq || receiptVerification.pq_valid === true)
  );
  if (receiptPolicy === 'require' && !trusted) {
    throw new AimarketSafetyException(
      invocation.receipt ? 'Hub receipt verification failed' : 'Hub returned no signed receipt'
    );
  }

  return {
    intent,
    selected,
    invocation,
    receiptVerification,
    trusted,
    paymentKind: payment.kind,
    protocolVersion: 'v2',
  };
}

/** Zero-setup market entry point: `await market.run({ intent, input })`. */
export const market = Object.freeze({ run });

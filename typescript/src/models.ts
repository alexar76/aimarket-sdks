/** Data models for AI Market Protocol v2. */

export interface Capability {
  capability_id: string;
  product_id: string;
  name: string;
  version: string;
  description: string;
  input_schema?: Record<string, unknown>;
  output_schema?: Record<string, unknown>;
  price_per_call_usd: number;
  p50_latency_ms?: number;
  success_rate_30d?: number;
  source_hub: string;
  source_hub_name?: string;
  trust_score?: number;
}

export interface Channel {
  channel_id: string;
  deposit_usd: number;
  balance_usd: number;
  token: string;
  chain: string;
  expires_at: string;
  /**
   * One-time debit secret returned by the hub at channel open (secure-by-default
   * channels). It is required — as the `X-Payment-Channel-Secret` header — on
   * every debit/invoke, so a leaked channel id alone can't drain the channel.
   * Returned ONCE; the SDK captures it and sends it automatically.
   */
  channel_secret?: string;
}

export interface TeeAttestation {
  platform: string;
  enclave_id: string;
  code_hash: string;
  pcr_values: Record<string, string>;
  instance_id: string;
  region: string;
  timestamp: string;
  ttl_s: number;
  signature: string;
}

export interface TeeReceipt {
  receipt_id: string;
  input_hash: string;
  output_hash: string;
  signature: string;
}

export interface InvokeResult {
  success: boolean;
  // Capability output. The hub returns this under `result` (protocol v1/v2 invoke
  // body), NOT `output` — reading `output` silently dropped every capability's
  // payload. Kept optional because error responses omit it.
  result?: Record<string, unknown>;
  price_usd: number;
  latency_ms: number;
  receipt?: Record<string, unknown>;
  continuation?: Record<string, unknown>;
  safety_blocked?: boolean;
  safety_reason?: string;
  tee_verified?: boolean;
  tee_attestation?: TeeAttestation;
  tee_receipt?: TeeReceipt;
  error?: string;
  error_type?: string;
  // Pay-on-Verified envelope (hub `verification` field): status pending|settled|
  // refunded|skipped, verified, verify_score, trace_id, … Present only when the
  // invoke opted in via the request's `verify` block.
  verification?: Record<string, unknown>;
}

export interface PlanStep {
  capability: Capability;
  relevance_score: number;
  rationale: string;
}

export interface Settlement {
  channel_id: string;
  total_spent_usd: number;
  refund_usd: number;
  invocations: number;
}

export interface BillOfMaterials {
  task: string;
  plan: PlanStep[];
  results: InvokeResult[];
  settlement?: Settlement;
  total_spent_usd: number;
  protocol_version: string;
}

/**
 * A single match in the hub's GET /ai-market/v2/search response. The hub returns
 * a FLAT capability dict per match (not a `{capability, relevance_score}` wrapper).
 */
export interface SearchMatch {
  capability_id: string;
  product_id: string;
  name?: string;
  description?: string;
  version?: string;
  price_per_call_usd?: number;
  routed_price_usd?: number;
  score?: number;
  trust_score?: number;
  p50_latency_ms?: number;
  source_hub?: string;
  source_hub_name?: string;
  status_label?: string;
}

/** Envelope of GET /ai-market/v2/search — matches the live hub contract. */
export interface SearchResponse {
  query: string;
  matches: SearchMatch[];
  catalog?: string;
  protocol_version?: string;
}

/** Map a flat hub search match to the SDK's PlanStep shape. */
export function searchMatchToPlanStep(m: SearchMatch): PlanStep {
  return {
    capability: {
      capability_id: m.capability_id,
      product_id: m.product_id,
      name: m.name ?? m.capability_id,
      version: m.version ?? 'v1',
      description: m.description ?? '',
      price_per_call_usd: m.price_per_call_usd ?? m.routed_price_usd ?? 0,
      p50_latency_ms: m.p50_latency_ms,
      source_hub: m.source_hub ?? 'local',
      source_hub_name: m.source_hub_name,
      trust_score: m.trust_score,
    },
    relevance_score: m.score ?? 0,
    rationale: m.status_label ?? '',
  };
}

/** Ratio of remaining balance to original deposit (0..1). */
export function channelBalanceRatio(channel: Channel): number {
  if (channel.deposit_usd <= 0) return 0;
  return channel.balance_usd / channel.deposit_usd;
}

/** Whether the channel has passed its expiry timestamp. */
export function channelIsExpired(channel: Channel): boolean {
  const ts = Date.parse(channel.expires_at);
  if (Number.isNaN(ts)) return true;
  return ts < Date.now();
}

/** Canonical string used for TEE attestation signature verification. */
export function attestationCanonical(att: TeeAttestation): string {
  return (
    `platform:${att.platform}|enclave_id:${att.enclave_id}|code_hash:${att.code_hash}` +
    `|pcr0:${att.pcr_values.pcr0 ?? ''}|instance:${att.instance_id}` +
    `|region:${att.region}|timestamp:${att.timestamp}|ttl:${att.ttl_s}`
  );
}

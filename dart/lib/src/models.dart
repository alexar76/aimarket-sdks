/// Data models for AI Market Protocol v2.
///
/// Each model supports JSON serialization via [fromJson] and [toJson]
/// for round-trip safety. Getters like [Channel.isExpired] and
/// [InvokeResult.isSuccessful] provide convenience accessors.
library;

/// Read a required [String] field from a hub JSON payload.
///
/// Unlike the permissive `as String? ?? ''` pattern, this fails loudly when
/// the hub omits a required key or sends the wrong type, so financially
/// significant fields can never silently collapse to a default. The empty
/// string is accepted (some fields are legitimately empty), but `null` and
/// non-string values are rejected.
String _requireString(Map<String, dynamic> json, String key, String model) {
  final value = json[key];
  if (value is String) return value;
  throw FormatException(
    '$model.fromJson: missing or invalid required string field "$key" '
    '(got ${value.runtimeType})',
  );
}

/// Read a required numeric field from a hub JSON payload as a [double].
///
/// Rejects a missing or non-numeric value with a [FormatException] rather
/// than defaulting to `0`, which for price fields would silently turn a paid
/// capability into a free one.
double _requireDouble(Map<String, dynamic> json, String key, String model) {
  final value = json[key];
  if (value is num) return value.toDouble();
  throw FormatException(
    '$model.fromJson: missing or invalid required numeric field "$key" '
    '(got ${value.runtimeType})',
  );
}

/// A discoverable capability from the marketplace.
///
/// Returned by [AimarketAgent.discover] and represents a single
/// AI function available for purchase on the hub.
class Capability {
  /// Protocol capability identifier (the `capability_id` field, e.g.
  /// `translate.multi@v2`). Matches the TypeScript, Rust and Python SDKs.
  final String capabilityId;
  final String productId;
  final String name;
  final String version;
  final String description;
  final Map<String, dynamic>? inputSchema;
  final Map<String, dynamic>? outputSchema;
  final double pricePerCallUsd;
  final double? p50LatencyMs;
  final double? successRate30d;
  final String sourceHub;
  final String? sourceHubName;
  final double? trustScore;

  const Capability({
    required this.capabilityId,
    required this.productId,
    required this.name,
    required this.version,
    required this.description,
    this.inputSchema,
    this.outputSchema,
    required this.pricePerCallUsd,
    this.p50LatencyMs,
    this.successRate30d,
    required this.sourceHub,
    this.sourceHubName,
    this.trustScore,
  });

  /// The capability identifier.
  ///
  /// Backward-compatible alias for [capabilityId], kept so existing callers
  /// keep working. The protocol (and the TS/Rust/Python SDKs) name this
  /// `capability_id`; prefer [capabilityId] in new code.
  String get id => capabilityId;

  /// Deserialize from the hub's JSON response.
  ///
  /// Required protocol fields (`capability_id`, `product_id`, `name`,
  /// `description`, `price_per_call_usd`, `source_hub`) are
  /// validated: a missing or wrongly-typed value throws a [FormatException]
  /// instead of silently defaulting. This prevents a malformed hub response
  /// from yielding e.g. a free capability (`price_per_call_usd` defaulting to
  /// 0), which would be a financial risk for the consumer. `version` is
  /// optional metadata the protocol/hub may omit, so it defaults rather than
  /// throwing (keeps the SDK interoperable with hubs that don't emit it).
  factory Capability.fromJson(Map<String, dynamic> json) {
    return Capability(
      capabilityId: _requireString(json, 'capability_id', 'Capability'),
      productId: _requireString(json, 'product_id', 'Capability'),
      name: _requireString(json, 'name', 'Capability'),
      version: (json['version'] as String?) ?? '1.0.0',
      description: _requireString(json, 'description', 'Capability'),
      inputSchema: json['input_schema'] as Map<String, dynamic>?,
      outputSchema: json['output_schema'] as Map<String, dynamic>?,
      pricePerCallUsd:
          _requireDouble(json, 'price_per_call_usd', 'Capability'),
      p50LatencyMs: (json['p50_latency_ms'] as num?)?.toDouble(),
      successRate30d: (json['success_rate_30d'] as num?)?.toDouble(),
      sourceHub: _requireString(json, 'source_hub', 'Capability'),
      sourceHubName: json['source_hub_name'] as String?,
      trustScore: (json['trust_score'] as num?)?.toDouble(),
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'capability_id': capabilityId,
        'product_id': productId,
        'name': name,
        'version': version,
        'description': description,
        'input_schema': inputSchema,
        'output_schema': outputSchema,
        'price_per_call_usd': pricePerCallUsd,
        'p50_latency_ms': p50LatencyMs,
        'success_rate_30d': successRate30d,
        'source_hub': sourceHub,
        'source_hub_name': sourceHubName,
        'trust_score': trustScore,
      };

  /// Create a capability with sensible defaults for testing.
  ///
  /// All parameters have defaults so callers only override what matters:
  /// ```dart
  /// final cap = Capability.withDefaults(id: 'my-cap');
  /// ```
  ///
  /// The [id] parameter sets [capabilityId]; the name is kept for backward
  /// compatibility with existing callers.
  factory Capability.withDefaults({
    String id = 'default-capability',
    String productId = 'default-product',
    String name = 'Default Capability',
    String version = '1.0.0',
    String description = 'A capability with default values',
    double pricePerCallUsd = 0.10,
    String sourceHub = 'https://hub.aicom.io',
    double? trustScore,
  }) {
    return Capability(
      capabilityId: id,
      productId: productId,
      name: name,
      version: version,
      description: description,
      pricePerCallUsd: pricePerCallUsd,
      sourceHub: sourceHub,
      trustScore: trustScore,
    );
  }
}

/// A pre-funded payment channel.
///
/// Channels are opened on-chain, deposited with a token (e.g., USDT on Base),
/// and drawn down per invocation. Close the channel to get a refund for the
/// unused balance.
class Channel {
  final String id;
  final double depositUsd;
  final double balanceUsd;
  final String token;
  final String chain;
  final DateTime expiresAt;

  const Channel({
    required this.id,
    required this.depositUsd,
    required this.balanceUsd,
    required this.token,
    required this.chain,
    required this.expiresAt,
  });

  /// Deserialize from the hub's channel open response.
  factory Channel.fromJson(Map<String, dynamic> json) {
    return Channel(
      id: json['channel_id'] as String? ?? '',
      depositUsd: (json['deposit_usd'] as num?)?.toDouble() ?? 0,
      balanceUsd: (json['balance_usd'] as num?)?.toDouble() ?? 0,
      token: json['token'] as String? ?? 'USDT',
      chain: json['chain'] as String? ?? 'base',
      expiresAt: DateTime.tryParse(json['expires_at'] as String? ?? '') ??
          DateTime.now().add(const Duration(hours: 24)),
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'channel_id': id,
        'deposit_usd': depositUsd,
        'balance_usd': balanceUsd,
        'token': token,
        'chain': chain,
        'expires_at': expiresAt.toUtc().toIso8601String(),
      };

  /// Whether the channel's on-chain expiry has passed.
  ///
  /// Expired channels cannot be used for payment and should be closed.
  bool get isExpired => DateTime.now().toUtc().isAfter(expiresAt);

  /// Fraction of the original deposit still available as balance.
  ///
  /// A ratio > 0.5 means more than half the funds remain.
  double get balanceRatio =>
      depositUsd > 0 ? balanceUsd / depositUsd : 0.0;
}

/// Result of a capability invocation.
class InvokeResult {
  final bool success;
  final Map<String, dynamic>? output;
  final double priceUsd;
  final double latencyMs;
  final bool safetyBlocked;
  final String? safetyReason;
  final bool teeVerified;
  final TeeAttestation? teeAttestation;
  final TeeReceipt? teeReceipt;
  final String? error;

  const InvokeResult({
    required this.success,
    this.output,
    required this.priceUsd,
    required this.latencyMs,
    this.safetyBlocked = false,
    this.safetyReason,
    this.teeVerified = false,
    this.teeAttestation,
    this.teeReceipt,
    this.error,
  });

  /// Deserialize from the hub's invoke response.
  factory InvokeResult.fromJson(Map<String, dynamic> json) {
    return InvokeResult(
      success: json['success'] as bool? ?? false,
      output: json['output'] as Map<String, dynamic>?,
      priceUsd: (json['price_usd'] as num?)?.toDouble() ?? 0,
      latencyMs: (json['latency_ms'] as num?)?.toDouble() ?? 0,
      safetyBlocked: json['safety_blocked'] as bool? ?? false,
      safetyReason: json['safety_reason'] as String?,
      teeVerified: json['tee_verified'] as bool? ?? false,
      teeAttestation: json['tee_attestation'] != null
          ? TeeAttestation.fromJson(
              json['tee_attestation'] as Map<String, dynamic>)
          : null,
      teeReceipt: json['tee_receipt'] != null
          ? TeeReceipt.fromJson(json['tee_receipt'] as Map<String, dynamic>)
          : null,
      error: json['error'] as String?,
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'success': success,
        'output': output,
        'price_usd': priceUsd,
        'latency_ms': latencyMs,
        'safety_blocked': safetyBlocked,
        'safety_reason': safetyReason,
        'tee_verified': teeVerified,
        if (teeAttestation != null) 'tee_attestation': teeAttestation!.toJson(),
        if (teeReceipt != null) 'tee_receipt': teeReceipt!.toJson(),
        if (error != null) 'error': error,
      };

  /// Whether the invocation was genuinely successful.
  ///
  /// False if the server returned a non-success status, the safety gate
  /// blocked the request, or an error message was set.
  bool get isSuccessful => success && !safetyBlocked && error == null;
}

/// TEE attestation proving code runs in a secure enclave.
class TeeAttestation {
  /// Platform identifier: aws_nitro, intel_tdx, amd_sev, azure_cc.
  final String platform;

  /// Unique enclave instance identifier.
  final String enclaveId;

  /// Hash of the code running inside the enclave.
  final String codeHash;

  /// Platform configuration registers (e.g., PCR0 for Nitro).
  final Map<String, String> pcrValues;

  /// Cloud instance ID hosting the enclave.
  final String instanceId;

  /// Cloud region where the instance runs.
  final String region;

  /// ISO 8601 timestamp of attestation generation.
  final String timestamp;

  /// Time-to-live in seconds from [timestamp].
  final int ttlS;

  /// Enclave signature over the canonical representation.
  final String signature;

  const TeeAttestation({
    required this.platform,
    required this.enclaveId,
    required this.codeHash,
    required this.pcrValues,
    required this.instanceId,
    required this.region,
    required this.timestamp,
    required this.ttlS,
    required this.signature,
  });

  /// Deserialize from the hub's attestation response.
  factory TeeAttestation.fromJson(Map<String, dynamic> json) {
    return TeeAttestation(
      platform: json['platform'] as String? ?? '',
      enclaveId: json['enclave_id'] as String? ?? '',
      codeHash: json['code_hash'] as String? ?? '',
      pcrValues:
          (json['pcr_values'] as Map<String, dynamic>?)
                  ?.map((k, v) => MapEntry(k, v.toString())) ??
              {},
      instanceId: json['instance_id'] as String? ?? '',
      region: json['region'] as String? ?? '',
      timestamp: json['timestamp'] as String? ?? '',
      ttlS: json['ttl_s'] as int? ?? 300,
      signature: json['signature'] as String? ?? '',
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'platform': platform,
        'enclave_id': enclaveId,
        'code_hash': codeHash,
        'pcr_values': pcrValues,
        'instance_id': instanceId,
        'region': region,
        'timestamp': timestamp,
        'ttl_s': ttlS,
        'signature': signature,
      };

  /// Canonical string used for signature verification.
  ///
  /// Format: platform|enclave_id|code_hash|pcr0|instance|region|timestamp|ttl
  String get canonical =>
      'platform:$platform|enclave_id:$enclaveId|code_hash:$codeHash'
      '|pcr0:${pcrValues['pcr0'] ?? ''}|instance:$instanceId'
      '|region:$region|timestamp:$timestamp|ttl:$ttlS';

  /// Parse [timestamp] as a UTC instant, or `null` if it is not a valid
  /// ISO-8601 / RFC-3339 string.
  ///
  /// The protocol emits the attestation timestamp in ISO-8601 with a `Z`
  /// (UTC) suffix, e.g. `2026-05-23T12:00:00Z`. Dart's [DateTime.parse]
  /// interprets a string *without* an explicit zone offset as local time,
  /// which would skew the age calculation by the host's UTC offset. To stay
  /// correct regardless of the producer, a zone-less timestamp is reinterpreted
  /// as UTC; offset-bearing timestamps are converted to UTC normally.
  DateTime? get timestampUtc {
    final parsed = DateTime.tryParse(timestamp);
    if (parsed == null) return null;
    if (parsed.isUtc) return parsed;
    // No zone designator in the source string: treat the wall-clock value as
    // UTC rather than local, matching the protocol's documented format.
    final trimmed = timestamp.trim();
    final hasZone = trimmed.endsWith('Z') ||
        trimmed.endsWith('z') ||
        RegExp(r'[+-]\d{2}:?\d{2}$').hasMatch(trimmed);
    if (!hasZone) {
      return DateTime.utc(
        parsed.year,
        parsed.month,
        parsed.day,
        parsed.hour,
        parsed.minute,
        parsed.second,
        parsed.millisecond,
        parsed.microsecond,
      );
    }
    return parsed.toUtc();
  }

  /// Whether the attestation has exceeded its TTL window.
  ///
  /// Considers the [timestamp] plus [ttlS] seconds. The timestamp is expected
  /// to be an ISO-8601 / RFC-3339 string (see [timestampUtc]). Returns true if
  /// the timestamp is unparseable, the TTL is non-positive, or the window has
  /// elapsed — i.e. it fails closed so an unverifiable attestation is never
  /// treated as fresh.
  bool get isExpired {
    final ts = timestampUtc;
    if (ts == null || ttlS <= 0) return true;
    return DateTime.now().toUtc().difference(ts).inSeconds > ttlS;
  }
}

/// Receipt proving execution happened inside a TEE.
///
/// Links the input that was sent, the output that was returned, and the
/// enclave identity that processed them.
class TeeReceipt {
  final String receiptId;
  final String inputHash;
  final String outputHash;
  final String signature;

  const TeeReceipt({
    required this.receiptId,
    required this.inputHash,
    required this.outputHash,
    required this.signature,
  });

  /// Deserialize from the hub's receipt response.
  factory TeeReceipt.fromJson(Map<String, dynamic> json) {
    return TeeReceipt(
      receiptId: json['receipt_id'] as String? ?? '',
      inputHash: json['input_hash'] as String? ?? '',
      outputHash: json['output_hash'] as String? ?? '',
      signature: json['signature'] as String? ?? '',
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'receipt_id': receiptId,
        'input_hash': inputHash,
        'output_hash': outputHash,
        'signature': signature,
      };
}

/// Settlement returned after closing a payment channel.
///
/// Includes the total amount spent, the refund returned to the wallet,
/// and the number of invocations that were paid from this channel.
class Settlement {
  final String channelId;
  final double totalSpentUsd;
  final double refundUsd;
  final int invocations;

  const Settlement({
    required this.channelId,
    required this.totalSpentUsd,
    required this.refundUsd,
    required this.invocations,
  });

  /// Deserialize from the hub's channel close response.
  factory Settlement.fromJson(Map<String, dynamic> json) {
    return Settlement(
      channelId: json['channel_id'] as String? ?? '',
      totalSpentUsd: (json['total_spent_usd'] as num?)?.toDouble() ?? 0,
      refundUsd: (json['refund_usd'] as num?)?.toDouble() ?? 0,
      invocations: json['invocations'] as int? ?? 0,
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'channel_id': channelId,
        'total_spent_usd': totalSpentUsd,
        'refund_usd': refundUsd,
        'invocations': invocations,
      };
}

/// A plan step from discovery — capability matched to an intent.
///
/// Includes the [capability] itself plus a [relevanceScore] and [rationale]
/// explaining why the hub matched it to the consumer's intent.
class PlanStep {
  final Capability capability;
  final double relevanceScore;
  final String rationale;

  const PlanStep({
    required this.capability,
    required this.relevanceScore,
    required this.rationale,
  });

  /// Deserialize from the hub's search response.
  factory PlanStep.fromJson(Map<String, dynamic> json) {
    return PlanStep(
      capability:
          Capability.fromJson(json['capability'] as Map<String, dynamic>? ?? {}),
      relevanceScore: (json['relevance_score'] as num?)?.toDouble() ?? 0,
      rationale: json['rationale'] as String? ?? '',
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'capability': capability.toJson(),
        'relevance_score': relevanceScore,
        'rationale': rationale,
      };
}

/// Bill of Materials — full trace of a marketplace interaction.
///
/// Captures the entire lifecycle of a discover -> open -> invoke -> settle
/// cycle, including the plan, results, and settlement details.
class BillOfMaterials {
  final String task;
  final List<PlanStep> plan;
  final List<InvokeResult> results;
  final Settlement? settlement;
  final double totalSpentUsd;
  final String protocolVersion;

  const BillOfMaterials({
    required this.task,
    required this.plan,
    required this.results,
    this.settlement,
    required this.totalSpentUsd,
    required this.protocolVersion,
  });

  /// Deserialize from a previously serialized BOM.
  factory BillOfMaterials.fromJson(Map<String, dynamic> json) {
    return BillOfMaterials(
      task: json['task'] as String? ?? '',
      plan: (json['plan'] as List<dynamic>?)
              ?.map((e) => PlanStep.fromJson(e as Map<String, dynamic>))
              .toList() ??
          [],
      results: (json['results'] as List<dynamic>?)
              ?.map((e) => InvokeResult.fromJson(e as Map<String, dynamic>))
              .toList() ??
          [],
      settlement: json['settlement'] != null
          ? Settlement.fromJson(json['settlement'] as Map<String, dynamic>)
          : null,
      totalSpentUsd: (json['total_spent_usd'] as num?)?.toDouble() ?? 0,
      protocolVersion: json['protocol_version'] as String? ?? 'v2',
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'task': task,
        'plan': plan.map((p) => p.toJson()).toList(),
        'results': results.map((r) => r.toJson()).toList(),
        if (settlement != null) 'settlement': settlement!.toJson(),
        'total_spent_usd': totalSpentUsd,
        'protocol_version': protocolVersion,
      };

  /// Return a human-readable summary of the entire interaction.
  ///
  /// Includes total amount spent, average latency, success rate,
  /// and invocation count.
  String summary() {
    final avgLatency = results.isEmpty
        ? 0.0
        : results.fold<double>(0, (s, r) => s + r.latencyMs) / results.length;
    final successCount = results.where((r) => r.isSuccessful).length;
    final successRate =
        results.isEmpty ? 0.0 : (successCount / results.length) * 100;
    final totalResultSpent =
        results.fold<double>(0, (s, r) => s + r.priceUsd);

    return '''Bill of Materials: $task
  Total spent: \$${totalSpentUsd.toStringAsFixed(4)}
  Invoke spend: \$${totalResultSpent.toStringAsFixed(4)}
  Avg latency: ${avgLatency.toStringAsFixed(1)} ms
  Success rate: ${successRate.toStringAsFixed(1)}%  ($successCount/${results.length})
  Settlement refund: \$${settlement?.refundUsd.toStringAsFixed(4) ?? 'N/A'}
  Protocol: $protocolVersion''';
  }
}

/// A hub search/discovery response — the typed shape returned by the
/// `/search` endpoint.
///
/// Mirrors the `SearchResponse` model in the TypeScript and Rust SDKs:
/// the matched [results] plus the [total] match count and the [hub] that
/// served the response.
class SearchResponse {
  final List<PlanStep> results;
  final int total;
  final String hub;

  const SearchResponse({
    required this.results,
    required this.total,
    required this.hub,
  });

  /// Deserialize from the hub's `/search` response body.
  factory SearchResponse.fromJson(Map<String, dynamic> json) {
    return SearchResponse(
      results: (json['results'] as List<dynamic>?)
              ?.map((e) => PlanStep.fromJson(e as Map<String, dynamic>))
              .toList() ??
          [],
      total: (json['total'] as num?)?.toInt() ?? 0,
      hub: json['hub'] as String? ?? '',
    );
  }

  /// Serialize back to hub-compatible JSON.
  Map<String, dynamic> toJson() => {
        'results': results.map((r) => r.toJson()).toList(),
        'total': total,
        'hub': hub,
      };
}

/// JSON serialization round-trip helper for models.
///
/// Verifies that a model class serializes and deserializes without
/// information loss for the given set of field keys.
///
/// Use in tests:
/// ```dart
/// expect(jsonRoundTrip(Capability.fromJson, {'name': 'test', ...}), isTrue);
/// ```
typedef JsonFactory<T> = T Function(Map<String, dynamic>);

/// Verify that a model round-trips through JSON without field loss.
///
/// Serializes the model produced by [fromJson] with [input], then checks
/// that every key in [input] survives the round trip.
bool jsonRoundTrip<T>(
    JsonFactory<T> fromJson, Map<String, dynamic> input) {
  // Dynamic dispatch to toJson via duck typing — each model implements it.
  final model = fromJson(input) as dynamic;
  final output = model.toJson() as Map<String, dynamic>;
  for (final key in input.keys) {
    if (output.containsKey(key) &&
        output[key] != null &&
        input[key] != null) {
      // Deep-equality for maps, simple equality for scalars.
      if (output[key] is Map && input[key] is Map) {
        final outMap = output[key] as Map;
        final inMap = input[key] as Map;
        for (final k in inMap.keys) {
          if (outMap[k] != inMap[k]) return false;
        }
      } else {
        if (output[key] != input[key]) return false;
      }
    }
  }
  return true;
}

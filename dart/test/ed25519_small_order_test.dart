// Strict Ed25519 against test-vectors/ed25519_small_order.json (2026-10-12).
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:aimarket_agent/aimarket_agent.dart';
import 'package:test/test.dart';

String _sig(String hexStr) {
  final bytes = Uint8List.fromList(List<int>.generate(
      hexStr.length ~/ 2, (i) => int.parse(hexStr.substring(2 * i, 2 * i + 2), radix: 16)));
  return 'ed25519:${base64Encode(bytes)}';
}

void main() {
  final v = jsonDecode(File('../test-vectors/ed25519_small_order.json').readAsStringSync())
      as Map<String, dynamic>;
  final signer = MarketSigner(privateKeyHex: '11' * 32);

  test('RFC 8032 TEST 1 verifies', () {
    final t = v['rfc8032Test1'] as Map<String, dynamic>;
    expect(signer.verify(t['publicKeyHex'] as String, _sig(t['signatureHex'] as String),
        t['message'] as String), isTrue);
  });

  test('every small-order forgery is refused for every message', () {
    for (final f in (v['forgeries'] as List).cast<Map<String, dynamic>>()) {
      for (final m in (f['messages'] as List).cast<String>()) {
        expect(signer.verify(f['publicKeyHex'] as String, _sig(f['signatureHex'] as String), m),
            isFalse, reason: f['name'] as String);
      }
    }
  });
}

/// The channel-open payer proof, bound to the shared protocol vector.
///
/// The hub rebuilds this message independently and compares recovered signers, so the
/// vector is the contract: these tests assert against its bytes rather than against our
/// own implementation agreeing with itself.
library;

import 'dart:convert';
import 'dart:io';

import 'package:aimarket_agent/aimarket_agent.dart';
import 'package:test/test.dart';
import 'package:web3dart/credentials.dart';
import 'package:web3dart/crypto.dart';

void main() {
  final file = File('../../aimarket-protocol/test-vectors/payer-proof.json');
  if (!file.existsSync()) {
    // Skipped loudly rather than silently passing: without the vector these tests
    // would only prove the implementation agrees with itself.
    test('shared payer-proof vector is present', () {
      fail('missing ${file.path} — the cross-SDK contract cannot be checked');
    });
    return;
  }
  final vector = jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
  final cases = (vector['cases'] as List).cast<Map<String, dynamic>>();

  group('channel-open payer proof', () {
    test('the vector has cases to check', () {
      expect(cases.length, greaterThan(4));
    });

    for (final c in cases) {
      test('reproduces the canonical message: ${c['name']}', () {
        expect(
          channelOpenProofMessage(
            chain: c['chain'] as String,
            txHash: c['tx_hash'] as String,
            payer: c['payer'] as String,
            amountUsd: c['amount_usd'] as num,
          ),
          equals(c['message'] as String),
        );
      });

      test('converts the amount the way the hub does: ${c['name']}', () {
        expect(
          canonicalProofAmountCents(c['amount_usd'] as num),
          equals(c['amount_cents'] as int),
        );
      });
    }

    test('rounds a half-cent to EVEN, not away from zero', () {
      // (0.125 * 100).round() is 13 in Dart; the hub's Python round() gives 12, so the
      // naive rule produces a valid-looking signature the hub refuses.
      expect(canonicalProofAmountCents(0.125), equals(12));
      expect(canonicalProofAmountCents(0.135), equals(14));
      expect((0.125 * 100).round(), equals(13)); // the trap, spelled out
    });

    test('refuses an unusable amount deterministically', () {
      expect(canonicalProofAmountCents(double.nan), equals(-1));
      expect(canonicalProofAmountCents(double.infinity), equals(-1));
    });

    test('signs exactly what the vector signed', () {
      final key = vector['test_key'] as String;
      final signed = cases.where((x) => x.containsKey('signature')).toList();
      expect(signed, isNotEmpty);
      for (final c in signed) {
        expect(
          signChannelOpenProof(
            privateKeyHex: key,
            chain: c['chain'] as String,
            txHash: c['tx_hash'] as String,
            payer: c['payer'] as String,
            amountUsd: c['amount_usd'] as num,
          ),
          equals(c['signature'] as String),
          reason: 'the signature must match the vector byte for byte',
        );
      }
    });

    test('the signature recovers to the paying wallet', () {
      final key = vector['test_key'] as String;
      final c = cases.firstWhere((x) => x.containsKey('signature'));
      final params = (
        chain: c['chain'] as String,
        txHash: c['tx_hash'] as String,
        payer: c['payer'] as String,
        amountUsd: c['amount_usd'] as num,
      );
      final digest = channelOpenProofHash(
        chain: params.chain,
        txHash: params.txHash,
        payer: params.payer,
        amountUsd: params.amountUsd,
      );
      final sig = signChannelOpenProof(
        privateKeyHex: key,
        chain: params.chain,
        txHash: params.txHash,
        payer: params.payer,
        amountUsd: params.amountUsd,
      );
      final bytes = hexToBytes(sig.substring(2));
      final recovered = EthereumAddress.fromPublicKey(
        ecRecover(
          digest,
          MsgSignature(
            bytesToUnsignedInt(bytes.sublist(0, 32)),
            bytesToUnsignedInt(bytes.sublist(32, 64)),
            bytes[64],
          ),
        ),
      ).hexEip55.toLowerCase();
      expect(recovered, equals(EthPrivateKey.fromHex(key).address.hexEip55.toLowerCase()));
    });
  });
}

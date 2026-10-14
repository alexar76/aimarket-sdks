/// Dart bound to the SHARED cross-SDK EIP-712 fixture.
///
/// The TypeScript and Rust suites already assert against
/// `aimarket-sdks/test-vectors/debit_authorization.json`; until now Dart computed the same
/// digest but checked it only against itself, which the parity guard reported as
/// "not yet cross-checked". A per-language copy of a signing vector is how three SDKs end
/// up confidently disagreeing, so this reads the one committed fixture.
///
/// It also checks the SIGNATURE, not just the digest. A digest test passes even when the
/// signing call hashes its input a second time — the result is self-consistent, and
/// verifiable by no contract on earth.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:aimarket_agent/aimarket_agent.dart';
import 'package:test/test.dart';
import 'package:web3dart/credentials.dart';
import 'package:web3dart/crypto.dart';

void main() {
  final file = File('../test-vectors/debit_authorization.json');
  if (!file.existsSync()) {
    test('shared cross-SDK fixture is present', () {
      fail('missing ${file.path} — the cross-SDK contract cannot be checked');
    });
    return;
  }
  final vector = jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
  final params = vector['params'] as Map<String, dynamic>;

  Uint8List digest() => computeDebitDigest(
        channelId: params['channelId'] as String,
        hub: params['hub'] as String,
        token: params['token'] as String,
        amount: BigInt.parse(params['amount'] as String),
        receiptId: params['receiptId'] as String,
        nonce: BigInt.parse(params['nonce'] as String),
        deadline: params['deadline'] as int,
        chainId: params['chainId'] as int,
        verifyingContract: params['verifyingContract'] as String,
      );

  group('cross-SDK debit authorization', () {
    test('the digest matches the shared fixture', () {
      final expected =
          (vector['expectedDigest'] as String).toLowerCase().replaceFirst('0x', '');
      expect(bytesToHex(digest()), equals(expected));
    });

    test('the digest matches the committed sidecar too', () {
      // The sidecar is what Rust includes at compile time; if the two ever diverge the
      // parity guard fails, but this makes the Dart side notice as well.
      final sidecar =
          File('../test-vectors/debit_authorization.digest').readAsStringSync().trim();
      expect(bytesToHex(digest()), equals(sidecar.toLowerCase().replaceFirst('0x', '')));
    });

    test('the SDK signature recovers to the fixture wallet', () {
      // The real contract-compatibility question, and it is asked of the SDK's own
      // signing entry point rather than of a raw web3dart call — otherwise the test
      // guards nothing that ships. AIMarketEscrow.debitChannel runs ECDSA.recover over
      // THIS digest and compares against the depositor, so a signature over anything else
      // (e.g. over keccak(digest), which is what EthPrivateKey.signToEcSignature produces
      // when handed a pre-computed digest) reverts as InvalidSignature().
      final key = '0x${vector['ethereumPrivateKeyHex']}';
      final signer = MarketSigner(
        privateKeyHex: vector['ed25519SeedHex'] as String,
        ethereumPrivateKeyHex: key,
      );
      final encoded = signer.signDebitAuthorization(
        channelId: params['channelId'] as String,
        hub: params['hub'] as String,
        token: params['token'] as String,
        amount: BigInt.parse(params['amount'] as String),
        receiptId: params['receiptId'] as String,
        nonce: BigInt.parse(params['nonce'] as String),
        deadline: params['deadline'] as int,
        chainId: params['chainId'] as int,
        verifyingContract: params['verifyingContract'] as String,
      );
      // Byte-for-byte against the shared reference: RFC-6979 is deterministic and low-s,
      // so all three SDKs must emit the same signature. Recovery alone would still pass
      // for a signature over the wrong preimage if the check repeated the same mistake.
      expect(
        encoded.substring('eip712:'.length),
        equals(vector['expectedSignature'] as String),
        reason: 'must match the shared cross-SDK reference signature',
      );
      expect(encoded, startsWith('eip712:0x'));
      final raw = hexToBytes(encoded.substring('eip712:0x'.length));
      expect(raw.length, equals(65), reason: 'r(32) + s(32) + v(1)');
      expect(raw[64], anyOf(27, 28),
          reason: 'Solidity ECDSA.recover wants v in {27,28}, not an EIP-155 encoding');
      final recovered = EthereumAddress.fromPublicKey(
        ecRecover(
          digest(),
          MsgSignature(
            bytesToUnsignedInt(raw.sublist(0, 32)),
            bytesToUnsignedInt(raw.sublist(32, 64)),
            raw[64],
          ),
        ),
      ).hexEip55.toLowerCase();
      expect(recovered, equals(EthPrivateKey.fromHex(key).address.hexEip55.toLowerCase()),
          reason: 'the signature must verify against the EIP-712 digest itself');
    });
  });
}

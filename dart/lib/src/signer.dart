/// Production Ed25519 + EIP-712 signing for AI Market Protocol v2.
library;

import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:ed25519_edwards/ed25519_edwards.dart' as ed25519;
import 'package:web3dart/credentials.dart';

import 'eip712.dart';

export 'eip712.dart' show computeDebitDigest, debitTypehashHeader, escrowContractName, escrowContractVersion;

class Eip712Domain {
  final String name;
  final String version;
  final int chainId;
  final String verifyingContract;

  const Eip712Domain({
    this.name = escrowContractName,
    this.version = escrowContractVersion,
    this.chainId = 8453,
    this.verifyingContract = '0x0000000000000000000000000000000000000000',
  });

  Uint8List get domainSeparatorBytes => buildDomainSeparator(
        chainId: chainId,
        verifyingContract: verifyingContract,
      );

  String get domainSeparator => MarketSigner._bytesToHex(domainSeparatorBytes);
}

class TypedData {
  final Eip712Domain domain;
  final String primaryType;
  final Map<String, dynamic> message;

  const TypedData({
    required this.domain,
    required this.primaryType,
    required this.message,
  });
}

class MarketSigner {
  final ed25519.PrivateKey _ed25519PrivateKey;
  final String? _ethereumPrivateKeyHex;

  MarketSigner({
    required String privateKeyHex,
    String? ethereumPrivateKeyHex,
  })  : _ed25519PrivateKey = ed25519.newKeyFromSeed(_parseSeedBytes(privateKeyHex)),
        _ethereumPrivateKeyHex = ethereumPrivateKeyHex;

  ed25519.PublicKey get _publicKey => ed25519.public(_ed25519PrivateKey);

  String publicKeyBase64() =>
      base64Encode(Uint8List.fromList(_publicKey.bytes));

  String publicKeyHex() => _bytesToHex(Uint8List.fromList(_publicKey.bytes));

  String signCanonical(String canonical) {
    final sig = ed25519.sign(_ed25519PrivateKey, utf8.encode(canonical));
    return 'ed25519:${base64Encode(sig)}';
  }

  bool verify(String publicKey, String signature, String canonical) {
    if (!signature.startsWith('ed25519:')) return false;
    try {
      final pubBytes = _decodePublicKey(publicKey);
      final sigBytes = base64Decode(signature.substring(8));
      final pub = ed25519.PublicKey(pubBytes);
      return ed25519.verify(pub, utf8.encode(canonical), sigBytes);
    } catch (_) {
      return false;
    }
  }

  String signEip712TypedData(TypedData data) {
    throw UnsupportedError(
      'signEip712TypedData removed — use signDebitAuthorization() for AIMarketEscrow',
    );
  }

  String signDebitAuthorization({
    required String channelId,
    required String hub,
    required String token,
    required BigInt amount,
    required String receiptId,
    required BigInt nonce,
    required int deadline,
    int chainId = 8453,
    String verifyingContract = '0x0000000000000000000000000000000000000000',
  }) {
    final ethKey = _ethereumPrivateKeyHex;
    if (ethKey == null) {
      throw StateError(
        'ethereumPrivateKeyHex is required for EIP-712 debit signing. '
        'Pass it to MarketSigner(ethereumPrivateKeyHex: ...).',
      );
    }

    final digest = computeDebitDigest(
      channelId: channelId,
      hub: hub,
      token: token,
      amount: amount,
      receiptId: receiptId,
      nonce: nonce,
      deadline: deadline,
      chainId: chainId,
      verifyingContract: verifyingContract,
    );

    final creds = EthPrivateKey.fromHex(ethKey.startsWith('0x') ? ethKey : '0x$ethKey');
    final ecSig = creds.signToEcSignature(digest, chainId: chainId);
    final r = _bytesToHex(_bigIntToFixedBytes(ecSig.r, 32));
    final s = _bytesToHex(_bigIntToFixedBytes(ecSig.s, 32));
    final v = ecSig.v.toRadixString(16).padLeft(2, '0');
    return 'eip712:0x$r$s$v';
  }

  Map<String, String> signedHeaders({
    required String channelId,
    required String capabilityId,
    required String affiliate,
  }) {
    final canonical =
        'channel:$channelId|capability:$capabilityId|affiliate:$affiliate';
    return {
      'X-Payment-Channel': channelId,
      'X-AIMarket-Affiliate': affiliate,
      'X-Market-Signature': signCanonical(canonical),
    };
  }

  String requestId(String intent) {
    final ts = DateTime.now().toUtc().millisecondsSinceEpoch;
    final data = utf8.encode('$intent:$ts');
    return sha256.convert(data).toString().substring(0, 16);
  }

  bool verifyHubSignature(String hubPublicKey, String message, String signature) {
    if (signature.startsWith('eip712:')) return false;
    final sig = signature.startsWith('ed25519:') ? signature : 'ed25519:$signature';
    return verify(hubPublicKey, sig, message);
  }

  static ({String seedHex, String publicKeyBase64}) generateEd25519Keypair() {
    final random = Random.secure();
    final seed = Uint8List.fromList(List<int>.generate(32, (_) => random.nextInt(256)));
    final pk = ed25519.newKeyFromSeed(seed);
    final pub = ed25519.public(pk);
    return (
      seedHex: _bytesToHex(seed),
      publicKeyBase64: base64Encode(Uint8List.fromList(pub.bytes)),
    );
  }

  static ({String privateKey, String address}) generateEthereumWallet() {
    final creds = EthPrivateKey.createRandom(Random.secure());
    return (
      // web3dart returns variable-length BigInt bytes (31–33); normalize to 32.
      privateKey: _bytesToHex(_normalizePrivateKeyBytes(creds.privateKey)),
      address: creds.address.hexEip55,
    );
  }

  static Uint8List _normalizePrivateKeyBytes(Uint8List raw) {
    if (raw.length == 32) return raw;
    final out = Uint8List(32);
    if (raw.length < 32) {
      out.setRange(32 - raw.length, 32, raw);
      return out;
    }
    out.setRange(0, 32, raw.sublist(raw.length - 32));
    return out;
  }

  static ({String privateKey, String address}) generateWallet() =>
      generateEthereumWallet();

  static Uint8List _parseSeedBytes(String hexOrDev) {
    final normalized = hexOrDev.startsWith('0x') ? hexOrDev.substring(2) : hexOrDev;
    if (normalized.length == 64 && RegExp(r'^[0-9a-fA-F]+$').hasMatch(normalized)) {
      return Uint8List.fromList(List<int>.generate(32, (i) {
        return int.parse(normalized.substring(i * 2, i * 2 + 2), radix: 16);
      }));
    }
    return Uint8List.fromList(sha256.convert(utf8.encode(hexOrDev)).bytes);
  }

  static Uint8List _decodePublicKey(String publicKey) {
    final trimmed = publicKey.trim();
    if (trimmed.length == 64 && RegExp(r'^[0-9a-fA-F]+$').hasMatch(trimmed)) {
      return Uint8List.fromList(List<int>.generate(32, (i) {
        return int.parse(trimmed.substring(i * 2, i * 2 + 2), radix: 16);
      }));
    }
    return Uint8List.fromList(base64Decode(trimmed));
  }

  static Uint8List _bigIntToFixedBytes(BigInt value, int length) {
    final hex = value.toRadixString(16).padLeft(length * 2, '0');
    return Uint8List.fromList(List<int>.generate(length, (i) {
      return int.parse(hex.substring(i * 2, i * 2 + 2), radix: 16);
    }));
  }

  static String _bytesToHex(Uint8List bytes) {
    return bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  }
}

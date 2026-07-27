/// Production EIP-712 encoding for AIMarketEscrow debit authorizations.
library;

import 'dart:typed_data';

import 'package:web3dart/crypto.dart';

const String eip712DomainType =
    'EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)';

const String debitTypehashHeader =
    'DebitAuthorization(bytes32 channelId,address hub,address token,uint256 amount,bytes32 receiptId,uint256 nonce,uint256 deadline)';

const String escrowContractName = 'AIMarketEscrow';
const String escrowContractVersion = '1';

Uint8List _keccak256(Uint8List data) => keccak256(data);

Uint8List _keccak256Utf8(String data) => _keccak256(Uint8List.fromList(data.codeUnits));

Uint8List _parseHex32(String value) {
  final hex = value.startsWith('0x') ? value.substring(2) : value;
  final bytes = List<int>.generate(hex.length ~/ 2, (i) {
    return int.parse(hex.substring(i * 2, i * 2 + 2), radix: 16);
  });
  if (bytes.length != 32) {
    throw ArgumentError('expected 32-byte hex, got ${bytes.length}');
  }
  return Uint8List.fromList(bytes);
}

Uint8List _encodeAddress(String address) {
  final hex = address.startsWith('0x') ? address.substring(2) : address;
  final bytes = List<int>.generate(hex.length ~/ 2, (i) {
    return int.parse(hex.substring(i * 2, i * 2 + 2), radix: 16);
  });
  final out = Uint8List(32);
  out.setRange(32 - bytes.length, 32, bytes);
  return out;
}

Uint8List _encodeU256(BigInt value) {
  final out = Uint8List(32);
  final bytes = _bigIntToBytes(value);
  out.setRange(32 - bytes.length, 32, bytes);
  return out;
}

List<int> _bigIntToBytes(BigInt value) {
  if (value == BigInt.zero) return [0];
  final bytes = <int>[];
  var v = value;
  while (v > BigInt.zero) {
    bytes.insert(0, (v & BigInt.from(0xff)).toInt());
    v >>= 8;
  }
  return bytes;
}

Uint8List _abiEncode(List<Uint8List> words) {
  final out = BytesBuilder();
  for (final word in words) {
    out.add(word);
  }
  return out.toBytes();
}

/// Compute EIP-712 domain separator for AIMarketEscrow.
Uint8List buildDomainSeparator({
  required int chainId,
  required String verifyingContract,
}) {
  final domainTypeHash = _keccak256Utf8(eip712DomainType);
  final nameHash = _keccak256Utf8(escrowContractName);
  final versionHash = _keccak256Utf8(escrowContractVersion);
  return _keccak256(_abiEncode([
    domainTypeHash,
    nameHash,
    versionHash,
    _encodeU256(BigInt.from(chainId)),
    _encodeAddress(verifyingContract),
  ]));
}

/// Compute EIP-712 digest for a debit authorization.
Uint8List computeDebitDigest({
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
  final typeHash = _keccak256Utf8(debitTypehashHeader);
  final structHash = _keccak256(_abiEncode([
    typeHash,
    _parseHex32(channelId),
    _encodeAddress(hub),
    _encodeAddress(token),
    _encodeU256(amount),
    _parseHex32(receiptId),
    _encodeU256(nonce),
    _encodeU256(BigInt.from(deadline)),
  ]));
  final domain = buildDomainSeparator(
    chainId: chainId,
    verifyingContract: verifyingContract,
  );
  final buf = BytesBuilder();
  buf.addByte(0x19);
  buf.addByte(0x01);
  buf.add(domain);
  buf.add(structHash);
  return _keccak256(buf.toBytes());
}

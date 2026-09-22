/// Canonical channel-open payer proof (EIP-191).
///
/// Opening a payment channel against an on-chain deposit requires proving control of the
/// wallet that PAID. Every deposit lands in the same platform settlement wallet, so
/// recipient+amount alone only shows that *somebody* paid — without this signature, anyone
/// watching inbound transfers could quote a stranger's (public) tx hash and be credited.
///
/// Personal-sign rather than typed data on purpose: any ordinary wallet or hardware device
/// can produce it with no EIP-712 support.
///
/// The message must match `aimarket-protocol/test-vectors/payer-proof.json` byte for byte —
/// the hub rebuilds it independently and compares recovered signers, so a one-character
/// difference reaches the user as an unexplainable "invalid payer proof".
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:web3dart/credentials.dart';
import 'package:web3dart/crypto.dart';

const String payerProofDomain = 'AIMarket-Payer-Proof';
const int payerProofVersion = 1;
const String payerProofChannelOpen = 'channel-open';

final RegExp _hexOnly = RegExp(r'^[0-9a-fA-F]+$');

/// Canonical chain id: chain names are ASCII and case-free.
String canonicalProofChain(String chain) => chain.trim().toLowerCase();

/// Canonical transaction id.
///
/// An EVM hash is hex and case-insensitive at the JSON-RPC layer, so `0xABC…` and `0xabc…`
/// name the same transaction and must produce the same challenge. The `0x` prefix is
/// normalised IN as well, because the two hub stacks disagree about whether they strip it
/// first. Anything non-hex (a base58 Solana signature) is case-SIGNIFICANT and left exact.
String canonicalProofTxHash(String txHash) {
  final tx = txHash.trim();
  final body = tx.length >= 2 && tx.substring(0, 2).toLowerCase() == '0x'
      ? tx.substring(2)
      : tx;
  if (body.isNotEmpty && _hexOnly.hasMatch(body)) {
    return '0x${body.toLowerCase()}';
  }
  return tx;
}

/// Canonical payer: EIP-55 mixed case is a checksum, not identity, so an address lowercases.
String canonicalProofPayer(String payer) {
  final addr = payer.trim();
  if (addr.length == 42 &&
      addr.substring(0, 2).toLowerCase() == '0x' &&
      _hexOnly.hasMatch(addr.substring(2))) {
    return '0x${addr.substring(2).toLowerCase()}';
  }
  return addr;
}

/// Deposit amount as the integer cents both ledgers bill in.
///
/// ROUND-HALF-TO-EVEN, because the hub computes this with Python's `round()`, which is
/// banker's rounding — `0.125 → 12`, not 13. Dart's `double.round()` rounds half AWAY from
/// zero, so using it here silently produces a different preimage (and a rejected proof) for
/// any amount landing exactly on a half-cent. Returns -1 for an unusable amount so the
/// challenge stays deterministic and can never match a real deposit.
int canonicalProofAmountCents(num amountUsd) {
  final value = amountUsd.toDouble();
  if (value.isNaN || value.isInfinite) return -1;
  final scaled = value * 100;
  final floor = scaled.floor();
  final diff = scaled - floor;
  if (diff > 0.5) return floor + 1;
  if (diff < 0.5) return floor;
  return floor.isEven ? floor : floor + 1;
}

/// The exact text the paying wallet signs.
String channelOpenProofMessage({
  required String chain,
  required String txHash,
  required String payer,
  required num amountUsd,
}) {
  return [
    '$payerProofDomain/v$payerProofVersion',
    'purpose:$payerProofChannelOpen',
    'chain:${canonicalProofChain(chain)}',
    'tx:${canonicalProofTxHash(txHash)}',
    'payer:${canonicalProofPayer(payer)}',
    'amount_cents:${canonicalProofAmountCents(amountUsd)}',
  ].join('\n');
}

/// The EIP-191 digest: keccak256("\x19Ethereum Signed Message:\n" + len + message).
Uint8List channelOpenProofHash({
  required String chain,
  required String txHash,
  required String payer,
  required num amountUsd,
}) {
  final message = channelOpenProofMessage(
    chain: chain, txHash: txHash, payer: payer, amountUsd: amountUsd,
  );
  // UTF-8, not codeUnits: the prefix length counts BYTES, and a non-ASCII character would
  // otherwise be counted once but encoded as several.
  final body = Uint8List.fromList(utf8.encode(message));
  final prefix = Uint8List.fromList(
    utf8.encode('Ethereum Signed Message:\n${body.length}'),
  );
  return keccak256(Uint8List.fromList([...prefix, ...body]));
}

/// Sign the challenge with the paying wallet's key. Returns `0x`-prefixed r‖s‖v (65 bytes).
///
/// Verified against `payer-proof.json`: the bytes this produces are the bytes the hub's
/// `eth_account` recovery accepts.
String signChannelOpenProof({
  required String privateKeyHex,
  required String chain,
  required String txHash,
  required String payer,
  required num amountUsd,
}) {
  final message = channelOpenProofMessage(
    chain: chain, txHash: txHash, payer: payer, amountUsd: amountUsd,
  );
  final key = privateKeyHex.startsWith('0x') ? privateKeyHex : '0x$privateKeyHex';
  final creds = EthPrivateKey.fromHex(key);
  // signPersonalMessageToUint8List applies the EIP-191 prefix AND hashes, so it takes the
  // raw message bytes. Passing a pre-computed digest to signToEcSignature instead produces
  // a signature over keccak(digest) — self-consistent, verifiable by nothing, and exactly
  // the kind of bug a vector catches and a round-trip test does not.
  final sig = creds.signPersonalMessageToUint8List(
    Uint8List.fromList(utf8.encode(message)),
  );
  return '0x${_bytesToHex(sig)}';
}

String _bytesToHex(Uint8List bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

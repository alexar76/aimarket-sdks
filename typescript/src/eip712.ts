/**
 * Production EIP-712 encoding for AIMarketEscrow debit authorizations.
 *
 * Matches `contracts/evm/src/AIMarketEscrow.sol` and OpenZeppelin
 * `MessageHashUtils.toTypedDataHash`.
 */

import { hashTypedData, type Address, type Hex } from 'viem';

export const EIP712_DOMAIN_TYPE =
  'EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)';

export const DEBIT_AUTHORIZATION_TYPES = {
  DebitAuthorization: [
    { name: 'channelId', type: 'bytes32' },
    { name: 'hub', type: 'address' },
    { name: 'token', type: 'address' },
    { name: 'amount', type: 'uint256' },
    { name: 'receiptId', type: 'bytes32' },
    { name: 'nonce', type: 'uint256' },
    { name: 'deadline', type: 'uint256' },
  ],
} as const;

export interface DebitDigestParams {
  channelId: Hex;
  hub: Address;
  token: Address;
  amount: bigint;
  receiptId: Hex;
  nonce: bigint;
  deadline: bigint;
  chainId: number;
  verifyingContract: Address;
}

/** Compute the EIP-712 digest a depositor signs for `debitChannel`. */
export function computeDebitDigest(params: DebitDigestParams): Hex {
  return hashTypedData({
    domain: {
      name: 'AIMarketEscrow',
      version: '1',
      chainId: params.chainId,
      verifyingContract: params.verifyingContract,
    },
    types: DEBIT_AUTHORIZATION_TYPES,
    primaryType: 'DebitAuthorization',
    message: {
      channelId: params.channelId,
      hub: params.hub,
      token: params.token,
      amount: params.amount,
      receiptId: params.receiptId,
      nonce: params.nonce,
      deadline: params.deadline,
    },
  });
}

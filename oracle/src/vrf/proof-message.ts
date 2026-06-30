import { Address, nativeToScVal, xdr } from '@stellar/stellar-sdk';

/**
 * Builds the Ed25519 message that must be signed for `provide_randomness`.
 * Must match `build_vrf_proof_message` in the on-chain raffle-instance contract.
 */
export function buildVrfProofMessage(
  raffleContract: string,
  requestId: bigint,
  randomSeed: bigint,
): Buffer {
  const address = new Address(raffleContract);
  const scVal = xdr.ScVal.scvVec([
    address.toScVal(),
    nativeToScVal(requestId, { type: 'u64' }),
    nativeToScVal(randomSeed, { type: 'u64' }),
  ]);
  return Buffer.from(scVal.toXDR());
}

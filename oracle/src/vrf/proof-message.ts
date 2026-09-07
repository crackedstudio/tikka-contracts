import { Address, nativeToScVal, xdr } from '@stellar/stellar-sdk';
import { createHash } from 'crypto';

/**
 * Encodes a u64 seed as an 8-byte big-endian buffer.
 *
 * This mirrors the Rust on-chain verifier:
 *   `let message = Bytes::from_array(&env, &random_seed.to_be_bytes());`
 *
 * Throws RangeError for values outside [0, 2^64).
 */
export function buildProofMessage(seed: bigint): Buffer {
  if (seed < 0n) {
    throw new RangeError(`seed must be >= 0, got ${seed}`);
  }
  if (seed >= 2n ** 64n) {
    throw new RangeError(`seed must be < 2^64, got ${seed}`);
  }
  const buf = Buffer.allocUnsafe(8);
  // Write as two 32-bit halves to avoid JS bitwise truncation at 32 bits.
  const hi = Number((seed >> 32n) & 0xffffffffn);
  const lo = Number(seed & 0xffffffffn);
  buf.writeUInt32BE(hi, 0);
  buf.writeUInt32BE(lo, 4);
  return buf;
}

/**
 * Builds the Ed25519 message that must be signed for `provide_randomness`.
 * Must match `build_vrf_proof_message` in the on-chain raffle-instance contract.
 */
export function buildVrfProofMessage(
  raffleContract: string,
  requestId: bigint
): Buffer {
  const address = new Address(raffleContract);
  const scVal = xdr.ScVal.scvVec([address.toScVal(), nativeToScVal(requestId, { type: 'u64' })]);
  return Buffer.from(scVal.toXDR());
}

/**
 * Derive a deterministic u64 seed from the signed proof bytes.
 *
 * The oracle cannot choose this value independently: it is the first 8 bytes
 * of SHA-256(proof), interpreted as a big-endian u64.
 */
export function deriveRandomSeedFromProof(proof: Uint8Array | Buffer): bigint {
  const digest = createHash('sha256').update(proof).digest();
  return digest.readBigUInt64BE(0);
}

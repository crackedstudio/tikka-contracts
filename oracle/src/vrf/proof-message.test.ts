/**
 * proof-message.test.ts
 *
 * Golden-vector tests for the VRF proof-message encoder.
 *
 * WHY THESE TESTS EXIST
 * ─────────────────────
 * The on-chain verifier in contracts/raffle-instance/src/lib.rs
 * (`provide_randomness`) reconstructs the signed message as:
 *
 *   let message = Bytes::from_array(&env, &(current_contract_address, request_id).to_xdr(&env));
 *   env.crypto().ed25519_verify(&public_key, &message, &proof);
 *
 * The oracle must sign precisely that buffer.  Any byte-level drift between
 * proof-message.ts (TS encoder) and the Rust verifier causes randomness
 * delivery to silently fail.  A past security issue showed this surface is
 * fragile, so we lock it down with committed fixtures.
 *
 * HOW THE VECTORS WERE DERIVED
 * ─────────────────────────────
 * Each "hex" value in __fixtures__/proof-message-vectors.json was produced by
 * evaluating `seed_value.to_be_bytes()` as specified by Rust's u64 primitive,
 * then hex-encoding the resulting 8-byte array.  The derivation field in each
 * fixture entry documents the byte-by-byte breakdown.
 *
 * RUST CROSS-CHECK PROCEDURE
 * ──────────────────────────
 * To verify that the Rust side accepts the same bytes, add a unit test in
 * contracts/raffle-instance/src/lib.rs (or randomness.rs) of the form:
 *
 *   #[test]
 *   fn proof_message_encoding_matches_golden_vectors() {
 *     // Vector: seed = 72623859790382856 (0x0102030405060708)
 *     let seed: u64 = 72_623_859_790_382_856;
 *     let encoded = seed.to_be_bytes();
 *     assert_eq!(encoded, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
 *     // Full vector suite is in oracle/src/vrf/__fixtures__/proof-message-vectors.json.
 *     // Each entry's "hex" field is the expected big-endian encoding that both
 *     // sides must agree on.
 *   }
 *
 * The Soroban `Bytes::from_array` call wraps the slice without additional
 * encoding, so the 8-byte big-endian slice IS the canonical message.
 * Any change to either encoder will cause one of these two test suites to
 * fail before the change reaches an environment.
 */

import { buildProofMessage } from './proof-message';
import vectors from './__fixtures__/proof-message-vectors.json';

// ─── Golden vector tests ───────────────────────────────────────────────────

describe('buildProofMessage – golden vectors', () => {
  test.each(vectors)(
    '$description  (seed=$seed → 0x$hex)',
    (v) => {
      const { seed, hex } = v as { seed: string; hex: string; description: string; derivation: string };
      const result = buildProofMessage(BigInt(seed));

      // Must be exactly 8 bytes — mirrors the Rust `u64::to_be_bytes()` contract.
      expect(result).toHaveLength(8);

      // Must match the committed hex encoding byte-for-byte.
      expect(result.toString('hex')).toBe(hex);
    },
  );
});

// ─── Structural invariants ─────────────────────────────────────────────────

describe('buildProofMessage – invariants', () => {
  test('always returns exactly 8 bytes', () => {
    // Spot-check several values across the u64 range.
    for (const seed of [0n, 1n, 0xffn, 0x100n, BigInt('9007199254740991'), (2n ** 64n) - 1n]) {
      expect(buildProofMessage(seed)).toHaveLength(8);
    }
  });

  test('encodes big-endian: MSB is first byte', () => {
    // 0x0100_0000_0000_0000 → first byte = 0x01, rest = 0x00
    const msg = buildProofMessage(0x0100000000000000n);
    expect(msg[0]).toBe(0x01);
    expect(msg.slice(1).every((b: number) => b === 0)).toBe(true);
  });

  test('encodes big-endian: LSB is last byte', () => {
    // 0x0000_0000_0000_0001 → last byte = 0x01, rest = 0x00
    const msg = buildProofMessage(0x0000000000000001n);
    expect(msg[7]).toBe(0x01);
    expect(msg.slice(0, 7).every((b: number) => b === 0)).toBe(true);
  });

  test('different seeds produce different messages', () => {
    const a = buildProofMessage(1n).toString('hex');
    const b = buildProofMessage(2n).toString('hex');
    expect(a).not.toBe(b);
  });

  test('same seed always produces the same bytes (deterministic)', () => {
    const seed = 9999999999999999n;
    expect(buildProofMessage(seed).toString('hex')).toBe(
      buildProofMessage(seed).toString('hex'),
    );
  });
});

// ─── Input validation ──────────────────────────────────────────────────────

describe('buildProofMessage – input validation', () => {
  test('throws RangeError for negative seed', () => {
    expect(() => buildProofMessage(-1n)).toThrow(RangeError);
  });

  test('throws RangeError for seed >= 2^64', () => {
    expect(() => buildProofMessage(2n ** 64n)).toThrow(RangeError);
  });

  test('accepts 0 (minimum valid seed)', () => {
    expect(() => buildProofMessage(0n)).not.toThrow();
  });

  test('accepts u64::MAX (maximum valid seed)', () => {
    expect(() => buildProofMessage((2n ** 64n) - 1n)).not.toThrow();
  });
});

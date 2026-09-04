# Randomness Protocol Design & Trust Model

This document outlines the cryptographic design, trust model, and security considerations of the Raffle's randomness protocol, focusing on the Verifiable Random Function (VRF) mode and the multi-operator `k-of-n` Quorum mode.

---

## 1. Internal PRNG (`RandomnessSource::Internal = 0`)

### How the seed is built

Primary helper: `build_internal_seed_u64` in `contracts/raffle-instance/src/helpers.rs`; Internal draws in `draw.rs` pass that seed into `do_finalize_with_seed`.

The internal seed hashes this XDR-packed tuple and takes the first 8 bytes as a `u64`:

1. Ledger timestamp
1. Ledger sequence
1. Current raffle contract address

Values are XDR-packed and hashed with `env.crypto().sha256`, then fed to `env.prng().seed(...)`. Winner indices are selected via a **partial Fisher–Yates shuffle**: exactly `k` deterministic draws from the PRNG, using swap tracking to guarantee uniqueness without retries or modulo bias. This replaces the previous rejection-sampling loop which had an unbounded retry probability as `k` approached `n`.

### Who can influence it

- Anyone who can choose **when** `finalize_raffle` lands can work from visible ledger state.
- Validators can influence timestamp/sequence.
- Outcomes are **deterministic** for identical ledger + raffle inputs (good for audit, bad against motivated bias).

The compact u64 seed used when finalizing through `do_finalize_with_seed` hashes `(timestamp, sequence, current_contract_address)` and takes the first 8 bytes.

### Who can influence it

- Anyone who can choose **when** `finalize_raffle` lands can work from visible ledger state.
- Validators can influence timestamp/sequence.
- Outcomes are **deterministic** for identical ledger + raffle inputs (good for audit, bad against motivated bias).

### Timeout / fallback

None. Finalize completes in the same call once tickets meet `min_tickets`.

### Cost
## 1. Single-Oracle VRF Mode

In the single-oracle mode, a single trusted oracle is responsible for generating and delivering randomness.

### Protocol Flow
1. The Raffle contract emits a `RandomnessRequested` event containing a unique `request_id`.
2. The Oracle service detects the event, reads the `request_id` and contract ID, and generates a Verifiable Random Function (VRF) proof.
3. The Oracle submits the proof and the generated random seed back to the contract via `provide_randomness`.
4. The contract verifies the VRF proof on-chain using the Oracle's public key. If the proof is valid, the random seed is accepted.

### Trust Model & Mitigations
- *Unpredictability*: Because VRF proofs are cryptographically tied to the Oracle's private key, the random seed is completely unpredictable to anyone (including the players) before it is submitted.
- *Non-manipulation*: The Oracle cannot bias the randomness because there is only one valid VRF output for a given input (`request_id` + contract address`). The Oracle's only options are to submit the correct value or refuse to submit (causing a Denial of Service, which is monitored and alerted).

---

## 2. Multi-Operator Quorum Mode (k-of-n)

In Quorum mode, a decentralized group of $n$ independent oracles participate, and at least $k$ unique oracle submissions are required to construct the final random seed.

### Protocol Flow
1. The Raffle contract is configured with quorum parameters $k$ (threshold) and a list of $n$ authorized oracle addresses.
2. The contract emits a `RandomnessRequested` event.
3. Each participating oracle generates a cryptographically secure random seed independently and submits it to the contract via `provide_quorum_randomness(request_id, random_seed)`.
4. The contract stores the submitted seeds.
5. Once $k$ unique oracles have submitted their seeds, the contract combines the seeds (typically by hashing them together, e.g., `hash(seed_1 + seed_2 + ... + seed_k)`) to produce the final raffle seed.

---

## 3. CommitReveal (`RandomnessSource::CommitReveal = 2`)

### How the seed is built

1. During `Active`, ticket owners call `submit_commit(ticket_id, hash)` with `hash = sha256(secret)`.
1. Entries stored as persistent `CommitEntry(ticket_id)` → `{ committer, hash }` (ticket-keyed so transfers keep entropy; see [COMMIT_REVEAL.md](COMMIT_REVEAL.md)).
1. On `finalize_raffle`, contract concatenates all present commit hashes in ticket-id order, SHA-256s the blob, and uses the **first 8 bytes** as a `u64` seed.
1. Finalize proceeds via `do_finalize_with_seed` with `RandomnessType::Prng`.

### Who can influence it

- Each committer contributes preimage entropy (if they later reveal off-chain).
- Parties who **withhold** commits reduce entropy.
- If **zero** commits exist at finalize, the contract **falls back to Internal PRNG** (same path as Internal after the CommitReveal branch).

### Timeout / fallback

No oracle timeout. Fallback is immediate at finalize when `commits_found == 0`.

### Cost

One `submit_commit` per participating ticket (user-paid) plus finalize. No oracle host required.

### When to use

Medium-stakes raffles where buyers can be asked to commit, and you want stronger bias resistance than Internal without operating an oracle. Educate users to commit; otherwise you silently degrade to Internal.

---

## 4. Quorum-of-oracles randomness (`RandomnessSource::Quorum { k, oracles }`)

### How the seed is built

1. On draw initiation (`finalize_raffle` or ticket sales complete), the contract transitions to `Drawing` and emits `RandomnessRequested` events for all $n$ registered oracle addresses (`request fan-out`).
2. Each oracle submits its randomness via `provide_randomness(random_seed, public_key, proof, request_id)`.
3. The contract verifies the Ed25519 proof, matches `public_key` to a registered oracle in `oracles`, calls `oracle.require_auth()`, and enforces per-oracle deduplication (`duplicate submissions rejected`).
4. Delivered seeds are accumulated on-chain under `DataKey::QuorumSeeds` and `DataKey::QuorumOraclesSubmitted`.
5. Once at least $k$ unique registered oracles have submitted valid seeds, the contract aggregates all delivered seeds via SHA-256 over their concatenated big-endian bytes (`aggregate_quorum_seeds`) to form the final 64-bit seed.
6. The raffle is finalized via `do_finalize_with_seed` using the aggregated VRF seed.


## K-of-N Quorum Randomness Scheme

To eliminate single-oracle trust assumptions in high-stakes raffles, the contract supports a `Quorum` randomness mode.

### Architecture & Protocol Steps

1. **Request Fan-out**: When a draw is initiated, a `Quorum { k, oracles }` configuration specifies the threshold `k` and the set of $n$ authorized oracle addresses (`Vec<Address>`).
2. **Per-Oracle Deduplication**: Each registered oracle can submit its seed via `provide_randomness(env, caller, seed)`. The contract tracks delivered seeds in storage using an `AddressSet` / map indexed by oracle address. Duplicate submissions from the same oracle are rejected with `Error::DuplicateOracleSubmission`.
3. **Aggregation Function**: The aggregated seed is accumulated iteratively as seeds arrive using bitwise XOR and SHA-256 hashing:
   $$\text{Aggregated Seed} = \text{SHA-256}(\text{Accumulated Seed} \oplus \text{Oracle Seed})$$
   Once $k$ unique valid oracle submissions are delivered, the state transitions to `Ready` and the draw can be executed.
4. **Timeout & Fallback**: If $k$ oracles fail to submit seeds within `ORACLE_TIMEOUT_LEDGERS` ledgers from the draw request height, any caller can trigger a fallback mechanism (e.g., falling back to commit-reveal or admin fallback seed depending on protocol fallback policy).

### Who can influence it

- No single oracle alone can bias or predict the outcome.
- As long as at least 1 of the $k$ delivered seeds comes from an honest oracle, the output of the SHA-256 aggregation function is cryptographically uniform and un-biasable.
- Collusion among at least $k$ oracles is required to manipulate or predict the outcome.

### Timeout / fallback (`ORACLE_TIMEOUT_LEDGERS = 200`)

If fewer than $k$ oracles deliver valid seeds before `request_ledger + 200`:

| `trigger_randomness_fallback(..., do_refund)` | Result |
|---|---|
| `do_refund = true` | Status → `Cancelled` (`CancelReason::OracleTimeout`); clears request & quorum state |
| `do_refund = false` | Finalize with **Internal** u64 seed and `RandomnessType::Fallback` |

---


## Guidance thresholds

These are **policy recommendations** aligned with README / code comments — not on-chain enforced limits:

| Prize / risk profile | Suggested mode |
|---|---|
| Demo, tiny rewards, trusted community (≲ ~500 XLM) | **Internal** |
| Meaningful value, engaged ticket buyers | **CommitReveal** (+ document commit UX) |
| Large prizes, public adversarial setting, institutional | **External** (+ monitored oracle, tested fallback) |

Also consider:

- Can you run `oracle/` with `ORACLE_SECRET_KEY` secured? If no → avoid External.
- Will most tickets call `submit_commit`? If no → CommitReveal ≈ Internal at finalize.
- Is validator/finalizer collusion in-scope? If yes → External (or CommitReveal with high commit participation).

---

## Failure modes summary

| Mode | Primary failure mode | Protocol response |
|---|---|---|
| Internal | Biased finalize timing | None (inherent) |
| External | Oracle silent | After 200 ledgers: refund cancel **or** Internal fallback |
| External | Wrong `request_id` / bad proof | Tx rejects (`InvalidParameters` / crypto fail) |
| CommitReveal | No commits | Internal PRNG fallback |
| Any | `tickets_sold < min_tickets` or zero sold | `Failed` + `RaffleFailed` (no draw) |
| Any | Concurrent finalize | `DrawingLock` → `DrawingAlreadyInProgress` |

---

## Code map

| Concern | Location |
|---|---|
| Enum | `contracts/raffle-shared/src/lib.rs` → `RandomnessSource` |
| Timeout constant | `contracts/raffle-shared/src/constants.rs` → `ORACLE_TIMEOUT_LEDGERS` |
| Seed + strategies | `contracts/raffle-instance/src/randomness.rs` |
| Finalize / oracle / fallback | `contracts/raffle-instance/src/draw.rs` |
| Commits | `contracts/raffle-instance/src/tickets.rs` → `submit_commit` |
| Off-chain oracle | `oracle/` |

## Related docs

- [COMMIT_REVEAL.md](COMMIT_REVEAL.md) — commit/reveal protocol details  
- [STORAGE.md](STORAGE.md) — randomness-related keys and tiers  
- [ARCHITECTURE.md](ARCHITECTURE.md) — factory → instance → oracle flow  
- [EVENTS.md](EVENTS.md) — `RandomnessRequested`, `RandomnessReceived`, fallback events  

---

## Independent Draw Verification (Unimplemented / Unverified)

> **Build status:** The attestation source exists, but the current checkout
> does not wire `attestation.rs` into the raffle-instance module tree. The
> contract API and examples in this section are therefore not available until
> that build issue is resolved. Treat this section as design documentation,
> not as a description of deployed or working behaviour.

Third parties can verify that a finalized raffle draw was performed correctly without trusting off-chain indexers or making multiple contract queries. The contract provides a single-call attestation interface designed for independent auditors.

### Quick Start: Verify a Draw

```rust
// 1. Query the complete attestation package
let attestation = contract.get_draw_attestation(&env)?;

// 2. Verify configuration integrity
let recomputed_config_hash = hash_config(&attestation);
assert_eq!(recomputed_config_hash, attestation.config_hash);

// 3. Reproduce winner selection
let reproduced_winners = select_winners(
    attestation.fairness_data.seed,
    attestation.fairness_data.ticket_ids,
    attestation.prize_distribution_bp.len()
);

// 4. Compare with recorded winners
assert_eq!(reproduced_winners, attestation.winning_ticket_ids);
assert_eq!(resolve_owners(reproduced_winners), attestation.winner_addresses);
```

### What `get_draw_attestation()` Returns

The `DrawAttestation` struct combines everything needed for verification:

```rust
pub struct DrawAttestation {
    /// Seed, ticket IDs, winning indices, timestamp, sequence
    pub fairness_data: FairnessData,
    
    /// SHA-256 hash of off-chain metadata
    pub metadata_hash: BytesN<32>,
    
    /// Winner addresses in prize-tier order
    pub winner_addresses: Vec<Address>,
    
    /// Winning ticket IDs (1-indexed) in tier order
    pub winning_ticket_ids: Vec<u32>,
    
    /// Randomness source (Internal, External, CommitReveal)
    pub randomness_source: RandomnessSource,
    
    /// SHA-256 hash of effective config at draw time
    pub config_hash: BytesN<32>,
    
    /// Total tickets sold
    pub total_tickets_sold: u32,
    
    /// Prize distribution basis points
    pub prize_distribution_bp: Vec<u32>,
    
    /// Total prize amount
    pub prize_amount: i128,
    
    /// Individual ticket price
    pub ticket_price: i128,
}
```

### Availability

- **Status requirement**: Only callable when raffle is `Finalized` or `Claimed`
- **Error on early call**: Returns `Error::InvalidStatus` (23) if draw not complete
- **Storage location**: Fairness metadata stored in persistent storage (`DataKey::RandomnessSeed`) so it survives ledger-entry expiry

### Verification Procedure

A complete verification involves four checks:

#### 1. Configuration Hash Integrity

Verify the raffle parameters haven't been tampered with:

```rust
// Recompute configuration hash
let config_xdr = (
    attestation.max_tickets,
    attestation.ticket_price,
    attestation.prize_amount,
    attestation.prize_distribution_bp.to_xdr(env),
    attestation.randomness_source.to_xdr(env),
    // ... other config fields
).to_xdr(env);

let computed_hash = env.crypto().sha256(&config_xdr);
assert_eq!(computed_hash, attestation.config_hash);
```

#### 2. Metadata Hash Check

Verify off-chain metadata hasn't changed:

```rust
// Fetch claimed metadata from IPFS/Arweave/etc
let metadata_content = fetch_metadata(raffle_id);
let computed_metadata_hash = sha256(metadata_content);
assert_eq!(computed_metadata_hash, attestation.metadata_hash);
```

#### 3. Winner Selection Reproduction

Independently reproduce the winner selection using the recorded seed:

```rust
use soroban_sdk::Env;

// Partial Fisher–Yates winner selection: exactly k draws, no unbounded loop,
// no modulo bias, O(k) draws.  Off-chain reproducibility is achieved by
// replicating the seed construction and the shuffle algorithm below.
let seed = attestation.fairness_data.seed;
let ticket_ids = attestation.fairness_data.ticket_ids;
let num_winners = attestation.prize_distribution_bp.len();
let n = ticket_ids.len() as u64;

// Build the u64 seed the same way the contract does (first 8 bytes of the
// finalized seed — see build_internal_seed / PrngWinnerSelection for details).
let mut current_seed = seed;

// Partial Fisher–Yates shuffle
let mut remaining = n;
let mut swaps: std::collections::BTreeMap<u64, u64> = std::collections::BTreeMap::new();
let mut winners = std::vec::Vec::new();

for _ in 0..num_winners {
    // Generate an unbiased u64 in [0, remaining) using rejection sampling.
    let largest_multiple_remaining = (u64::MAX / remaining) * remaining;
    let mut candidate = loop {
        if current_seed < largest_multiple_remaining {
            break current_seed;
        }
        current_seed = current_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    };
    let r = (candidate % remaining) as usize;

    // Map candidate to actual index via swap tracking.
    let mut actual = r as u64;
    if let Some(&val) = swaps.get(&actual) {
        actual = val;
    }

    // The winning ticket (1-indexed) is ticket_ids[actual]
    winners.push(ticket_ids[actual as usize].clone());

    // Swap: position r now contains what was at position remaining-1.
    let last = remaining - 1;
    let mut last_actual = last as u64;
    if let Some(&val) = swaps.get(&last) {
        last_actual = val;
    }

    // Record the swap: position r now contains what was at position last.
    swaps.insert(r, last_actual);

    remaining -= 1;
}
```

```rust
// Verify the reproduced winners match the on-chain result.
assert_eq!(winners.len(), attestation.winning_ticket_ids.len());
for (i, (reproduced, on_chain)) in winners.iter().zip(attestation.winning_ticket_ids.iter()).enumerate() {
    assert_eq!(reproduced, on_chain, "winner {} mismatch at index {}", i, reproduced);
}
```

#### 4. Winner-to-Owner Resolution

Verify that winning ticket IDs resolve to the claimed winner addresses:

```rust
for (i, ticket_id) in attestation.winning_ticket_ids.iter().enumerate() {
    let owner = contract.get_ticket(ticket_id)?.owner;
    assert_eq!(owner, attestation.winner_addresses[i]);
}
```

This requires either:
- Access to on-chain ticket records (if still in storage before wipe)
- Trusted off-chain ticket ownership index
- Reconstruction from `TicketPurchased` events

### Randomness Source Considerations

Verification strength depends on the randomness source:

| Source | Verification confirms | External trust needed |
|---|---|---|
| **External (VRF)** | Ed25519 signature over `(contract, request_id, seed)` binds oracle to unpredictable commitment | Oracle didn't collude with finalize timing |
| **CommitReveal** | Seed derived from ticket-holder commits; verify commits via `CommitEntry(ticket_id)` storage | Enough participants committed unpredictable secrets |
| **Internal** | Deterministic from ledger state; any validator could predict at finalize time | Finalizer timing wasn't adversarially chosen |
| **Fallback** | Same as Internal (used when External oracle timed out) | Same trust model as Internal |

For External/VRF draws, verify the Ed25519 signature:

```rust
// Message format: XDR(contract_address, request_id, random_seed)
let message = build_vrf_proof_message(
    raffle_contract,
    attestation.fairness_data.seed, // request_id stored in FairnessMetadata
    attestation.fairness_data.seed
);

env.crypto().ed25519_verify(
    &oracle_public_key,
    &message,
    &stored_proof // from provide_randomness callback
);
```

### Example: Complete Audit Script

```rust
fn audit_raffle_draw(env: &Env, raffle_contract: &Address) -> Result<AuditReport, Error> {
    // 1. Fetch attestation
    let attestation = contract_client.get_draw_attestation(env)?;
    
    // 2. Verify config hash
    let config_valid = verify_config_hash(&attestation);
    
    // 3. Verify metadata hash
    let metadata_valid = verify_metadata_hash(&attestation)?;
    
    // 4. Reproduce winner selection
    let reproduced = reproduce_winners(
        attestation.fairness_data.seed,
        attestation.fairness_data.ticket_ids,
        attestation.prize_distribution_bp.len()
    );
    
    let winners_match = reproduced == attestation.winning_ticket_ids;
    
    // 5. Check winner ownership (if tickets still in storage)
    let owners_match = verify_winner_ownership(env, raffle_contract, &attestation)?;
    
    // 6. Verify VRF proof if External source
    let vrf_valid = if attestation.randomness_source == RandomnessSource::External {
        verify_vrf_proof(env, &attestation)?
    } else {
        true // N/A for Internal/CommitReveal
    };
    
    Ok(AuditReport {
        config_hash_valid: config_valid,
        metadata_hash_valid,
        winner_selection_valid: winners_match,
        ownership_valid: owners_match,
        vrf_signature_valid: vrf_valid,
        overall_valid: config_valid && metadata_valid && winners_match && owners_match && vrf_valid,
    })
}
```

### When to Verify
## 3. Last-Submitter Bias

The primary security challenge in threshold-based randomness protocols is **Last-Submitter Bias** (or Last-Revealer Bias).

### The Attack Vector
When $k-1$ oracles have submitted their seeds on-chain, those seeds are public. The $k$-th oracle (the last submitter required to reach the threshold) can:
1. Read the $k-1$ public seeds.
2. Precompute the combined raffle seed for different values of their own seed, or simply calculate the single outcome of their submission.
3. Determine the winning ticket based on that combined seed.
4. **Bias the outcome**: If the $k$-th oracle (or a colluding party) is unhappy with the winner (e.g., they didn't win), they can choose to **withhold** their submission, refusing to complete the quorum. They might wait for the raffle to time out (allowing refunds) or wait for a different block height if the contract allows late submissions under different conditions.

### Mitigations in Tikka Contracts

#### 1. Independent Cryptographic Seeds
No single oracle can force the final seed to be a specific desired value. Because the final seed is a hash of all $k$ seeds, changing the $k$-th seed changes the final hash in an unpredictable way (due to the avalanche effect of cryptographic hash functions). The last submitter can only choose between two options:
- Submit their honest seed and accept the resulting winner.
- Abort/withhold the transaction, preventing the draw from finishing.

### Timeout / fallback

No oracle timeout. Fallback is immediate at finalize when `commits_found == 0`.

### Cost

One `submit_commit` per participating ticket (user-paid) plus finalize. No oracle host required.

### When to use

Medium-stakes raffles where buyers can be asked to commit, and you want stronger bias resistance than Internal without operating an oracle. Educate users to commit; otherwise you silently degrade to Internal.

---

## 4. Quorum-of-oracles randomness (`RandomnessSource::Quorum { k, oracles }`)

### How the seed is built

1. On draw initiation (`finalize_raffle` or ticket sales complete), the contract transitions to `Drawing` and emits `RandomnessRequested` events for all $n$ registered oracle addresses (`request fan-out`).
2. Each oracle submits its randomness via `provide_randomness(random_seed, public_key, proof, request_id)`.
3. The contract verifies the Ed25519 proof, matches `public_key` to a registered oracle in `oracles`, calls `oracle.require_auth()`, and enforces per-oracle deduplication (`duplicate submissions rejected`).
4. Delivered seeds are accumulated on-chain under `DataKey::QuorumSeeds` and `DataKey::QuorumOraclesSubmitted`.
5. Once at least $k$ unique registered oracles have submitted valid seeds, the contract aggregates all delivered seeds via SHA-256 over their concatenated big-endian bytes (`aggregate_quorum_seeds`) to form the final 64-bit seed.
6. The raffle is finalized via `do_finalize_with_seed` using the aggregated VRF seed.


## K-of-N Quorum Randomness Scheme

To eliminate single-oracle trust assumptions in high-stakes raffles, the contract supports a `Quorum` randomness mode.

### Architecture & Protocol Steps

1. **Request Fan-out**: When a draw is initiated, a `Quorum { k, oracles }` configuration specifies the threshold `k` and the set of $n$ authorized oracle addresses (`Vec<Address>`).
2. **Per-Oracle Deduplication**: Each registered oracle can submit its seed via `provide_randomness(env, caller, seed)`. The contract tracks delivered seeds in storage using an `AddressSet` / map indexed by oracle address. Duplicate submissions from the same oracle are rejected with `Error::DuplicateOracleSubmission`.
3. **Aggregation Function**: The aggregated seed is accumulated iteratively as seeds arrive using bitwise XOR and SHA-256 hashing:
   $$\text{Aggregated Seed} = \text{SHA-256}(\text{Accumulated Seed} \oplus \text{Oracle Seed})$$
   Once $k$ unique valid oracle submissions are delivered, the state transitions to `Ready` and the draw can be executed.
4. **Timeout & Fallback**: If $k$ oracles fail to submit seeds within `ORACLE_TIMEOUT_LEDGERS` ledgers from the draw request height, any caller can trigger a fallback mechanism (e.g., falling back to commit-reveal or admin fallback seed depending on protocol fallback policy).

### Who can influence it

- No single oracle alone can bias or predict the outcome.
- As long as at least 1 of the $k$ delivered seeds comes from an honest oracle, the output of the SHA-256 aggregation function is cryptographically uniform and un-biasable.
- Collusion among at least $k$ oracles is required to manipulate or predict the outcome.

### Timeout / fallback (`ORACLE_TIMEOUT_LEDGERS = 200`)

If fewer than $k$ oracles deliver valid seeds before `request_ledger + 200`:

| `trigger_randomness_fallback(..., do_refund)` | Result |
|---|---|
| `do_refund = true` | Status → `Cancelled` (`CancelReason::OracleTimeout`); clears request & quorum state |
| `do_refund = false` | Finalize with **Internal** u64 seed and `RandomnessType::Fallback` |

---

## 5. Unique Winners Resolution (`unique_winners: true`)

When `RaffleConfig.unique_winners` is enabled, each participant address may win at most one prize tier. If a sampled ticket index resolves to an owner who has already won a higher prize tier, the contract invokes `resolve_unique_winner`.

### Probing Algorithm

`resolve_unique_winner` re-samples ticket indices starting from `initial_index` using a **linear probe**:

$$\text{candidate\_index} = (\text{initial\_index} + \text{step}) \pmod{\text{total\_tickets}} \quad \text{for } \text{step} \in [0, \text{total\_tickets})$$

For each `candidate_index`:
1. Retrieve the ticket owner via `get_ticket_owner(env, candidate_index + 1)`.
2. Check if the owner is already present in `winners`.
3. If the owner is distinct (not in `winners`), select `candidate_index` as the winner for that tier and break.

### Properties

- **Termination**: Probing is bounded by at most `total_tickets` steps. If a single address owns every ticket or no remaining distinct owner exists, probing terminates and falls back to `initial_index`.
- **Determinism**: The linear probing sequence is strictly deterministic and reproducible from `(seed, tier, initial_index)` and the on-chain ticket registry. Off-chain auditors and verifiers can replay the draw by mimicking this probe.
- **Fairness Note**: Linear probing skews slightly toward ticket indices immediately following a collision (similar to standard linear open-address hash table probing).

---


## Guidance thresholds

These are **policy recommendations** aligned with README / code comments — not on-chain enforced limits:

| Prize / risk profile | Suggested mode |
|---|---|
| Demo, tiny rewards, trusted community (≲ ~500 XLM) | **Internal** |
| Meaningful value, engaged ticket buyers | **CommitReveal** (+ document commit UX) |
| Large prizes, public adversarial setting, institutional | **External** (+ monitored oracle, tested fallback) |

Also consider:

- Can you run `oracle/` with `ORACLE_SECRET_KEY` secured? If no → avoid External.
- Will most tickets call `submit_commit`? If no → CommitReveal ≈ Internal at finalize.
- Is validator/finalizer collusion in-scope? If yes → External (or CommitReveal with high commit participation).

---

## Failure modes summary

| Mode | Primary failure mode | Protocol response |
|---|---|---|
| Internal | Biased finalize timing | None (inherent) |
| External | Oracle silent | After 200 ledgers: refund cancel **or** Internal fallback |
| External | Wrong `request_id` / bad proof | Tx rejects (`InvalidParameters` / crypto fail) |
| CommitReveal | No commits | Internal u64 seed fallback |
| Any | `tickets_sold < min_tickets` or zero sold | `Failed` + `RaffleFailed` (no draw) |
| Any | Concurrent finalize | `DrawingLock` → `DrawingAlreadyInProgress` |

---

## Code map

| Concern | Location |
|---|---|
| Enum | `contracts/raffle-shared/src/lib.rs` → `RandomnessSource` |
| Timeout constant | `contracts/raffle-shared/src/constants.rs` → `ORACLE_TIMEOUT_LEDGERS` |
| Seed + strategies | `contracts/raffle-instance/src/randomness.rs` |
| Finalize / oracle / fallback | `contracts/raffle-instance/src/draw.rs` |
| Commits | `contracts/raffle-instance/src/tickets.rs` → `submit_commit` |
| Off-chain oracle | `oracle/` |

## Related docs

- [COMMIT_REVEAL.md](COMMIT_REVEAL.md) — commit/reveal protocol details  
- [STORAGE.md](STORAGE.md) — randomness-related keys and tiers  
- [ARCHITECTURE.md](ARCHITECTURE.md) — factory → instance → oracle flow  
- [EVENTS.md](EVENTS.md) — `RandomnessRequested`, `RandomnessReceived`, fallback events  

---

## Independent Draw Verification (Unimplemented / Unverified)

> **Build status:** The attestation source exists, but the current checkout
> does not wire `attestation.rs` into the raffle-instance module tree. The
> contract API and examples in this section are therefore not available until
> that build issue is resolved. Treat this section as design documentation,
> not as a description of deployed or working behaviour.

Third parties can verify that a finalized raffle draw was performed correctly without trusting off-chain indexers or making multiple contract queries. The contract provides a single-call attestation interface designed for independent auditors.

### Quick Start: Verify a Draw

```rust
// 1. Query the complete attestation package
let attestation = contract.get_draw_attestation(&env)?;

// 2. Verify configuration integrity
let recomputed_config_hash = hash_config(&attestation);
assert_eq!(recomputed_config_hash, attestation.config_hash);

// 3. Reproduce winner selection
let reproduced_winners = select_winners(
    attestation.fairness_data.seed,
    attestation.fairness_data.ticket_ids,
    attestation.prize_distribution_bp.len()
);

// 4. Compare with recorded winners
assert_eq!(reproduced_winners, attestation.winning_ticket_ids);
assert_eq!(resolve_owners(reproduced_winners), attestation.winner_addresses);
```

### What `get_draw_attestation()` Returns

The `DrawAttestation` struct combines everything needed for verification:

```rust
pub struct DrawAttestation {
    /// Seed, ticket IDs, winning indices, timestamp, sequence
    pub fairness_data: FairnessData,
    
    /// SHA-256 hash of off-chain metadata
    pub metadata_hash: BytesN<32>,
    
    /// Winner addresses in prize-tier order
    pub winner_addresses: Vec<Address>,
    
    /// Winning ticket IDs (1-indexed) in tier order
    pub winning_ticket_ids: Vec<u32>,
    
    /// Randomness source (Internal, External, CommitReveal)
    pub randomness_source: RandomnessSource,
    
    /// SHA-256 hash of effective config at draw time
    pub config_hash: BytesN<32>,
    
    /// Total tickets sold
    pub total_tickets_sold: u32,
    
    /// Prize distribution basis points
    pub prize_distribution_bp: Vec<u32>,
    
    /// Total prize amount
    pub prize_amount: i128,
    
    /// Individual ticket price
    pub ticket_price: i128,
}
```

### Availability

- **Status requirement**: Only callable when raffle is `Finalized` or `Claimed`
- **Error on early call**: Returns `Error::InvalidStatus` (23) if draw not complete
- **Storage location**: Fairness metadata stored in persistent storage (`DataKey::RandomnessSeed`) so it survives ledger-entry expiry

### Verification Procedure

A complete verification involves four checks:

#### 1. Configuration Hash Integrity

Verify the raffle parameters haven't been tampered with:

```rust
// Recompute configuration hash
let config_xdr = (
    attestation.max_tickets,
    attestation.ticket_price,
    attestation.prize_amount,
    attestation.prize_distribution_bp.to_xdr(env),
    attestation.randomness_source.to_xdr(env),
    // ... other config fields
).to_xdr(env);

let computed_hash = env.crypto().sha256(&config_xdr);
assert_eq!(computed_hash, attestation.config_hash);
```

#### 2. Metadata Hash Check

Verify off-chain metadata hasn't changed:

```rust
// Fetch claimed metadata from IPFS/Arweave/etc
let metadata_content = fetch_metadata(raffle_id);
let computed_metadata_hash = sha256(metadata_content);
assert_eq!(computed_metadata_hash, attestation.metadata_hash);
```

#### 3. Winner Selection Reproduction

Independently reproduce the winner selection using the recorded seed:

```rust
use rejection_sampling::*;

let seed = attestation.fairness_data.seed;
let ticket_ids = attestation.fairness_data.ticket_ids;
let num_winners = attestation.prize_distribution_bp.len();

// Rejection sampling (same algorithm as OracleSeedWinnerSelection)
let mut rng = initialize_rng(seed);
let mut winners = Vec::new();
let mut used = HashSet::new();

while winners.len() < num_winners {
    let candidate = uniform_u64_in_range(&mut rng, 0, ticket_ids.len());
    if !used.contains(&candidate) {
        winners.push(ticket_ids[candidate]);
        used.insert(candidate);
    }
}

assert_eq!(winners, attestation.winning_ticket_ids);
```

The key algorithm is rejection sampling without modulo bias:

- Generate uniform random `u64` in range `[0, n)`
- Track used indices in a set
- Retry on collision until `num_winners` distinct indices selected

This matches `OracleSeedWinnerSelection::select_winner_indices` in `randomness.rs`.

#### 4. Winner-to-Owner Resolution

Verify that winning ticket IDs resolve to the claimed winner addresses:

```rust
for (i, ticket_id) in attestation.winning_ticket_ids.iter().enumerate() {
    let owner = contract.get_ticket(ticket_id)?.owner;
    assert_eq!(owner, attestation.winner_addresses[i]);
}
```

This requires either:
- Access to on-chain ticket records (if still in storage before wipe)
- Trusted off-chain ticket ownership index
- Reconstruction from `TicketPurchased` events

### Randomness Source Considerations

Verification strength depends on the randomness source:

| Source | Verification confirms | External trust needed |
|---|---|---|
| **External (VRF)** | Ed25519 signature over `(contract, request_id, seed)` binds oracle to unpredictable commitment | Oracle didn't collude with finalize timing |
| **CommitReveal** | Seed derived from ticket-holder commits; verify commits via `CommitEntry(ticket_id)` storage | Enough participants committed unpredictable secrets |
| **Internal** | Deterministic from ledger state; any validator could predict at finalize time | Finalizer timing wasn't adversarially chosen |
| **Fallback** | Same as Internal (used when External oracle timed out) | Same trust model as Internal |

For External/VRF draws, verify the Ed25519 signature:

```rust
// Message format: XDR(contract_address, request_id, random_seed)
let message = build_vrf_proof_message(
    raffle_contract,
    attestation.fairness_data.seed, // request_id stored in FairnessMetadata
    attestation.fairness_data.seed
);

env.crypto().ed25519_verify(
    &oracle_public_key,
    &message,
    &stored_proof // from provide_randomness callback
);
```

### Example: Complete Audit Script

```rust
fn audit_raffle_draw(env: &Env, raffle_contract: &Address) -> Result<AuditReport, Error> {
    // 1. Fetch attestation
    let attestation = contract_client.get_draw_attestation(env)?;
    
    // 2. Verify config hash
    let config_valid = verify_config_hash(&attestation);
    
    // 3. Verify metadata hash
    let metadata_valid = verify_metadata_hash(&attestation)?;
    
    // 4. Reproduce winner selection
    let reproduced = reproduce_winners(
        attestation.fairness_data.seed,
        attestation.fairness_data.ticket_ids,
        attestation.prize_distribution_bp.len()
    );
    
    let winners_match = reproduced == attestation.winning_ticket_ids;
    
    // 5. Check winner ownership (if tickets still in storage)
    let owners_match = verify_winner_ownership(env, raffle_contract, &attestation)?;
    
    // 6. Verify VRF proof if External source
    let vrf_valid = if attestation.randomness_source == RandomnessSource::External {
        verify_vrf_proof(env, &attestation)?
    } else {
        true // N/A for Internal/CommitReveal
    };
    
    Ok(AuditReport {
        config_hash_valid: config_valid,
        metadata_hash_valid,
        winner_selection_valid: winners_match,
        ownership_valid: owners_match,
        vrf_signature_valid: vrf_valid,
        overall_valid: config_valid && metadata_valid && winners_match && owners_match && vrf_valid,
    })
}
```

### When to Verify

- **Before prize claims**: Confirm fairness before winners withdraw
- **Post-mortem audits**: Historical verification after raffle completion
- **Continuous monitoring**: Automated verification by watchdog services
- **Dispute resolution**: Third-party arbitration when fairness is challenged

### Limitations

- **Ticket ownership resolution**: If `wipe_storage` has been called, ticket records are deleted. Verifiers must use off-chain ticket indexes or event logs.
- **CommitReveal entropy**: Verification confirms the seed was derived from commits, but not that commits were unpredictable (requires off-chain commit/reveal tracking).
- **Internal/Fallback bias**: Verification confirms correct execution but cannot prevent validator/finalizer timing bias.

### Code References

| Component | Location |
|---|---|
| Attestation struct | `contracts/raffle-instance/src/attestation.rs` |
| Public function | `contracts/raffle-instance/src/lib.rs` → `get_draw_attestation` |
| Winner selection algorithm | `contracts/raffle-instance/src/randomness.rs` → `OracleSeedWinnerSelection` |
| Fairness metadata storage | `contracts/raffle-instance/src/helpers.rs` → `do_finalize_with_seed` |
| VRF proof verification | `contracts/raffle-instance/src/draw.rs` → `provide_randomness` |

#### 3. Timeouts and Default Fallbacks
To prevent a malicious or lazy $k$-th oracle from holding the raffle hostage indefinitely, the contract implements:
- **Draw Timeouts**: If a quorum is not reached within a specified block window, the raffle can be cancelled, and all ticket buyers are refunded.
- **Slashed Stake / Operator Penalties**: Node operators who fail to submit within the timeout window can be penalized on-chain or removed from the active oracle set.

---

## 4. Internal Seed Construction (Draw Seed)

For deterministic winner selection, the contract derives an internal seed by hashing the XDR encoding of a tuple containing the ledger timestamp, ledger sequence number, network identifier, and raffle contract address.

### Byte Layout
The raw value fed to `hash_bytes32`is the XDR serialization of:

```
(timestamp: u64, sequence: u32, network_id: BytesN<32>, raffle_address: Address)
```

Where:

- `timestamp` is `env.ledger().timestamp()`.
- `sequence` is `env.ledger().sequence()`.
- `network_id` is `env.network_id()`, which is the network passphrase identifier (e.g. main, 4testnet, futurenet). This ensures that identical raffle parameters produce different draws on different networks.
- `raffle_address` is the current contract address (`env.current_contract_address()`), which uniquely identifies the raffle instance.

The XDR tuple is hashed using SHA-256 (`hash_bytes32`). The returned `BytesN<32>` seed is converted to the `u64` internal seed by taking the first 8 bytes of the SHA-256 output and interpreting them as a big-endian integer.

This is the only internal seed construction in the crate; all callers use `build_internal_seed_u64(env, &env.current_contract_address())`.

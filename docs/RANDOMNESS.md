# Randomness Modes: Internal vs External vs CommitReveal

Tikka raffles select winners using the `RandomnessSource` values defined in
`contracts/raffle-shared/src/lib.rs`. The mode is fixed in `RaffleConfig` at
creation and enforced by `finalize_raffle` / `provide_randomness`
(`contracts/raffle-instance/src/draw.rs`, `randomness.rs`). Quorum support is
present in the configuration model but is not documented here as a completed,
verified deployment feature.

## Quick decision table

| Mode | Trust assumption | Who can influence outcome? | Extra cost / ops | Recommended prize scale |
|---|---|---|---|---|
| **Internal** | Honest-enough validators + unpredictable timing | Finalizer timing; validators biasing ledger timestamp/sequence | Lowest — single finalize tx | **Enforced ≤ `MAX_INTERNAL_RANDOMNESS_PRIZE_AMOUNT`** (5e9 stroops ≈ 500 XLM) — see §1 |
| **External** | Honest oracle key + live oracle service | Oracle (bounded by Ed25519 proof over request); timeout fallback | Oracle hosting + callback tx; possible fallback tx | Medium / high-stakes |
| **CommitReveal** | Enough buyers submit unpredictable commits | Buyers who commit; last-mover / withholding risks; zero-commit → Internal fallback | Per-ticket `submit_commit` txs | Medium-stakes when buyers are engaged |
| **Quorum** | At least 1 honest oracle out of k-of-n delivered | Single oracle cannot bias outcome; requires k-of-n collusion to manipulate | Multi-oracle hosting + k callback txs | High-stakes / large treasuries |


If you need protocol detail for commits, also read [COMMIT_REVEAL.md](COMMIT_REVEAL.md).

---

## 1. Internal PRNG (`RandomnessSource::Internal = 0`)

### How the seed is built

Primary helper: `build_internal_seed` / `PrngWinnerSelection` in `randomness.rs`, and `build_internal_seed_u64` used by the finalize path in `lib.rs` / `draw.rs`.

Entropy mixed into the 32-byte path:

1. Ledger timestamp  
1. Ledger sequence  
1. Network id (SHA-256 of network passphrase)  
1. Raffle contract address  

Values are XDR-packed and hashed with `env.crypto().sha256`, then fed to `env.prng().seed(...)`. Winner indices are sampled without replacement via `u64_in_range`.

The compact u64 seed used when finalizing through `do_finalize_with_seed` hashes `(timestamp, sequence, current_contract_address)` and takes the first 8 bytes.

### Who can influence it

- Anyone who can choose **when** `finalize_raffle` lands can work from visible ledger state.
- Validators can influence timestamp/sequence.
- Outcomes are **deterministic** for identical ledger + raffle inputs (good for audit, bad against motivated bias).

### Timeout / fallback

None. Finalize completes in the same call once tickets meet `min_tickets`.

### Cost

One successful `finalize_raffle` (plus prior ticket txs). No oracle.

### When to use

Low-stakes community raffles, demos, and tests. **Do not** rely on Internal for large treasuries or adversarial settings.

### Enforced prize cap (#773)

`RaffleConfig.prize_amount` is rejected at both `raffle-instance::init` and
`raffle-factory::create_raffle` when `randomness_source == Internal` and
`prize_amount > MAX_INTERNAL_RANDOMNESS_PRIZE_AMOUNT`
(`contracts/raffle-shared/src/constants.rs`, currently `5_000_000_000`
stroops ≈ 500 XLM). This turns the previous "≲ ~500 XLM" policy guidance
below into an on-chain enforced limit — instance `init` returns
`Error::RandomnessSourceTooWeakForPrize`, and the factory returns
`ContractError::RandomnessSourceTooWeakForPrize` *before* deploying the
raffle instance WASM, saving the creator deployment cost. Choose `External`,
`CommitReveal`, or `Quorum` for anything above this cap.

**Note (CommitReveal gap):** `CommitReveal` silently falls back to the same
predictable seed derivation as `Internal` when zero commits exist at
finalize (see §3 below). This cap is scoped to `RandomnessSource::Internal`
only and does **not** currently protect a high-value `CommitReveal` raffle
that receives zero commits — that gap is tracked as a candidate follow-up
issue, not addressed here.

---

## 2. External oracle (`RandomnessSource::External = 1`)

### How the seed is built

1. `finalize_raffle` transitions to `Drawing`, sets `DrawingLock`, and calls `request_randomness`.
1. Contract stores `RandomnessRequested`, `RandomnessRequestLedger`, and a `RandomnessRequestId` derived from `(timestamp, sequence, contract_address)` via SHA-256 → first 8 bytes.
1. Emits `RandomnessRequested` for the off-chain `oracle/` service.
1. Oracle calls `provide_randomness(random_seed, public_key, proof, request_id)`.
1. Contract verifies Ed25519 over `build_vrf_proof_message` = XDR`(contract_address, request_id, random_seed)`.
1. `OracleSeedWinnerSelection` maps the seed to winner indices with rejection sampling (no modulo bias).

`Fairness` / seed metadata is stored under `DataKey::RandomnessSeed` (persistent).

### Who can influence it

- Only the configured `oracle_address` can auth the callback.
- The oracle chooses `random_seed` but must present a valid signature bound to `request_id` and the raffle contract.
- If the oracle never answers, creator/admin may call `trigger_randomness_fallback` after the timeout.

### Timeout / fallback (`ORACLE_TIMEOUT_LEDGERS = 200`)

Defined in `raffle-shared::constants` and the instance crate (~**200 ledgers ≈ 17 minutes** at 5s/ledger).

A **minimum** delay also applies before a submitted VRF response is
accepted: `provide_randomness` rejects with `Error::RandomnessTooEarly` if
called before `request_ledger + RANDOMNESS_MIN_DELAY_LEDGERS` (10 ledgers)
has elapsed, using the same `RandomnessRequestLedger` value set when the
request was made. This does not apply to `Quorum` submissions or to
`Internal`, whose draw is atomic (transition-to-drawing and finalize happen
in the same transaction, so there is no second phase to delay).

After `request_ledger + 200`:

| `trigger_randomness_fallback(..., do_refund)` | Result |
|---|---|
| `do_refund = true` | Status → `Cancelled` (`CancelReason::OracleTimeout`); request keys cleared; lock released |
| `do_refund = false` | Finalize with **Internal** u64 seed and `RandomnessType::Fallback`; emits `RandomnessFallbackTriggered` |

Calling fallback **before** the timeout returns `Error::FallbackTooEarly` (9).

### Cost

- Oracle process (Node 20, see `oracle/README.md`)
- Extra callback transaction
- Possible fallback transaction if the oracle is down

### When to use

High-stakes draws, public prize pools, or any case where Internal bias is unacceptable. Requires a reliable oracle key and monitoring.

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

These are **policy recommendations** aligned with README / code comments.
The Internal row below is now also an **on-chain enforced limit**
(`MAX_INTERNAL_RANDOMNESS_PRIZE_AMOUNT`); the other rows remain
recommendations only:

| Prize / risk profile | Suggested mode |
|---|---|
| Demo, tiny rewards, trusted community (**enforced ≤ ~500 XLM for Internal**) | **Internal** |
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
| Internal | `prize_amount` above `MAX_INTERNAL_RANDOMNESS_PRIZE_AMOUNT` | Rejected at `init`/`create_raffle` (`RandomnessSourceTooWeakForPrize`) |
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
| Internal prize cap | `contracts/raffle-shared/src/constants.rs` → `MAX_INTERNAL_RANDOMNESS_PRIZE_AMOUNT`; `contracts/raffle-shared/src/lib.rs` → `exceeds_internal_randomness_cap` |
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


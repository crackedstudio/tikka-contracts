# Architecture

This document maps every crate and module in the repository so contributors
know where code belongs before they write a single line.

---

## Repository layout

```
tikka-contracts/
├── contracts/
│   ├── raffle/               # Factory contract (RaffleFactory)
│   │   └── src/
│   │       ├── lib.rs        # Public entrypoints only — thin delegation layer
│   │       └── events.rs     # All factory-emitted events
│   │
│   ├── raffle-instance/      # Per-raffle contract (RaffleInstance)
│   │   └── src/
│   │       ├── lib.rs        # Public entrypoints only — thin delegation layer
│   │       ├── events.rs     # All instance-emitted events
│   │       ├── randomness.rs # Seed construction + winner selection strategies
│   │       └── test.rs       # Integration tests (cfg(test) only)
│   │
│   └── raffle-shared/        # Shared types used by both contracts + the oracle
│       └── src/
│           └── lib.rs        # Enums, structs, traits, constants
│
├── oracle/                   # Off-chain VRF oracle (TypeScript / Node.js)
│   └── src/
│       ├── keys/             # Key management
│       ├── tx/               # Transaction submission
│       └── vrf/              # VRF computation
│
├── fuzz/                     # cargo-fuzz targets
│   └── fuzz_targets/
│       ├── fuzz_buy_ticket.rs
│       └── fuzz_finalize_raffle.rs
│
├── scripts/                  # Shell scripts for deploy / verify / invoke
├── docs/                     # Human-readable documentation
├── deployments/              # Deployment receipts (JSON)
├── Makefile                  # Single source of truth for all local + CI checks
├── rust-toolchain.toml       # Pinned Rust compiler version
└── Cargo.toml                # Workspace manifest
```

---

## Crate responsibilities

### `raffle-shared`

The single shared library. Every type that crosses a crate boundary lives here.

- `RaffleStatus`, `RaffleConfig`, `Ticket`, `FairnessData` — on-chain data types
- `RandomnessSource`, `RandomnessType`, `CancelReason` — enumerations
- `RandomnessOracleTrait`, `RandomnessReceiverTrait` — cross-contract interfaces
- Constants (`DEFAULT_PAGE_LIMIT`, etc.)

**Rule:** If two crates need the same type, it goes in `raffle-shared`.

### `raffle` (factory)

Creates and tracks raffle instances. Delegates administration.

- `lib.rs` — entrypoints only; each function is ≤ 30 lines and calls helpers
- `events.rs` — every `#[contracttype]` event struct + `publish()` impl

**Rule:** No business logic in `lib.rs`. Extract helpers into focused modules.

### `raffle-instance`

Runs a single raffle from prize deposit through winner claim.

- `lib.rs` — entrypoints only; orchestrates calls to helpers
- `events.rs` — every event the instance emits
- `randomness.rs` — `build_internal_seed`, `PrngWinnerSelection`, `OracleSeedWinnerSelection`
- `test.rs` — all `#[cfg(test)]` tests live here, never inline in `lib.rs`

**Rule:** `lib.rs` must not exceed 600 lines. Extract cohesive groups into new modules.

### `oracle`

Off-chain service. Receives randomness requests via events, computes a VRF proof,
and calls `provide_randomness` on the raffle instance.

- `src/vrf/` — VRF seed generation and proof construction
- `src/keys/` — signing key management
- `src/tx/` — Stellar transaction building and submission

---

## Where does my code go?

| What you're adding | Where it goes |
|---|---|
| New public entrypoint | `lib.rs` (thin delegation only) |
| Business logic for an entrypoint | New module file, imported in `lib.rs` |
| New on-chain event | `events.rs` in the relevant contract |
| New error variant | `Error` enum in `lib.rs` + entry in `docs/ERRORS.md` |
| New storage key | `DataKey` enum in `lib.rs` + entry in `DEVELOPMENT.md` key table |
| Shared type used by 2+ crates | `raffle-shared/src/lib.rs` |
| Constants | `raffle-shared/src/lib.rs` (if shared) or top of the relevant module |
| Randomness / winner selection | `raffle-instance/src/randomness.rs` |
| Tests | `src/test.rs` — never inline in `lib.rs` |
| Oracle TypeScript code | `oracle/src/` in the appropriate subdirectory |
| Shell deployment script | `scripts/` |
| Human-readable docs | `docs/` |

---

## File-size guideline

**Hard limit: 600 lines per `.rs` file.**

Soroban `lib.rs` files tend to bloat because all entrypoints are `#[contractimpl]`.
The 600-line limit forces extraction before files become unnavigable. When you hit
the limit, create a new module (e.g., `src/finalize.rs`) and re-export through `lib.rs`.

---

## State machine

```
PendingPrize ──deposit_prize()──► Active
Active ──buy_tickets() fills max──► Drawing
Active ──cancel_raffle()──────────► Cancelled
Active ──finalize_raffle() (min not met)──► Failed
Drawing ──finalize_raffle() / provide_randomness() / fallback──► Finalized
Drawing ──cancel_raffle() / fallback(refund)──► Cancelled
Finalized ──all prizes claimed──► Claimed
Finalized / Drawing ──emergency_withdraw()──► Cancelled
Cancelled / Failed ──refund_prize()──► (prize returned, status unchanged)
```

---

## Cross-contract call graph

```
Creator ──► RaffleFactory.create_raffle()
                └──► deploys RaffleInstance

Buyer ──► RaffleInstance.buy_tickets()
              └──► RaffleFactory.record_volume()   (optional)
              └──► RaffleFactory.track_participant() (optional)

Creator ──► RaffleInstance.finalize_raffle()       (Internal path: draws inline)
Oracle  ──► RaffleInstance.provide_randomness()    (External path: oracle callback)
```

---

## Adding a new module (step-by-step)

1. Create `contracts/raffle-instance/src/my_feature.rs`
2. Add `//! One-line description of what this module does.` at the top
3. Add `mod my_feature;` in `lib.rs`
4. Keep `lib.rs` entrypoints as thin wrappers: `my_feature::do_thing(&env, raffle)`
5. Add tests to `src/test.rs`, not to `my_feature.rs`
6. Update this document if the module map changes

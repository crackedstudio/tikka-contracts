# Contributing

Thanks for your interest in contributing to Tikka! This project targets Stellar/Soroban smart contracts and welcomes PRs for improvements, tests, and docs.

## Getting Started

1. Fork the repository and create a feature branch.
2. Make your changes with clear, focused commits.
3. Run tests locally before opening a PR.

## Pre-Push Requirement

**Before pushing any commit, you MUST run the full CI-equivalent locally:**

```bash
make ci
```

`make ci` is the single source of truth and runs exactly the same checks as the GitHub Actions pipeline:
- `make check` – `cargo fmt --check` (formatting)
- `make clippy` – `cargo clippy` with warnings denied
- `make build` – WASM build for `wasm32v1-none`
- `make test` – `cargo test --workspace` (no crate skipping)
- `make size-check` – WASM binary size validation (< 512 KB)
- `make docs-check` – Required documentation presence check
- `make shellcheck` – Lint deploy/invoke shell scripts
- `make oracle-lint` – Oracle TypeScript build, lint, and format check

The pre-commit hook (see `.pre-commit-config.yaml`) runs the fast subset (`cargo fmt`, `cargo clippy`, oracle format check) on every commit. `make ci` is still required before push because it includes the full build, tests, and all slow checks.

## Development Expectations

-   Keep changes scoped and easy to review.
-   Write tests for new behavior when possible.
-   Update documentation if behavior or APIs change.

## Where does my code go?

Contributors are expected to respect the following crate/module placement rules. Do not dump new logic into monolithic `lib.rs` files.

| Change type | Target location | Notes |
|---|---|---|
| **New contract entrypoints** | Topical module in the relevant crate (`contracts/raffle/src/<topic>.rs` or `contracts/raffle-instance/src/<topic>.rs`). The crate's `lib.rs` holds only a one-line `pub mod <topic>;` delegation plus the `#[contractimpl]` dispatch. | If a module grows beyond ~600 lines, split it further. |
| **New constants** | `contracts/raffle-shared/src/constants.rs` (create if missing) or `contracts/<crate>/src/constants.rs` for crate-internal constants. | **Never inline magic numbers.** All tunable parameters, limits, and fees go here. |
| **New errors** | Append a new discriminant to the `#[contracterror]` enum in the relevant crate (`raffle/src/lib.rs` for factory, `raffle-instance/src/lib.rs` for instance), then **regenerate `docs/ERRORS.md`** with the new entry. | Never renumber or reuse old discriminants — it breaks deployed indexers. |
| **New events** | `contracts/<crate>/src/events.rs` (event struct definitions + `publish()` helpers). Every new event must also be documented in `docs/EVENTS.md` with fields, topics, and intended indexer usage. | Events are the primary off-chain state source — do not skip docs. |
| **New storage keys** | Append a new variant to the crate's `DataKey` enum in its `lib.rs`. Document the new key (storage tier, semantics, TTL strategy) in `docs/STORAGE.md`. | Persistent vs instance storage choice has TTL and cost implications — call it out in the PR description. |
| **New shared types / enums / structs** | `contracts/raffle-shared/src/lib.rs` (or a topical module under `raffle-shared/src/`). | `RaffleConfig`, `RaffleStatus`, pagination types, etc. all live here so both factory and instance crates consume a single source of truth. |
| **Tests** | `contracts/<crate>/src/tests/<name>.rs` — a dedicated file per behavior area. | **Never inline tests at the bottom of `lib.rs`.** The `#[cfg(test)] mod tests { ... }` block inside `lib.rs` is forbidden for new suites; existing inline tests should be extracted into their own file opportunistically. |
| **Oracle service code** | Matching `oracle/src/<name>/` directory: e.g., `oracle/src/vrf/` for VRF logic, `oracle/src/tx/` for transaction submission. | Mirror the Rust crate layout — topically-organized modules, not a single 2000-line `index.ts`. |

### File size expectation

Aim for **~600 lines as a soft ceiling** per `.rs` / `.ts` source file. Anything above ~800 lines must be split into topical sub-modules before merge. PRs that introduce >600-line files without justification will be returned for restructuring.

The intent is not dogma about line counts — it's readability, reviewability, and keeping module responsibility single-purpose.

## Tests

```bash
# Workspace-wide (preferred)
cargo test --workspace

# Per crate
cargo test -p raffle-factory
cargo test -p raffle-instance
cargo test -p raffle-shared
```

## Pull Requests

-   Provide a concise summary of what changed and why.
-   Link any relevant issues.
-   Note any follow-up work or limitations.
-   Confirm that `make ci` passes locally before marking the PR ready for review.
-   If adding or moving code, confirm it follows the placement table above.

## Code of Conduct

Be respectful and constructive in discussions. Harassment or abuse is not tolerated.

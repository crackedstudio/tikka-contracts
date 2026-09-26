# Infrastructure, Documentation, and Tooling Alignment Implementation Plan

## Repository Research

### Current State Summary

**Crate/Module Layout:**
- `contracts/raffle-factory/` - Factory contract (lib.rs, events.rs, views.rs, registry.rs)
- `contracts/raffle-instance/` - Instance contract (lib.rs, admin.rs, attestation.rs, claim.rs, draw.rs, events.rs, helpers.rs, init.rs, randomness.rs, tickets.rs, views.rs, test.rs)
- `contracts/raffle-shared/` - Shared types (lib.rs, constants.rs, errors.rs, events.rs, config_builder.rs)
- `oracle/` - TypeScript oracle service (src/{alert,deduplication,health,keys,listener,logging,metrics,queue,quorum,shutdown,tx,vrf}/)
- `fuzz/` - Rust fuzz targets
- `scripts/` - Bash/TypeScript deployment and verification scripts
- `docs/` - 18 documentation files

**Key File Statuses:**
- `CONTRIBUTING.md` - Exists but missing "Where does my code go?" section, no `make ci` pre-push requirement
- `docs/ARCHITECTURE.md` - Exists with flow diagrams but lacks a complete crate/module map
- `README.md` - Lacks feature status table, pre-audit warning, BUILD issue links, authoritative docs pointer
- `docs/README.md` - Good documentation index, needs verification of completeness and links
- `rust-toolchain.toml` - Already created with 1.94.0, clippy, rustfmt, wasm32v1-none ✓
- `Cargo.toml` (root) - Already has `rust-version = "1.70"` in `[workspace.package]` ✓
- `.github/workflows/ci.yml` - Still uses explicit `toolchain: stable` in check job; manual toolchain install instead of defaulting to rust-toolchain.toml
- `.devcontainer/Dockerfile` - Uses `wasm32-unknown-unknown` (not `wasm32v1-none`), doesn't pin exact Rust version
- `docs/DEPLOYMENT.md` - Mentions toolchain but lacks WASM size change PR tracking for upgrades
- `Makefile` - Missing check, size-check, docs-check, shellcheck, oracle-lint targets; no `make ci`; `all` target runs lint+test+build but not the full CI pipeline
- `.pre-commit-config.yaml` - Uses `npx prettier` instead of pinned local `npm run format:check`

**Rust Module Headers:**
- `raffle-factory/src/lib.rs` - Missing `//!` header
- `raffle-instance/src/lib.rs` - Missing `//!` header
- `raffle-shared/src/lib.rs` - Missing `//!` header
- All sub-module .rs files need `//!` headers added where missing

## Files and Modules

### 1. Codebase Architecture & Contribution Guidelines
- `CONTRIBUTING.md`: Add "Where does my code go?" section + 600-line max file expectation + `make ci` pre-push requirement
- `docs/ARCHITECTURE.md`: Add complete crate and module map section
- All `contracts/*/src/*.rs` files: Add `//!` module responsibility headers

### 2. README Accuracy & Feature Status
- `README.md`: Add feature status table, pre-audit warning, BUILD issue links, verify code examples, add docs/ authoritative source pointer
- `docs/README.md`: Verify completeness and all links resolve correctly

### 3. Rust Toolchain Pinning
- `rust-toolchain.toml`: Already correct - verify and keep as-is
- `Cargo.toml` (root): Already has rust-version in workspace.package - verify and keep
- `.github/workflows/ci.yml`: Remove explicit `toolchain:` definitions; let rust-toolchain.toml be the default
- `.devcontainer/Dockerfile`: Pin to exact Rust version from rust-toolchain.toml; install wasm32v1-none target
- `docs/DEPLOYMENT.md`: Record pinned toolchain version; add note that upgrades require a PR tracking WASM size changes

### 4. CI and Local Check Unification
- `Makefile`: Add targets: `check`, `size-check`, `docs-check`, `shellcheck`, `oracle-lint`, `ci`; update `ci` to run full CI pipeline
- `.github/workflows/ci.yml`: Replace shell commands with `make` target invocations
- `.pre-commit-config.yaml`: Replace `npx prettier` with `npm run format:check`; ensure fmt + clippy hooks present
- `CONTRIBUTING.md`: Document `make ci` as required pre-push command

## Implementation Steps

### Phase 1: Contribution & Architecture Docs
1. Add "Where does my code go?" section to CONTRIBUTING.md with all 9 rules + file-size expectation
2. Update CONTRIBUTING.md CI section to require `make ci` as pre-push command
3. Add crate/module map table to docs/ARCHITECTURE.md, documenting every crate, its sub-modules, and each module's responsibility
4. Add `//!` headers to all Rust .rs files:
   - `raffle-factory/src/lib.rs`: Factory contract crate root — deploys instances, manages registry, governance
   - `raffle-factory/src/events.rs`: Factory event structs and publish helpers
   - `raffle-factory/src/views.rs`: Factory read-only query surface
   - `raffle-factory/src/registry.rs`: Creator profiles, leaderboards, partner stats
   - `raffle-instance/src/lib.rs`: Raffle instance contract crate root — entrypoints and core types
   - `raffle-instance/src/admin.rs`: Admin-only entrypoints (pause, withdraw, cancel, rescue)
   - `raffle-instance/src/attestation.rs`: Draw attestation package for third-party verification
   - `raffle-instance/src/claim.rs`: Prize claim, refund, sweep paths
   - `raffle-instance/src/draw.rs`: Finalization, randomness handling, winner selection
   - `raffle-instance/src/events.rs`: Instance event structs and publish helpers
   - `raffle-instance/src/helpers.rs`: Shared internal helpers (guards, solvency, finalization internals)
   - `raffle-instance/src/init.rs`: Instance initialization and prize deposit
   - `raffle-instance/src/randomness.rs`: Randomness aggregation, VRF proof message, quorum seed combine
   - `raffle-instance/src/tickets.rs`: Ticket purchase, commit-reveal commits, NFT mint hooks
   - `raffle-instance/src/views.rs`: Instance read-only query surface
   - `raffle-instance/src/test.rs`: Test-only utilities and test harness (existing)
   - `raffle-shared/src/lib.rs`: Shared types crate root — RaffleConfig, enums, macros
   - `raffle-shared/src/constants.rs`: Protocol-wide magic numbers (single source of truth)
   - `raffle-shared/src/errors.rs`: Shared error types and ProtocolError enum
   - `raffle-shared/src/events.rs`: Shared event structs used across crates
   - `raffle-shared/src/config_builder.rs`: RaffleConfig builder helpers
   - `raffle-shared/src/nft_mint_test.rs`: NFT mint test harness

### Phase 2: README & Docs Index
5. Update README.md:
   - Add prominent **pre-audit / not production-ready** warning near the top
   - Add feature status table (Shipped / In progress / Planned) covering:
     - Shipped: Factory deploy, ticket sales, Internal PRNG draws, claims, refunds, creator profiles, pagination views, attestation data, protocol fees, pause controls, state checkpoints, recurring raffles, Quorum randomness mode
     - In progress: CommitReveal wiring, bundle pricing, batch refunds, draw-attestation end-to-end verification, TTL extension entrypoint
     - Planned: Oracle VRF service production hardening, swap-router integration, tikka-token incentives, multi-advisor governance, NFT ticket receipts, bundle pricing actualization
   - Add "Current Priorities" section linking to `[BUILD]` tagged issues (reference GitHub search: `is:issue is:open label:BUILD`)
   - Add explicit banner: "For authoritative architecture, storage, randomness, event, error, testing, and deployment docs, see the `docs/` directory."
   - Verify all code example function signatures match the current contract API (check against lib.rs entrypoints and RaffleConfig fields)
6. Review docs/README.md:
   - Verify every documented file exists under docs/
   - Verify all relative links resolve correctly
   - Add any missing cross-references to oracle/, fuzz/, scripts/ directories

### Phase 3: Toolchain Pinning Finalization
7. Keep `rust-toolchain.toml` as-is (already correct: 1.94.0, clippy, rustfmt, wasm32v1-none)
8. Keep root `Cargo.toml` rust-version in workspace.package (already correct)
9. Update `.github/workflows/ci.yml`:
   - Remove explicit `toolchain: stable` from the check job's dtolnay/rust-toolchain step
   - Remove the manual `rustup toolchain install` shell step in build_and_test — rustup auto-discovers rust-toolchain.toml
   - Simplify both steps to just use default rust-toolchain.toml resolution
10. Update `.devcontainer/Dockerfile`:
    - Replace the generic rust base image version or use explicit `rustup install 1.94.0` + `rustup default 1.94.0`
    - Replace `wasm32-unknown-unknown` target install with `wasm32v1-none`
    - Install clippy and rustfmt components explicitly via rustup
11. Update `docs/DEPLOYMENT.md`:
    - Add a "Toolchain Pinning" section recording the current pinned version (1.94.0), target (wasm32v1-none), and components (clippy, rustfmt)
    - Add a "Toolchain Upgrade Procedure" section requiring: a PR that bumps rust-toolchain.toml, runs `scripts/check_wasm_sizes.py` against the baseline in `baselines/wasm_sizes.json`, and reports the WASM byte size delta for both factory and instance contracts before merge

### Phase 4: CI/Local Pipeline Unification
12. Update `Makefile`:
    - Add `check`: `cargo check --workspace --all-targets --all-features`
    - Add `size-check`: `python3 scripts/check_wasm_sizes.py` (after build)
    - Add `docs-check`: `python3 scripts/generate_error_docs.py && python3 scripts/generate_event_docs.py && git diff --exit-code docs/ERRORS.md docs/EVENTS.md`
    - Add `shellcheck`: `shellcheck scripts/*.sh` (with note to install shellcheck)
    - Add `oracle-lint`: `cd oracle && npm run lint && npm run typecheck`
    - Add `ci` target that runs exactly: `fmt-check check clippy orphan-check build size-check docs-check test shellcheck oracle-ci`
      - where `fmt-check` = `cargo fmt --all -- --check`
      - `clippy` = `cargo clippy --all-targets --all-features -- -D warnings`
      - `orphan-check` = `python3 scripts/check_orphan_modules.py`
      - `test` = `cargo test --workspace && cargo test --workspace --doc`
      - `oracle-ci` = `cd oracle && npm ci && npm run format:check && npm run lint && npm run typecheck && npm run test:ci`
    - Keep existing `oracle-lint` (also referenced by pre-commit) separate from `oracle-ci`
    - Update `all` to be an alias for a developer-local subset if appropriate, but `ci` is the canonical one
13. Update `.github/workflows/ci.yml`:
    - In build_and_test: replace the cargo fmt, orphan modules, build, size check, error docs, event docs, clippy, test, doc-test steps with `make ci` invocation (or split check + build_and_test cleanly — make sure the check job still runs a quick `make check`)
    - In shellcheck job: replace `shellcheck scripts/*.sh` with `make shellcheck`
    - In oracle_check: replace individual npm steps with `make oracle-ci` or keep per-step but reference make targets
14. Update `.pre-commit-config.yaml`:
    - Replace `npx prettier --check` hook with `cd oracle && npm run format:check` (pinned to local devDependency)
    - Verify cargo-fmt-check and cargo-clippy hooks are already present (they are)
15. Final CONTRIBUTING.md cross-check: ensure CI section explicitly states "Run `make ci` before pushing; passing locally implies passing in CI"

## Dependencies and Considerations

- **Feature Status Table**: Stubs vs shipped must be derived from actual code — check for `unimplemented!()`, `todo!()`, or placeholder-returning functions in attestation.rs, randomness.rs (Quorum), claim.rs (batch_refund), draw.rs (CommitReveal), lib.rs (extend_ttl returns InvalidParameters)
- **WASM size baseline**: `baselines/wasm_sizes.json` must exist and be referenced by the upgrade procedure
- **rust-toolchain.toml 1.94 vs Cargo.toml rust-version 1.70 mismatch**: Cargo.toml's `rust-version = "1.70"` (MSRV) can be lower than the pinned toolchain. Keep both — 1.70 is the MSRV (minimum supported rust version), 1.94.0 is the pinned (reproduces exact WASM). Note this in DEPLOYMENT.md.
- **Makefile target ordering**: `size-check` depends on `build` (needs the compiled WASMs). `docs-check` must run generate scripts then diff exit code.
- **CI consistency**: The goal is that `make ci` locally and GitHub Actions produce the same result — so every step in CI must map exactly to a make target.
- **Shellcheck installation**: The Makefile shellcheck target should note it requires shellcheck installed (apt-get or brew)
- **Oracle ci vs lint**: `oracle-ci` = full npm install + format + lint + typecheck + test:ci; `oracle-lint` = just lint + typecheck (faster for pre-commit)

## Validation

1. **CONTRIBUTING.md**: Read the "Where does my code go?" and CI sections; confirm each of the 9 placement rules and file-size limit is present
2. **docs/ARCHITECTURE.md**: Confirm crate/module map lists every crate (factory, instance, shared, oracle, fuzz, scripts) and every sub-module with a one-line responsibility
3. **Rust files**: Grep for `#!\\[no_std\\]` in each lib.rs — ensure the line right after or before is a `//!...` module doc comment with a clear responsibility statement
4. **README.md**: Verify:
   - Pre-audit warning at top (visible, bold/blockquote)
   - Feature status table has at least 3 columns (Feature | Status | Notes) and 3 statuses (Shipped, In progress, Planned) with accurate assignments
   - `[BUILD]` issues link present
   - `docs/` authoritative-source banner present
   - All code snippets compile/render correctly (no non-existent methods)
5. **docs/README.md**: Follow every relative link in the file; ensure none 404
6. **rust-toolchain.toml**: Confirm channel=1.94.0, targets includes wasm32v1-none, components has clippy and rustfmt
7. **Cargo.toml workspace.package.rust-version**: Confirm it says "1.70" (MSRV)
8. **.github/workflows/ci.yml**:
   - No `toolchain:` argument to dtolnay/rust-toolchain in check job
   - No manual `rustup toolchain install` in build_and_test
   - build_and_test steps reference `make ci` or equivalent make targets
   - shellcheck job calls `make shellcheck`
9. **.devcontainer/Dockerfile**:
   - Explicit 1.94.0 pin via rustup or base image
   - `wasm32v1-none` target (not wasm32-unknown-unknown)
   - clippy + rustfmt components installed
10. **docs/DEPLOYMENT.md**:
    - Pinned version (1.94.0) + target + components recorded
    - Upgrade procedure requires PR with WASM size delta report
11. **Makefile**: Run `make -n ci` (dry run) to confirm target chain is correct; ensure all new targets (check, size-check, docs-check, shellcheck, oracle-lint, oracle-ci, ci) exist and .PHONY includes them
12. **.pre-commit-config.yaml**: Confirm prettier hook uses `npm run format:check` (not npx)
13. **Final sanity check**: `cargo check --workspace` passes after edits (no syntax changes should break it but header additions must be syntactically valid)

## Risks

- **Risk: Adding `//!` headers in the wrong order** — In Rust, crate-level `#!` attributes and `//!` docs must come before items. Handle carefully by inserting the `//!` immediately after any existing `#![...]` inner attributes in each file. **Handling**: Use Edit tool with precise old_string matching that includes the first few lines (no_std + deny attrs).
- **Risk: Makefile `ci` target shell ordering** — If docs-check runs before build or size-check before build, it fails. **Handling**: Use standard make dependencies (size-check depends on build, etc.) and order prereqs correctly in the `ci` target definition.
- **Risk: Feature status table accuracy** — Marking something "Shipped" when it's actually a stub (e.g., extend_ttl returns InvalidParameters). **Handling**: Cross-reference each claimed-shipped feature against the implementation before adding it to the table.
- **Risk: CI make target invocation fails in GitHub Actions** — GitHub Actions runners may not have stellar CLI installed in the early steps. **Handling**: Ensure the `stellar` CLI is still installed before any make target that calls `stellar contract build`. Currently CI already installs stellar CLI implicitly via the setup steps; verify this remains intact.
- **Risk: Cargo.toml rust-version mismatch** — The user request says 1.70 but rust-toolchain.toml is 1.94.0. These serve different purposes (MSRV vs pinned exact). **Handling**: Document this distinction in DEPLOYMENT.md and leave both values as-is (MSRV 1.70, pinned 1.94.0 for reproducibility).

# Deployment & Deterministic WASM Builds

This document describes the **reproducible deployment process** for the
Tikka raffle contracts.  The primary goals are:

1.  Any third party can compile the same source tree with the same toolchain
    pin and produce a byte-for-byte identical `.wasm` binary.
2.  A deployed on-chain WASM hash can be audited against a tagged commit.
3.  Rust toolchain upgrades are explicitly reviewed, versioned, and checked
    for WASM size regressions before merging.

---

## 1. Prerequisites & toolchain pinning

The project pins the Rust toolchain at the **repository root** in
[`rust-toolchain.toml`](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/rust-toolchain.toml).
This file is the **single source of truth**:

- `channel` – exact Rust version.
- `components` – `clippy`, `rustfmt` (matches `rust-version` in the Cargo
  workspace).
- `targets` – the Soroban WebAssembly target `wasm32v1-none`.

Additionally, the workspace [`Cargo.toml`](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/Cargo.toml)
declares `[workspace.package] rust-version = …` so that the cargo resolver
rejects dependency versions that require a newer compiler than the one we
officially support.

> **DO NOT** install Rust from your system package manager for deployments.
> Use `rustup`.  When you `cd` into this repo, `rustup` automatically reads
> `rust-toolchain.toml` and activates the pinned toolchain.

### Verifying your local toolchain

```bash
cd tikka-contracts
rustc --version        # MUST match rust-toolchain.toml → channel
rustup show            # Confirm "Active toolchain" points at the pinned one
rustup target list --installed | grep wasm32v1-none
```

Any deviation from the pinned channel means your `.wasm` output will differ
from the canonical CI artefact — **you must not deploy from a dirty
toolchain.**

---

## 2. Deterministic WASM build

The release profile in the workspace `Cargo.toml` is tuned for minimal and
reproducible `.wasm` output:

```toml
[profile.release]
opt-level = "z"         # Size optimizations (z = size, s = speed/size)
overflow-checks = true
debug = 0
strip = "symbols"
debug-assertions = false
panic = "abort"
codegen-units = 1       # Single codegen unit ⇒ deterministic
lto = "fat"
```

### One-shot build

```bash
# Recommended — this is exactly what CI runs:
make build

# Low-level equivalent:
cargo build \
  --target wasm32v1-none \
  --release \
  -p raffle-factory -p raffle-instance
```

Outputs land at:

- `target/wasm32v1-none/release/raffle_factory.wasm`
- `target/wasm32v1-none/release/raffle_instance.wasm`

### Size validation

```bash
make size-check
```

Each WASM binary MUST be below the 512 KB recommended ceiling.  Anything
above needs explicit design review.

---

## 3. Deployment record — toolchain & size MUST be captured

**For every on-chain deployment (testnet, mainnet, preview), append a row
to the deployment ledger below and commit it in the same PR that bumps the
deployed WASM.**

### Mandatory fields per deployment

| Field | Why |
|---|---|
| Commit SHA | Source code that was compiled. Must be tagged. |
| `rustc --version` (exact, from rustup) | Guarantees reproducibility. |
| `rust-toolchain.toml` content hash | Defensive check against unrecorded edits. |
| `sha256sum` of `raffle_factory.wasm` | Deployed hash ⇔ source proof. |
| `sha256sum` of `raffle_instance.wasm` | Same, for the instance binary. |
| WASM sizes (both, in bytes) | Baseline for detecting future regressions. |
| Network + block height | On-chain reference for the verifier. |
| Deployed factory address | Contracts can be looked up on-chain. |
| Deployed `InstanceWasmHash` | What factory's create_raffle() will deploy. |
| Operator identity / key path used | Operational audit. |
| Date (UTC) | Human-readable timestamp. |

**Do not deploy without filling this in.**  Third-party verifiers rely on
this table to confirm that the on-chain bytecode matches the audited source.

### Ledger template

| Field | Value |
|---|---|
| Commit SHA | `git rev-parse HEAD` → paste here |
| Tag | `vX.Y.Z-testnet.N` or `vX.Y.Z-mainnet.N` |
| rustc version | Output of `rustc --version` |
| toolchain.toml sha256 | `sha256sum rust-toolchain.toml` |
| factory.wasm sha256 | `sha256sum target/wasm32v1-none/release/raffle_factory.wasm` |
| instance.wasm sha256 | `sha256sum target/wasm32v1-none/release/raffle_instance.wasm` |
| factory.wasm KB | `wc -c` / 1024 |
| instance.wasm KB | same |
| Network (testnet / mainnet / …) | — |
| Block height | — |
| Factory address | `C…` |
| InstanceWasmHash | hex 64 chars |
| Operator / signing key | — |
| Date (UTC) | YYYY-MM-DD HH:MM |

Existing deployments are recorded in `deployments/<network>.json`; update
that file and this ledger together.

---

## 4. Verifying a deployed contract (third-party)

1.  Checkout the tagged commit from the deployment ledger.
2.  `rustup show` → confirm the pinned toolchain is provisioned; if not,
    rustup does it automatically on first `cargo` invocation.
3.  `make build` → produces the two `.wasm` files.
4.  `./scripts/verify.sh` (if deployer credentials are set up) downloads the
    remote WASM via `stellar contract inspect --wasm-hash …` and compares
    SHA-256s; or, do it by hand:
    ```bash
    local_sha=$(sha256sum target/wasm32v1-none/release/raffle_factory.wasm | cut -d' ' -f1)
    onchain_sha=$(stellar contract ins ... --output json | jq -r '.wasmHash')
    test "$local_sha" = "$onchain_sha" && echo "MATCH: verified reproducible build"
    ```
5.  Compare sizes, components, targets, and rustc version against the row
    in the ledger.  If any field differs, flag the deployment.

---

## 5. Toolchain upgrade process (REQUIRED — do not bump rust-toolchain.toml in a regular PR)

Raising the Rust toolchain channel (e.g. `1.83.0 → 1.84.0`) is a **special
change** that can silently alter WASM codegen, change binary sizes, and
even break previously reproducible deployments.  All toolchain upgrades
MUST go through the following gated process:

### Preconditions on a toolchain-upgrade PR

1.  **Header label:** `[TOOLCHAIN]` prefix in the PR title.
2.  **Three-file bump (atomic):**
    - `rust-toolchain.toml` → new channel + components + targets
    - `Cargo.toml` → `[workspace.package] rust-version = "X.Y.Z"` (MSRV)
    - `docs/DEPLOYMENT.md` → append to the upgrade-change log at the end
      of this file.
    These three files **must** be changed in the same commit.
3.  **WASM size delta table (non-negotiable):**

    | Binary | Old size (KB) | New size (KB) | Δ KB | Δ % |
    |---|---|---|---|---|
    | raffle_factory.wasm | — | — | — | — |
    | raffle_instance.wasm | — | — | — | — |

    Compute this BEFORE opening the PR by building old then new on the same
    machine with the same `target/` warm cache disabled (`cargo clean`
    between builds is fine).  Any regression > 2 % in either binary
    requires a written explanation of what changed in rustc/LLVM codegen
    and explicit approval by two reviewers.
4.  **Functional verification:**
    - `make ci` green on the PR (naturally).
    - At least one testnet redeploy of `raffle-instance` against the new
      toolchain and a full `cargo test --workspace` against it.
    - `./scripts/verify.sh` match confirmed.
5.  **Changelog note:** 3-5 bullets of *why* we are upgrading (e.g. new
    stable rustc feature, security CVE, known codegen quality regression
    in the old version).  "Just because there is a new release" is not a
    valid reason.

### Approval

- Toolchain upgrades require **two approvals**, one of which must be a
  maintainer who has previously done a reproducible-build verification.
- The merge commit MUST be GPG-signed by the merger, and the signature
  recorded in the deployment ledger for audit purposes.

---

## 6. Onward checklist for every release PR

Copy this checklist into every release / deploy PR description:

- [ ] `rust-toolchain.toml` did not change outside of a `[TOOLCHAIN]` PR.
- [ ] `Cargo.toml` `rust-version` matches `rust-toolchain.toml`.
- [ ] `make ci` passes locally and on CI.
- [ ] `make size-check` reports both binaries under 512 KB.
- [ ] Deployment ledger row above filled in with all 10 required fields.
- [ ] `./scripts/verify.sh` reports `Match: YES` against the target network.
- [ ] TTL bump tooling runbook in DEVELOPMENT.md was executed immediately
      after deploy (factory + instances; instance persistent `Ticket(u32)`
      keys are a common gotcha).
- [ ] `docs/ERRORS.md`, `docs/EVENTS.md`, and `docs/STORAGE.md` reflect
      the state of the freshly deployed code.

---

## 7. Toolchain change log

| From → To | Reason | Size Δ F/Δ I | Approved by | Date | Commit |
|---|---|---|---|---|---|
| *Initial pin: 1.83.0* | Baseline | — | bootstrap | 2026-08-29 | (initial commit) |

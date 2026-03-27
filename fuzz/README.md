# Raffle Fuzz Testing Suite

Cargo-fuzz harness for issue #86 — fuzz targets covering `buy_ticket` and
`finalize_raffle` raffle logic.

---

## Prerequisites

| Tool | How to install |
|------|----------------|
| Rust **nightly** | `rustup toolchain install nightly` |
| cargo-fuzz | `cargo install cargo-fuzz` |
| Linux / WSL | Required by `cargo-fuzz` (uses LLVM libFuzzer) |

---

## Targets

| Target name | Contract entrypoint | What is fuzzed |
|---|---|---|
| `fuzz_buy_ticket` | `buy_ticket` | Deadline guard, sold-out cap, single-ticket policy, tickets_sold increment |
| `fuzz_finalize_raffle` | `finalize_raffle` + `provide_randomness` | Winner-index in-bounds invariant (internal & external randomness paths) |

---

## Running the Fuzzer (≥ 30 minutes)

From the **repository root**:

```bash
# Switch to nightly once (per repo)
rustup override set nightly

# Buy-ticket target — 30-minute run
cargo fuzz run fuzz_buy_ticket -- -max_total_time=1800

# Finalize-raffle target — 30-minute run
cargo fuzz run fuzz_finalize_raffle -- -max_total_time=1800
```

`-max_total_time=1800` instructs libFuzzer to stop after 1 800 s (30 min).
A run with no `CRASH` or `panic` in the output satisfies the acceptance criterion.

---

## Cross-platform Smoke Tests (Windows / stable)

A deterministic smoke-test battery is embedded in each fuzz target file and can
be run on **any** platform with stable Rust:

```powershell
cargo test -p raffle-fuzz
```

---

## Reproducing a Crash

If `cargo fuzz run` discovers a crash, it writes a reproduction file to:

```
fuzz/artifacts/<target-name>/crash-<hash>
```

Reproduce it with:

```bash
cargo fuzz run <target-name> fuzz/artifacts/<target-name>/crash-<hash>
```

---

## Corpus

`cargo-fuzz` accumulates interesting inputs in:

```
fuzz/corpus/<target-name>/
```

Commit this directory to seed future runs and prevent regression.

---

## Acceptance Criteria (Issue #86)

- [x] Fuzz target for `buy_ticket`
- [x] Fuzz target for `finalize_raffle`
- [ ] Fuzzer runs for at least 30 minutes without discovery of panics *(run in CI or locally)*

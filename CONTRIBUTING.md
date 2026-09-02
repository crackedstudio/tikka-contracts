# Contributing to Tikka Contracts

Thanks for your interest in contributing! This project targets Stellar/Soroban smart contracts
and welcomes PRs for fixes, tests, and documentation.

---

## Getting started

1. Fork the repository and create a feature branch.
2. Install the pinned toolchain (handled automatically by `rustup`):
   ```bash
   rustup show   # reads rust-toolchain.toml and installs if missing
   ```
3. Install pre-commit hooks (optional but recommended for fast feedback):
   ```bash
   pip install pre-commit
   pre-commit install
   ```
4. Make your changes with clear, focused commits.
5. Run the full CI pipeline locally before pushing — **this is mandatory**:
   ```bash
   make ci
   ```

---

## Mandatory pre-push check

```bash
make ci
```

`make ci` runs every check that GitHub Actions runs, in the same order:

| Step | Command | What it catches |
|---|---|---|
| Format | `cargo fmt --all -- --check` | Unformatted Rust |
| Type-check | `cargo check --workspace` | Compile errors without full codegen |
| Lint | `cargo clippy … -D warnings` | Style and correctness warnings |
| Tests | `cargo test --workspace` | Failing unit/integration tests |
| WASM size | custom | Binaries > 200 kB |
| Docs | `cargo doc -D warnings` | Missing or broken rustdoc |
| Oracle lint | `tsc --noEmit` | TypeScript type errors |

If `make ci` passes locally, it will pass in CI. There should be no surprises.

---

## Where does my code go?

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full module map.
Quick reference:

| What you're adding | Where |
|---|---|
| New public entrypoint | `lib.rs` (thin delegation only, ≤ 30 lines per function) |
| Business logic | New module file, imported in `lib.rs` |
| New on-chain event | `events.rs` in the relevant contract |
| New error variant | `Error` enum in `lib.rs` **+ entry in `docs/ERRORS.md`** |
| New storage key | `DataKey` enum in `lib.rs` **+ entry in `DEVELOPMENT.md` key table** |
| Shared type (used by 2+ crates) | `raffle-shared/src/lib.rs` |
| Constants | `raffle-shared/src/lib.rs` (shared) or top of the relevant module |
| Randomness / winner selection | `raffle-instance/src/randomness.rs` |
| Tests | `src/test.rs` — **never inline in `lib.rs`** |
| Oracle TypeScript | `oracle/src/` appropriate subdirectory |
| Shell scripts | `scripts/` |
| Documentation | `docs/` |

### File-size rule

**Hard limit: 600 lines per `.rs` file.**

When a file approaches 600 lines, extract a cohesive group of functions into a
new module before adding more code. This keeps diffs reviewable and modules
single-purpose.

### Adding a new error variant

1. Append the variant to the `Error` enum in `lib.rs` (do **not** renumber existing variants).
2. Add a row to `docs/ERRORS.md` with the numeric code, variant name, description, and
   suggested frontend message.
3. Add at least one test that triggers the new error path.

### Adding a new event

1. Add the event struct to `events.rs` in the relevant contract.
2. Implement a `publish(&self, env: &Env)` method.
3. Add a row to `docs/EVENTS.md`.

---

## Tests

Run the full suite:
```bash
cargo test --workspace
```

Run a specific crate:
```bash
cargo test -p raffle-instance
cargo test -p raffle-factory
```

- All tests live in `src/test.rs` (or `src/tests/` for larger test suites).
- Never add `#[cfg(test)]` blocks inline in `lib.rs`.
- Write at least one test for every new error path and every new happy path.

---

## Module header template

Every new `.rs` module must start with a module-level doc comment:

```rust
//! Brief one-line description of what this module does.
//!
//! ## Responsibilities
//! - Responsibility A
//! - Responsibility B
//!
//! ## What does NOT belong here
//! - Cross-cutting concern X → see `other_module.rs`
```

---

## Pull requests

- Provide a concise summary of what changed and why.
- Link any relevant issues.
- Note any follow-up work or known limitations.
- Ensure `make ci` passes before opening the PR.
- Keep PRs focused — one logical change per PR makes review faster.

---

## Code of conduct

Be respectful and constructive. Harassment or abuse is not tolerated.

# Contributing

Thanks for your interest in contributing to Tikka! This project targets Stellar/Soroban smart contracts and welcomes PRs for improvements, tests, and docs.

## Getting Started

1. Fork the repository and create a feature branch.
2. Make your changes with clear, focused commits.
3. Run `cargo fmt --all` to format code before committing.
4. Run tests locally before opening a PR.
5. Install the recommended VS Code extensions when prompted and keep format-on-save enabled.
6. Install the local hooks with `pip install pre-commit && pre-commit install`.

Setup problems (missing WASM target, Stellar CLI vs SDK 23 mismatch, Node 20 for `oracle/`, `stellar` vs `soroban` naming, deploy script paths) are answered in [`docs/FAQ.md`](docs/FAQ.md).

## Finding an Issue

New contributors should start by finding an issue labeled **`good first issue`**. This label marks tasks scoped for learning the codebase with minimal risk.

### Issue Labels & Difficulty

- **`good first issue`**: Scoped for newcomers. Self-contained, well-documented, and low risk. Start here.
- **`help wanted`**: Contribution welcome but may require some codebase familiarity.
- **`difficulty: easy`**: Straightforward task, likely in one module or document.
- **`difficulty: medium`**: Requires understanding multiple components or APIs.
- **`difficulty: hard`**: Complex, cross-cutting, or touches contract internals.
- **`type: bug`**: Defect that needs fixing.
- **`type: feature`**: New capability or enhancement.
- **`type: docs`**: Documentation improvement.

### Assignment Etiquette

Before starting work on an issue:

1. **Check if it's assigned**: If someone is already working on it, pick a different issue.
2. **Comment to claim it**: Reply with "I'd like to work on this" or similar. This signals your intent and prevents duplicate work.
3. **Get guidance if unsure**: Ask for clarification on scope or approach in the issue comments. The maintainers will help.

### Finding Good First Issues

**GitHub filter**: [Good first issue filter](https://github.com/stellar/tikka-contracts/issues?q=is%3Aissue+is%3Aopen+label%3A"good+first+issue")

You can also filter by `good first issue` and `type:docs` to start with documentation improvements, which are lower-risk and help the community.

### Terminology Reference

Unfamiliar with a term in the issue? Check [`docs/GLOSSARY.md`](docs/GLOSSARY.md) for one-paragraph definitions with code references.

## Where does my code go?

Use the following placement rules to keep the workspace searchable and to avoid
accidentally turning a crate root into a monolith. Files should stay around
**~600 lines maximum**; when a module grows beyond that scale, split it along
responsibility lines (tests, helpers, views) rather than piling on.

| What are you adding?                                                         | Where it goes                                                                                                     |
| ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| New contract entrypoint (`#[contractimpl]` method)                          | The topical module inside the crate — `admin.rs`, `claim.rs`, `draw.rs`, `tickets.rs`, `views.rs`. `lib.rs` holds only a one-line delegation. |
| New numeric or string constant                                              | [`contracts/raffle-shared/src/constants.rs`](contracts/raffle-shared/src/constants.rs). **Never inline a magic number.** |
| New error variant on a `#[contracterror]` enum                               | Append a fresh, unused discriminant in the relevant range (1–99 shared, 100–199 instance, 200–299 factory). Then regenerate [`docs/ERRORS.md`](docs/ERRORS.md) with `python3 scripts/generate_error_docs.py`. |
| New event (`#[contractevent]` struct)                                        | `events.rs` in the relevant crate (`raffle-shared`, `raffle-factory`, or `raffle-instance`). Regenerate [`docs/EVENTS.md`](docs/EVENTS.md) with `python scripts/generate_event_docs.py`. Every struct and every field must carry a `///` doc comment. |
| New persistent / temporary / instance storage key                            | The crate's `DataKey` enum (inside `contracts/<crate>/src/lib.rs`). Document the key, its tier, and its write pattern in [`docs/STORAGE.md`](docs/STORAGE.md). |
| New type shared across factory and instance (config, enums, client traits)  | [`contracts/raffle-shared/`](contracts/raffle-shared/). Unit tests for shared types live alongside.               |
| Tests for a contract entrypoint or helper                                    | `src/tests/<name>.rs` under the relevant crate. **Never inline `#[test]` blocks inside `lib.rs` except for tiny shared-type unit tests.** |
| Oracle / off-chain service logic (VRF, queue, listener, alerter, …)         | Matching directory under [`oracle/src/<name>/`](oracle/src/) (e.g. `oracle/src/vrf/`, `oracle/src/queue/`). Unit tests live in the same directory. |
| Script for build / deploy / verification                                    | [`scripts/`](scripts/). Run `shellcheck scripts/*.sh` against any new `.sh` file.                                |

## Development Expectations

- Keep changes scoped and easy to review.
- Write tests for new behavior when possible.
- Every new privileged entrypoint must have both a positive authorization test for the configured admin and a negative test proving a non-admin is rejected. Keep these checks table-driven where the entrypoints share setup so missing coverage is visible in review.
- Update documentation if behavior or APIs change.
- Include the corresponding documentation update in the same PR whenever
	behavior or an API changes; mark unfinished behavior as unimplemented.

## Tests

```bash
cargo test -p raffle-factory
cargo test -p raffle-instance
```

## Error Documentation Sync

If you modify or add any variants to the `Error` enum in `contracts/raffle-instance/src/lib.rs`
or the `ContractError` enum in `contracts/raffle-factory/src/lib.rs`, regenerate
`docs/ERRORS.md` before committing:

```bash
python3 scripts/generate_error_docs.py
```

CI will fail if `docs/ERRORS.md` is out of sync with either enum. Every variant must
have a `///` doc comment; duplicate discriminants and undeclared-but-used variants
also fail the generator.

## Continuous Integration

All CI jobs must pass before a pull request can merge. A red build blocks merge —
there are no exceptions for individual jobs. Required checks on `master` are:

- `Build, Test, and Lint` (includes `cargo check`, formatting, clippy, and tests)
- `Oracle Service CI`
- `Shell Script Checks`

Repository admins must enable branch protection on `master` so these checks are
required and branches must be up to date before merging. Workflow changes land in
PRs first; enforcement is enabled once the pipeline is green.

### Required pre-push check: `make ci`

Before you push, run **`make ci`** from the repository root. It runs exactly the
same pipeline CI runs (fmt check, `cargo check`, clippy, orphan-module check,
contract build, WASM size check, docs sync check, full test suite including
doc-tests, `shellcheck` over `scripts/*.sh`, and the oracle `npm ci` +
typecheck + lint + format + test:ci suite). Passing `make ci` locally implies
passing in CI. If a check fails locally that the remote would also catch, fix
it before pushing — it saves everyone queue time.

## Events Documentation Sync

If you modify or add any `#[contractevent]` struct in
`contracts/raffle-shared/src/events.rs`, `contracts/raffle-factory/src/events.rs`,
or `contracts/raffle-instance/src/events.rs`, regenerate `docs/EVENTS.md` before
committing:

```bash
python scripts/generate_event_docs.py
```

Every event struct **and** every field must carry a `///` doc comment, and
numeric fields must state whether they are 0-based **indices** or 1-based
**IDs** (see the "Index-vs-ID convention" section of `docs/EVENTS.md`).

CI will fail if `docs/EVENTS.md` is out of sync with the event structs.

## Code Coverage

Coverage is collected in CI for both the Rust workspace (`cargo llvm-cov`) and
the oracle service (Jest with `--coverage`):

- Rust and oracle `lcov` artifacts are uploaded as build artifacts.
- Rust line coverage is enforced via a **ratchet**:
  `scripts/check_coverage_ratchet.py` compares the current `lcov` output with
  the committed baseline in `coverage/coverage-ratchet.json`. Coverage must
  never **decrease** relative to that baseline; increases are automatically
  adopted.

To arm an updated ratchet after a big behavior change, regenerate the baseline
and commit it in the same PR:

```bash
python scripts/check_coverage_ratchet.py \
  --lcov coverage/lcov.info \
  --baseline coverage/coverage-ratchet.json \
  --update
```

## Markdown

Run markdownlint before opening a PR to keep documentation style consistent:

```bash
npx markdownlint-cli2 "**/*.md"
```

The configuration lives in `.markdownlint.jsonc`. Auto-fixable issues can be resolved with `npx markdownlint-cli2 --fix "**/*.md"`.

## Error Code Policy

Error codes are defined in `#[contracterror]` enums and are part of the on-chain ABI.
Once a code is assigned it is **never reused or reassigned**, even if the variant is
later deprecated.  New errors must follow the reserved ranges:

| Range     | Owner            | Purpose                                      |
| --------- | ---------------- | -------------------------------------------- |
| 1 – 99    | Shared           | Conditions used by both instance and factory  |
| 100 – 199 | Instance-only    | New instance-specific errors                 |
| 200 – 299 | Factory-only     | New factory-specific errors                  |

If a new shared condition is needed, add it to `ProtocolError` in
`contracts/raffle-shared/src/errors.rs` with a code in the 1–99 range, then update
both `raffle-instance/src/lib.rs` and `raffle-factory/src/lib.rs` to use it.

Run `python scripts/check_error_codes.py` before submitting a PR to verify no
duplicate discriminants exist.

## Pull Requests

- Provide a concise summary of what changed and why.
- Link any relevant issues.
- Note any follow-up work or limitations.
- Use the PR template at `.github/PULL_REQUEST_TEMPLATE.md` to ensure all required information is included.

## Dependency updates

Dependabot opens weekly PRs for **GitHub Actions**, **Cargo**, **npm** (`oracle/`), and
**Docker** (`oracle/Dockerfile`). Review policy:

| Ecosystem | Grouping | Review expectations |
|-----------|----------|---------------------|
| **Cargo — production** | Individual PRs for `soroban-sdk`, `ed25519-dalek`, `sha2`, and other runtime/crypto deps | Rebuild WASM (`cargo build --target wasm32-unknown-unknown --release`), run the full test suite, and verify host behaviour before merge. Soroban SDK minors can change contract semantics. |
| **Cargo — dev tooling** | Grouped (`proptest`, test helpers, fuzz crates, etc.) | Run `cargo test --workspace` and `cargo clippy`. |
| **npm — production** | Individual PRs for `@stellar/stellar-sdk`, `dotenv` | Confirm SDK major matches the Soroban protocol (see `oracle/README.md`), run `npm test` and `npm run build`. |
| **npm — dev tooling** | Grouped (Jest, TypeScript, Prettier, types) | Run `npm test` and `npm run format:check`. |
| **Docker** | Individual PRs for base image bumps | Rebuild the oracle image and smoke-test `/health`. |
| **GitHub Actions** | Individual PRs per action bump | Ensure SHA pins include a version comment; `actionlint` must pass. |

All dependency PRs route to `@crackedstudio/maintainers` via CODEOWNERS. Do not merge
supply-chain bumps without maintainer approval.

## Supply-chain policy (`cargo-deny`)

CI runs [`cargo-deny`](https://embarkstudios.github.io/cargo-deny/) on every PR using
the root [`deny.toml`](deny.toml). The policy enforces allowed licenses, warns on
duplicate crate versions, and denies known advisories. Advisories are also refreshed
on a weekly schedule.

### Adding an exception

If `cargo-deny` reports a finding that cannot be resolved immediately:

1. Prefer fixing the dependency (upgrade, replace, or remove) over allowlisting.
2. If an allowlist entry is unavoidable, add it to the relevant section in `deny.toml`
   with an inline `reason` (or `ignore` entry for advisories) explaining the risk
   accepted and the planned remediation.
3. Obtain approval from a `@crackedstudio/maintainers` reviewer — exceptions require
   explicit maintainer sign-off in the PR.
4. Link any related RUSTSEC advisory ID in the PR description.

Run locally before opening a PR:

```bash
cargo install cargo-deny --locked
cargo deny check
```

## Stale issues and PRs

To keep the contribution queue healthy, we run GitHub's [`actions/stale`](https://github.com/actions/stale) bot (see [`.github/workflows/stale.yml`](.github/workflows/stale.yml)):

- Issues and pull requests with no activity for **21 days** are marked `stale` with a friendly reminder.
- If there is still no activity for **7 more days**, they are closed automatically.
- Items labeled `critical` or assigned to a milestone are exempt.

If your issue or PR is marked stale and you are still working on it, leave a comment or push an update and we will gladly keep it open.

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

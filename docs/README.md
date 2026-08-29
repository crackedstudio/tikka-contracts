# Docs Index

This `docs/` folder is the **authoritative reference** for the Tikka raffle
contracts.  The top-level `README.md` is a human-readable introduction and
status dashboard; for implementation details, error codes, event schemas,
storage semantics, deployment process, or architecture decisions, read the
document below that covers your topic.

---

## Quick links

| Document | Covers | Audience |
|---|---|---|
| [ARCHITECTURE.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/docs/ARCHITECTURE.md) | Crate & module map, dependency graph, off-chain directory layout, decision tree for where new code goes | Every contributor, code reviewer |
| [ERRORS.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/docs/ERRORS.md) | Full discriminant table for both factory & instance error enums, with frontend mapping + React example | Frontend, integrators, indexer teams |
| [EVENTS.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/docs/EVENTS.md) | Every `#[contractevent]`, two-symbol topic scheme, lifecycle/admin/internal tables, indexer notes | Indexer, analytics, UI teams |
| [STORAGE.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/docs/STORAGE.md) | `DataKey → Tier → Semantics → TTL risk → Bump priority` for every storage key in both contracts; phase-based bump schedules; new-key checklist | Operators, auditors, anyone adding a storage key |
| [DEPLOYMENT.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/docs/DEPLOYMENT.md) | Toolchain pinning (`rust-toolchain.toml`), deterministic WASM build & verify, deployment checklist, toolchain upgrade process, WASM size-change audit requirement | DevOps, release managers, auditors, anyone doing an on-chain deploy |

---

## Top-level companion documents (not in `docs/`)

These live at the repo root but are equally important:

| Document | Covers |
|---|---|
| [README.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/README.md) | Project elevator pitch, **Feature Status Table (honest)**, pre-audit warning, minimal onboarding to build & test |
| [CONTRIBUTING.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/CONTRIBUTING.md) | `make ci` pre-push mandate, code placement rules ("Where does my code go?"), ~600-line soft ceiling, file layout conventions, PR checklist |
| [DEVELOPMENT.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/DEVELOPMENT.md) | Local dev setup, TTL risk matrix, Stellar CLI installation, script usage (`deploy-*.sh`, `invoke.sh`, `fund-testnet.sh`, `verify.sh`) |
| [fuzz/README.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/fuzz/README.md) | How to run the cargo-fuzz targets for `buy_tickets` and `finalize_raffle` | Auditors, QA |
| [oracle/README.md](file:///c:/Users/USA/Documents/Osuocha/tikka-contracts/oracle/README.md) | Off-chain oracle service (TypeScript) overview & status | Oracle integrators |

---

## Rule of thumb: README vs docs/

-   **Unsure whether a feature is shipped?** → `README.md` Feature Status Table.
-   **Want to understand why the code is laid out the way it is?** → `ARCHITECTURE.md`.
-   **Debugging a `ScVal` error from the SDK?** → `ERRORS.md`.
-   **Indexer not seeing the event you expected?** → `EVENTS.md`.
-   **Storage key expired or need to set up the TTL bot?** → `STORAGE.md` + `DEVELOPMENT.md`.
-   **Deploying to a network and want reproducible WASM?** → `DEPLOYMENT.md` + `scripts/verify.sh`.
-   **Writing a PR and not sure which file to edit?** → `CONTRIBUTING.md` + `ARCHITECTURE.md` decision tree.

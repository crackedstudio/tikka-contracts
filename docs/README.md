# Tikka Documentation Index

This directory contains comprehensive documentation for the Tikka decentralized raffle platform. Each document serves a specific audience and purpose.

## Document Overview

### [GLOSSARY.md](GLOSSARY.md)

**Audience:** All  
Defines key terms used throughout the Tikka system (e.g., `RaffleStatus`, `Ticket`, `Randomness Source`) with one-paragraph explanations and direct links to their code definitions. Start here when you encounter unfamiliar terminology.

### [ARCHITECTURE.md](ARCHITECTURE.md)

**Audience:** Contributors, Integrators, Auditors  
Explains the high-level architecture of the Tikka system, including the factory pattern, raffle instances, oracle service, and client interactions. Includes Mermaid diagrams showing the factory → instance → oracle flow and the raffle state machine. Essential for understanding how components interact.

### [DEPLOYMENT.md](DEPLOYMENT.md)

**Audience:** Contributors, Operators  
Complete guide to deploying Tikka contracts to testnet and mainnet. Covers the `scripts/` deployment toolchain, environment variables, deployment steps, and the `deployments/` registry. Includes prerequisites for Rust, Stellar CLI v23.x, and WASM targets. Required for anyone deploying or operating the protocol.

### [RANDOMNESS.md](RANDOMNESS.md)

**Audience:** Contributors, Integrators, Auditors  
Detailed explanation of the three randomness modes: Internal PRNG, External oracle, and CommitReveal. Includes a decision table for choosing the appropriate mode based on trust assumptions, cost, and prize scale. Critical for understanding winner selection security properties.

The independent draw-attestation material is consolidated at the end of this document. It describes the current source status as unverified where the implementation is not wired into the module tree.

### [DEVELOPMENT.md](DEVELOPMENT.md)

**Audience:** Contributors
Development setup, local checks, repository conventions, and links to the focused testing and deployment guides. Build claims are subject to the current repository build status.

### [COMMIT_REVEAL.md](COMMIT_REVEAL.md)

**Audience:** Contributors, Integrators  
In-depth technical documentation of the Commit-Reveal randomness protocol. Explains the four-phase lifecycle (creation, commit, draw, reveal), ticket transfer invariants, fallback behavior, and includes TypeScript and Rust code examples for generating commit hashes.

### [CREATOR_PROFILES.md](CREATOR_PROFILES.md)

**Audience:** Frontend Developers, Integrators  
Documentation for the on-chain creator profile system. Explains display names, verified badges, and track records. Covers profile management APIs, frontend integration patterns, trust indicators, and storage costs. Required for building creator reputation features.

### [STORAGE.md](STORAGE.md)

**Audience:** Contributors, Operators, Auditors  
Canonical map of Soroban storage keys for both factory and instance contracts. Documents storage tiers (instance, persistent, temporary), write patterns, archival risks, and TTL bump policies. Essential for operators managing contract storage lifetime and auditors reviewing data persistence.

### [EVENTS.md](EVENTS.md)

**Audience:** Integrators, Frontend Developers, Indexers  
Comprehensive reference for all events emitted by the Tikka raffle system. Covers both factory and instance events with field types, emission conditions, and indexer implementation notes. Required for building frontends, event listeners, or off-chain indexers.

### [ERRORS.md](ERRORS.md)

**Audience:** Contributors, Integrators, Frontend Developers  
Complete documentation of all error codes used in Tikka contracts. Includes error code mappings, descriptions, and suggested frontend messages. Contains code examples for React error handling and testing guidance. Essential for frontend developers building user-friendly error displays.

### [FEE_MODEL.md](FEE_MODEL.md)

**Audience:** Contributors, Integrators  
Explains the implemented Tikka protocol fee model. Fees are currently
collected at ticket purchase only; prize-claim fees are not implemented.

### [FAQ.md](FAQ.md)

**Audience:** Contributors  
Troubleshooting guide for common setup problems encountered during development. Covers WASM target issues, Stellar CLI version skew, Node.js requirements, environment variable configuration, and package naming after the factory rename. First stop when encountering build or deployment errors.

### [TESTING.md](TESTING.md)
**Audience:** Contributors  
How to run and extend unit, integration, and fuzz tests across contracts (`cargo test -p …`), the oracle Jest suite (`npm test`), and `cargo-fuzz` targets. Documents `test.rs` helper conventions and when to add a fuzz harness versus a focused unit test.

### [MIGRATION-426.md](MIGRATION-426.md)

**Audience:** Contributors, Operators, Auditors  
Migration guide for PR #426, which changed the factory contract storage from a `Vec`-based layout to a stable-index map. Documents the complexity improvements, stable ID system, new public entry points, and provides testnet migration steps. Historical reference for understanding storage evolution.

### [VIEWS.md](VIEWS.md)

**Audience:** Integrators, Frontend Developers  
Documents the read-only query surface extracted into `contracts/raffle-factory/src/views.rs`. Covers all 14 view functions, pagination conventions (`effective_limit`, `PageResultRaffles`), and provides a table of the complete query surface for integrators.

## Recommended Reading Order for New Contributors

1. **Start here:** [ARCHITECTURE.md](ARCHITECTURE.md) — Understand the big picture and component relationships
2. **Development setup:** [FAQ.md](FAQ.md) — Review common setup issues before you encounter them
3. **Testing:** [TESTING.md](TESTING.md) — Run unit, integration, oracle, and fuzz tests
4. **Deployment:** [DEPLOYMENT.md](DEPLOYMENT.md) — Learn how to build and deploy contracts
5. **Core protocol:** [RANDOMNESS.md](RANDOMNESS.md) — Understand winner selection security
6. **Storage:** [STORAGE.md](STORAGE.md) — Learn about storage layout and TTL management
7. **Integration:** [EVENTS.md](EVENTS.md) and [ERRORS.md](ERRORS.md) — Reference for building integrations

For auditors, add [COMMIT_REVEAL.md](COMMIT_REVEAL.md) and [FEE_MODEL.md](FEE_MODEL.md) to understand protocol specifics and economic design.

## Related Documentation

- [Project overview](../README.md)
- [Contributing guidelines](../CONTRIBUTING.md)
- [Security policy](../SECURITY.md)
- [Support policy](../SUPPORT.md)
- [Code of conduct](../CODE_OF_CONDUCT.md)
- [Changelog](../CHANGELOG.md)
- [License](../LICENSE)
- [Oracle service README](../oracle/README.md)
- [Oracle runbook](../oracle/RUNBOOK.md)
- [Fuzzing README](../fuzz/README.md)

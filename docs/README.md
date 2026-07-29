# Tikka Documentation Index

This directory contains comprehensive documentation for the Tikka decentralized raffle platform. Each document serves a specific audience and purpose.

## Document Overview

### [ARCHITECTURE.md](ARCHITECTURE.md)

**Audience:** Contributors, Integrators, Auditors  
Explains the high-level architecture of the Tikka system, including the factory pattern, raffle instances, oracle service, and client interactions. Includes Mermaid diagrams showing the factory → instance → oracle flow and the raffle state machine. Essential for understanding how components interact.

### [DEPLOYMENT.md](DEPLOYMENT.md)

**Audience:** Contributors, Operators  
Complete guide to deploying Tikka contracts to testnet and mainnet. Covers the `scripts/` deployment toolchain, environment variables, deployment steps, and the `deployments/` registry. Includes prerequisites for Rust, Stellar CLI v23.x, and WASM targets. Required for anyone deploying or operating the protocol.

### [RANDOMNESS.md](RANDOMNESS.md)

**Audience:** Contributors, Integrators, Auditors  
Detailed explanation of the three randomness modes: Internal PRNG, External oracle, and CommitReveal. Includes a decision table for choosing the appropriate mode based on trust assumptions, cost, and prize scale. Critical for understanding winner selection security properties.

### [COMMIT_REVEAL.md](COMMIT_REVEAL.md)

**Audience:** Contributors, Integrators  
In-depth technical documentation of the Commit-Reveal randomness protocol. Explains the four-phase lifecycle (creation, commit, draw, reveal), ticket transfer invariants, fallback behavior, and includes TypeScript and Rust code examples for generating commit hashes.

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
Explains the Tikka protocol fee model, including fee collection points at ticket purchase and prize claim. Provides formulas, examples, and effective total fee calculations. Useful for understanding protocol economics and revenue distribution.

### [env/README.md](env/README.md)

**Audience:** Contributors, Operators  
Single source of truth for every environment variable read across the Tikka monorepo. Covers oracle service variables (required, optional, and integration-test-only), deployment script variables, and a cross-reference table showing which variables are shared between services. Start here when setting up a local environment.

### [FAQ.md](FAQ.md)

**Audience:** Contributors  
Troubleshooting guide for common setup problems encountered during development. Covers WASM target issues, Stellar CLI version skew, Node.js requirements, environment variable configuration, and package naming after the factory rename. First stop when encountering build or deployment errors.

### [MIGRATION-426.md](MIGRATION-426.md)

**Audience:** Contributors, Operators, Auditors  
Migration guide for PR #426, which changed the factory contract storage from a `Vec`-based layout to a stable-index map. Documents the complexity improvements, stable ID system, new public entry points, and provides testnet migration steps. Historical reference for understanding storage evolution.

## Recommended Reading Order for New Contributors

1. **Start here:** [ARCHITECTURE.md](ARCHITECTURE.md) — Understand the big picture and component relationships
1. **Environment setup:** [env/README.md](env/README.md) — Configure required environment variables before running anything
1. **Development setup:** [FAQ.md](FAQ.md) — Review common setup issues before you encounter them
1. **Deployment:** [DEPLOYMENT.md](DEPLOYMENT.md) — Learn how to build and deploy contracts
1. **Core protocol:** [RANDOMNESS.md](RANDOMNESS.md) — Understand winner selection security
1. **Storage:** [STORAGE.md](STORAGE.md) — Learn about storage layout and TTL management
1. **Integration:** [EVENTS.md](EVENTS.md) and [ERRORS.md](ERRORS.md) — Reference for building integrations

For auditors, add [COMMIT_REVEAL.md](COMMIT_REVEAL.md) and [FEE_MODEL.md](FEE_MODEL.md) to understand protocol specifics and economic design.

## Related Documentation

- **Development guide:** See [`../DEVELOPMENT.md`](../DEVELOPMENT.md) for local build workflows and TTL bump examples
- **Contributing guidelines:** See [`../CONTRIBUTING.md`](../CONTRIBUTING.md) for contribution expectations and PR process
- **Project overview:** See [`../README.md`](../README.md) for feature descriptions and getting started

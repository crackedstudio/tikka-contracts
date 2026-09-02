# Tikka — Decentralized Raffle Platform

> ⚠️ **Pre-audit — not production-ready.**
> This codebase has not been independently audited. Do not deploy to mainnet or
> handle real funds until a full security review is complete.
> See the [status table](#feature-status) for what is and is not shipped.

Tikka is a decentralized raffle platform built on Stellar using Soroban smart contracts.
Creators deposit a prize, sell tickets priced in any Stellar asset, and distribute prizes
on-chain using either an internal PRNG draw (low-stakes) or an external VRF oracle (high-stakes).

---

## Feature status

| Feature | Status | Notes |
|---|---|---|
| Raffle creation via factory | ✅ Shipped | |
| Prize deposit and escrow | ✅ Shipped | |
| Ticket purchase (`buy_tickets`) | ✅ Shipped | |
| Internal PRNG draw (`finalize_raffle`) | ✅ Shipped | Low-stakes only — see randomness note |
| Multiple prize tiers | ✅ Shipped | Basis-point splits |
| Configurable claim lockup | ✅ Shipped | Default 1 hour, max 7 days |
| Protocol fee collection | ✅ Shipped | Basis-point fee to treasury |
| Raffle cancellation + ticket refunds | ✅ Shipped | |
| Emergency withdraw (admin/creator) | ✅ Shipped | 90-day time-lock |
| Oracle VRF randomness (`provide_randomness`) | 🚧 In progress | Interface exists; oracle service wiring incomplete |
| Oracle race-condition fix | 🚧 In progress | `OracleRequestPending` guard — see spec |
| Commit-reveal randomness | 📋 Planned | `RandomnessSource::CommitReveal` stub only |
| Periodic state snapshots | 📋 Planned | Spec drafted |
| Paginated query system | 📋 Planned | Spec drafted |
| Ticket refund system (partial sales) | 📋 Planned | Spec drafted |
| Time-locked admin operations | 📋 Planned | Spec drafted |
| Emergency pause migration | 📋 Planned | Spec drafted |
| Independent security audit | 📋 Planned | Required before mainnet |

**Legend:** ✅ Shipped · 🚧 In progress · 📋 Planned

> The current development priority is the items marked 🚧 In progress.
> See [`TODO.md`](TODO.md) for the active fix list.

---

## How it works

### Raffle lifecycle

```
PendingPrize ──deposit_prize()──► Active
Active ──buy_tickets() fills max──► Drawing
Active ──cancel_raffle()──────────► Cancelled
Drawing ──finalize_raffle()───────► Finalized   (Internal PRNG)
Drawing ──provide_randomness()────► Finalized   (External VRF oracle)
Drawing ──fallback after timeout──► Finalized   (PRNG fallback)
Finalized ──all prizes claimed────► Claimed
```

### Randomness

| Source | Use case | Trust model |
|---|---|---|
| `Internal` (PRNG) | Low-stakes (< ~500 XLM) | Deterministic; ledger timestamp and sequence are influenceable by validators |
| `External` (VRF oracle) | High-stakes | Off-chain VRF proof; not yet fully wired |

### Contracts

| Crate | Responsibility |
|---|---|
| `raffle` | Factory — deploys and tracks raffle instances |
| `raffle-instance` | Per-raffle logic — tickets, draws, claims |
| `raffle-shared` | Shared types and traits |

Full module map: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)

---

## Getting started

### Prerequisites

- Rust (pinned via `rust-toolchain.toml` — `rustup` handles this automatically)
- Stellar CLI (optional, for deployment)
- Node.js 20+ (for oracle service)

### Build

```bash
make build
```

### Test

```bash
make test
```

### Full CI pipeline (run before every push)

```bash
make ci
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full development workflow.

---

## Testnet deployment

| Network | Contract address |
|---|---|
| Stellar Testnet | `CCTCPMI66REXIJQPVOPNTNUZBCMSRM7TZLMIPQROZIID44XNP2P2MKFZ` |

---

## Metadata integrity (`metadata_hash`)

Every raffle commits a `metadata_hash: BytesN<32>` — a SHA-256 hash of the off-chain metadata
JSON stored on IPFS. This hash is immutable after creation, preventing organizers from changing
the description, rules, or prize after tickets are sold.

```bash
# Generate the hash (Linux/macOS)
sha256sum metadata.json | cut -d' ' -f1
```

Use compact, sorted JSON (`sort_keys=True`, no extra spaces) for reproducibility.

---

## Documentation

| Document | Contents |
|---|---|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Crate map, module layout, where code belongs |
| [`docs/ERRORS.md`](docs/ERRORS.md) | All error codes with frontend messages |
| [`docs/EVENTS.md`](docs/EVENTS.md) | All on-chain events |
| [`DEVELOPMENT.md`](DEVELOPMENT.md) | Local setup, build, deploy, TTL management |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | PR workflow, code placement rules, `make ci` |
| [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) | Deployment receipts and toolchain history |

---

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Run `make ci` before opening a PR.

## License

MIT — see [`LICENSE`](LICENSE).

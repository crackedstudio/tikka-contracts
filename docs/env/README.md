# Environment Variables Reference

This page is the single source of truth for every environment variable read
across the Tikka monorepo. Copy the relevant `.env.example` file, fill in the
required values, and keep secrets out of version control.

---

## Oracle service (`oracle/`)

The oracle is a Node 20 TypeScript service that listens for `RandomnessRequested`
events and calls `provide_randomness` on raffle instances.

**Quick start**

```sh
cp oracle/.env.example oracle/.env
# edit oracle/.env
cd oracle && nvm use && npm ci && npm start
```

### Core variables (loaded by `oracle/src/config.ts`)

| Variable | Required | Default | Description |
|---|---|---|---|
| `ORACLE_SECRET_KEY` | **Yes** | — | Oracle signing key. Accepts a Stellar `S…` secret, 32-byte hex, or base64-encoded seed. Never logged. |
| `STELLAR_RPC_URL` | **Yes** | — | Soroban RPC endpoint, e.g. `https://soroban-testnet.stellar.org` |
| `FACTORY_CONTRACT_ID` | **Yes** | — | Factory contract ID (`C…`) the listener subscribes to at startup |
| `POLL_INTERVAL_MS` | No | `5000` | Event polling interval in milliseconds |
| `ORACLE_POLL_INTERVAL_MS` | No | `5000` | Backward-compatible alias for `POLL_INTERVAL_MS`; `POLL_INTERVAL_MS` takes precedence |
| `LOG_LEVEL` | No | `info` | Log verbosity: `debug`, `info`, `warn`, `error` |

### Runtime / service-layer variables (loaded by individual services)

| Variable | Required | Default | Description |
|---|---|---|---|
| `STELLAR_NETWORK_PASSPHRASE` | No | Testnet passphrase | Network passphrase for transaction signing. Defaults to `Test SDF Network ; September 2015` |
| `ORACLE_ADDRESS` | No* | — | Oracle public key (`G…`). Required by the event listener to filter events for this oracle instance |
| `ORACLE_CHECKPOINT_PATH` | No | `.oracle-checkpoint.json` | Path to the ledger checkpoint file used for restart recovery |

### Integration-test-only variables

These are only read when `STELLAR_INTEGRATION_TEST=1` is set; they are never
needed for normal operation.

| Variable | Required | Default | Description |
|---|---|---|---|
| `STELLAR_INTEGRATION_TEST` | No | — | Set to `1` to enable integration tests against a live testnet |
| `RAFFLE_CONTRACT_ADDRESS` | Integration tests | — | Deployed raffle **instance** contract ID (`C…`) |
| `RANDOMNESS_REQUEST_ID` | Integration tests | — | Pending randomness request ID to fulfill |
| `RANDOMNESS_SEED` | No | `42` | Seed value used by the integration test for `provide_randomness` |

---

## Deployment scripts (`scripts/`)

Every script under `scripts/` sources `.env` when the file is present
(`export $(cat .env | xargs)`). Copy the root `.env.example`:

```sh
cp .env.example .env
# edit .env
```

| Variable | Required by | Description |
|---|---|---|
| `DEPLOYER_SECRET_KEY` | `deploy-testnet.sh`, `deploy-mainnet.sh`, `invoke.sh` | Stellar secret key (`S…`) for the account signing deploy/invoke transactions |
| `RAFFLE_CONTRACT_ADDRESS` | `invoke.sh`, `verify.sh` | Contract ID to invoke or verify (`C…`) |
| `FACTORY_CONTRACT_ID` | manual init, oracle service | Factory contract ID; also consumed by the oracle |
| `STELLAR_NETWORK` | `invoke.sh`, `verify.sh` | Target network name; defaults to `testnet` |
| `STELLAR_RPC_URL` | oracle / manual CLI | Soroban RPC endpoint; shared with the oracle |
| `STELLAR_HORIZON_URL` | optional | Horizon REST endpoint; not required by scripts |
| `ORACLE_ADDRESS` | External raffles | Oracle public key (`G…`) registered as the `oracle_address` on a raffle instance |
| `ORACLE_SECRET_KEY` | oracle service | See oracle table above |
| `ADMIN_ADDRESS` | manual `init_factory` | Factory admin G-address |

> **Security:** Never commit `.env` or any file containing `DEPLOYER_SECRET_KEY`
> or `ORACLE_SECRET_KEY`. Use a secrets manager in production (see
> `oracle/README.md` → Production Setup).

---

## Variable overlap between services

Several variables are shared — set them once in root `.env` and the oracle
will pick them up when launched from the project root:

| Variable | Root `.env.example` | `oracle/.env.example` |
|---|:---:|:---:|
| `STELLAR_RPC_URL` | ✓ | ✓ |
| `FACTORY_CONTRACT_ID` | ✓ | ✓ |
| `ORACLE_ADDRESS` | ✓ | ✓ |
| `ORACLE_SECRET_KEY` | ✓ | ✓ |
| `ORACLE_POLL_INTERVAL_MS` | ✓ | ✓ |
| `ORACLE_CHECKPOINT_PATH` | ✓ | ✓ |
| `LOG_LEVEL` | ✓ | ✓ |

---

## Related docs

- **Oracle README:** [`../../oracle/README.md`](../../oracle/README.md) — key security, architecture, testing
- **Deployment guide:** [`../DEPLOYMENT.md`](../DEPLOYMENT.md) — full scripts reference and end-to-end testnet flow
- **Architecture:** [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — factory → instance → oracle flow

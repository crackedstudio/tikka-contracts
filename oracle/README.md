# Oracle Service

This directory contains the off-chain oracle service for the Tikka Contracts. The oracle is responsible for generating randomness securely and submitting reveal transactions.

## Configuration & Key Security

The oracle requires a secure keypair to sign reveal transactions. The `KeyService` handles loading and securing this keypair at runtime.

### Required environment variables

| Variable                     | Required          | Description                                             |
| ---------------------------- | ----------------- | ------------------------------------------------------- |
| `ORACLE_SECRET_KEY`          | Yes               | Oracle secret key (`S...`), 32-byte hex, or base64 seed |
| `STELLAR_RPC_URL`            | Yes               | Soroban RPC endpoint                                    |
| `FACTORY_CONTRACT_ID`        | Yes               | Contract id that the listener subscribes to at startup  |
| `STELLAR_NETWORK_PASSPHRASE` | No                | Network passphrase for transaction signing              |
| `RAFFLE_CONTRACT_ADDRESS`    | Integration tests | Deployed raffle instance contract                       |
| `RANDOMNESS_REQUEST_ID`      | Integration tests | Pending randomness request id                           |
| `RANDOMNESS_SEED`            | No                | Seed value for integration tests                        |
| `POLL_INTERVAL_MS`           | No                | Event poller interval (default: `5000`)                 |
| `ORACLE_POLL_INTERVAL_MS`    | No                | Backward-compatible poll interval alias                 |
| `LOG_LEVEL`                  | No                | Log verbosity (`info` by default)                       |
| `ORACLE_CHECKPOINT_PATH`     | No                | Ledger checkpoint file for restart recovery             |
| `ORACLE_ADDRESS`             | Event listener    | This oracle's public key (`G...`)                       |
| `ALERT_WEBHOOK_URL`          | No                | Generic JSON POST webhook (Slack/Discord/PagerDuty). Empty disables alerting |
| `ALERT_FAILURE_THRESHOLD`    | No                | Consecutive tx-submission failures before alerting (default: `3`) |
| `ALERT_RATE_LIMIT_MS`        | No                | Min interval between alerts of the same type (default: `60000`) |
| `ALERT_QUEUE_DEPTH_LIMIT`    | No                | Queue depth that triggers a warning (default: `10`)     |
| `ALERT_QUEUE_AGE_LIMIT_MS`   | No                | Max age of the oldest queued request (default: `300000`) |
| `ALERT_RPC_UNREACHABLE_THRESHOLD` | No           | Consecutive RPC poll failures before alerting (default: `3`) |

### Local Development (Environment Variables)

For local development or testing, provide the secret key via environment variables. The `KeyService` uses the `EnvSecretsAdapter` by default.

```sh
ORACLE_SECRET_KEY="S..."
STELLAR_RPC_URL="https://soroban-testnet.stellar.org"
FACTORY_CONTRACT_ID="C..."
```

The `KeyService` validates the key on startup and never logs the private key.

### Production Setup (Secrets Manager / HSM)

For production deployments, use a secrets adapter instead of a raw environment variable:

- `AwsKmsSecretsAdapter` — AWS KMS / Secrets Manager
- `GcpSecretsAdapter` — Google Cloud Secret Manager
- `VaultSecretsAdapter` — HashiCorp Vault

```typescript
import { KeyService, AwsKmsSecretsAdapter } from './keys/key.service';

const adapter = new AwsKmsSecretsAdapter('us-east-1');
const keyService = new KeyService(adapter, 'prod/oracle/secret_key');
await keyService.initialize();
```

Call `keyService.shutdown()` on process exit to zeroize in-memory secret bytes.

### Key rotation

Register a new oracle public key on-chain via the raffle admin/oracle update flow, deploy the new secret through your secrets manager, restart the oracle service, and decommission the previous key after in-flight requests complete.

## Architecture

- **`KeyService` (`src/keys/key.service.ts`)**: securely loads the keypair and exposes `.getPublicKey()`, `.getPublicKeyBytes()`, `.sign()`, and `.shutdown()`.
- **`VrfService` (`src/vrf/vrf.service.ts`)**: signs context-bound randomness proofs for `provide_randomness`.
- **`TxSubmitterService` (`src/tx/tx-submitter.service.ts`)**: submits `provide_randomness` transactions to Soroban RPC.
- **`EventListenerService` (`src/listener/event-listener.service.ts`)**: polls `RandomnessRequested` contract events and enqueues work for this oracle.
- **`Alerter` (`src/alert/alerter.ts`)**: pushes operational alerts to a generic JSON webhook with per-type rate limiting.

## Operational Alerting

When `ALERT_WEBHOOK_URL` is set, the oracle pushes a generic JSON POST to the
webhook (works with Slack, Discord, and PagerDuty endpoints). The alert body is:

```json
{
  "type": "rpc_unreachable",
  "severity": "critical",
  "message": "RPC unreachable after 3 consecutive polling failures",
  "timestamp": 1700000000000,
  "details": { "consecutiveFailures": 3, "threshold": 3 }
}
```

Alert triggers:

- **`submission_failure`** — N consecutive `provide_randomness` submissions fail (`ALERT_FAILURE_THRESHOLD`).
- **`rpc_unreachable`** — N consecutive event-poll RPC calls fail (`ALERT_RPC_UNREACHABLE_THRESHOLD`).
- **`queue_depth` / `queue_age`** — the request queue exceeds `ALERT_QUEUE_DEPTH_LIMIT`, or the oldest queued request is older than `ALERT_QUEUE_AGE_LIMIT_MS`.
- **`process_start` / `process_stop`** — emitted by the bootstrap on startup and on `SIGINT`/`SIGTERM`.

Alerts are rate-limited per type: only one webhook delivery per `ALERT_RATE_LIMIT_MS`
window, so sustained failures aggregate into a single notification instead of a
webhook storm. Test coverage lives in `src/alert/alerter.test.ts` and the
service-level tests.

## Dependency Policy

### `@stellar/stellar-sdk` ↔ Soroban Protocol Version Alignment

The `@stellar/stellar-sdk` major version must match the Soroban protocol version used by the contracts:

| Soroban Protocol | `@stellar/stellar-sdk` | Stellar CLI |
|------------------|------------------------|-------------|
| 23               | 14.x                   | 23.x        |

When the protocol upgrades (e.g., Protocol 24), the SDK major must be bumped to the corresponding major (15.x) before deployment. Dependabot PRs that bump the SDK major version must be reviewed against this policy to confirm alignment with the target protocol version.

The workspace `Cargo.toml` pins `soroban-sdk = "23"` and the README mandates Stellar CLI v23.x; the JS SDK major must stay in sync.

## Testing

```sh
cd oracle
nvm use
npm ci
npm test
```

Set `STELLAR_INTEGRATION_TEST=1` with funded testnet credentials to run the transaction submission integration test.

## Scripts

| Script | Purpose |
| --- | --- |
| `npm run build` | Compile TypeScript to `dist/` using `tsc -p tsconfig.json`. |
| `npm start` | Run the built service from `dist/index.js`. |
| `npm run dev` | Watch TypeScript sources and rebuild on change. Pair with `npm start` in a second terminal if you want a running process while iterating. |
| `npm test` | Run the Jest suite once. |
| `npm run test:watch` | Run Jest in watch mode for focused local iteration. |
| `npm run test:coverage` | Run Jest with coverage enabled. |
| `npm run test:ci` | CI-friendly coverage run with `--runInBand`. |
| `npm run lint` | Run ESLint with zero warnings allowed. |
| `npm run typecheck` | Run `tsc --noEmit` for a fast type-only check. |
| `npm run format` | Rewrite source and markdown files with Prettier. |
| `npm run format:check` | Check formatting without changing files. |

### Build output

The TypeScript compiler is configured with `rootDir: "./src"` and `outDir: "./dist"`, so the entrypoint emitted by `npm run build` is `dist/index.js`. Both `main` and `npm start` point there.

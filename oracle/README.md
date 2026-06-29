# Oracle Service

This directory contains the off-chain oracle service for the Tikka Contracts. The oracle is responsible for generating randomness securely and submitting reveal transactions.

## Configuration & Key Security

The oracle requires a secure keypair to sign reveal transactions. The `KeyService` handles loading and securing this keypair at runtime.

### Required environment variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ORACLE_SECRET_KEY` | Yes (local) | Oracle secret key (`S...`), 32-byte hex, or base64 seed |
| `STELLAR_RPC_URL` | No | Soroban RPC endpoint (default: testnet) |
| `STELLAR_NETWORK_PASSPHRASE` | No | Network passphrase for transaction signing |
| `RAFFLE_CONTRACT_ADDRESS` | Integration tests | Deployed raffle instance contract |
| `RANDOMNESS_REQUEST_ID` | Integration tests | Pending randomness request id |
| `RANDOMNESS_SEED` | No | Seed value for integration tests |
| `ORACLE_POLL_INTERVAL_MS` | No | Event poller interval (default: `5000`) |
| `ORACLE_CHECKPOINT_PATH` | No | Ledger checkpoint file for restart recovery |
| `ORACLE_ADDRESS` | Event listener | This oracle's public key (`G...`) |

### Local Development (Environment Variables)

For local development or testing, provide the secret key via environment variables. The `KeyService` uses the `EnvSecretsAdapter` by default.

```sh
ORACLE_SECRET_KEY="S..."
STELLAR_RPC_URL="https://soroban-testnet.stellar.org"
ORACLE_ADDRESS="G..."
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

* **`KeyService` (`src/keys/key.service.ts`)**: securely loads the keypair and exposes `.getPublicKey()`, `.getPublicKeyBytes()`, `.sign()`, and `.shutdown()`.
* **`VrfService` (`src/vrf/vrf.service.ts`)**: signs context-bound randomness proofs for `provide_randomness`.
* **`TxSubmitterService` (`src/tx/tx-submitter.service.ts`)**: submits `provide_randomness` transactions to Soroban RPC.
* **`EventListenerService` (`src/listener/event-listener.service.ts`)**: polls `RandomnessRequested` contract events and enqueues work for this oracle.

## Testing

```sh
cd oracle
npm test
```

Set `STELLAR_INTEGRATION_TEST=1` with funded testnet credentials to run the transaction submission integration test.

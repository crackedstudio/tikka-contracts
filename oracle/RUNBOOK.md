# Oracle Service Runbook

## Crash-safety audit and deduplication

This document maps crash windows and explains the deduplication durability choices.

### Failure windows

1. Crash before checkpointing ledger: events may be re-processed after restart; deduplication must prevent double-submission.
2. Crash after submission but before persisting dedup record: submission may succeed on-chain but the off-chain store not reflect it (risk of duplicate submission after restart).
3. Crash between enqueue and submission: a job may be lost if it was only in-memory and not checkpointed.

### Current design

- `ledger-checkpoint` persists the last processed ledger to `data/checkpoint.json`.
- `DeduplicationStore` persists seen requests to `data/dedup.json` and provides duplicate detection.
- The service marks a request as seen after successful submission to avoid false-positive filtering.

### Tradeoffs and mitigation

- Marking deduplication *after* successful submission avoids lost requests, but introduces a tiny window where a crash after on-chain success but before persistence could lead to duplicate submission.
- The dedup store is written synchronously to disk on each check.
- The ledger checkpoint ensures we don't skip events silently.

## Startup

### Prerequisites

The oracle service requires the following environment variables:

- `ORACLE_SECRET_KEY`: Stellar Ed25519 secret key (S... format or 32-byte hex/base64)
- `STELLAR_RPC_URL`: Soroban RPC endpoint (e.g., https://soroban-testnet.stellar.org)
- `FACTORY_CONTRACT_ID`: Stellar contract address for the raffle factory

Optional configuration:

- `POLL_INTERVAL_MS`: Event polling interval in milliseconds (default: 5000)
- `ALERT_WEBHOOK_URL`: Webhook URL for operational alerts
- `ALERT_FAILURE_THRESHOLD`: Consecutive failures before alerting (default: 3)
- `ALERT_RATE_LIMIT_MS`: Minimum time between alerts (default: 60000)
- `ALERT_QUEUE_DEPTH_LIMIT`: Queue depth alert threshold (default: 10)
- `ALERT_QUEUE_AGE_LIMIT_MS`: Queue age alert threshold (default: 300000)
- `ALERT_RPC_UNREACHABLE_THRESHOLD`: RPC unreachable alert threshold (default: 3)

### Starting the service

```bash
# From the oracle directory
npm run build
npm start
```

Or directly with Node.js:

```bash
node dist/src/index.js
```

### Expected log lines

On successful startup, you should see:

```
Starting oracle service for contracts: <FACTORY_CONTRACT_ID>
Oracle service started successfully
```

If alerts are configured and enabled:

```
Oracle service started (poll interval <POLL_INTERVAL_MS>ms)
```

If alerts are disabled (no webhook URL):

```
ALERT_WEBHOOK_URL is not set; operational alerts are disabled.
```

### Runtime logs

When processing randomness requests:

```
Successfully submitted provide_randomness: <tx_hash> for raffle=<contract> requestId=<id>
```

When skipping duplicates:

```
Skipping duplicate request: raffle=<contract> requestId=<id>
```

### Shutdown

On graceful shutdown (SIGINT/SIGTERM):

```
Shutting down oracle service...
Received SIGTERM — starting graceful shutdown.
Draining <n> in-flight job(s) before shutdown.
Job drained: raffle=<contract> requestId=<id>
Checkpoint persisted at ledger <n>.
Graceful shutdown complete. Exiting 0.
```

If shutdown timeout is exceeded:

```
Graceful shutdown drain exceeded 30000 ms — forcing exit 1.
```

## Pipeline components

The oracle service wires the following components:

1. **KeyService**: Manages the oracle's Ed25519 keypair for signing
2. **EventListenerService**: Polls Soroban RPC for RandomnessRequested events
3. **RequestQueue**: Queues jobs for processing with health monitoring
4. **DeduplicationStore**: Prevents duplicate submissions
5. **VrfService**: Generates VRF proofs for randomness
6. **TxSubmitterService**: Submits provide_randomness transactions with retry logic
7. **GracefulShutdown**: Drains in-flight jobs before exit

## Data persistence

The service creates two data files in the `./data` directory:

- `checkpoint.json`: Last processed ledger number
- `dedup.json`: Set of processed (raffle_contract, request_id) pairs

Ensure the `./data` directory is writable by the service process.

## Key Rotation Procedure

To rotate the cryptographic keys used by the oracle:

1. **Generate a new keypair**: Generate a new Stellar Ed25519 keypair.
2. **Register the new oracle on-chain**: Add/register the new public key (address) as a authorized oracle in the Raffle contract/factory configurations.
3. **Update Secret Storage**:
   - If using **HashiCorp Vault** (production), update the configured path in Vault with the new private key.
   - If using **EnvVars** (non-production), update the `ORACLE_SECRET_KEY` environment variable. Note that the env var is parsed into memory as a zeroizable buffer and immediately wiped from `process.env` for security.
4. **Initiate Graceful Shutdown**: Send a `SIGTERM` signal to the active process. The oracle will finish processing all queued jobs, save its checkpoint, and exit.
5. **Redeploy/Restart**: Restart the container/process. It will automatically load the new secret key, verify the public key, and resume processing from the last saved ledger checkpoint.

## Graceful Shutdown Details

The oracle registers signal listeners for `SIGINT` and `SIGTERM`. When received, the entrypoint starts the graceful shutdown flow:

1. **Stop Pollers**: Stops the event listener from querying the Soroban RPC for new events.
2. **Queue Draining**: In-flight jobs currently in the `RequestQueue` are processed to completion.
3. **Checkpoint Saving**: The current ledger sequence is written to `data/checkpoint.json` so no events are lost or duplicated upon restart.
4. **Alert notification**: Sends a `process_stop` operational alert indicating a graceful stop.
5. **Exit**: The process cleanly exits with code `0`. If draining takes longer than 30 seconds, a forced timeout triggers an exit with code `1`.

## Multi-Operator Setup (Quorum Mode)

When the raffle contract uses the `k-of-n` Quorum randomness mode:

- Multiple independent oracle operators must run instances of this service.
- Each operator configures their service with their own unique private key.
- The service automatically checks if its public key is part of the raffle's configured `oracles` list.
- When an event is detected, participating services independently generate cryptographically secure random seeds and submit them via `provide_quorum_randomness` (without requiring a VRF proof).
- The transaction submitter retries transient network errors to ensure each operator's contribution lands successfully.



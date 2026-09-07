# Oracle Service Runbook

Operational guide for the Tikka randomness oracle service.

## Health and Metrics Endpoints

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Liveness probe — returns `{"status":"ok"}` |
| `GET /metrics` | Prometheus text exposition format |

Default port: `9090` (override with `HEALTH_PORT`).

## Metrics Reference

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `oracle_requests_observed_total` | Counter | `raffle` | `RandomnessRequested` events enqueued for this oracle |
| `oracle_request_latency_seconds` | Histogram | — | Wall time from event observation to confirmed on-chain submission |
| `oracle_submissions_total` | Counter | `outcome` | Submission results: `success`, `retry`, or `fatal` |
| `oracle_queue_depth` | Gauge | — | Current number of pending randomness jobs |
| `oracle_queue_oldest_age_seconds` | Gauge | — | Age of the oldest queued job in seconds |
| `oracle_dead_letter_total` | Counter | — | Jobs permanently failed after exhausting retries |
| `oracle_listener_ledger_lag` | Gauge | — | Ledgers between network tip and last processed checkpoint |
| `oracle_rpc_errors_total` | Counter | `kind` | RPC errors by phase: `poll`, `simulate`, `send` |
| `oracle_fees_spent_stroops_total` | Counter | — | Cumulative transaction fees paid for submissions |

## Suggested Alert Rules

These mirror the thresholds in `.env.example` and the existing webhook alerter.

### Queue depth

```yaml
- alert: OracleQueueDepthHigh
  expr: oracle_queue_depth > 10
  for: 2m
  labels:
    severity: warning
  annotations:
    summary: Oracle request queue depth exceeds limit
```

Env: `ALERT_QUEUE_DEPTH_LIMIT=10`

### Queue age

```yaml
- alert: OracleQueueAgeHigh
  expr: oracle_queue_oldest_age_seconds > 300
  for: 1m
  labels:
    severity: warning
  annotations:
    summary: Oldest queued randomness request is stale
```

Env: `ALERT_QUEUE_AGE_LIMIT_MS=300000` (300 seconds)

### RPC unreachable

```yaml
- alert: OracleRpcUnreachable
  expr: increase(oracle_rpc_errors_total{kind="poll"}[5m]) >= 3
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: Oracle cannot reach Soroban RPC
```

Env: `ALERT_RPC_UNREACHABLE_THRESHOLD=3`

### Submission failures

```yaml
- alert: OracleSubmissionFailures
  expr: increase(oracle_submissions_total{outcome="fatal"}[10m]) > 0
  for: 0m
  labels:
    severity: critical
  annotations:
    summary: Oracle failed to submit provide_randomness
```

Env: `ALERT_FAILURE_THRESHOLD=3` (consecutive failures before webhook alert)

### Listener lag

```yaml
- alert: OracleListenerLag
  expr: oracle_listener_ledger_lag > 50
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: Oracle event listener is falling behind chain tip
```

### Fee burn rate

```yaml
- alert: OracleFeeBurnHigh
  expr: rate(oracle_fees_spent_stroops_total[1h]) > 1000000
  for: 15m
  labels:
    severity: info
  annotations:
    summary: Oracle transaction fee spend rate is elevated
```

## Crash-safety and Deduplication

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

## Log schema

The oracle emits structured JSON logs (via `pino`). In development, logs are formatted with `pino-pretty` for readability.

### Common fields

| Field | Type | Description |
|---|---|---|
| `level` | number | Pino log level (10=debug, 30=warn, 50=error) |
| `msg` | string | Human-readable log message |
| `time` | string | ISO-8601 timestamp |
| `pid` | number | Process ID |
| `hostname` | string | Machine hostname |

### Request-correlation fields

When processing a randomness request, logs are emitted from a child logger bound with:

| Field | Description |
|---|---|
| `requestId` | BigInt string of the on-chain `RandomnessRequested` `request_id` |
| `raffleId` | Soroban contract ID of the raffle |

These fields allow you to `grep` a single `requestId` across listener → queue → VRF → submission.

### Example log lines

**Production (JSON):**
```json
{"level":30,"time":"2026-08-29T08:00:00.000Z","pid":1234,"hostname":"oracle-1","msg":"Enqueuing randomness request","requestId":"42","raffleContract":"CABC...","timestamp":"1234567890"}
{"level":30,"time":"2026-08-29T08:00:01.000Z","pid":1234,"hostname":"oracle-1","requestId":"42","raffleId":"CABC...","msg":"Successfully submitted provide_randomness: abc123..."}
```

**Development (pretty):**
```
[2026-08-29 08:00:00.000 +0000] WARN: Enqueuing randomness request requestId=42 raffleId=CABC...
[2026-08-29 08:00:01.000 +0000] WARN: Successfully submitted provide_randomness: abc123... requestId=42 raffleId=CABC...
```

### Secrets redaction

The logger redacts the following from any log message:

- `ORACLE_SECRET_KEY`, `secretKey`, `secret`, `password`, `token`, `apiKey`, `api_key`, `accessKey`, `access_key`, `privateKey`, `private_key`, `passphrase`
- Hex/base64 strings longer than 32 characters when preceded by `hex=` or `base64=`

Raw key material is **never** written to logs.

## Startup

### Prerequisites

The oracle service requires the following environment variables:

- `ORACLE_SECRET_KEY`: Stellar Ed25519 secret key (S... format or 32-byte hex/base64)
- `STELLAR_RPC_URL`: Soroban RPC endpoint (e.g., https://soroban-testnet.stellar.org)
- `FACTORY_CONTRACT_ID`: Stellar contract address for the raffle factory

Optional configuration:

- `LOG_LEVEL`: Logging verbosity (`debug`, `info`, `warn`, `error`; default: `info`)
- `POLL_INTERVAL_MS`: Event polling interval in milliseconds (default: 5000)
- `HEALTH_PORT`: Port for `/health` and `/metrics` (default: 9090)
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

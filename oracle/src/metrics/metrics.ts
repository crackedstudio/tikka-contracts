import { Counter, Gauge, Histogram, Registry, collectDefaultMetrics } from 'prom-client';

/** Shared Prometheus registry for all oracle metrics. */
export const registry = new Registry();

collectDefaultMetrics({ register: registry });

export const oracleRequestsObservedTotal = new Counter({
  name: 'oracle_requests_observed_total',
  help: 'RandomnessRequested events observed by the listener',
  labelNames: ['raffle'],
  registers: [registry],
});

export const oracleRequestLatencySeconds = new Histogram({
  name: 'oracle_request_latency_seconds',
  help: 'Seconds from observing RandomnessRequested to successful on-chain submission',
  buckets: [0.5, 1, 2, 5, 10, 30, 60, 120, 300],
  registers: [registry],
});

export const oracleSubmissionsTotal = new Counter({
  name: 'oracle_submissions_total',
  help: 'provide_randomness submission outcomes',
  labelNames: ['outcome'],
  registers: [registry],
});

export const oracleQueueDepth = new Gauge({
  name: 'oracle_queue_depth',
  help: 'Current depth of the randomness request queue',
  registers: [registry],
});

export const oracleQueueOldestAgeSeconds = new Gauge({
  name: 'oracle_queue_oldest_age_seconds',
  help: 'Age in seconds of the oldest queued randomness request',
  registers: [registry],
});

export const oracleDeadLetterTotal = new Counter({
  name: 'oracle_dead_letter_total',
  help: 'Requests permanently failed after exhausting submission retries',
  registers: [registry],
});

export const oracleListenerLedgerLag = new Gauge({
  name: 'oracle_listener_ledger_lag',
  help: 'Ledgers between network tip and the last processed ledger checkpoint',
  registers: [registry],
});

export const oracleRpcErrorsTotal = new Counter({
  name: 'oracle_rpc_errors_total',
  help: 'RPC errors encountered by the oracle service',
  labelNames: ['kind'],
  registers: [registry],
});

export const oracleFeesSpentStroopsTotal = new Counter({
  name: 'oracle_fees_spent_stroops_total',
  help: 'Cumulative transaction fees paid for provide_randomness submissions (stroops)',
  registers: [registry],
});

/** Refresh queue gauge metrics from the current queue state. */
export function updateQueueMetrics(depth: number, oldestAgeSeconds: number): void {
  oracleQueueDepth.set(depth);
  oracleQueueOldestAgeSeconds.set(oldestAgeSeconds);
}

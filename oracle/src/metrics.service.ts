import client from 'prom-client';
import { HealthService } from './health.service';

const register = new client.Registry();

const requestsTotal = new client.Counter({
  name: 'oracle_requests_total',
  help: 'Total number of oracle requests processed',
  labelNames: ['status'],
  registers: [register],
});

const queueDepthGauge = new client.Gauge({
  name: 'oracle_queue_depth',
  help: 'Current oracle queue depth',
  registers: [register],
});

const latencyHistogram = new client.Summary({
  name: 'oracle_tx_submission_latency_seconds',
  help: 'Oracle transaction submission latency in seconds',
  percentiles: [0.5, 0.9, 0.99],
  registers: [register],
});

const lastProcessedLedgerGauge = new client.Gauge({
  name: 'oracle_last_processed_ledger',
  help: 'Last processed ledger number by the oracle',
  registers: [register],
});

register.setDefaultLabels({ app: 'tikka-oracle' });
client.collectDefaultMetrics({ register });

export class MetricsService {
  constructor(private readonly healthService: HealthService) {}

  recordRequest(success: boolean): void {
    requestsTotal.labels({ status: success ? 'success' : 'failure' }).inc();
  }

  setQueueDepth(depth: number): void {
    queueDepthGauge.set(Math.max(0, depth));
  }

  observeLatency(seconds: number): void {
    if (seconds >= 0) {
      latencyHistogram.observe(seconds);
    }
  }

  setLatestLedger(latestLedger: number): void {
    lastProcessedLedgerGauge.set(latestLedger);
  }

  async metrics(): Promise<string> {
    this.setQueueDepth(this.healthService.getSnapshot().queueDepth);
    const latestLedger = this.healthService.getSnapshot().latestLedger;
    if (latestLedger !== null) {
      this.setLatestLedger(latestLedger);
    }
    return await register.metrics();
  }
}

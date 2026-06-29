export interface OracleHealthSnapshot {
  status: 'ok' | 'down';
  connected: boolean;
  latestLedger: number | null;
  queueDepth: number;
  requestsProcessed: number;
  requestsFailed: number;
  uptimeSeconds: number;
}

const DEFAULT_STALE_LEDGER_MS = 5 * 60 * 1000;

export class HealthService {
  private connected = false;
  private latestLedger: number | null = null;
  private queueDepth = 0;
  private requestsProcessed = 0;
  private requestsFailed = 0;
  private startedAt = Date.now();
  private lastProcessedAt: number | null = null;

  constructor(private readonly staleThresholdMs = DEFAULT_STALE_LEDGER_MS) {}

  setConnected(connected: boolean): void {
    this.connected = connected;
  }

  setQueueDepth(queueDepth: number): void {
    this.queueDepth = Math.max(0, Math.floor(queueDepth));
  }

  recordRequest(success: boolean): void {
    if (success) {
      this.requestsProcessed += 1;
    } else {
      this.requestsFailed += 1;
    }
  }

  recordProcessedLedger(latestLedger: number, processedAt: number = Date.now()): void {
    this.latestLedger = latestLedger;
    this.lastProcessedAt = processedAt;
  }

  isHealthy(): boolean {
    if (!this.connected) {
      return false;
    }

    if (this.lastProcessedAt === null) {
      return false;
    }

    return Date.now() - this.lastProcessedAt <= this.staleThresholdMs;
  }

  getSnapshot(): OracleHealthSnapshot {
    return {
      status: this.isHealthy() ? 'ok' : 'down',
      connected: this.connected,
      latestLedger: this.latestLedger,
      queueDepth: this.queueDepth,
      requestsProcessed: this.requestsProcessed,
      requestsFailed: this.requestsFailed,
      uptimeSeconds: Math.floor((Date.now() - this.startedAt) / 1000),
    };
  }
}

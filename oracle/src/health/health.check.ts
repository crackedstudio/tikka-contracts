import * as http from 'http';
import { DeadLetterStore } from '../queue/dead-letter.store';
import { RequestQueue } from '../queue/request-queue';

export interface HealthSnapshot {
  status: 'ok' | 'degraded';
  queueDepth: number;
  deadLetterDepth: number;
  oldestQueuedAgeMs: number | null;
  timestamp: number;
}

export interface HealthCheckOptions {
  port?: number;
  host?: string;
  queue: RequestQueue;
  deadLetterStore: DeadLetterStore;
  /** Queue depth above this marks readiness as degraded (default 10). */
  queueDepthDegraded?: number;
  /** Dead-letter depth above this marks readiness as degraded (default 1). */
  deadLetterDegraded?: number;
}

/**
 * Lightweight HTTP readiness server exposing queue and dead-letter metrics.
 * Used by Docker HEALTHCHECK and external monitoring.
 */
export class HealthCheck {
  private server?: http.Server;
  private readonly port: number;
  private readonly host: string;
  private readonly queue: RequestQueue;
  private readonly deadLetterStore: DeadLetterStore;
  private readonly queueDepthDegraded: number;
  private readonly deadLetterDegraded: number;

  constructor(options: HealthCheckOptions) {
    this.port = options.port ?? Number(process.env.HEALTH_PORT ?? 3000);
    this.host = options.host ?? '127.0.0.1';
    this.queue = options.queue;
    this.deadLetterStore = options.deadLetterStore;
    this.queueDepthDegraded =
      options.queueDepthDegraded ?? Number(process.env.ALERT_QUEUE_DEPTH_LIMIT ?? 10);
    this.deadLetterDegraded = options.deadLetterDegraded ?? 1;
  }

  snapshot(now: number = Date.now()): HealthSnapshot {
    const queueDepth = this.queue.size();
    const deadLetterDepth = this.deadLetterStore.size();
    const oldestQueuedAgeMs = this.queue.oldestAgeMs(now);

    const degraded =
      queueDepth > this.queueDepthDegraded || deadLetterDepth >= this.deadLetterDegraded;

    return {
      status: degraded ? 'degraded' : 'ok',
      queueDepth,
      deadLetterDepth,
      oldestQueuedAgeMs,
      timestamp: now,
    };
  }

  start(): void {
    if (this.server) {
      return;
    }

    this.server = http.createServer((req, res) => {
      if (req.url !== '/health' && req.url !== '/ready') {
        res.writeHead(404);
        res.end();
        return;
      }

      const snapshot = this.snapshot();
      const statusCode = snapshot.status === 'ok' ? 200 : 503;
      res.writeHead(statusCode, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(snapshot));
    });

    this.server.listen(this.port, this.host);
  }

  stop(): void {
    this.server?.close();
    this.server = undefined;
  }
}

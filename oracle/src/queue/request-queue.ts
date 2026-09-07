import { Alerter } from '../alert/alerter';
import { DeadLetterEntry, DeadLetterStore } from './dead-letter.store';
import { isFatalRandomnessError } from './fatal-errors';

export interface RandomnessJob {
  requestId: bigint;
  raffleContract: string;
  timestamp: bigint;
  /** Wall-clock time when the listener observed the RandomnessRequested event. */
  observedAtMs: number;
}

export interface TrackedJob {
  job: RandomnessJob;
  attemptCount: number;
  firstEnqueuedAtMs: number;
}

export type DeadLetterReason = DeadLetterEntry['reason'];

export interface RequestQueueOptions {
  alerter?: Alerter;
  deadLetterStore?: DeadLetterStore;
  depthLimit?: number;
  ageLimitMs?: number;
  maxAttempts?: number;
}

export class RequestQueue {
  private readonly jobs: TrackedJob[] = [];
  private readonly alerter?: Alerter;
  private readonly deadLetterStore: DeadLetterStore;
  private readonly depthLimit: number;
  private readonly ageLimitMs: number;
  private readonly maxAttempts: number;

  constructor(options: RequestQueueOptions = {}) {
    this.alerter = options.alerter;
    this.deadLetterStore = options.deadLetterStore ?? new DeadLetterStore(':memory:');
    this.depthLimit = options.depthLimit ?? Number(process.env.ALERT_QUEUE_DEPTH_LIMIT ?? 10);
    this.ageLimitMs = options.ageLimitMs ?? Number(process.env.ALERT_QUEUE_AGE_LIMIT_MS ?? 300_000);
    this.maxAttempts = options.maxAttempts ?? Number(process.env.QUEUE_MAX_ATTEMPTS ?? 5);
  }

  enqueue(job: RandomnessJob): void {
    this.jobs.push({
      job,
      attemptCount: 0,
      firstEnqueuedAtMs: Date.now(),
    });
  }

  /** Re-enqueue a job returned from manual replay (resets attempt counter). */
  requeue(job: RandomnessJob): void {
    this.enqueue(job);
  }

  drain(): RandomnessJob[] {
    const pending = this.jobs.map((tracked) => tracked.job);
    this.jobs.length = 0;
    return pending;
  }

  /** Returns pending jobs without removing them (for worker iteration). */
  peek(): TrackedJob[] {
    return [...this.jobs];
  }

  size(): number {
    return this.jobs.length;
  }

  deadLetterDepth(): number {
    return this.deadLetterStore.size();
  }

  getDeadLetterStore(): DeadLetterStore {
    return this.deadLetterStore;
  }

  oldestAgeMs(now: number = Date.now()): number | null {
    const oldest = this.jobs[0];
    if (!oldest) {
      return null;
    }
    return now - oldest.firstEnqueuedAtMs;
  }

  /**
   * Record a processing failure. Fatal errors and exhausted retries move the
   * job to the dead-letter store; transient errors are re-queued with an
   * incremented attempt count.
   */
  recordFailure(
    raffleContract: string,
    requestId: bigint,
    error: string,
    now: number = Date.now(),
  ): 'retry' | 'dead_lettered' {
    const index = this.jobs.findIndex(
      (tracked) =>
        tracked.job.raffleContract === raffleContract && tracked.job.requestId === requestId,
    );
    if (index === -1) {
      return 'dead_lettered';
    }

    const [tracked] = this.jobs.splice(index, 1);
    tracked.attemptCount += 1;

    const fatal = isFatalRandomnessError(error);
    const exhausted = tracked.attemptCount >= this.maxAttempts;

    if (fatal || exhausted) {
      const reason: DeadLetterReason = fatal
        ? 'fatal'
        : exhausted
          ? 'retry_exhausted'
          : 'retry_exhausted';
      this.deadLetter(tracked, error, reason, now);
      return 'dead_lettered';
    }

    this.jobs.push(tracked);
    return 'retry';
  }

  /**
   * Move stale or excess jobs to the dead-letter store when health thresholds
   * are breached. Returns the number of jobs dead-lettered.
   */
  evacuateUnhealthy(now: number = Date.now()): number {
    let evacuated = 0;

    while (this.size() > this.depthLimit) {
      const tracked = this.jobs.shift();
      if (!tracked) {
        break;
      }
      this.deadLetter(
        tracked,
        `Queue depth (${this.size() + 1}) exceeded limit (${this.depthLimit})`,
        'queue_depth',
        now,
      );
      evacuated += 1;
    }

    while (this.jobs.length > 0) {
      const oldest = this.jobs[0];
      if (now - oldest.firstEnqueuedAtMs <= this.ageLimitMs) {
        break;
      }
      const tracked = this.jobs.shift()!;
      this.deadLetter(
        tracked,
        `Request age (${now - tracked.firstEnqueuedAtMs}ms) exceeded limit (${this.ageLimitMs}ms)`,
        'queue_age',
        now,
      );
      evacuated += 1;
    }

    return evacuated;
  }

  /**
   * Alerts when the queue has grown too deep or the oldest request has been
   * waiting too long, then evacuates breached jobs to the dead-letter store.
   */
  checkHealth(now: number = Date.now()): void {
    const depth = this.size();
    if (this.alerter && depth > this.depthLimit) {
      void this.alerter.notify({
        type: 'queue_depth',
        severity: 'warning',
        message: `Request queue depth (${depth}) exceeds limit (${this.depthLimit})`,
        details: { depth, limit: this.depthLimit },
      });
    }

    const oldestAgeMs = this.oldestAgeMs(now);
    if (this.alerter && oldestAgeMs !== null && oldestAgeMs > this.ageLimitMs) {
      void this.alerter.notify({
        type: 'queue_age',
        severity: 'warning',
        message: `Oldest queued request is ${oldestAgeMs}ms old (limit ${this.ageLimitMs}ms)`,
        details: { ageMs: oldestAgeMs, limit: this.ageLimitMs },
      });
    }

    this.evacuateUnhealthy(now);
  }

  private deadLetter(
    tracked: TrackedJob,
    error: string,
    reason: DeadLetterReason,
    now: number,
  ): void {
    const entry: DeadLetterEntry = {
      job: tracked.job,
      error,
      attemptCount: tracked.attemptCount,
      firstEnqueuedAtMs: tracked.firstEnqueuedAtMs,
      deadLetteredAtMs: now,
      reason,
    };

    this.deadLetterStore.add(entry);

    if (this.alerter) {
      void this.alerter.notify({
        type: 'dead_letter',
        severity: 'critical',
        bypassRateLimit: true,
        message: `Randomness request dead-lettered (${reason}): raffle=${tracked.job.raffleContract} requestId=${tracked.job.requestId}`,
        details: {
          reason,
          error,
          attemptCount: tracked.attemptCount,
          raffleContract: tracked.job.raffleContract,
          requestId: tracked.job.requestId.toString(),
          firstEnqueuedAtMs: tracked.firstEnqueuedAtMs,
          deadLetteredAtMs: now,
        },
      });
    }

    console.error(
      `Dead-lettered randomness request: raffle=${tracked.job.raffleContract} ` +
        `requestId=${tracked.job.requestId} reason=${reason} error=${error}`,
    );
  }
}

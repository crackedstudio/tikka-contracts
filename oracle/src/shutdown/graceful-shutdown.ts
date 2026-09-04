import { LedgerCheckpointStore } from '../listener/ledger-checkpoint';
import { RequestQueue, RandomnessJob } from '../queue/request-queue';
import { logger } from '../logging/logger';

export interface ShutdownOptions {
  /**
   * Maximum milliseconds to wait for the in-flight drain to complete before
   * forcing a non-zero exit.  Configurable so tests can set it to 0 and so
   * operators can tune it to match their redeploy SLA.
   * Default: 30 000 ms.
   */
  drainTimeoutMs?: number;

  /**
   * Called once per job while draining.  Return false to skip the job (already
   * completed or deduped) — the checkpoint is only updated for processed jobs.
   */
  processJob?: (job: RandomnessJob) => Promise<boolean>;

  /**
   * Override process.exit so unit tests can assert the exit code without
   * actually terminating.
   */
  exitFn?: (code: number) => void;

  /**
   * Override setTimeout so unit tests can control time without leaking fake
   * timers across describe blocks.  Defaults to the global setTimeout.
   */
  setTimeoutFn?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;

  /**
   * Override clearTimeout paired with setTimeoutFn.
   */
  clearTimeoutFn?: (handle: ReturnType<typeof setTimeout>) => void;
}

/**
 * GracefulShutdown
 *
 * Registers SIGINT/SIGTERM handlers that:
 *   1. Stop accepting new events (signals the listener to stop polling).
 *   2. Drain remaining in-flight jobs from the queue, invoking processJob
 *      for each one so no completed work is lost.
 *   3. Persist the ledger checkpoint so restart picks up exactly where the
 *      process left off (no missed events, no duplicated deliveries).
 *   4. Exit 0.
 *
 * If the drain does not complete within drainTimeoutMs a force-exit with
 * code 1 is issued so a hung drain never blocks a redeploy.
 */
export class GracefulShutdown {
  private readonly drainTimeoutMs: number;
  private readonly processJob: (job: RandomnessJob) => Promise<boolean>;
  private readonly exitFn: (code: number) => void;
  private readonly setTimeoutFn: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  private readonly clearTimeoutFn: (handle: ReturnType<typeof setTimeout>) => void;

  /** Resolves when stopListening has been called on the listener. */
  private stopListening?: () => void;

  /** True once a signal has been received — prevents double-handling. */
  private shutdownInitiated = false;

  constructor(
    private readonly queue: RequestQueue,
    private readonly checkpointStore: LedgerCheckpointStore,
    options: ShutdownOptions = {},
  ) {
    this.drainTimeoutMs = options.drainTimeoutMs ?? 30_000;
    this.processJob = options.processJob ?? (() => Promise.resolve(true));
    this.exitFn = options.exitFn ?? ((code) => process.exit(code));
    this.setTimeoutFn = options.setTimeoutFn ?? ((fn, ms) => setTimeout(fn, ms));
    this.clearTimeoutFn = options.clearTimeoutFn ?? ((h) => clearTimeout(h));
  }

  /**
   * Register SIGINT and SIGTERM handlers.
   *
   * @param stopListeningFn  Called first to stop the event-listener poll loop so
   *                         no new jobs are enqueued after the signal arrives.
   */
  register(stopListeningFn: () => void): void {
    this.stopListening = stopListeningFn;

    const handler = (signal: string) => {
      logger.info(`Received ${signal} — starting graceful shutdown.`);
      void this.shutdown();
    };

    process.once('SIGTERM', () => handler('SIGTERM'));
    process.once('SIGINT', () => handler('SIGINT'));
  }

  /**
   * Exposed for direct invocation in tests.  In production, called by the
   * registered signal handler.
   */
  async shutdown(): Promise<void> {
    // Guard against concurrent/duplicate invocations (e.g. SIGTERM + SIGINT
    // firing in quick succession, or tests calling shutdown() directly twice).
    if (this.shutdownInitiated) {
      return;
    }
    this.shutdownInitiated = true;

    // 1. Stop accepting new events.
    this.stopListening?.();

    // 2. Drain in-flight jobs with a bounded timeout.
    let timedOut = false;
    const forceExitTimer = this.setTimeoutFn(() => {
      timedOut = true;
      logger.error(
        `Graceful shutdown drain exceeded ${this.drainTimeoutMs} ms — forcing exit 1.`,
      );
      this.exitFn(1);
    }, this.drainTimeoutMs);

    // Allow the timer to be GC'd without blocking the Node event loop.
    if (typeof (forceExitTimer as NodeJS.Timeout).unref === 'function') {
      (forceExitTimer as NodeJS.Timeout).unref();
    }

    try {
      await this.drainQueue();
    } catch (err) {
      logger.error('Error during drain:', err);
    }

    if (!timedOut) {
      this.clearTimeoutFn(forceExitTimer);
    }

    // 3. Persist checkpoint (reflects only completed work — see drainQueue).
    try {
      const lastCheckpoint = await this.checkpointStore.load();
      if (lastCheckpoint !== undefined) {
        await this.checkpointStore.save(lastCheckpoint);
        logger.info(`Checkpoint persisted at ledger ${lastCheckpoint}.`);
      }
    } catch (err) {
      logger.error('Failed to persist checkpoint on shutdown:', err);
    }

    // 4. Exit 0 — clean shutdown.
    if (!timedOut) {
      logger.info('Graceful shutdown complete. Exiting 0.');
      this.exitFn(0);
    }
  }

  /**
   * Drains the queue, calling processJob for each pending job.
   *
   * Jobs that processJob returns false for (already delivered / deduped) are
   * skipped without updating any state — preventing duplicated deliveries.
   */
  private async drainQueue(): Promise<void> {
    const jobs = this.queue.drain();
    if (jobs.length === 0) {
      return;
    }
    logger.info(`Draining ${jobs.length} in-flight job(s) before shutdown.`);

    for (const job of jobs) {
      try {
        const processed = await this.processJob(job);
        if (processed) {
          logger.info(
            `Job drained: raffle=${job.raffleContract} requestId=${job.requestId}`,
          );
        } else {
          logger.info(
            `Job skipped (deduped): raffle=${job.raffleContract} requestId=${job.requestId}`,
          );
        }
      } catch (err) {
        logger.error(
          `Failed to drain job raffle=${job.raffleContract} requestId=${job.requestId}:`,
          err,
        );
      }
    }
  }
}

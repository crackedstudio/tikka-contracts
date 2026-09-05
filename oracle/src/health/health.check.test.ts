import { HealthCheck } from './health.check';
import { DeadLetterStore } from '../queue/dead-letter.store';
import { RequestQueue } from '../queue/request-queue';

function job(requestId = 1n) {
  return {
    requestId,
    raffleContract: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M',
    timestamp: 0n,
  };
}

describe('HealthCheck', () => {
  it('reports queue and dead-letter depth', () => {
    const queue = new RequestQueue({
      deadLetterStore: new DeadLetterStore(':memory:'),
      depthLimit: 100,
      ageLimitMs: 60_000,
    });
    const deadLetterStore = queue.getDeadLetterStore();

    queue.enqueue(job(1n));
    queue.enqueue(job(2n));
    deadLetterStore.add({
      job: job(99n),
      error: 'fatal',
      attemptCount: 1,
      firstEnqueuedAtMs: 0,
      deadLetteredAtMs: 1,
      reason: 'fatal',
    });

    const health = new HealthCheck({ queue, deadLetterStore, queueDepthDegraded: 10 });
    const snapshot = health.snapshot();

    expect(snapshot.queueDepth).toBe(2);
    expect(snapshot.deadLetterDepth).toBe(1);
    expect(snapshot.status).toBe('degraded');
  });

  it('returns ok when depths are within limits', () => {
    const queue = new RequestQueue({
      deadLetterStore: new DeadLetterStore(':memory:'),
    });
    const health = new HealthCheck({
      queue,
      deadLetterStore: queue.getDeadLetterStore(),
      queueDepthDegraded: 10,
      deadLetterDegraded: 5,
    });

    expect(health.snapshot().status).toBe('ok');
  });
});

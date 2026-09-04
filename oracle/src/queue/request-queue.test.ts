import { RequestQueue } from './request-queue';
import { Alerter } from '../alert/alerter';
import { DeadLetterStore } from './dead-letter.store';
import { isFatalRandomnessError } from './fatal-errors';

function mockFetch() {
  return jest.fn().mockResolvedValue({ ok: true, status: 200 } as Response);
}

function job(requestId = 1n, raffle = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M') {
  return { requestId, raffleContract: raffle, timestamp: 0n };
}

describe('RequestQueue', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env['ALERT_QUEUE_DEPTH_LIMIT'];
    delete process.env['ALERT_QUEUE_AGE_LIMIT_MS'];
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('enqueues, drains, and reports size', () => {
    const queue = new RequestQueue({ deadLetterStore: new DeadLetterStore(':memory:') });
    queue.enqueue(job(1n));
    queue.enqueue(job(2n));
    expect(queue.size()).toBe(2);

    const drained = queue.drain();
    expect(drained.map((j) => j.requestId)).toEqual([1n, 2n]);
    expect(queue.size()).toBe(0);
  });

  it('does not alert when no alerter is configured', () => {
    const queue = new RequestQueue({
      deadLetterStore: new DeadLetterStore(':memory:'),
      depthLimit: 1,
      ageLimitMs: 1,
    });
    queue.enqueue(job());
    queue.enqueue(job());
    expect(() => queue.checkHealth(Date.now() + 1_000)).not.toThrow();
  });

  it('alerts when queue depth exceeds the limit', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
    const queue = new RequestQueue({
      alerter,
      deadLetterStore: new DeadLetterStore(':memory:'),
      depthLimit: 1,
      ageLimitMs: 60_000,
    });

    queue.enqueue(job(1n));
    queue.enqueue(job(2n));

    queue.checkHealth();
    await new Promise((resolve) => setImmediate(resolve));

    expect(fetchImpl).toHaveBeenCalled();
    const body = JSON.parse(fetchImpl.mock.calls[0][1].body);
    expect(body.type).toBe('queue_depth');
    expect(body.details.depth).toBe(2);
  });

  it('dead-letters on fatal contract errors without retrying', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
    const deadLetterStore = new DeadLetterStore(':memory:');
    const queue = new RequestQueue({
      alerter,
      deadLetterStore,
      maxAttempts: 5,
    });

    queue.enqueue(job(7n));
    const result = queue.recordFailure(
      job(7n).raffleContract,
      7n,
      'Simulation failed: NoRandomnessRequest',
    );

    expect(result).toBe('dead_lettered');
    expect(queue.size()).toBe(0);
    expect(deadLetterStore.size()).toBe(1);
    expect(isFatalRandomnessError('NoRandomnessRequest')).toBe(true);

    await new Promise((resolve) => setImmediate(resolve));
    const deadLetterAlert = fetchImpl.mock.calls.find(
      (call) => JSON.parse(call[1].body).type === 'dead_letter',
    );
    expect(deadLetterAlert).toBeDefined();
  });

  it('dead-letters after retry cap is exhausted', () => {
    const deadLetterStore = new DeadLetterStore(':memory:');
    const queue = new RequestQueue({
      deadLetterStore,
      maxAttempts: 2,
    });

    queue.enqueue(job(3n));
    expect(queue.recordFailure(job(3n).raffleContract, 3n, 'ECONNRESET')).toBe('retry');
    expect(queue.size()).toBe(1);

    expect(queue.recordFailure(job(3n).raffleContract, 3n, 'ECONNRESET')).toBe('dead_lettered');
    expect(deadLetterStore.list()[0].reason).toBe('retry_exhausted');
  });

  it('evacuates stale jobs past the age limit to dead-letter', () => {
    const deadLetterStore = new DeadLetterStore(':memory:');
    const queue = new RequestQueue({
      deadLetterStore,
      ageLimitMs: 1_000,
      depthLimit: 100,
    });

    const base = Date.now() - 5_000;
    queue.enqueue(job(1n));
    const evacuated = queue.evacuateUnhealthy(base + 5_000);

    expect(evacuated).toBe(1);
    expect(deadLetterStore.list()[0].reason).toBe('queue_age');
  });

  it('supports manual replay via requeue', () => {
    const deadLetterStore = new DeadLetterStore(':memory:');
    const queue = new RequestQueue({ deadLetterStore });

    const original = job(55n);
    deadLetterStore.add({
      job: original,
      error: 'fatal',
      attemptCount: 1,
      firstEnqueuedAtMs: 0,
      deadLetteredAtMs: 1,
      reason: 'fatal',
    });

    const removed = deadLetterStore.remove(original.raffleContract, original.requestId);
    expect(removed).toBeDefined();
    queue.requeue(removed!.job);
    expect(queue.size()).toBe(1);
  });

  it('alerts when the oldest request exceeds the age limit', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
    const queue = new RequestQueue({
      alerter,
      deadLetterStore: new DeadLetterStore(':memory:'),
      depthLimit: 10,
      ageLimitMs: 5_000,
    });

    queue.enqueue(job(1n));
    queue.checkHealth(Date.now() + 6_000);
    await new Promise((resolve) => setImmediate(resolve));

    expect(fetchImpl).toHaveBeenCalled();
    const body = JSON.parse(fetchImpl.mock.calls[0][1].body);
    expect(body.type).toBe('queue_age');
    expect(body.details.ageMs).toBeGreaterThanOrEqual(6_000);
    expect(body.details.ageMs).toBeLessThan(6_100);
    expect(body.details.limit).toBe(5_000);
  });

  it('does not alert when depth and age are within limits', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
    const queue = new RequestQueue({
      alerter,
      deadLetterStore: new DeadLetterStore(':memory:'),
      depthLimit: 10,
      ageLimitMs: 5_000,
    });

    queue.enqueue(job(1n));
    queue.checkHealth();

    await new Promise((resolve) => setImmediate(resolve));
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});

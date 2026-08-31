import { RequestQueue } from './request-queue';
import { Alerter } from '../alert/alerter';

function mockFetch() {
  return jest.fn().mockResolvedValue({ ok: true, status: 200 } as Response);
}

function job(requestId = 1n) {
  return { requestId, raffleContract: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M', timestamp: 0n };
}

describe('RequestQueue', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env.ALERT_QUEUE_DEPTH_LIMIT;
    delete process.env.ALERT_QUEUE_AGE_LIMIT_MS;
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('enqueues, drains, and reports size', () => {
    const queue = new RequestQueue();
    queue.enqueue(job(1n));
    queue.enqueue(job(2n));
    expect(queue.size()).toBe(2);

    const drained = queue.drain();
    expect(drained.map((j) => j.requestId)).toEqual([1n, 2n]);
    expect(queue.size()).toBe(0);
  });

  it('does not alert when no alerter is configured', () => {
    const queue = new RequestQueue({ depthLimit: 1, ageLimitMs: 1 });
    queue.enqueue(job());
    queue.enqueue(job());
    expect(() => queue.checkHealth(Date.now() + 1_000)).not.toThrow();
  });

  it('alerts when queue depth exceeds the limit', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
    const queue = new RequestQueue({ alerter, depthLimit: 1, ageLimitMs: 60_000 });

    queue.enqueue(job(1n));
    queue.enqueue(job(2n));

    await new Promise((resolve) => setImmediate(resolve));
    expect(fetchImpl).toHaveBeenCalledTimes(0);

    queue.checkHealth();
    await new Promise((resolve) => setImmediate(resolve));

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const body = JSON.parse(fetchImpl.mock.calls[0][1].body);
    expect(body.type).toBe('queue_depth');
    expect(body.details.depth).toBe(2);
    expect(body.details.limit).toBe(1);
  });

  it('alerts when the oldest request exceeds the age limit', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
    const queue = new RequestQueue({ alerter, depthLimit: 10, ageLimitMs: 5_000 });

    queue.enqueue(job(1n));

    queue.checkHealth(Date.now() + 6_000);
    await new Promise((resolve) => setImmediate(resolve));

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const body = JSON.parse(fetchImpl.mock.calls[0][1].body);
    expect(body.type).toBe('queue_age');
    expect(body.details.ageMs).toBeGreaterThanOrEqual(6_000);
    expect(body.details.ageMs).toBeLessThan(6_100);
    expect(body.details.limit).toBe(5_000);
  });

  it('does not alert when depth and age are within limits', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
    const queue = new RequestQueue({ alerter, depthLimit: 10, ageLimitMs: 5_000 });

    queue.enqueue(job(1n));
    queue.checkHealth();

    await new Promise((resolve) => setImmediate(resolve));
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});

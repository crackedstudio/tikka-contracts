import { Alerter } from './alerter';

jest.mock('./logging/logger', () => ({
  logger: {
    error: jest.fn(),
  },
}));

const { logger } = jest.requireMock('./logging/logger') as {
  logger: { error: jest.Mock };
};

function mockFetch() {
  return jest.fn().mockResolvedValue({ ok: true, status: 200 } as Response);
}

describe('Alerter', () => {
  const webhookUrl = 'https://hooks.example.com/alert';
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env['ALERT_WEBHOOK_URL'];
    delete process.env['ALERT_RATE_LIMIT_MS'];
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('is disabled when no webhook URL is configured', async () => {
    const alerter = new Alerter({ fetchImpl: mockFetch() });
    expect(alerter.enabled).toBe(false);

    const delivered = await alerter.notify({
      type: 'submission_failure',
      severity: 'critical',
      message: 'boom',
    });

    expect(delivered).toBe(false);
  });

  it('POSTs a generic JSON alert to the webhook', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl, fetchImpl });

    await alerter.notify({
      type: 'rpc_unreachable',
      severity: 'critical',
      message: 'RPC is down',
      details: { consecutiveFailures: 3 },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const [url, init] = fetchImpl.mock.calls[0];
    expect(url).toBe(webhookUrl);
    expect(init.method).toBe('POST');
    expect(init.headers).toEqual({ 'Content-Type': 'application/json' });

    const body = JSON.parse(init.body);
    expect(body.type).toBe('rpc_unreachable');
    expect(body.severity).toBe('critical');
    expect(body.message).toBe('RPC is down');
    expect(body.details.consecutiveFailures).toBe(3);
    expect(typeof body.timestamp).toBe('number');
  });

  it('rate-limits repeated alerts of the same type within the window', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl, rateLimitMs: 60_000, fetchImpl });

    const first = await alerter.notify({
      type: 'rpc_unreachable',
      severity: 'critical',
      message: 'first',
    });
    const second = await alerter.notify({
      type: 'rpc_unreachable',
      severity: 'critical',
      message: 'second',
    });

    expect(first).toBe(true);
    expect(second).toBe(false);
    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });

  it('delivers again once the rate-limit window elapses', async () => {
    jest.useFakeTimers();
    try {
      const fetchImpl = mockFetch();
      const alerter = new Alerter({ webhookUrl, rateLimitMs: 1_000, fetchImpl });

      await alerter.notify({ type: 'queue_depth', severity: 'warning', message: 'a' });
      jest.advanceTimersByTime(1_001);
      await alerter.notify({ type: 'queue_depth', severity: 'warning', message: 'b' });

      expect(fetchImpl).toHaveBeenCalledTimes(2);
    } finally {
      jest.useRealTimers();
    }
  });

  it('does not rate-limit across different alert types', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl, rateLimitMs: 60_000, fetchImpl });

    await alerter.notify({ type: 'rpc_unreachable', severity: 'critical', message: 'a' });
    await alerter.notify({ type: 'queue_depth', severity: 'warning', message: 'b' });

    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it('logs and swallows webhook delivery failures', async () => {
    const fetchImpl = jest.fn().mockRejectedValue(new Error('network down'));
    const alerter = new Alerter({ webhookUrl, fetchImpl });

    const delivered = await alerter.notify({
      type: 'process_start',
      severity: 'info',
      message: 'started',
    });

    expect(delivered).toBe(true);
    expect(logger.error).toHaveBeenCalledWith(
      expect.stringContaining('Alert delivery failed'),
    );
  });

  it('logs a non-OK webhook response', async () => {
    const fetchImpl = jest.fn().mockResolvedValue({ ok: false, status: 500 } as Response);
    const alerter = new Alerter({ webhookUrl, fetchImpl });

    await alerter.notify({ type: 'process_stop', severity: 'info', message: 'stopped' });

    expect(logger.error).toHaveBeenCalledWith('Alert delivery failed: HTTP 500');
  });

  it('reads the webhook URL and rate limit from the environment', async () => {
    process.env['ALERT_WEBHOOK_URL'] = webhookUrl;
    process.env['ALERT_RATE_LIMIT_MS'] = '5000';
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ fetchImpl });

    expect(alerter.enabled).toBe(true);
    await alerter.notify({ type: 'queue_age', severity: 'warning', message: 'stale' });

    const [url] = fetchImpl.mock.calls[0];
    expect(url).toBe(webhookUrl);
  });

  it('rejects alerts containing Stellar, Hex, or Base64 secret key patterns', async () => {
    const fetchImpl = mockFetch();
    const alerter = new Alerter({ webhookUrl, fetchImpl });

    // Stellar secret
    await expect(
      alerter.notify({
        type: 'leak_check',
        severity: 'critical',
        message: 'A secret key leaked: SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4',
      }),
    ).rejects.toThrow('Stellar secret key detected');

    // Hex secret
    await expect(
      alerter.notify({
        type: 'leak_check',
        severity: 'critical',
        message: 'A hex key leaked',
        details: { key: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2' },
      }),
    ).rejects.toThrow('Hex secret key detected');

    // Config object containing secretKey
    await expect(
      alerter.notify({
        type: 'leak_check',
        severity: 'critical',
        message: 'A config leaked',
        details: { config: { secretKey: 'someKey', rpcUrl: 'http://localhost' } },
      }),
    ).rejects.toThrow('Secret key field "secretKey" detected');

    expect(fetchImpl).not.toHaveBeenCalled();
  });
});

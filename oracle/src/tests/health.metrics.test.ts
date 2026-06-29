import request from 'supertest';
import app, { healthService, metricsService } from '../index';

describe('Oracle health and metrics endpoints', () => {
  beforeEach(() => {
    healthService.setConnected(true);
    healthService.setQueueDepth(0);
    healthService.recordRequest(true);
    healthService.recordRequest(false);
    healthService.recordProcessedLedger(12345678);
  });

  it('returns 200 for healthy state and the expected JSON structure', async () => {
    const response = await request(app).get('/health');
    expect(response.status).toBe(200);
    expect(response.body).toEqual(
      expect.objectContaining({
        status: 'ok',
        connected: true,
        latestLedger: 12345678,
        queueDepth: 0,
        requestsProcessed: expect.any(Number),
        requestsFailed: expect.any(Number),
        uptimeSeconds: expect.any(Number),
      })
    );
  });

  it('returns 503 when not connected', async () => {
    healthService.setConnected(false);
    const response = await request(app).get('/health');
    expect(response.status).toBe(503);
    expect(response.body.status).toBe('down');
    expect(response.body.connected).toBe(false);
  });

  it('returns 503 when last processed ledger is stale', async () => {
    healthService.recordProcessedLedger(12345678, Date.now() - 6 * 60 * 1000);

    const response = await request(app).get('/health');
    expect(response.status).toBe(503);
    expect(response.body.status).toBe('down');
  });

  it('exposes Prometheus metrics at /metrics', async () => {
    await request(app).get('/metrics');
    const response = await request(app).get('/metrics');
    expect(response.status).toBe(200);
    expect(response.text).toContain('oracle_requests_total');
    expect(response.text).toContain('oracle_queue_depth');
    expect(response.text).toContain('oracle_last_processed_ledger');
  });
});

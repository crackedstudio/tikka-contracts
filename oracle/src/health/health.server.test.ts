import http from 'node:http';
import { registry } from '../metrics/metrics';
import { startHealthServer } from './health.server';

describe('health server', () => {
  let server: http.Server;

  afterEach((done) => {
    if (server) {
      server.close(done);
    } else {
      done();
    }
  });

  it('serves /health and /metrics', async () => {
    server = startHealthServer({ port: 0 });
    await new Promise<void>((resolve) => server.once('listening', resolve));
    const address = server.address();
    if (address === null || typeof address === 'string') {
      throw new Error('expected server to bind to a TCP port');
    }

    const base = `http://127.0.0.1:${address.port}`;
    const health = await fetch(`${base}/health`);
    expect(health.status).toBe(200);
    expect(await health.json()).toEqual({ status: 'ok' });

    const metrics = await fetch(`${base}/metrics`);
    expect(metrics.status).toBe(200);
    expect(metrics.headers.get('content-type')).toContain('text/plain');
    const body = await metrics.text();
    expect(body).toContain('oracle_queue_depth');
    expect(body).toContain('process_cpu_user_seconds_total');
    await registry.resetMetrics();
  });
});

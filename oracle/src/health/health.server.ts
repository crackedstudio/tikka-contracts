import http from 'node:http';
import { registry } from '../metrics/metrics';

export interface HealthServerOptions {
  port?: number;
}

/**
 * Serves `/health` (JSON liveness) and `/metrics` (Prometheus text format)
 * on a single HTTP server.
 */
export function startHealthServer(options: HealthServerOptions = {}): http.Server {
  const port = options.port ?? Number(process.env['HEALTH_PORT'] ?? 9090);

  const server = http.createServer(async (req, res) => {
    const path = req.url?.split('?')[0];

    if (path === '/health') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ status: 'ok' }));
      return;
    }

    if (path === '/metrics') {
      res.writeHead(200, { 'Content-Type': registry.contentType });
      res.end(await registry.metrics());
      return;
    }

    res.writeHead(404);
    res.end();
  });

  server.listen(port);
  return server;
}

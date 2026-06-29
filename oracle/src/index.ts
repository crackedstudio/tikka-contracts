import express from 'express';
import { HealthService } from './health.service';
import { MetricsService } from './metrics.service';

const port = Number(process.env.HEALTH_PORT ?? 3000);

export const healthService = new HealthService();
export const metricsService = new MetricsService(healthService);

export function createApp(
  health: HealthService,
  metrics: MetricsService
): express.Express {
  const app = express();

  app.get('/health', (req, res) => {
    const snapshot = health.getSnapshot();
    const statusCode = snapshot.status === 'ok' ? 200 : 503;
    res.status(statusCode).json(snapshot);
  });

  app.get('/metrics', async (req, res) => {
    try {
      const data = await metrics.metrics();
      res.set('Content-Type', 'text/plain; version=0.0.4');
      res.send(data);
    } catch (error) {
      res.status(500).send('Unable to collect metrics');
    }
  });

  return app;
}

const app = createApp(healthService, metricsService);

if (require.main === module) {
  app.listen(port, () => {
    console.log(`Oracle health server listening on port ${port}`);
  });
}

export default app;

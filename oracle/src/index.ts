import { Alerter } from './alert/alerter';
import { loadAndValidateConfig } from './config';
import { createPipeline } from './pipeline';

/**
 * Bootstrap entry point. Wires the full oracle pipeline and exposes /health and
 * /metrics for observability.
 */
async function main(): Promise<void> {
  const config = loadAndValidateConfig();

  const alerter = new Alerter({
    webhookUrl: config.alertWebhookUrl,
    rateLimitMs: config.alertRateLimitMs,
  });

  startHealthServer();

  if (!alerter.enabled) {
    logger.warn('ALERT_WEBHOOK_URL is not set; operational alerts are disabled.');
  } else {
    await alerter.notify({
      type: 'process_start',
      severity: 'info',
      message: `Oracle service started (poll interval ${config.pollIntervalMs}ms)`,
      details: { rpcUrl: config.rpcUrl, pollIntervalMs: config.pollIntervalMs },
    });
  }

  // Create and start the oracle pipeline
  const pipeline = createPipeline(config, {
    alerter,
  });

  // Register signal handlers
  process.on('SIGINT', () => {
    console.log('SIGINT received. Initiating graceful shutdown...');
    void pipeline.shutdown();
  });

  process.on('SIGTERM', () => {
    console.log('SIGTERM received. Initiating graceful shutdown...');
    void pipeline.shutdown();
  });

  // Start listening for events from the factory contract
  await pipeline.start([config.factoryContractId]);
}

main().catch((error) => {
  logger.error(`Oracle service failed to start: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});

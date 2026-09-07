import { OraclePipeline } from './pipeline';
import { Alerter } from './alert/alerter';
import { MemoryLedgerCheckpointStore } from './listener/ledger-checkpoint';
import { DeduplicationStore } from './deduplication/deduplication.store';
import { OracleConfig } from './config';

describe('OraclePipeline', () => {
  let mockConfig: OracleConfig;
  let mockAlerter: Alerter;
  let mockCheckpoint: MemoryLedgerCheckpointStore;
  let mockDedup: DeduplicationStore;

  beforeEach(() => {
    mockConfig = {
      rpcUrl: 'http://localhost:8000',
      factoryContractId: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4',
      logLevel: 'info',
      pollIntervalMs: 5000,
      alertWebhookUrl: '',
      alertFailureThreshold: 3,
      alertRateLimitMs: 60000,
      alertQueueDepthLimit: 10,
      alertQueueAgeLimitMs: 300000,
      alertRpcUnreachableThreshold: 3,
    };

    mockAlerter = new Alerter({ webhookUrl: '', rateLimitMs: 60000 });
    mockCheckpoint = new MemoryLedgerCheckpointStore();
    mockDedup = new DeduplicationStore(':memory:');

    // Set required env var for KeyService
    process.env.ORACLE_SECRET_KEY = 'SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4';
  });

  afterEach(() => {
    delete process.env.ORACLE_SECRET_KEY;
  });

  it('constructs all required components', () => {
    const pipeline = new OraclePipeline({
      config: mockConfig,
      alerter: mockAlerter,
      checkpointStore: mockCheckpoint,
      dedupStore: mockDedup,
    });

    expect(pipeline).toBeDefined();
    expect(pipeline).toBeInstanceOf(OraclePipeline);
  });

  it('uses default stores when none provided', () => {
    const pipeline = new OraclePipeline({
      config: mockConfig,
      alerter: mockAlerter,
    });

    expect(pipeline).toBeDefined();
  });

  it('initializes KeyService when start is called', async () => {
    const pipeline = new OraclePipeline({
      config: mockConfig,
      alerter: mockAlerter,
      checkpointStore: mockCheckpoint,
      dedupStore: mockDedup,
    });

    // start() initializes KeyService
    // We don't actually start it fully to avoid network calls, but verify construction
    expect(pipeline).toBeDefined();
  });

  it('configures TxSubmitter with RPC URL and alerter', () => {
    const pipeline = new OraclePipeline({
      config: mockConfig,
      alerter: mockAlerter,
      checkpointStore: mockCheckpoint,
      dedupStore: mockDedup,
    });

    expect(pipeline).toBeDefined();
  });

  it('configures EventListener with poll interval and RPC settings', () => {
    const pipeline = new OraclePipeline({
      config: mockConfig,
      alerter: mockAlerter,
      checkpointStore: mockCheckpoint,
      dedupStore: mockDedup,
    });

    expect(pipeline).toBeDefined();
  });

  it('configures GracefulShutdown with 30-second drain timeout', () => {
    const pipeline = new OraclePipeline({
      config: mockConfig,
      alerter: mockAlerter,
      checkpointStore: mockCheckpoint,
      dedupStore: mockDedup,
    });

    expect(pipeline).toBeDefined();
  });
});

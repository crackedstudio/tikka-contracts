import { Keypair } from '@stellar/stellar-sdk';
import { loadAndValidateConfig } from './config';

jest.mock('./logging/logger', () => ({
  logger: {
    error: jest.fn(),
  },
}));

const { logger } = jest.requireMock('./logging/logger') as {
  logger: { error: jest.Mock };
};

describe('loadAndValidateConfig', () => {
  const originalEnv = process.env;
  let exitSpy: jest.SpyInstance;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env['ORACLE_SECRET_KEY'];
    delete process.env['STELLAR_RPC_URL'];
    delete process.env['FACTORY_CONTRACT_ID'];
    delete process.env['POLL_INTERVAL_MS'];
    delete process.env['ORACLE_POLL_INTERVAL_MS'];
    delete process.env['LOG_LEVEL'];
    delete process.env['ALERT_WEBHOOK_URL'];
    delete process.env['ALERT_FAILURE_THRESHOLD'];
    delete process.env['ALERT_RATE_LIMIT_MS'];
    delete process.env['ALERT_QUEUE_DEPTH_LIMIT'];
    delete process.env['ALERT_QUEUE_AGE_LIMIT_MS'];
    delete process.env['ALERT_RPC_UNREACHABLE_THRESHOLD'];

    exitSpy = jest.spyOn(process, 'exit').mockImplementation(((code?: number) => {
      throw new Error(`process.exit:${code ?? 0}`);
    }) as never);
    jest.clearAllMocks();
  });

  afterEach(() => {
    process.env = originalEnv;
    exitSpy.mockRestore();
  });

  it('exits with code 1 when required env vars are missing', () => {
    expect(() => loadAndValidateConfig()).toThrow('process.exit:1');
    expect(errorSpy).toHaveBeenCalledWith('Configuration errors:');
    expect(errorSpy).toHaveBeenCalledWith(' - STELLAR_RPC_URL is required');
    expect(errorSpy).toHaveBeenCalledWith(' - FACTORY_CONTRACT_ID is required');
  });

  it('returns validated config when env is valid', () => {
    process.env['ORACLE_SECRET_KEY'] = Keypair.random().secret();
    process.env['STELLAR_RPC_URL'] = 'https://soroban-testnet.stellar.org';
    process.env['FACTORY_CONTRACT_ID'] = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M';
    process.env['POLL_INTERVAL_MS'] = '7000';
    process.env['LOG_LEVEL'] = 'debug';

    const config = loadAndValidateConfig();

    expect(config.rpcUrl).toBe(process.env['STELLAR_RPC_URL']);
    expect(config.factoryContractId).toBe(process.env['FACTORY_CONTRACT_ID']);
    expect(config.logLevel).toBe('debug');
    expect(config.pollIntervalMs).toBe(7000);
  });

  it('defaults alert config when ALERT_* variables are unset', () => {
    process.env['ORACLE_SECRET_KEY'] = Keypair.random().secret();
    process.env['STELLAR_RPC_URL'] = 'https://soroban-testnet.stellar.org';
    process.env['FACTORY_CONTRACT_ID'] = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M';

    const config = loadAndValidateConfig();

    expect(config.alertWebhookUrl).toBe('');
    expect(config.alertFailureThreshold).toBe(3);
    expect(config.alertRateLimitMs).toBe(60_000);
    expect(config.alertQueueDepthLimit).toBe(10);
    expect(config.alertQueueAgeLimitMs).toBe(300_000);
    expect(config.alertRpcUnreachableThreshold).toBe(3);
  });

  it('reads ALERT_* config from env', () => {
    process.env['ORACLE_SECRET_KEY'] = Keypair.random().secret();
    process.env['STELLAR_RPC_URL'] = 'https://soroban-testnet.stellar.org';
    process.env['FACTORY_CONTRACT_ID'] = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M';
    process.env['ALERT_WEBHOOK_URL'] = 'https://hooks.example.com/alert';
    process.env['ALERT_FAILURE_THRESHOLD'] = '5';
    process.env['ALERT_RATE_LIMIT_MS'] = '30000';
    process.env['ALERT_QUEUE_DEPTH_LIMIT'] = '20';
    process.env['ALERT_QUEUE_AGE_LIMIT_MS'] = '600000';
    process.env['ALERT_RPC_UNREACHABLE_THRESHOLD'] = '2';

    const config = loadAndValidateConfig();

    expect(config.alertWebhookUrl).toBe(process.env.ALERT_WEBHOOK_URL);
    expect(config.alertFailureThreshold).toBe(5);
    expect(config.alertRateLimitMs).toBe(30_000);
    expect(config.alertQueueDepthLimit).toBe(20);
    expect(config.alertQueueAgeLimitMs).toBe(600_000);
    expect(config.alertRpcUnreachableThreshold).toBe(2);
  });

  it('exits with code 1 when an ALERT_* value is not a positive number', () => {
    process.env['ORACLE_SECRET_KEY'] = Keypair.random().secret();
    process.env['STELLAR_RPC_URL'] = 'https://soroban-testnet.stellar.org';
    process.env['FACTORY_CONTRACT_ID'] = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M';
    process.env['ALERT_RATE_LIMIT_MS'] = 'not-a-number';

    expect(() => loadAndValidateConfig()).toThrow('process.exit:1');
    expect(logger.error).toHaveBeenCalledWith(' - ALERT_RATE_LIMIT_MS must be a positive number');
  });
});

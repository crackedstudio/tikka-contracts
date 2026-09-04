import { Keypair, rpc as SorobanRpc } from '@stellar/stellar-sdk';
import { Alerter } from '../alert/alerter';
import { TxSubmitterService } from './tx-submitter.service';
import { KeyService } from '../keys/key.service';
import { Alerter } from '../alert/alerter';
import { rpc } from '@stellar/stellar-sdk';

const { Server, Api } = rpc;

jest.mock('../keys/key.service');
jest.mock('@stellar/stellar-sdk', () => {
  const actual = jest.requireActual('@stellar/stellar-sdk');
  return {
    ...actual,
    rpc: {
      ...actual.rpc,
      Server: jest.fn(),
      assembleTransaction: jest.fn(),
    },
    TransactionBuilder: jest.fn(),
    Account: jest.fn(),
    Contract: jest.fn(() => ({
      call: jest.fn(),
    })),
    nativeToScVal: jest.fn((val) => val),
  };
});

const mockKeyService = {
  getKeypair: jest.fn(() => ({ publicKey: () => 'GABC' })),
  getPublicKeyBytes: jest.fn(() => Buffer.from('publickeybytes')),
  sign: jest.fn(() => Buffer.from('signature')),
} as unknown as KeyService;

const baseParams: ProvideRandomnessParams = {
  raffleContract: 'contract',
  randomSeed: BigInt(42),
  publicKey: new Uint8Array(),
  proof: new Uint8Array(),
  requestId: BigInt(1),
};

describe('RetryPolicy', () => {
  const policy = new RetryPolicy({ baseMs: 100, maxMs: 1000, maxAttempts: 3 });

  const cases: { name: string; message: string; expectedClass: RetryClass; expectedRetry: boolean }[] = [
    { name: 'RPC unreachable', message: 'ECONNRESET', expectedClass: RetryClass.TRANSIENT, expectedRetry: true },
    { name: '5xx status', message: 'Server error: status 502', expectedClass: RetryClass.TRANSIENT, expectedRetry: true },
    { name: 'TRY_AGAIN_LATER', message: 'TRY_AGAIN_LATER', expectedClass: RetryClass.TRANSIENT, expectedRetry: true },
    { name: 'sequence collision', message: 'AccountSequenceMismatch', expectedClass: RetryClass.SEQUENCE_COLLISION, expectedRetry: true },
    { name: 'insufficient fee', message: 'InsufficientFee', expectedClass: RetryClass.INSUFFICIENT_FEE, expectedRetry: true },
    { name: 'tx expired', message: 'TxTooLate', expectedClass: RetryClass.TX_EXPIRED, expectedRetry: true },
    { name: 'ledger bounds', message: 'transaction exceeded ledger bounds', expectedClass: RetryClass.TX_EXPIRED, expectedRetry: true },
    { name: 'already satisfied', message: 'RandomnessAlreadyRequested', expectedClass: RetryClass.ALREADY_SATISFIED, expectedRetry: false },
    { name: 'duplicate', message: 'duplicate transaction', expectedClass: RetryClass.ALREADY_SATISFIED, expectedRetry: false },
    { name: 'invalid state', message: 'NoRandomnessRequest', expectedClass: RetryClass.INVALID_STATE, expectedRetry: false },
    { name: 'fatal fallback', message: 'Some other error', expectedClass: RetryClass.FATAL, expectedRetry: false },
  ];

  test.each(cases)('classifies $name', ({ message, expectedClass, expectedRetry }) => {
    const decision = policy.classify(new Error(message));
    expect(decision.class).toBe(expectedClass);
    expect(decision.retry).toBe(expectedRetry);
  });

  test('assigns correct action for sequence collision', () => {
    const decision = policy.classify(new Error('AccountSequenceMismatch'));
    expect(decision.action).toBe('refresh-sequence');
  });

  test('assigns correct action for insufficient fee', () => {
    const decision = policy.classify(new Error('InsufficientFee'));
    expect(decision.action).toBe('bump-fee');
  });

  test('assigns correct action for tx expired', () => {
    const decision = policy.classify(new Error('TxTooLate'));
    expect(decision.action).toBe('rebuild-bounds');
  });

  test('nextDelay is bounded and includes jitter', () => {
    for (let attempt = 0; attempt < 10; attempt++) {
      const delay = policy.nextDelay(attempt);
      const maxExpected = Math.min(1000, 100 * 2 ** attempt);
      expect(delay).toBeGreaterThanOrEqual(0);
      expect(delay).toBeLessThanOrEqual(maxExpected);
    }
  });

  test('maxAttempts is configurable', () => {
    const custom = new RetryPolicy({ maxAttempts: 7 });
    expect(custom.maxAttempts).toBe(7);
  });
});

describe('Alerter', () => {
  test('does not alert before threshold', () => {
    const alerter = new Alerter(3);
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    alerter.recordFailure();
    alerter.recordFailure();
    expect(consoleSpy).not.toHaveBeenCalled();
    consoleSpy.mockRestore();
  });

  test('alerts at threshold', () => {
    const alerter = new Alerter(3);
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    alerter.recordFailure();
    alerter.recordFailure();
    alerter.recordFailure();
    expect(consoleSpy).toHaveBeenCalledTimes(1);
    consoleSpy.mockRestore();
  });

  test('resets on success', () => {
    const alerter = new Alerter(3);
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    alerter.recordFailure();
    alerter.recordFailure();
    alerter.recordSuccess();
    alerter.recordFailure();
    alerter.recordFailure();
    expect(consoleSpy).not.toHaveBeenCalled();
    consoleSpy.mockRestore();
  });
});

describe('TxSubmitterService', () => {
  let mockServer: any;
  let mockGetAccount: jest.Mock;
  let mockSimulate: jest.Mock;
  let mockSend: jest.Mock;
  let mockGetTransaction: jest.Mock;
  let policy: RetryPolicy;
  let alerter: Alerter;

  beforeEach(() => {
    mockGetAccount = jest.fn();
    mockSimulate = jest.fn();
    mockSend = jest.fn();
    mockGetTransaction = jest.fn();
    policy = new RetryPolicy({ baseMs: 10, maxMs: 100, maxAttempts: 3 });
    alerter = new Alerter(3);

    mockServer = {
      getAccount: mockGetAccount,
      simulateTransaction: mockSimulate,
      sendTransaction: mockSend,
      getTransaction: mockGetTransaction,
    };

    (Server as jest.MockedClass<typeof Server>).mockImplementation(() => mockServer as any);
    (require('@stellar/stellar-sdk').rpc as any).assembleTransaction = jest.fn(() => ({
      build: () => ({ sign: jest.fn() }),
    }));
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  function buildSubmitter(): TxSubmitterService {
    return new TxSubmitterService(mockKeyService, policy, alerter, 'http://localhost:8000', 'Test SDF Network ; September 2015');
  }

  async function simulateSuccess(hash: string): Promise<SubmitResult> {
    mockGetAccount.mockResolvedValue({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockResolvedValue({ status: 'PENDING', hash });
    mockGetTransaction.mockResolvedValue({ status: Api.GetTransactionStatus.SUCCESS });

    return buildSubmitter().submitProvideRandomness(baseParams);
  }

  test('returns hash and attempts on success', async () => {
    const result = await simulateSuccess('abc123');
    expect(result.hash).toBe('abc123');
    expect(result.attempts).toBe(1);
  });

  test('retries on transient RPC error then succeeds', async () => {
    mockGetAccount
      .mockRejectedValueOnce(new Error('ECONNRESET'))
      .mockResolvedValueOnce({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockResolvedValue({ status: 'PENDING', hash: 'def456' });
    mockGetTransaction.mockResolvedValue({ status: Api.GetTransactionStatus.SUCCESS });

    const result = await buildSubmitter().submitProvideRandomness(baseParams);
    expect(result.hash).toBe('def456');
    expect(result.attempts).toBe(2);
  });

  test('retries on TRY_AGAIN_LATER then succeeds', async () => {
    mockGetAccount
      .mockRejectedValueOnce(new Error('TRY_AGAIN_LATER'))
      .mockResolvedValueOnce({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockResolvedValue({ status: 'PENDING', hash: 'ghi789' });
    mockGetTransaction.mockResolvedValue({ status: Api.GetTransactionStatus.SUCCESS });

    const result = await buildSubmitter().submitProvideRandomness(baseParams);
    expect(result.hash).toBe('ghi789');
    expect(result.attempts).toBe(2);
  });

  test('retries on sequence collision by refreshing sequence', async () => {
    mockGetAccount
      .mockResolvedValueOnce({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' })
      .mockResolvedValueOnce({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '101' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockResolvedValue({ status: 'PENDING', hash: 'seq1' });
    mockGetTransaction
      .mockResolvedValueOnce({ status: Api.GetTransactionStatus.FAILED })
      .mockResolvedValueOnce({ status: Api.GetTransactionStatus.SUCCESS });

    const result = await buildSubmitter().submitProvideRandomness(baseParams);
    expect(result.hash).toBe('seq1');
    expect(result.attempts).toBe(2);
  });

  test('retries on insufficient fee by bumping fee', async () => {
    mockGetAccount.mockResolvedValue({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend
      .mockResolvedValueOnce({ status: 'ERROR', errorResult: { toXDR: () => 'base64' } })
      .mockResolvedValueOnce({ status: 'PENDING', hash: 'fee1' });
    mockGetTransaction.mockResolvedValue({ status: Api.GetTransactionStatus.SUCCESS });

    const result = await buildSubmitter().submitProvideRandomness(baseParams);
    expect(result.hash).toBe('fee1');
    expect(result.attempts).toBe(2);
  });

  test('retries on tx expired by rebuilding bounds', async () => {
    mockGetAccount.mockResolvedValue({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockResolvedValue({ status: 'PENDING', hash: 'exp1' });
    mockGetTransaction
      .mockResolvedValueOnce({ status: Api.GetTransactionStatus.NOT_FOUND })
      .mockResolvedValueOnce({ status: Api.GetTransactionStatus.SUCCESS });

    const result = await buildSubmitter().submitProvideRandomness(baseParams);
    expect(result.hash).toBe('exp1');
    expect(result.attempts).toBe(2);
  });

  test('does not retry on already satisfied', async () => {
    mockGetAccount.mockResolvedValue({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockRejectedValue(new Error('RandomnessAlreadyRequested'));

    await expect(buildSubmitter().submitProvideRandomness(baseParams)).rejects.toThrow(
      /Permanent failure.*already-satisfied/,
    );
  });

  test('does not retry on invalid state', async () => {
    mockGetAccount.mockResolvedValue({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockRejectedValue(new Error('NoRandomnessRequest'));

    await expect(buildSubmitter().submitProvideRandomness(baseParams)).rejects.toThrow(
      /Permanent failure.*invalid-state/,
    );
  });

  test('does not retry on fatal error', async () => {
    mockGetAccount.mockResolvedValue({ accountId: () => 'G'.padEnd(56, 'A'), sequenceNumber: () => '100' });
    mockSimulate.mockResolvedValue({});
    mockSend.mockRejectedValue(new Error('Some other error'));

    await expect(buildSubmitter().submitProvideRandomness(baseParams)).rejects.toThrow(
      /Permanent failure.*fatal/,
    );
  });

  test('fails after max attempts', async () => {
    mockGetAccount.mockRejectedValue(new Error('ECONNRESET'));

    await expect(buildSubmitter().submitProvideRandomness(baseParams)).rejects.toThrow(
      /Failed to submit after 3 attempts/,
    );
  });

  test('alerter fires at threshold', async () => {
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    mockGetAccount.mockRejectedValue(new Error('ECONNRESET'));

    await expect(buildSubmitter().submitProvideRandomness(baseParams)).rejects.toThrow();

    expect(consoleSpy).toHaveBeenCalledTimes(1);
    consoleSpy.mockRestore();
  });
});

jest.mock('@stellar/stellar-sdk', () => {
  const original = jest.requireActual('@stellar/stellar-sdk');
  const mock = Object.create(original);

  const mockRpc = Object.create(original.rpc);
  Object.defineProperty(mockRpc, 'assembleTransaction', {
    value: jest.fn().mockImplementation((tx: any) => ({
      build: () => tx,
    })),
    writable: true,
    configurable: true,
  });
  mock.rpc = mockRpc;

  return mock;
});

/**
 * Integration test — skipped unless STELLAR_INTEGRATION_TEST=1 and env vars are set.
 * Run against testnet with a funded oracle account and deployed raffle contract.
 */
describe('TxSubmitterService integration', () => {
  const runIntegration = process.env['STELLAR_INTEGRATION_TEST'] === '1';

  (runIntegration ? it : it.skip)(
    'submits provide_randomness to testnet contract',
    async () => {
      const keyService = new KeyService();
      await keyService.initialize();

      const raffleContract = process.env['RAFFLE_CONTRACT_ADDRESS'];
      const requestId = process.env['RANDOMNESS_REQUEST_ID'];
      if (!raffleContract || !requestId) {
        throw new Error('RAFFLE_CONTRACT_ADDRESS and RANDOMNESS_REQUEST_ID required');
      }

      const randomSeed = BigInt(process.env['RANDOMNESS_SEED'] ?? '42');
      const message = buildVrfProofMessage(raffleContract, BigInt(requestId), randomSeed);
      const proof = keyService.sign(message);
      const publicKey = keyService.getPublicKeyBytes();

      const submitter = new TxSubmitterService(keyService);
      const result = await submitter.submitProvideRandomness({
        raffleContract,
        randomSeed,
        publicKey: new Uint8Array(publicKey),
        proof: new Uint8Array(proof),
        requestId: BigInt(requestId),
      });

      expect(result.hash).toMatch(/^[a-f0-9]{64}$/);
    },
    120_000
  );
});

describe('TxSubmitterService alerting', () => {
  const webhookUrl = 'https://hooks.example.com/alert';

  async function buildSubmitter(overrides?: {
    failureThreshold?: number;
    fetchImpl?: jest.Mock;
  }) {
    const keyService = new KeyService({
      getSecret: async () => Buffer.from(Keypair.random().secret()),
    });
    await keyService.initialize();

    const fetchImpl =
      overrides?.fetchImpl ?? jest.fn().mockResolvedValue({ ok: true, status: 200 } as Response);
    const alerter = new Alerter({ webhookUrl, fetchImpl });

    const submitter = new TxSubmitterService(keyService, {
      alerter,
      failureThreshold: overrides?.failureThreshold ?? 1,
      sleep: async () => {
        throw new Error('test must not sleep');
      },
    });

    return { submitter, fetchImpl };
  }

  function params() {
    return {
      raffleContract: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M',
      randomSeed: 42n,
      publicKey: new Uint8Array(32),
      proof: new Uint8Array(64),
      requestId: 7n,
    };
  }

  it('alerts when consecutive submission failures reach the threshold', async () => {
    const { submitter, fetchImpl } = await buildSubmitter({ failureThreshold: 1 });

    const mockServer = {
      getAccount: jest.fn().mockRejectedValue(new Error('boom')),
      simulateTransaction: jest.fn(),
      sendTransaction: jest.fn(),
      getTransaction: jest.fn(),
    };
    (submitter as unknown as { server: typeof mockServer }).server = mockServer;

    await expect(submitter.submitProvideRandomness(params())).rejects.toThrow(
      /Permanent failure/,
    );
    await new Promise((resolve) => setImmediate(resolve));

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const body = JSON.parse(fetchImpl.mock.calls[0][1].body);
    expect(body.type).toBe('submission_failure');
    expect(body.severity).toBe('critical');
    expect(body.details.consecutiveFailures).toBe(1);
    expect(body.details.threshold).toBe(1);
  });

  it('does not alert until the consecutive failure threshold is reached', async () => {
    const { submitter, fetchImpl } = await buildSubmitter({ failureThreshold: 3 });

    const mockServer = {
      getAccount: jest.fn().mockRejectedValue(new Error('boom')),
      simulateTransaction: jest.fn(),
      sendTransaction: jest.fn(),
      getTransaction: jest.fn(),
    };
    (submitter as unknown as { server: typeof mockServer }).server = mockServer;

    await expect(submitter.submitProvideRandomness(params())).rejects.toThrow(
      /Permanent failure/,
    );
    await new Promise((resolve) => setImmediate(resolve));

    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('aggregates repeated submission failures into a single alert within the window', async () => {
    const { submitter, fetchImpl } = await buildSubmitter({ failureThreshold: 1 });

    const mockServer = {
      getAccount: jest.fn().mockRejectedValue(new Error('boom')),
      simulateTransaction: jest.fn(),
      sendTransaction: jest.fn(),
      getTransaction: jest.fn(),
    };
    (submitter as unknown as { server: typeof mockServer }).server = mockServer;

    for (let i = 0; i < 3; i += 1) {
      await expect(submitter.submitProvideRandomness(params())).rejects.toThrow(
        /Permanent failure/,
      );
    }
    await new Promise((resolve) => setImmediate(resolve));

    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });

  it('submits provide_quorum_randomness successfully', async () => {
    const { submitter } = await buildSubmitter({ failureThreshold: 1 });
    const mockServer = {
      getAccount: jest.fn().mockImplementation((pubKey) => Promise.resolve({
        accountId: () => pubKey,
        sequenceNumber: () => '1',
      })),
      simulateTransaction: jest.fn().mockResolvedValue({
        transactionData: 'AAAAAgAAAABlM+QrJVf1z50IqnH57Ck35g==',
        minResourceFee: '100000',
      }),
      sendTransaction: jest.fn().mockResolvedValue({
        status: 'PENDING',
        hash: '1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef',
      }),
      getTransaction: jest.fn().mockResolvedValue({
        status: 'SUCCESS',
      }),
    };
    (submitter as any).server = mockServer;

    const hash = await submitter.submitProvideQuorumRandomness({
      raffleContract: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4',
      randomSeed: 99n,
      requestId: 42n,
    });

    expect(hash).toBe('1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef');
    expect(mockServer.getAccount).toHaveBeenCalled();
    expect(mockServer.simulateTransaction).toHaveBeenCalled();
    expect(mockServer.sendTransaction).toHaveBeenCalled();
  });
});

import { Keypair, rpc as SorobanRpc } from '@stellar/stellar-sdk';
import { Alerter } from '../alert/alerter';
import { TxSubmitterService } from './tx-submitter.service';
import { KeyService } from '../keys/key.service';
import { buildVrfProofMessage } from '../vrf/proof-message';

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
  const runIntegration = process.env.STELLAR_INTEGRATION_TEST === '1';

  (runIntegration ? it : it.skip)(
    'submits provide_randomness to testnet contract',
    async () => {
      const keyService = new KeyService();
      await keyService.initialize();

      const raffleContract = process.env.RAFFLE_CONTRACT_ADDRESS;
      const requestId = process.env.RANDOMNESS_REQUEST_ID;
      if (!raffleContract || !requestId) {
        throw new Error('RAFFLE_CONTRACT_ADDRESS and RANDOMNESS_REQUEST_ID required');
      }

      const randomSeed = BigInt(process.env.RANDOMNESS_SEED ?? '42');
      const message = buildVrfProofMessage(raffleContract, BigInt(requestId), randomSeed);
      const proof = keyService.sign(message);
      const publicKey = keyService.getPublicKeyBytes();

      const submitter = new TxSubmitterService(keyService);
      const hash = await submitter.submitProvideRandomness({
        raffleContract,
        randomSeed,
        publicKey: new Uint8Array(publicKey),
        proof: new Uint8Array(proof),
        requestId: BigInt(requestId),
      });

      expect(hash).toMatch(/^[a-f0-9]{64}$/);
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

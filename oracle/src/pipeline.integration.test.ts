import { Keypair, rpc as SorobanRpc, scValToNative, Address, xdr } from '@stellar/stellar-sdk';
import { createPipeline } from './pipeline';
import { Alerter } from './alert/alerter';
import { MemoryLedgerCheckpointStore } from './listener/ledger-checkpoint';
import { DeduplicationStore } from './deduplication/deduplication.store';

jest.mock('@stellar/stellar-sdk', () => {
  const original = jest.requireActual('@stellar/stellar-sdk');
  const mock = Object.create(original);
  
  Object.defineProperty(mock, 'scValToNative', {
    value: jest.fn(),
    writable: true,
    configurable: true,
  });
  
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

describe('Oracle Pipeline Integration - Happy Paths', () => {
  const rpcUrl = 'http://localhost:8000';
  const testOracleKeypair = Keypair.random();
  const testOracleAddress = testOracleKeypair.publicKey();
  const raffleContract = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4';
  const requestId = 42n;

  let mockAlerter: Alerter;
  let mockCheckpoint: MemoryLedgerCheckpointStore;
  let mockDedup: DeduplicationStore;

  const originalGetLatestLedger = SorobanRpc.Server.prototype.getLatestLedger;
  const originalGetEvents = SorobanRpc.Server.prototype.getEvents;
  const originalGetAccount = SorobanRpc.Server.prototype.getAccount;
  const originalSimulateTransaction = SorobanRpc.Server.prototype.simulateTransaction;
  const originalSendTransaction = SorobanRpc.Server.prototype.sendTransaction;
  const originalGetTransaction = SorobanRpc.Server.prototype.getTransaction;
  const originalExit = process.exit;

  beforeEach(() => {
    process.env.ORACLE_SECRET_KEY = testOracleKeypair.secret();
    mockAlerter = new Alerter({ webhookUrl: '', rateLimitMs: 60000 });
    mockCheckpoint = new MemoryLedgerCheckpointStore();
    mockDedup = new DeduplicationStore(':memory:');
    process.exit = jest.fn() as any;
  });

  afterEach(() => {
    delete process.env.ORACLE_SECRET_KEY;
    SorobanRpc.Server.prototype.getLatestLedger = originalGetLatestLedger;
    SorobanRpc.Server.prototype.getEvents = originalGetEvents;
    SorobanRpc.Server.prototype.getAccount = originalGetAccount;
    SorobanRpc.Server.prototype.simulateTransaction = originalSimulateTransaction;
    SorobanRpc.Server.prototype.sendTransaction = originalSendTransaction;
    SorobanRpc.Server.prototype.getTransaction = originalGetTransaction;
    process.exit = originalExit;
  });

  it('processes RandomnessRequested event and submits single-oracle provide_randomness', async () => {
    let getLatestLedgerCalled = false;
    let ledgerSeq = 100;
    SorobanRpc.Server.prototype.getLatestLedger = async () => {
      getLatestLedgerCalled = true;
      const seq = ledgerSeq;
      ledgerSeq++;
      return { sequence: seq } as any;
    };

    let eventsReturned = false;
    let getEventsCalled = false;
    SorobanRpc.Server.prototype.getEvents = async () => {
      getEventsCalled = true;
      if (eventsReturned) {
        return { latestLedger: 102, events: [] } as any;
      }
      eventsReturned = true;
      return {
        latestLedger: 101,
        events: [
          {
            contractId: { toString: () => raffleContract },
            topic: [xdr.ScVal.scvSymbol('RandomnessRequested')],
            value: xdr.ScVal.scvMap([
              new xdr.ScMapEntry({
                key: xdr.ScVal.scvSymbol('oracle'),
                val: Address.fromString(testOracleAddress).toScVal(),
              }),
              new xdr.ScMapEntry({
                key: xdr.ScVal.scvSymbol('request_id'),
                val: xdr.ScVal.scvU64(xdr.Uint64.fromString(requestId.toString())),
              }),
              new xdr.ScMapEntry({
                key: xdr.ScVal.scvSymbol('timestamp'),
                val: xdr.ScVal.scvU64(xdr.Uint64.fromString('1700000000')),
              }),
            ]),
          } as any,
        ],
      } as any;
    };

    let getAccountCalled = false;
    SorobanRpc.Server.prototype.getAccount = async () => {
      getAccountCalled = true;
      return {
        accountId: () => testOracleAddress,
        sequenceNumber: () => '1',
      } as any;
    };

    let simulationCount = 0;
    SorobanRpc.Server.prototype.simulateTransaction = async () => {
      simulationCount++;
      return {
        result: {
          retval: {} as any,
        },
      } as any;
    };

    const mockScValToNative = scValToNative as jest.Mock;
    mockScValToNative.mockImplementation(() => {
      if (simulationCount === 1) {
        return { randomness_source: 'External' };
      }
      return {};
    });

    let sendTransactionCalled = false;
    SorobanRpc.Server.prototype.sendTransaction = async () => {
      sendTransactionCalled = true;
      return {
        status: 'PENDING',
        hash: 'abc1234567890def1234567890def1234567890def1234567890def123456789',
      } as any;
    };

    let getTransactionCalled = false;
    SorobanRpc.Server.prototype.getTransaction = async () => {
      getTransactionCalled = true;
      return {
        status: SorobanRpc.Api.GetTransactionStatus.SUCCESS,
      } as any;
    };

    const config = {
      rpcUrl,
      factoryContractId: 'CFACTORY1',
      logLevel: 'info',
      pollIntervalMs: 1,
      alertWebhookUrl: '',
      alertFailureThreshold: 3,
      alertRateLimitMs: 60000,
      alertQueueDepthLimit: 10,
      alertQueueAgeLimitMs: 300000,
      alertRpcUnreachableThreshold: 3,
    };

    const pipeline = createPipeline(config, {
      alerter: mockAlerter,
      checkpointStore: mockCheckpoint,
      dedupStore: mockDedup,
    });

    await pipeline.start([raffleContract]);
    
    // Wait for the pipeline loop to process the event
    await new Promise((resolve) => setTimeout(resolve, 150));
    await pipeline.shutdown();

    expect(getLatestLedgerCalled).toBe(true);
    expect(getEventsCalled).toBe(true);
    expect(getAccountCalled).toBe(true);
    expect(simulationCount).toBeGreaterThanOrEqual(2);
    expect(sendTransactionCalled).toBe(true);
    expect(getTransactionCalled).toBe(true);
  });

  it('runs 3 simulated oracles with k=2 and verifies quorum seed submissions', async () => {
    // Generate 3 oracle keypairs
    const keypairA = Keypair.random();
    const keypairB = Keypair.random();
    const keypairC = Keypair.random();

    const addressA = keypairA.publicKey();
    const addressB = keypairB.publicKey();
    const addressC = keypairC.publicKey();

    const config = {
      rpcUrl,
      factoryContractId: 'CFACTORY1',
      logLevel: 'info',
      pollIntervalMs: 1,
      alertWebhookUrl: '',
      alertFailureThreshold: 3,
      alertRateLimitMs: 60000,
      alertQueueDepthLimit: 10,
      alertQueueAgeLimitMs: 300000,
      alertRpcUnreachableThreshold: 3,
    };

    SorobanRpc.Server.prototype.getLatestLedger = async () => {
      return { sequence: 100 } as any;
    };

    SorobanRpc.Server.prototype.getEvents = async () => {
      return { latestLedger: 102, events: [] } as any;
    };

    let simulationCount = 0;
    SorobanRpc.Server.prototype.simulateTransaction = async () => {
      simulationCount++;
      return {
        result: {
          retval: {} as any,
        },
      } as any;
    };

    const mockScValToNative = scValToNative as jest.Mock;
    mockScValToNative.mockImplementation(() => {
      return {
        randomness_source: {
          Quorum: {
            k: 2,
            oracles: [addressA, addressB, addressC],
          },
        },
      };
    });

    // Mock accounts lookup
    SorobanRpc.Server.prototype.getAccount = async (addr: string) => {
      return {
        accountId: () => addr,
        sequenceNumber: () => '1',
      } as any;
    };

    // Track transaction submissions
    const submittedContracts: string[] = [];

    SorobanRpc.Server.prototype.sendTransaction = async (tx) => {
      const op = tx.operations[0] as any;
      submittedContracts.push(op.destination ?? op.contractId ?? raffleContract);
      return {
        status: 'PENDING',
        hash: 'abc1234567890def1234567890def1234567890def1234567890def123456789',
      } as any;
    };

    SorobanRpc.Server.prototype.getTransaction = async () => {
      return {
        status: SorobanRpc.Api.GetTransactionStatus.SUCCESS,
      } as any;
    };

    // Create 3 pipelines
    process.env.ORACLE_SECRET_KEY = keypairA.secret();
    const pipelineA = createPipeline(config, { alerter: mockAlerter, checkpointStore: new MemoryLedgerCheckpointStore(), dedupStore: new DeduplicationStore(':memory:') });
    await pipelineA.start([raffleContract]);

    process.env.ORACLE_SECRET_KEY = keypairB.secret();
    const pipelineB = createPipeline(config, { alerter: mockAlerter, checkpointStore: new MemoryLedgerCheckpointStore(), dedupStore: new DeduplicationStore(':memory:') });
    await pipelineB.start([raffleContract]);

    process.env.ORACLE_SECRET_KEY = keypairC.secret();
    const pipelineC = createPipeline(config, { alerter: mockAlerter, checkpointStore: new MemoryLedgerCheckpointStore(), dedupStore: new DeduplicationStore(':memory:') });
    await pipelineC.start([raffleContract]);

    // Manually trigger processJob for each oracle to simulate receiving the event
    const job = { requestId: 99n, raffleContract, timestamp: 111n };
    await (pipelineA as any).processJob(job);
    await (pipelineB as any).processJob(job);
    await (pipelineC as any).processJob(job);

    await Promise.all([
      pipelineA.shutdown(),
      pipelineB.shutdown(),
      pipelineC.shutdown(),
    ]);

    // Verify all 3 oracles made a submission attempt
    expect(submittedContracts).toHaveLength(3);
  });
});

import { Keypair, rpc as SorobanRpc } from '@stellar/stellar-sdk';
import { Alerter } from './alert/alerter';
import { TxSubmitterService } from './tx/tx-submitter.service';
import { KeyService } from './keys/key.service';

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

describe('Oracle Pipeline Integration - Error and Retry Paths', () => {
  const rpcUrl = 'http://localhost:8000';
  const testOracleKeypair = Keypair.random();
  const testOracleAddress = testOracleKeypair.publicKey();
  const raffleContract = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4';
  const requestId = 42n;

  let keyService: KeyService;

  const originalGetAccount = SorobanRpc.Server.prototype.getAccount;
  const originalSimulateTransaction = SorobanRpc.Server.prototype.simulateTransaction;
  const originalSendTransaction = SorobanRpc.Server.prototype.sendTransaction;
  const originalGetTransaction = SorobanRpc.Server.prototype.getTransaction;

  beforeEach(async () => {
    process.env.ORACLE_SECRET_KEY = testOracleKeypair.secret();
    keyService = new KeyService();
    await keyService.initialize();
  });

  afterEach(() => {
    keyService.shutdown();
    delete process.env.ORACLE_SECRET_KEY;
    SorobanRpc.Server.prototype.getAccount = originalGetAccount;
    SorobanRpc.Server.prototype.simulateTransaction = originalSimulateTransaction;
    SorobanRpc.Server.prototype.sendTransaction = originalSendTransaction;
    SorobanRpc.Server.prototype.getTransaction = originalGetTransaction;
  });

  it('retries on transient RPC errors and eventually succeeds', async () => {
    const submitter = new TxSubmitterService(keyService, { rpcUrl });

    // Mock first getAccount to reject with a transient error, then succeed
    let attempts = 0;
    SorobanRpc.Server.prototype.getAccount = async () => {
      attempts++;
      if (attempts <= 2) {
        throw new Error('AccountSequenceMismatch');
      }
      return {
        accountId: () => testOracleAddress,
        sequenceNumber: () => '1',
      } as any;
    };

    SorobanRpc.Server.prototype.simulateTransaction = async () => {
      return {
        result: {
          retval: {} as any,
        },
      } as any;
    };

    SorobanRpc.Server.prototype.sendTransaction = async () => {
      return {
        status: 'PENDING',
        hash: 'def9876543210abc9876543210abc9876543210abc9876543210abc987654321',
      } as any;
    };

    SorobanRpc.Server.prototype.getTransaction = async () => {
      return {
        status: SorobanRpc.Api.GetTransactionStatus.SUCCESS,
      } as any;
    };

    // Bypass sleep to run test immediately
    (submitter as any).sleepImpl = async () => {};

    const txHash = await submitter.submitProvideRandomness({
      raffleContract,
      randomSeed: 111111n,
      publicKey: keyService.getPublicKeyBytes(),
      proof: new Uint8Array(64),
      requestId,
    });

    expect(txHash).toBe('def9876543210abc9876543210abc9876543210abc9876543210abc987654321');
    expect(attempts).toBe(3);
  });

  it('fails permanently on non-retryable errors', async () => {
    const submitter = new TxSubmitterService(keyService, { rpcUrl });

    SorobanRpc.Server.prototype.getAccount = async () => {
      return {
        accountId: () => testOracleAddress,
        sequenceNumber: () => '1',
      } as any;
    };

    SorobanRpc.Server.prototype.simulateTransaction = async () => {
      throw new Error('Simulation failed: invalid parameters');
    };

    (submitter as any).sleepImpl = async () => {};

    await expect(
      submitter.submitProvideRandomness({
        raffleContract,
        randomSeed: 222222n,
        publicKey: keyService.getPublicKeyBytes(),
        proof: new Uint8Array(64),
        requestId,
      }),
    ).rejects.toThrow(/Permanent failure/);
  });
});

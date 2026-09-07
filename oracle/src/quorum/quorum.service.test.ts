import { QuorumService } from './quorum.service';
import { rpc as SorobanRpc, scValToNative } from '@stellar/stellar-sdk';

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

describe('QuorumService', () => {
  const rpcUrl = 'http://localhost:8000';
  const networkPassphrase = 'Test Passphrase';
  const oracleAddress = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHB';
  const raffleContract = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4';

  it('generates secure seeds', () => {
    const service = new QuorumService(rpcUrl, networkPassphrase, oracleAddress);
    const seed1 = service.generateSecureSeed();
    const seed2 = service.generateSecureSeed();
    expect(typeof seed1).toBe('bigint');
    expect(seed1).not.toBe(seed2);
  });

  it('detects participation when oracle is in quorum list', async () => {
    const service = new QuorumService(rpcUrl, networkPassphrase, oracleAddress);

    jest.spyOn(SorobanRpc.Server.prototype, 'simulateTransaction').mockResolvedValue({
      result: {
        retval: {} as any,
      },
    } as any);

    (scValToNative as jest.Mock).mockReturnValue({
      randomness_source: {
        Quorum: {
          k: 2,
          oracles: [oracleAddress, 'GOTHERADDRESS'],
        },
      },
    });

    const result = await service.checkQuorumParticipation(raffleContract);
    expect(result.isParticipant).toBe(true);
    expect(result.k).toBe(2);
    expect(result.oracles).toContain(oracleAddress);
  });

  it('detects non-participation when oracle is not in list', async () => {
    const service = new QuorumService(rpcUrl, networkPassphrase, oracleAddress);

    jest.spyOn(SorobanRpc.Server.prototype, 'simulateTransaction').mockResolvedValue({
      result: {
        retval: {} as any,
      },
    } as any);

    (scValToNative as jest.Mock).mockReturnValue({
      randomness_source: {
        Quorum: {
          k: 2,
          oracles: ['GOTHERADDRESS1', 'GOTHERADDRESS2'],
        },
      },
    });

    const result = await service.checkQuorumParticipation(raffleContract);
    expect(result.isParticipant).toBe(false);
  });
});

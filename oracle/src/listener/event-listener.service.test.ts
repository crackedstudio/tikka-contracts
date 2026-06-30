import { Address, Keypair, xdr } from '@stellar/stellar-sdk';
import { EventListenerService } from './event-listener.service';
import { MemoryLedgerCheckpointStore } from './ledger-checkpoint';
import { RequestQueue } from '../queue/request-queue';

describe('EventListenerService', () => {
  const oracleKeypair = Keypair.random();
  const oracleAddress = oracleKeypair.publicKey();
  const raffleContract = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4';

  function buildRandomnessRequestedEvent(overrides?: {
    oracle?: string;
    requestId?: bigint;
    raffleContract?: string;
  }) {
    const oracle = overrides?.oracle ?? oracleAddress;
    const requestId = overrides?.requestId ?? 42n;
    const contractId = overrides?.raffleContract ?? raffleContract;

    return {
      contractId: { toString: () => contractId },
      topic: [xdr.ScVal.scvSymbol('RandomnessRequested')],
      value: xdr.ScVal.scvMap([
        new xdr.ScMapEntry({
          key: xdr.ScVal.scvSymbol('oracle'),
          val: Address.fromString(oracle).toScVal(),
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
    } as unknown as Parameters<EventListenerService['parseRandomnessRequestedEvent']>[0];
  }

  it('parses RandomnessRequested events', () => {
    const service = new EventListenerService(
      new RequestQueue(),
      oracleAddress,
      new MemoryLedgerCheckpointStore(),
    );

    const parsed = service.parseRandomnessRequestedEvent(buildRandomnessRequestedEvent());
    expect(parsed?.requestId).toBe(42n);
    expect(parsed?.oracle).toBe(oracleAddress);
    expect(parsed?.raffleContract).toBe(raffleContract);
  });

  it('enqueues matching oracle requests during polling', async () => {
    const queue = new RequestQueue();
    const checkpoint = new MemoryLedgerCheckpointStore();
    const service = new EventListenerService(queue, oracleAddress, checkpoint, {
      pollIntervalMs: 1,
      sleep: async () => {
        service.stopListening();
      },
    });

    const event = buildRandomnessRequestedEvent({ requestId: 99n });
    const mockServer = {
      getLatestLedger: jest.fn().mockResolvedValue({ sequence: 100 }),
      getEvents: jest.fn().mockResolvedValue({
        latestLedger: 101,
        events: [event],
      }),
    };
    (service as unknown as { server: typeof mockServer }).server = mockServer;

    await service.initialize();
    await service.startListening([raffleContract]);

    const jobs = queue.drain();
    expect(jobs).toHaveLength(1);
    expect(jobs[0].requestId).toBe(99n);
    expect(jobs[0].raffleContract).toBe(raffleContract);
    expect(await checkpoint.load()).toBe(101);
  });

  it('ignores events for other oracles', async () => {
    const queue = new RequestQueue();
    const service = new EventListenerService(
      queue,
      oracleAddress,
      new MemoryLedgerCheckpointStore(),
      {
        pollIntervalMs: 1,
        sleep: async () => {
          service.stopListening();
        },
      },
    );

    const otherOracle = Keypair.random().publicKey();
    const event = buildRandomnessRequestedEvent({ oracle: otherOracle });
    (service as unknown as {
      server: {
        getLatestLedger: jest.Mock;
        getEvents: jest.Mock;
      };
    }).server = {
      getLatestLedger: jest.fn().mockResolvedValue({ sequence: 10 }),
      getEvents: jest.fn().mockResolvedValue({
        latestLedger: 11,
        events: [event],
      }),
    };

    await service.initialize();
    await service.startListening([raffleContract]);

    expect(queue.size()).toBe(0);
  });
});

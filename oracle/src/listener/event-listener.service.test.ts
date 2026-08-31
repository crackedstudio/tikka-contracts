import { Address, Keypair, xdr } from '@stellar/stellar-sdk';
import { EventListenerService } from './event-listener.service';
import { MemoryLedgerCheckpointStore } from './ledger-checkpoint';
import { Alerter } from '../alert/alerter';
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

  function buildOracleSeedDeliveredEvent(overrides?: {
    oracle?: string;
    requestId?: bigint;
    currentCount?: number;
    threshold?: number;
  }) {
    const oracle = overrides?.oracle ?? oracleAddress;
    const requestId = overrides?.requestId ?? 42n;
    const currentCount = overrides?.currentCount ?? 1;
    const threshold = overrides?.threshold ?? 2;

    return {
      contractId: { toString: () => raffleContract },
      topic: [xdr.ScVal.scvSymbol('OracleSeedDelivered')],
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
          key: xdr.ScVal.scvSymbol('current_count'),
          val: xdr.ScVal.scvU32(currentCount),
        }),
        new xdr.ScMapEntry({
          key: xdr.ScVal.scvSymbol('threshold'),
          val: xdr.ScVal.scvU32(threshold),
        }),
      ]),
    } as unknown as Parameters<EventListenerService['parseOracleSeedDeliveredEvent']>[0];
  }

  it('parses RandomnessRequested events', () => {
    const service = new EventListenerService(
      new RequestQueue(),
      oracleAddress,
      new MemoryLedgerCheckpointStore()
    );

    const parsed = service.parseRandomnessRequestedEvent(buildRandomnessRequestedEvent());
    expect(parsed?.requestId).toBe(42n);
    expect(parsed?.oracle).toBe(oracleAddress);
    expect(parsed?.raffleContract).toBe(raffleContract);
  });

  it('parses OracleSeedDelivered events', () => {
    const service = new EventListenerService(
      new RequestQueue(),
      oracleAddress,
      new MemoryLedgerCheckpointStore()
    );

    const parsed = service.parseOracleSeedDeliveredEvent(
      buildOracleSeedDeliveredEvent({ currentCount: 2, threshold: 3 }),
    );
    expect(parsed?.requestId).toBe(42n);
    expect(parsed?.oracle).toBe(oracleAddress);
    expect(parsed?.currentCount).toBe(2);
    expect(parsed?.threshold).toBe(3);
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
      }
    );

    const otherOracle = Keypair.random().publicKey();
    const event = buildRandomnessRequestedEvent({ oracle: otherOracle });
    (
      service as unknown as {
        server: {
          getLatestLedger: jest.Mock;
          getEvents: jest.Mock;
        };
      }
    ).server = {
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

  it('does not alert when polling succeeds', async () => {
    const fetchImpl = jest.fn().mockResolvedValue({ ok: true, status: 200 } as Response);
    const alerter = new Alerter({ webhookUrl: 'https://hooks.example.com/alert', fetchImpl });
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
        alerter,
        rpcUnreachableThreshold: 1,
      },
    );
    (service as unknown as {
      server: {
        getLatestLedger: jest.Mock;
        getEvents: jest.Mock;
      };
    }).server = {
      getLatestLedger: jest.fn().mockResolvedValue({ sequence: 10 }),
      getEvents: jest.fn().mockResolvedValue({
        latestLedger: 11,
        events: [],
      }),
    };

    await service.initialize();
    await service.startListening([raffleContract]);

    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('aggregates RPC failures into exactly one alert within the rate-limit window', async () => {
    const originalRateLimitMs = process.env.ALERT_RATE_LIMIT_MS;
    const originalThreshold = process.env.ALERT_RPC_UNREACHABLE_THRESHOLD;
    process.env.ALERT_RATE_LIMIT_MS = '60000';
    process.env.ALERT_RPC_UNREACHABLE_THRESHOLD = '1';

    try {
      let iterations = 0;
      const fetchImpl = jest.fn().mockResolvedValue({ ok: true, status: 200 } as Response);
      const alerter = new Alerter({
        webhookUrl: 'https://hooks.example.com/alert',
        rateLimitMs: 60_000,
        fetchImpl,
      });

      const queue = new RequestQueue();
      const service = new EventListenerService(
        queue,
        oracleAddress,
        new MemoryLedgerCheckpointStore(),
        {
          pollIntervalMs: 1,
          sleep: async () => {
            iterations += 1;
            if (iterations >= 4) {
              service.stopListening();
            }
          },
          alerter,
          rpcUnreachableThreshold: 1,
        },
      );

      // RPC connectivity is killed: every getEvents call rejects.
      (service as unknown as { server: unknown }).server = {
        getLatestLedger: jest.fn().mockResolvedValue({ sequence: 10 }),
        getEvents: jest.fn().mockRejectedValue(new Error('ECONNREFUSED: RPC down')),
      };

      await service.initialize();
      await service.startListening([raffleContract]);

      // 4 polling iterations all fail, but rate limiting collapses them into one alert.
      expect(fetchImpl).toHaveBeenCalledTimes(1);
      const body = JSON.parse(fetchImpl.mock.calls[0][1].body);
      expect(body.type).toBe('rpc_unreachable');
      expect(body.severity).toBe('critical');
    } finally {
      if (originalRateLimitMs === undefined) delete process.env.ALERT_RATE_LIMIT_MS; else process.env.ALERT_RATE_LIMIT_MS = originalRateLimitMs;
      if (originalThreshold === undefined) delete process.env.ALERT_RPC_UNREACHABLE_THRESHOLD; else process.env.ALERT_RPC_UNREACHABLE_THRESHOLD = originalThreshold;
    }
  });
});

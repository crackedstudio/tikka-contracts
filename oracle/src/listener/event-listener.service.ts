import { Address, rpc as SorobanRpc, xdr } from '@stellar/stellar-sdk';
import { Alerter } from '../alert/alerter';
import { RequestQueue } from '../queue/request-queue';
import { LedgerCheckpointStore } from './ledger-checkpoint';

export interface EventListenerOptions {
  pollIntervalMs?: number;
  rpcUrl?: string;
  checkpointStore?: LedgerCheckpointStore;
  sleep?: (ms: number) => Promise<void>;
  alerter?: Alerter;
  rpcUnreachableThreshold?: number;
}

export interface ParsedRandomnessRequest {
  oracle: string;
  requestId: bigint;
  timestamp: bigint;
  raffleContract: string;
}

export class EventListenerService {
  private readonly server: SorobanRpc.Server;
  private readonly pollIntervalMs: number;
  private readonly sleep: (ms: number) => Promise<void>;
  private readonly alerter?: Alerter;
  private readonly rpcUnreachableThreshold: number;
  private startLedger: number;
  private listening = false;
  private consecutiveRpcFailures = 0;

  constructor(
    private readonly queue: RequestQueue,
    private readonly oracleAddress: string,
    private readonly checkpointStore: LedgerCheckpointStore,
    options: EventListenerOptions = {}
  ) {
    const rpcUrl =
      options.rpcUrl ?? process.env.STELLAR_RPC_URL ?? 'https://soroban-testnet.stellar.org';
    this.server = new SorobanRpc.Server(rpcUrl, { allowHttp: rpcUrl.startsWith('http://') });
    this.pollIntervalMs =
      options.pollIntervalMs ?? Number(process.env.ORACLE_POLL_INTERVAL_MS ?? 5000);
    this.sleep = options.sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
    this.alerter = options.alerter;
    this.rpcUnreachableThreshold =
      options.rpcUnreachableThreshold ?? Number(process.env.ALERT_RPC_UNREACHABLE_THRESHOLD ?? 3);
    this.startLedger = 1;
  }

  async initialize(): Promise<void> {
    const saved = await this.checkpointStore.load();
    if (saved !== undefined) {
      this.startLedger = saved + 1;
      return;
    }

    const latest = await this.server.getLatestLedger();
    this.startLedger = latest.sequence;
  }

  async startListening(contractIds: string[]): Promise<void> {
    if (this.listening) {
      return;
    }
    this.listening = true;

    while (this.listening) {
      let events: SorobanRpc.Api.GetEventsResponse;

      try {
        events = await this.server.getEvents({
          startLedger: this.startLedger,
          filters: [
            {
              type: 'contract',
              contractIds,
              topics: [[
                xdr.ScVal.scvSymbol('RandomnessRequested').toXDR('base64'),
                xdr.ScVal.scvSymbol('OracleSeedDelivered').toXDR('base64')
              ]],
            },
          ],
        });
      } catch (error) {
        this.consecutiveRpcFailures += 1;
        if (this.consecutiveRpcFailures >= this.rpcUnreachableThreshold) {
          this.alertRpcUnreachable(error);
        }
        await this.sleep(this.pollIntervalMs);
        continue;
      }

      this.consecutiveRpcFailures = 0;

      for (const event of events.events) {
        const topicName = event.topic[0]?.sym?.().toString();
        if (topicName === 'OracleSeedDelivered') {
          const parsedDelivered = this.parseOracleSeedDeliveredEvent(event);
          if (parsedDelivered) {
            console.log(
              `OracleSeedDelivered event received: raffle=${parsedDelivered.raffleContract} ` +
              `oracle=${parsedDelivered.oracle} request_id=${parsedDelivered.requestId} ` +
              `count=${parsedDelivered.currentCount}/${parsedDelivered.threshold}`
            );
          }
          continue;
        }

        const parsed = this.parseRandomnessRequestedEvent(event);
        if (!parsed) {
          continue;
        }

        if (parsed.oracle === this.oracleAddress) {
          this.queue.enqueue({
            requestId: parsed.requestId,
            raffleContract: parsed.raffleContract,
            timestamp: parsed.timestamp,
          });
        }
      }

      this.startLedger = events.latestLedger + 1;
      await this.checkpointStore.save(events.latestLedger);
      await this.sleep(this.pollIntervalMs);
    }
  }

  private alertRpcUnreachable(error: unknown): void {
    if (!this.alerter) {
      return;
    }

    void this.alerter.notify({
      type: 'rpc_unreachable',
      severity: 'critical',
      message: `RPC unreachable after ${this.consecutiveRpcFailures} consecutive polling failures`,
      details: {
        consecutiveFailures: this.consecutiveRpcFailures,
        threshold: this.rpcUnreachableThreshold,
        error: error instanceof Error ? error.message : String(error),
      },
    });
  }

  stopListening(): void {
    this.listening = false;
  }

  parseRandomnessRequestedEvent(
    event: SorobanRpc.Api.EventResponse
  ): ParsedRandomnessRequest | null {
    const topicName = event.topic[0]?.sym?.().toString();
    if (topicName !== 'RandomnessRequested') {
      return null;
    }

    const raffleContract = event.contractId?.toString();
    if (!raffleContract) {
      return null;
    }

    if (event.value.switch() !== xdr.ScValType.scvMap()) {
      return null;
    }

    let oracle = '';
    let requestId = 0n;
    let timestamp = 0n;

    for (const entry of event.value.map() ?? []) {
      const key = entry.key().sym().toString();
      const val = entry.val();
      if (key === 'oracle') {
        oracle = Address.fromScAddress(val.address()).toString();
      } else if (key === 'request_id') {
        requestId = BigInt(val.u64().toString());
      } else if (key === 'timestamp') {
        timestamp = BigInt(val.u64().toString());
      }
    }

    return { oracle, requestId, timestamp, raffleContract };
  }

  parseOracleSeedDeliveredEvent(
    event: SorobanRpc.Api.EventResponse
  ): { oracle: string; requestId: bigint; currentCount: number; threshold: number; raffleContract: string } | null {
    const topicName = event.topic[0]?.sym?.().toString();
    if (topicName !== 'OracleSeedDelivered') {
      return null;
    }

    const raffleContract = event.contractId?.toString();
    if (!raffleContract) {
      return null;
    }

    if (event.value.switch() !== xdr.ScValType.scvMap()) {
      return null;
    }

    let oracle = '';
    let requestId = 0n;
    let currentCount = 0;
    let threshold = 0;

    for (const entry of event.value.map() ?? []) {
      const key = entry.key().sym().toString();
      const val = entry.val();
      if (key === 'oracle') {
        oracle = Address.fromScAddress(val.address()).toString();
      } else if (key === 'request_id') {
        requestId = BigInt(val.u64().toString());
      } else if (key === 'current_count') {
        currentCount = Number(val.u32().toString());
      } else if (key === 'threshold') {
        threshold = Number(val.u32().toString());
      }
    }

    return { oracle, requestId, currentCount, threshold, raffleContract };
  }
}

import { Address, rpc as SorobanRpc, xdr } from '@stellar/stellar-sdk';
import { RequestQueue } from '../queue/request-queue';
import { LedgerCheckpointStore } from './ledger-checkpoint';

export interface EventListenerOptions {
  pollIntervalMs?: number;
  rpcUrl?: string;
  checkpointStore?: LedgerCheckpointStore;
  sleep?: (ms: number) => Promise<void>;
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
  private startLedger: number;
  private listening = false;

  constructor(
    private readonly queue: RequestQueue,
    private readonly oracleAddress: string,
    private readonly checkpointStore: LedgerCheckpointStore,
    options: EventListenerOptions = {},
  ) {
    const rpcUrl = options.rpcUrl ?? process.env.STELLAR_RPC_URL ?? 'https://soroban-testnet.stellar.org';
    this.server = new SorobanRpc.Server(rpcUrl, { allowHttp: rpcUrl.startsWith('http://') });
    this.pollIntervalMs = options.pollIntervalMs ?? Number(process.env.ORACLE_POLL_INTERVAL_MS ?? 5000);
    this.sleep = options.sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
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
      const events = await this.server.getEvents({
        startLedger: this.startLedger,
        filters: [
          {
            type: 'contract',
            contractIds,
            topics: [[xdr.ScVal.scvSymbol('RandomnessRequested').toXDR('base64')]],
          },
        ],
      });

      for (const event of events.events) {
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

  stopListening(): void {
    this.listening = false;
  }

  parseRandomnessRequestedEvent(
    event: SorobanRpc.Api.EventResponse,
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
}

import {
  Account,
  Contract,
  Networks,
  rpc as SorobanRpc,
  TransactionBuilder,
  nativeToScVal,
} from '@stellar/stellar-sdk';
import { Alerter } from '../alert/alerter';
import { KeyService } from '../keys/key.service';

const MAX_RETRIES = 5;
const BASE_BACKOFF_MS = 500;

export interface ProvideRandomnessParams {
  raffleContract: string;
  randomSeed: bigint;
  publicKey: Uint8Array;
  proof: Uint8Array;
  requestId: bigint;
}

export interface ProvideQuorumRandomnessParams {
  raffleContract: string;
  randomSeed: bigint;
  requestId: bigint;
}

export interface TxSubmitterOptions {
  rpcUrl?: string;
  networkPassphrase?: string;
  alerter?: Alerter;
  failureThreshold?: number;
  sleep?: (ms: number) => Promise<void>;
}

export class TxSubmitterService {
  private readonly server: SorobanRpc.Server;
  private readonly networkPassphrase: string;
  private readonly alerter?: Alerter;
  private readonly failureThreshold: number;
  private readonly sleepImpl: (ms: number) => Promise<void>;
  private sequenceCache?: string;
  private consecutiveFailures = 0;

  constructor(
    private readonly keyService: KeyService,
    options: TxSubmitterOptions | string = {},
  ) {
    // Allow passing a plain rpcUrl string for convenience (e.g. in tests).
    if (typeof options === 'string') {
      options = { rpcUrl: options };
    }
    const rpcUrl = options.rpcUrl ?? process.env.STELLAR_RPC_URL ?? 'https://soroban-testnet.stellar.org';
    this.server = new SorobanRpc.Server(rpcUrl, { allowHttp: rpcUrl.startsWith('http://') });
    this.networkPassphrase =
      options.networkPassphrase ?? process.env.STELLAR_NETWORK_PASSPHRASE ?? Networks.TESTNET;
    this.alerter = options.alerter;
    this.failureThreshold =
      options.failureThreshold ?? Number(process.env.ALERT_FAILURE_THRESHOLD ?? 3);
    this.sleepImpl = options.sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
  }

  async submitProvideRandomness(params: ProvideRandomnessParams): Promise<string> {
    let lastError: Error | undefined;

    for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
      try {
        const hash = await this.submitOnce(params);
        this.consecutiveFailures = 0;
        return hash;
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));
        const message = lastError.message;

        this.recordFailure(message);

        if (!this.isRetryable(message)) {
          throw new Error(`Permanent failure submitting provide_randomness: ${message}`);
        }

        if (message.includes('AccountSequenceMismatch') || message.includes('sequence')) {
          this.sequenceCache = undefined;
        }

        if (attempt < MAX_RETRIES - 1) {
          const delay = BASE_BACKOFF_MS * 2 ** attempt;
          await this.sleepImpl(delay);
        }
      }
    }

    throw new Error(
      `Failed to submit provide_randomness after ${MAX_RETRIES} attempts: ${lastError?.message}`
    );
  }

  private async submitOnce(params: ProvideRandomnessParams): Promise<string> {
    const publicKey = this.keyService.getPublicKey();
    const account = await this.server.getAccount(publicKey);
    const sequence = this.sequenceCache ?? account.sequenceNumber();
    const sourceAccount = new Account(account.accountId(), sequence);

    const contract = new Contract(params.raffleContract);
    const operation = contract.call(
      'provide_randomness',
      nativeToScVal(params.randomSeed, { type: 'u64' }),
      nativeToScVal(Buffer.from(params.publicKey), { type: 'bytes' }),
      nativeToScVal(Buffer.from(params.proof), { type: 'bytes' }),
      nativeToScVal(params.requestId, { type: 'u64' })
    );

    const tx = new TransactionBuilder(sourceAccount, {
      fee: '100000',
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(operation)
      .setTimeout(300)
      .build();

    const simulated = await this.server.simulateTransaction(tx);
    if (SorobanRpc.Api.isSimulationError(simulated)) {
      throw new Error(`Simulation failed: ${JSON.stringify(simulated)}`);
    }

    const prepared = SorobanRpc.assembleTransaction(tx, simulated).build();
    this.keyService.signTransaction(prepared);

    const sendResult = await this.server.sendTransaction(prepared);
    if (sendResult.status === 'ERROR') {
      throw new Error(`Send failed: ${sendResult.errorResult?.toXDR('base64') ?? 'unknown error'}`);
    }

    const hash = sendResult.hash;
    const status = await this.pollTransaction(hash);

    if (status.status === SorobanRpc.Api.GetTransactionStatus.SUCCESS) {
      this.sequenceCache = String(BigInt(sequence) + 1n);
      console.log(`provide_randomness confirmed: ${hash}`);
      return hash;
    }

    if (status.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`Transaction failed on-chain: ${hash}`);
    }

    throw new Error(`Transaction did not confirm: ${hash}`);
  }

  async submitProvideQuorumRandomness(params: ProvideQuorumRandomnessParams): Promise<string> {
    let lastError: Error | undefined;

    for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
      try {
        const hash = await this.submitQuorumOnce(params);
        this.consecutiveFailures = 0;
        return hash;
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));
        const message = lastError.message;

        this.recordFailure(message);

        if (!this.isRetryable(message)) {
          throw new Error(`Permanent failure submitting provide_quorum_randomness: ${message}`);
        }

        if (message.includes('AccountSequenceMismatch') || message.includes('sequence')) {
          this.sequenceCache = undefined;
        }

        if (attempt < MAX_RETRIES - 1) {
          const delay = BASE_BACKOFF_MS * 2 ** attempt;
          await this.sleepImpl(delay);
        }
      }
    }

    throw new Error(
      `Failed to submit provide_quorum_randomness after ${MAX_RETRIES} attempts: ${lastError?.message}`
    );
  }

  private async submitQuorumOnce(params: ProvideQuorumRandomnessParams): Promise<string> {
    const publicKey = this.keyService.getPublicKey();
    const account = await this.server.getAccount(publicKey);
    const sequence = this.sequenceCache ?? account.sequenceNumber();
    const sourceAccount = new Account(account.accountId(), sequence);

    const contract = new Contract(params.raffleContract);
    const operation = contract.call(
      'provide_quorum_randomness',
      nativeToScVal(params.randomSeed, { type: 'u64' }),
      nativeToScVal(params.requestId, { type: 'u64' })
    );

    const tx = new TransactionBuilder(sourceAccount, {
      fee: '100000',
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(operation)
      .setTimeout(300)
      .build();

    const simulated = await this.server.simulateTransaction(tx);
    if (SorobanRpc.Api.isSimulationError(simulated)) {
      throw new Error(`Simulation failed: ${JSON.stringify(simulated)}`);
    }

    const prepared = SorobanRpc.assembleTransaction(tx, simulated).build();
    this.keyService.signTransaction(prepared);

    const sendResult = await this.server.sendTransaction(prepared);
    if (sendResult.status === 'ERROR') {
      throw new Error(`Send failed: ${sendResult.errorResult?.toXDR('base64') ?? 'unknown error'}`);
    }

    const hash = sendResult.hash;
    const status = await this.pollTransaction(hash);

    if (status.status === SorobanRpc.Api.GetTransactionStatus.SUCCESS) {
      this.sequenceCache = String(BigInt(sequence) + 1n);
      console.log(`provide_quorum_randomness confirmed: ${hash}`);
      return hash;
    }

    if (status.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`Transaction failed on-chain: ${hash}`);
    }

    throw new Error(`Transaction did not confirm: ${hash}`);
  }

  private async pollTransaction(
    hash: string,
    maxAttempts = 30,
    intervalMs = 2000
  ): Promise<SorobanRpc.Api.GetTransactionResponse> {
    for (let i = 0; i < maxAttempts; i++) {
      const result = await this.server.getTransaction(hash);
      if (result.status !== SorobanRpc.Api.GetTransactionStatus.NOT_FOUND) {
        return result;
      }
      await this.sleepImpl(intervalMs);
    }
    throw new Error(`TxTooLate: transaction ${hash} not confirmed within timeout`);
  }

  private isRetryable(message: string): boolean {
    const retryable = [
      'TxTooLate',
      'InsufficientFee',
      'AccountSequenceMismatch',
      'ECONNRESET',
      'ETIMEDOUT',
      'fetch failed',
      'network',
      'not confirmed within timeout',
      'Send failed',
      // HTTP 5xx transient errors from getAccount / simulate / send
      'status code 5',
      'status code 429',
      'Request failed',
    ];
    return retryable.some((token) => message.includes(token));
  }

  private recordFailure(message: string): void {
    this.consecutiveFailures += 1;
    if (!this.alerter || this.consecutiveFailures < this.failureThreshold) {
      return;
    }

    void this.alerter.notify({
      type: 'submission_failure',
      severity: 'critical',
      message: `provide_randomness submission failed (${this.consecutiveFailures} consecutive): ${message}`,
      details: {
        consecutiveFailures: this.consecutiveFailures,
        threshold: this.failureThreshold,
        message,
      },
    });
  }
}

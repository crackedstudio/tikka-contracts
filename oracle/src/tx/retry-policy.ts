export enum RetryClass {
  TRANSIENT = 'transient',
  SEQUENCE_COLLISION = 'sequence-collision',
  INSUFFICIENT_FEE = 'insufficient-fee',
  TX_EXPIRED = 'tx-expired',
  ALREADY_SATISFIED = 'already-satisfied',
  INVALID_STATE = 'invalid-state',
  FATAL = 'fatal',
}

export interface RetryDecision {
  class: RetryClass;
  retry: boolean;
  action?: 'refresh-sequence' | 'bump-fee' | 'rebuild-bounds';
}

export interface BackoffConfig {
  baseMs: number;
  maxMs: number;
  maxAttempts: number;
}

const DEFAULT_BACKOFF: BackoffConfig = {
  baseMs: 500,
  maxMs: 30_000,
  maxAttempts: 5,
};

export class RetryPolicy {
  private readonly config: BackoffConfig;

  constructor(config: Partial<BackoffConfig> = {}) {
    this.config = { ...DEFAULT_BACKOFF, ...config };
  }

  classify(error: Error): RetryDecision {
    const message = error.message;

    if (
      message.includes('TRY_AGAIN_LATER') ||
      message.includes('ECONNRESET') ||
      message.includes('ETIMEDOUT') ||
      message.includes('fetch failed') ||
      message.includes('network') ||
      message.includes('5xx') ||
      /status[:\s]+50[0-9]/.test(message)
    ) {
      return { class: RetryClass.TRANSIENT, retry: true };
    }

    if (
      message.includes('AccountSequenceMismatch') ||
      message.includes('sequence')
    ) {
      return {
        class: RetryClass.SEQUENCE_COLLISION,
        retry: true,
        action: 'refresh-sequence',
      };
    }

    if (message.includes('InsufficientFee')) {
      return {
        class: RetryClass.INSUFFICIENT_FEE,
        retry: true,
        action: 'bump-fee',
      };
    }

    if (
      message.includes('TxTooLate') ||
      message.includes('not confirmed within timeout') ||
      message.includes('ledger bounds') ||
      message.includes('expired')
    ) {
      return {
        class: RetryClass.TX_EXPIRED,
        retry: true,
        action: 'rebuild-bounds',
      };
    }

    if (
      message.includes('RandomnessAlreadyRequested') ||
      message.includes('duplicate')
    ) {
      return { class: RetryClass.ALREADY_SATISFIED, retry: false };
    }

    if (
      message.includes('NoRandomnessRequest') ||
      message.includes('invalid state')
    ) {
      return { class: RetryClass.INVALID_STATE, retry: false };
    }

    return { class: RetryClass.FATAL, retry: false };
  }

  nextDelay(attempt: number): number {
    const exponential = Math.min(this.config.maxMs, this.config.baseMs * 2 ** attempt);
    const jitter = Math.random() * exponential;
    return Math.floor(jitter);
  }

  get maxAttempts(): number {
    return this.config.maxAttempts;
  }
}

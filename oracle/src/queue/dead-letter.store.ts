import * as fs from 'fs';
import * as path from 'path';
import { RandomnessJob } from './request-queue';

export interface DeadLetterEntry {
  job: RandomnessJob;
  error: string;
  attemptCount: number;
  firstEnqueuedAtMs: number;
  deadLetteredAtMs: number;
  reason: 'fatal' | 'retry_exhausted' | 'queue_age' | 'queue_depth';
}

interface DeadLetterFile {
  entries: SerializedDeadLetterEntry[];
}

interface SerializedDeadLetterEntry {
  job: {
    requestId: string;
    raffleContract: string;
    timestamp: string;
  };
  error: string;
  attemptCount: number;
  firstEnqueuedAtMs: number;
  deadLetteredAtMs: number;
  reason: DeadLetterEntry['reason'];
}

export class DeadLetterStore {
  private entries: DeadLetterEntry[] = [];
  private readonly filePath: string;
  private readonly inMemoryMode: boolean;

  constructor(storePath: string = path.join(__dirname, '../../data/dead-letter.json')) {
    this.filePath = storePath;
    this.inMemoryMode = storePath === ':memory:';
    if (!this.inMemoryMode) {
      this.loadFromDisk();
    }
  }

  private loadFromDisk(): void {
    try {
      if (fs.existsSync(this.filePath)) {
        const data = JSON.parse(fs.readFileSync(this.filePath, 'utf8')) as DeadLetterFile;
        this.entries = (data.entries ?? []).map(deserializeEntry);
      }
    } catch (error) {
      console.warn('Failed to load dead-letter store, starting fresh:', error);
      const dir = path.dirname(this.filePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
    }
  }

  private saveToDisk(): void {
    if (this.inMemoryMode) {
      return;
    }

    try {
      const dir = path.dirname(this.filePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      const payload: DeadLetterFile = {
        entries: this.entries.map(serializeEntry),
      };
      fs.writeFileSync(this.filePath, JSON.stringify(payload, null, 2));
    } catch (error) {
      console.error('Failed to save dead-letter store:', error);
    }
  }

  add(entry: DeadLetterEntry): void {
    this.entries.push(entry);
    this.saveToDisk();
  }

  list(): DeadLetterEntry[] {
    return [...this.entries];
  }

  size(): number {
    return this.entries.length;
  }

  /**
   * Remove a dead-letter entry and return it for manual replay.
   * Returns undefined when no matching entry exists.
   */
  remove(raffleContract: string, requestId: bigint): DeadLetterEntry | undefined {
    const index = this.entries.findIndex(
      (entry) =>
        entry.job.raffleContract === raffleContract && entry.job.requestId === requestId,
    );
    if (index === -1) {
      return undefined;
    }
    const [removed] = this.entries.splice(index, 1);
    this.saveToDisk();
    return removed;
  }

  /** Clears all entries (mainly useful for tests). */
  clear(): void {
    this.entries = [];
    this.saveToDisk();
  }
}

function serializeEntry(entry: DeadLetterEntry): SerializedDeadLetterEntry {
  return {
    job: {
      requestId: entry.job.requestId.toString(),
      raffleContract: entry.job.raffleContract,
      timestamp: entry.job.timestamp.toString(),
    },
    error: entry.error,
    attemptCount: entry.attemptCount,
    firstEnqueuedAtMs: entry.firstEnqueuedAtMs,
    deadLetteredAtMs: entry.deadLetteredAtMs,
    reason: entry.reason,
  };
}

function deserializeEntry(raw: SerializedDeadLetterEntry): DeadLetterEntry {
  return {
    job: {
      requestId: BigInt(raw.job.requestId),
      raffleContract: raw.job.raffleContract,
      timestamp: BigInt(raw.job.timestamp),
    },
    error: raw.error,
    attemptCount: raw.attemptCount,
    firstEnqueuedAtMs: raw.firstEnqueuedAtMs,
    deadLetteredAtMs: raw.deadLetteredAtMs,
    reason: raw.reason,
  };
}

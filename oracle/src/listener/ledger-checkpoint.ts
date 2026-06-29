import { promises as fs } from 'node:fs';
import * as path from 'node:path';

export interface LedgerCheckpointStore {
  load(): Promise<number | undefined>;
  save(ledger: number): Promise<void>;
}

export class FileLedgerCheckpointStore implements LedgerCheckpointStore {
  constructor(private readonly filePath: string) {}

  async load(): Promise<number | undefined> {
    try {
      const raw = await fs.readFile(this.filePath, 'utf8');
      const parsed = JSON.parse(raw) as { lastLedger?: number };
      return typeof parsed.lastLedger === 'number' ? parsed.lastLedger : undefined;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
        return undefined;
      }
      throw error;
    }
  }

  async save(ledger: number): Promise<void> {
    await fs.mkdir(path.dirname(this.filePath), { recursive: true });
    await fs.writeFile(this.filePath, JSON.stringify({ lastLedger: ledger }, null, 2), 'utf8');
  }
}

export class MemoryLedgerCheckpointStore implements LedgerCheckpointStore {
  private lastLedger?: number;

  async load(): Promise<number | undefined> {
    return this.lastLedger;
  }

  async save(ledger: number): Promise<void> {
    this.lastLedger = ledger;
  }
}

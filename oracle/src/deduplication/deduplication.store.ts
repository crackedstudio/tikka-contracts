import * as fs from 'fs';
import * as path from 'path';
import { logger } from '../logging/logger';

export class DeduplicationStore {
  private seen: Set<string> = new Set();
  private filePath: string;
  private inMemoryMode: boolean;

  constructor(storePath: string = path.join(__dirname, '../../data/seen-requests.json')) {
    this.filePath = storePath;
    this.inMemoryMode = storePath === ':memory:';

    if (!this.inMemoryMode) {
      this.loadFromDisk();
    }
  }

  private loadFromDisk() {
    try {
      if (fs.existsSync(this.filePath)) {
        const data = JSON.parse(fs.readFileSync(this.filePath, 'utf8'));
        this.seen = new Set(data.seen || []);
      }
    } catch (error) {
      logger.warn('Failed to load deduplication store, starting fresh:', error);
      // Ensure directory exists
      const dir = path.dirname(this.filePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
    }
  }

  private saveToDisk() {
    if (this.inMemoryMode) {
      return; // Skip disk I/O in memory mode
    }

    try {
      const dir = path.dirname(this.filePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(this.filePath, JSON.stringify({ seen: Array.from(this.seen) }));
    } catch (error) {
      logger.error('Failed to save deduplication store:', error);
    }
  }

  isDuplicate(requestId: bigint, raffleAddress: string): boolean {
    const key = `${raffleAddress}:${requestId.toString()}`;
    if (this.seen.has(key)) {
      return true;
    }
    this.seen.add(key);
    this.saveToDisk();
    return false;
  }

  // Check if we've already seen this request (does not mutate state)
  hasSeen(requestId: bigint, raffleAddress: string): boolean {
    const key = `${raffleAddress}:${requestId.toString()}`;
    return this.seen.has(key);
  }

  // Mark a request as seen and persist immediately
  markSeen(requestId: bigint, raffleAddress: string): void {
    const key = `${raffleAddress}:${requestId.toString()}`;
    if (!this.seen.has(key)) {
      this.seen.add(key);
      this.saveToDisk();
    }
  }
}

import { DeadLetterStore } from './dead-letter.store';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

describe('DeadLetterStore', () => {
  let testStorePath: string;
  let store: DeadLetterStore;

  beforeEach(() => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'dead-letter-test-'));
    testStorePath = path.join(tempDir, 'dead-letter.json');
    store = new DeadLetterStore(testStorePath);
  });

  afterEach(() => {
    if (fs.existsSync(testStorePath)) {
      fs.unlinkSync(testStorePath);
    }
    const tempDir = path.dirname(testStorePath);
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  const sampleJob = {
    requestId: 42n,
    raffleContract: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M',
    timestamp: 1700000000n,
  };

  it('persists entries with full diagnostic context', () => {
    store.add({
      job: sampleJob,
      error: 'Error(Contract, #8)',
      attemptCount: 3,
      firstEnqueuedAtMs: 1_000,
      deadLetteredAtMs: 2_000,
      reason: 'fatal',
    });

    const entries = store.list();
    expect(entries).toHaveLength(1);
    expect(entries[0].error).toBe('Error(Contract, #8)');
    expect(entries[0].attemptCount).toBe(3);
    expect(entries[0].reason).toBe('fatal');
  });

  it('survives restart by loading from disk', () => {
    store.add({
      job: sampleJob,
      error: 'DrawingAlreadyComplete',
      attemptCount: 1,
      firstEnqueuedAtMs: 5_000,
      deadLetteredAtMs: 6_000,
      reason: 'fatal',
    });

    const restarted = new DeadLetterStore(testStorePath);
    expect(restarted.size()).toBe(1);
    expect(restarted.list()[0].job.requestId).toBe(42n);
  });

  it('remove returns entry for manual replay', () => {
    store.add({
      job: sampleJob,
      error: 'InvalidStatus',
      attemptCount: 2,
      firstEnqueuedAtMs: 100,
      deadLetteredAtMs: 200,
      reason: 'retry_exhausted',
    });

    const removed = store.remove(sampleJob.raffleContract, sampleJob.requestId);
    expect(removed?.reason).toBe('retry_exhausted');
    expect(store.size()).toBe(0);
  });
});

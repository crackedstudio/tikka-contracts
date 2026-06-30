export interface RandomnessJob {
  requestId: bigint;
  raffleContract: string;
  timestamp: bigint;
}

export class RequestQueue {
  private readonly jobs: RandomnessJob[] = [];

  enqueue(job: RandomnessJob): void {
    this.jobs.push(job);
  }

  drain(): RandomnessJob[] {
    const pending = [...this.jobs];
    this.jobs.length = 0;
    return pending;
  }

  size(): number {
    return this.jobs.length;
  }
}

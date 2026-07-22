import {
  ClearraWasmRuntimeError,
  type ClearraDistributedVerifierProgress
} from './clearraWasmRuntime';

type VerifierResponse =
  | { type: 'prewarmed' }
  | { type: 'ready' }
  | {
      type: 'consumed';
      requestId: number;
      candidateCount: number;
      partial: ArrayBuffer | null;
      progress: ClearraDistributedVerifierProgress;
    }
  | { type: 'partial'; requestId: number; partial: ArrayBuffer }
  | { type: 'failed'; requestId?: number; code: string; message: string };

type PendingRequest = {
  resolve: (response: VerifierResponse) => void;
  reject: (error: Error) => void;
};

type VerifierConsumeResult = {
  candidateCount: number;
  partial: ArrayBuffer | null;
};

type PoolWaiter = {
  generation: number;
  resolve: () => void;
  reject: (error: Error) => void;
};

export type ClearraVerifierPoolProgress = {
  candidatesVerified: number;
  buildNodes: number;
  coverageChecks: number;
  activeWorkers: number;
  workerCount: number;
  oldestBatchMs: number;
};

class VerifierClient {
  private worker: Worker | null;
  private nextRequestId = 1;
  private pending = new Map<number, PendingRequest>();
  private ready: Promise<void> | null = null;
  private prewarmed: Promise<void> | null = null;
  private readyReject: ((error: Error) => void) | null = null;
  private prewarmReject: ((error: Error) => void) | null = null;
  private candidatesVerified = 0;
  private progress: ClearraDistributedVerifierProgress = emptyVerifierProgress();
  private batchStartedAt: number | null = null;
  busy = false;

  constructor() {
    this.worker = this.createWorker();
  }

  prewarm(): Promise<void> {
    if (this.prewarmed) return this.prewarmed;
    this.worker ??= this.createWorker();
    const worker = this.worker;
    this.prewarmed = new Promise<void>((resolve, reject) => {
      const rejectAndCleanup = (error: Error) => {
        cleanup();
        reject(error);
      };
      const cleanup = () => {
        worker.removeEventListener('message', onMessage);
        worker.removeEventListener('error', onError);
        if (this.prewarmReject === rejectAndCleanup) this.prewarmReject = null;
      };
      const onMessage = (event: MessageEvent<VerifierResponse>) => {
        if (event.data.type === 'prewarmed') {
          cleanup();
          resolve();
        } else if (event.data.type === 'failed' && event.data.requestId === undefined) {
          rejectAndCleanup(new ClearraWasmRuntimeError(event.data.code, event.data.message));
        }
      };
      const onError = (event: ErrorEvent) => {
        rejectAndCleanup(new Error(event.message || 'distributed verifier warmup failed'));
      };
      this.prewarmReject = rejectAndCleanup;
      worker.addEventListener('message', onMessage);
      worker.addEventListener('error', onError);
      try {
        worker.postMessage({ type: 'prewarm' });
      } catch (error) {
        rejectAndCleanup(asError(error));
      }
    }).catch((error) => {
      this.prewarmed = null;
      throw error;
    });
    return this.prewarmed;
  }

  async initialize(initialization: string | ArrayBuffer): Promise<void> {
    await this.prewarm();
    this.worker ??= this.createWorker();
    const worker = this.worker;
    this.candidatesVerified = 0;
    this.progress = emptyVerifierProgress();
    this.batchStartedAt = null;
    this.ready = new Promise<void>((resolve, reject) => {
      const rejectAndCleanup = (error: Error) => {
        cleanup();
        reject(error);
      };
      const cleanup = () => {
        worker.removeEventListener('message', onMessage);
        worker.removeEventListener('error', onError);
        if (this.readyReject === rejectAndCleanup) this.readyReject = null;
      };
      const onMessage = (event: MessageEvent<VerifierResponse>) => {
        if (event.data.type === 'ready') {
          cleanup();
          resolve();
        } else if (event.data.type === 'failed' && event.data.requestId === undefined) {
          rejectAndCleanup(new ClearraWasmRuntimeError(event.data.code, event.data.message));
        }
      };
      const onError = (event: ErrorEvent) => {
        rejectAndCleanup(new Error(event.message || 'distributed verifier initialization failed'));
      };
      this.readyReject = rejectAndCleanup;
      worker.addEventListener('message', onMessage);
      worker.addEventListener('error', onError);
      const workerInitialization =
        typeof initialization === 'string' ? initialization : initialization.slice(0);
      try {
        worker.postMessage(
          { type: 'initialize', initialization: workerInitialization },
          workerInitialization instanceof ArrayBuffer ? [workerInitialization] : []
        );
      } catch (error) {
        rejectAndCleanup(asError(error));
      }
    });
    return this.ready;
  }

  async consume(batch: ArrayBuffer): Promise<VerifierConsumeResult> {
    this.busy = true;
    this.batchStartedAt = performance.now();
    try {
      await this.ready;
      const response = await this.request({ type: 'consume', batch }, [batch]);
      if (response.type !== 'consumed') throw new Error('invalid verifier consume response');
      this.candidatesVerified += response.candidateCount;
      this.progress = response.progress;
      return { candidateCount: response.candidateCount, partial: response.partial };
    } finally {
      this.busy = false;
      this.batchStartedAt = null;
    }
  }

  progressSnapshot(now: number) {
    return {
      candidatesVerified: this.candidatesVerified,
      buildNodes: this.progress.buildNodes,
      coverageChecks: this.progress.coverageChecks,
      active: this.busy,
      batchAgeMs: this.batchStartedAt === null ? 0 : Math.max(0, now - this.batchStartedAt)
    };
  }

  async finish(): Promise<ArrayBuffer> {
    await this.ready;
    const response = await this.request({ type: 'finish' });
    if (response.type !== 'partial') throw new Error('invalid verifier finish response');
    return response.partial;
  }

  terminate() {
    this.release(new Error('distributed verifier terminated'));
  }

  dispose() {
    this.release(new Error('distributed verifier disposed'));
  }

  private request(
    message: { type: 'consume'; batch: ArrayBuffer } | { type: 'finish' },
    transfer: Transferable[] = []
  ): Promise<VerifierResponse> {
    const requestId = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      const worker = this.worker;
      if (!worker) {
        reject(new Error('distributed verifier is not initialized'));
        return;
      }
      this.pending.set(requestId, { resolve, reject });
      try {
        worker.postMessage({ ...message, requestId }, transfer);
      } catch (error) {
        this.pending.delete(requestId);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  private createWorker(): Worker {
    const worker = new Worker(new URL('./clearraVerifierWorker.ts', import.meta.url), {
      type: 'module'
    });
    worker.onmessage = (event: MessageEvent<VerifierResponse>) => {
      const response = event.data;
      if (response.type === 'ready') return;
      const requestId = response.requestId;
      if (requestId === undefined) return;
      const pending = this.pending.get(requestId);
      if (!pending) return;
      this.pending.delete(requestId);
      if (response.type === 'failed') {
        pending.reject(new ClearraWasmRuntimeError(response.code, response.message));
      } else {
        pending.resolve(response);
      }
    };
    worker.onerror = (event) => {
      const error = new Error(event.message || 'distributed verifier worker failed');
      this.release(error);
    };
    worker.onmessageerror = () => {
      this.release(new Error('distributed verifier worker returned an invalid message'));
    };
    return worker;
  }

  private release(error: Error) {
    const worker = this.worker;
    this.worker = null;
    if (worker) {
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      worker.terminate();
    }
    const prewarmReject = this.prewarmReject;
    const readyReject = this.readyReject;
    this.prewarmReject = null;
    this.readyReject = null;
    prewarmReject?.(error);
    readyReject?.(error);
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    this.ready = null;
    this.prewarmed = null;
    this.candidatesVerified = 0;
    this.progress = emptyVerifierProgress();
    this.batchStartedAt = null;
    this.busy = false;
  }
}

export class ClearraVerifierPool {
  private clients: VerifierClient[] = [];
  private waiters: PoolWaiter[] = [];
  private inFlight = new Set<Promise<void>>();
  private generation = 0;
  private active = false;
  private failure: Error | null = null;

  async prewarm(size: number) {
    try {
      while (this.clients.length < size) this.clients.push(new VerifierClient());
      while (this.clients.length > size) this.clients.pop()?.dispose();
      await Promise.all(this.clients.map((client) => client.prewarm()));
    } catch (error) {
      this.fail(error);
      throw this.failure;
    }
  }

  async initialize(initialization: string | ArrayBuffer, size: number) {
    const generation = ++this.generation;
    this.active = true;
    this.failure = null;
    try {
      while (this.clients.length < size) this.clients.push(new VerifierClient());
      while (this.clients.length > size) this.clients.pop()?.dispose();
      await Promise.all(this.clients.map((client) => client.initialize(initialization)));
      this.assertActive(generation);
    } catch (error) {
      this.fail(error);
      throw this.failure;
    }
  }

  async enqueue(batch: ArrayBuffer, consumePartial: (partial: ArrayBuffer) => void) {
    const generation = this.generation;
    this.assertActive(generation);
    let client = this.clients.find((candidate) => !candidate.busy);
    while (!client) {
      await new Promise<void>((resolve, reject) =>
        this.waiters.push({ generation, resolve, reject })
      );
      this.assertActive(generation);
      client = this.clients.find((candidate) => !candidate.busy);
    }
    const operation = client.consume(batch).then((result) => {
      if (result.partial && result.partial.byteLength > 0) consumePartial(result.partial);
    });
    this.inFlight.add(operation);
    const succeeded = () => {
      this.inFlight.delete(operation);
      this.wakeNextWaiter();
    };
    const failed = (error: unknown) => {
      this.inFlight.delete(operation);
      if (!this.active || generation !== this.generation) return;
      this.fail(error);
    };
    void operation.then(succeeded, failed);
  }

  async finish(consumePartial: (partial: ArrayBuffer) => void): Promise<void> {
    const generation = this.generation;
    await this.waitForIdle();
    this.assertActive(generation);
    const partials = this.clients.map((client) => client.finish());
    for (const partial of partials) {
      const value = await partial;
      if (value.byteLength > 0) consumePartial(value);
    }
    this.assertActive(generation);
    this.active = false;
  }

  async waitForIdle(): Promise<void> {
    const generation = this.generation;
    await Promise.allSettled([...this.inFlight]);
    this.assertActive(generation);
  }

  progressSnapshot(now = performance.now()): ClearraVerifierPoolProgress {
    const snapshots = this.clients.map((client) => client.progressSnapshot(now));
    return {
      candidatesVerified: snapshots.reduce(
        (total, snapshot) => total + snapshot.candidatesVerified,
        0
      ),
      buildNodes: snapshots.reduce((total, snapshot) => total + snapshot.buildNodes, 0),
      coverageChecks: snapshots.reduce(
        (total, snapshot) => total + snapshot.coverageChecks,
        0
      ),
      activeWorkers: snapshots.filter((snapshot) => snapshot.active).length,
      workerCount: snapshots.length,
      oldestBatchMs: snapshots.reduce(
        (oldest, snapshot) => Math.max(oldest, snapshot.batchAgeMs),
        0
      )
    };
  }

  cancel() {
    this.active = false;
    this.failure = null;
    this.generation++;
    for (const client of this.clients) client.terminate();
    this.clients = [];
    this.inFlight.clear();
    const error = new Error('distributed verifier pool cancelled');
    for (const waiter of this.waiters.splice(0)) waiter.reject(error);
  }

  private assertActive(generation: number) {
    if (this.failure) throw this.failure;
    if (!this.active || generation !== this.generation) {
      throw new Error('distributed verifier pool cancelled');
    }
  }

  private fail(error: unknown) {
    if (this.failure) return;
    this.failure = error instanceof Error ? error : new Error(String(error));
    this.active = false;
    this.generation++;
    for (const client of this.clients) client.terminate();
    this.clients = [];
    this.inFlight.clear();
    for (const waiter of this.waiters.splice(0)) waiter.reject(this.failure);
  }

  private wakeNextWaiter() {
    while (this.waiters.length > 0) {
      const waiter = this.waiters.shift()!;
      if (this.active && waiter.generation === this.generation) {
        waiter.resolve();
        return;
      }
      waiter.reject(new Error('distributed verifier pool cancelled'));
    }
  }
}

function emptyVerifierProgress(): ClearraDistributedVerifierProgress {
  return { candidateCount: 0, buildNodes: 0, coverageChecks: 0 };
}

function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

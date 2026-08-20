// SRP rationale: this module has one behavior-level change reason: coordinating verifier
// workers and aggregating their bounded availability and exactness telemetry.
import {
  ClearraWasmRuntimeError,
  type ClearraDistributedVerifierProgress,
  type ClearraWasmHostCapabilities
} from './clearraWasmRuntime';

type VerifierResponse =
  | { type: 'prewarmed' }
  | { type: 'ready' }
  | {
      type: 'heartbeat';
      requestId: number;
      progress: ClearraDistributedVerifierProgress;
    }
  | {
      type: 'consumed';
      requestId: number;
      candidateCount: number;
      candidateCountAvailable: boolean;
      candidateCountExact: boolean;
      partial: ArrayBuffer | null;
      progress: ClearraDistributedVerifierProgress;
    }
  | { type: 'partial'; requestId: number; partial: ArrayBuffer }
  | { type: 'finished'; requestId: number; partial: ArrayBuffer }
  | { type: 'failed'; requestId?: number; code: string; message: string };

type PendingRequest = {
  resolve: (response: VerifierResponse) => void;
  reject: (error: Error) => void;
  onPartial?: (partial: ArrayBuffer) => void;
  operation: 'consume' | 'finish';
  stallDeadlineAt: number;
};

type VerifierConsumeResult = {
  candidateCount: number;
  candidateCountAvailable: boolean;
  candidateCountExact: boolean;
  partial: ArrayBuffer | null;
};

export type ClearraVerifierRecoveryMode =
  | 'replay-state'
  | 'atomic-task'
  | 'streaming';

type PoolWaiter = {
  generation: number;
  resolve: () => void;
  reject: (error: Error) => void;
};

type VerifierWorkerFactory = () => Worker;

const VERIFIER_INITIALIZATION_TIMEOUT_MS = 90_000;
const VERIFIER_REQUEST_STALL_TIMEOUT_MS = 120_000;
const VERIFIER_FINISH_STALL_TIMEOUT_MS = 600_000;
const VERIFIER_WATCHDOG_MAX_SCAN_INTERVAL_MS = 1_000;

export type ClearraVerifierPoolOptions = {
  initializationTimeoutMs?: number;
  requestStallTimeoutMs?: number;
  finishStallTimeoutMs?: number;
};

class VerifierCommitError extends Error {
  constructor(cause: unknown) {
    super('distributed verifier result commit failed', { cause });
    this.name = 'VerifierCommitError';
  }
}

class VerifierTransportError extends Error {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'VerifierTransportError';
  }
}

export type ClearraVerifierPoolProgress = {
  candidatesVerified: number;
  buildNodes: number;
  coverageChecks: number;
  availability: ClearraVerifierPoolProgressFlags;
  exactness: ClearraVerifierPoolProgressFlags;
  readyWorkers: number;
  activeWorkers: number;
  workerCount: number;
  oldestBatchMs: number;
};

export type ClearraVerifierPoolProgressFlags = {
  candidatesVerified: boolean;
  buildNodes: boolean;
  coverageChecks: boolean;
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
  private candidatesVerifiedAvailable = true;
  private candidatesVerifiedExact = true;
  private progress: ClearraDistributedVerifierProgress = emptyVerifierProgress();
  private batchStartedAt: number | null = null;
  private initialized = false;
  private lifecycleOwnerId = '';
  private requestWatchdogScan: ReturnType<typeof setInterval> | null = null;
  private readonly requestWatchdogScanIntervalMs: number;
  busy = false;

  constructor(
    private readonly workerFactory: VerifierWorkerFactory,
    private readonly requestStallTimeoutMs: number,
    private readonly finishStallTimeoutMs: number
  ) {
    this.requestWatchdogScanIntervalMs = watchdogScanInterval(
      requestStallTimeoutMs,
      finishStallTimeoutMs
    );
    this.worker = this.createWorker();
  }

  prewarm(
    compiledModule?: WebAssembly.Module,
    lifecycleOwnerId = '',
    hostCapabilities?: ClearraWasmHostCapabilities
  ): Promise<void> {
    if (
      lifecycleOwnerId &&
      this.lifecycleOwnerId &&
      lifecycleOwnerId !== this.lifecycleOwnerId
    ) {
      this.release(new Error('distributed verifier owner changed'));
      this.worker = this.createWorker();
    }
    if (lifecycleOwnerId) this.lifecycleOwnerId = lifecycleOwnerId;
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
        worker.postMessage({
          type: 'prewarm',
          compiledModule,
          lifecycleOwnerId: this.lifecycleOwnerId,
          hostCapabilities
        });
      } catch (error) {
        if (compiledModule) {
          try {
            worker.postMessage({
              type: 'prewarm',
              lifecycleOwnerId: this.lifecycleOwnerId,
              hostCapabilities
            });
            return;
          } catch {
            // Report the original structured-clone failure below.
          }
        }
        rejectAndCleanup(asError(error));
      }
    }).catch((error) => {
      this.prewarmed = null;
      throw error;
    });
    return this.prewarmed;
  }

  async initialize(
    initialization: string | ArrayBuffer,
    compiledModule?: WebAssembly.Module,
    lifecycleOwnerId = '',
    hostCapabilities?: ClearraWasmHostCapabilities
  ): Promise<void> {
    this.initialized = false;
    await this.prewarm(compiledModule, lifecycleOwnerId, hostCapabilities);
    this.worker ??= this.createWorker();
    const worker = this.worker;
    this.candidatesVerified = 0;
    this.candidatesVerifiedAvailable = true;
    this.candidatesVerifiedExact = true;
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
          this.initialized = true;
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
          {
            type: 'initialize',
            initialization: workerInitialization,
            lifecycleOwnerId: this.lifecycleOwnerId,
            hostCapabilities
          },
          workerInitialization instanceof ArrayBuffer ? [workerInitialization] : []
        );
      } catch (error) {
        rejectAndCleanup(asError(error));
      }
    });
    return this.ready;
  }

  async consume(
    batch: ArrayBuffer,
    onPartial?: (partial: ArrayBuffer) => void
  ): Promise<VerifierConsumeResult> {
    this.busy = true;
    this.batchStartedAt = performance.now();
    try {
      await this.ready;
      const workerBatch = batch.slice(0);
      const response = await this.request(
        { type: 'consume', batch: workerBatch },
        [workerBatch],
        onPartial
      );
      if (response.type !== 'consumed') throw new Error('invalid verifier consume response');
      const accumulated = addProgressCounts(
        this.candidatesVerified,
        response.candidateCount
      );
      this.candidatesVerified = accumulated.value;
      this.candidatesVerifiedAvailable =
        this.candidatesVerifiedAvailable && response.candidateCountAvailable === true;
      this.candidatesVerifiedExact =
        this.candidatesVerifiedExact &&
        response.candidateCountAvailable === true &&
        response.candidateCountExact === true &&
        accumulated.exact;
      this.progress = response.progress;
      return {
        candidateCount: response.candidateCount,
        candidateCountAvailable: response.candidateCountAvailable,
        candidateCountExact: response.candidateCountExact,
        partial: response.partial
      };
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
      availability: {
        candidatesVerified: this.initialized && this.candidatesVerifiedAvailable,
        buildNodes: this.progress.availability.buildNodes,
        coverageChecks: this.progress.availability.coverageChecks
      },
      exactness: {
        candidatesVerified:
          this.initialized &&
          this.candidatesVerifiedAvailable &&
          this.candidatesVerifiedExact,
        buildNodes: this.progress.exactness.buildNodes,
        coverageChecks: this.progress.exactness.coverageChecks
      },
      ready: this.initialized,
      active: this.busy,
      batchAgeMs: this.batchStartedAt === null ? 0 : Math.max(0, now - this.batchStartedAt)
    };
  }

  async finish(): Promise<ArrayBuffer> {
    this.busy = true;
    this.batchStartedAt = performance.now();
    try {
      await this.ready;
      const response = await this.request({ type: 'finish' });
      if (response.type !== 'finished') throw new Error('invalid verifier finish response');
      return response.partial;
    } finally {
      this.busy = false;
      this.batchStartedAt = null;
    }
  }

  isReady(): boolean {
    return this.initialized;
  }

  terminate() {
    this.release(new Error('distributed verifier terminated'));
  }

  dispose() {
    this.release(new Error('distributed verifier disposed'));
  }

  private request(
    message: { type: 'consume'; batch: ArrayBuffer } | { type: 'finish' },
    transfer: Transferable[] = [],
    onPartial?: (partial: ArrayBuffer) => void
  ): Promise<VerifierResponse> {
    const requestId = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      const worker = this.worker;
      if (!worker) {
        reject(new Error('distributed verifier is not initialized'));
        return;
      }
      const pending: PendingRequest = {
        resolve,
        reject,
        onPartial,
        operation: message.type,
        stallDeadlineAt: this.requestStallDeadline(message.type)
      };
      this.pending.set(requestId, pending);
      this.ensureRequestWatchdogScan();
      try {
        worker.postMessage({ ...message, requestId }, transfer);
      } catch (error) {
        this.deletePendingRequest(requestId);
        reject(
          new VerifierTransportError('distributed verifier request transport failed', {
            cause: error
          })
        );
      }
    });
  }

  private createWorker(): Worker {
    const worker = this.workerFactory();
    worker.onmessage = (event: MessageEvent<VerifierResponse>) => {
      const response = event.data;
      if (response.type === 'ready') return;
      if (!('requestId' in response)) return;
      const requestId = response.requestId;
      if (requestId === undefined) return;
      const pending = this.pending.get(requestId);
      if (!pending) return;
      if (response.type === 'heartbeat') {
        this.progress = response.progress;
        pending.stallDeadlineAt = this.requestStallDeadline(pending.operation);
        return;
      }
      if (response.type === 'partial') {
        pending.stallDeadlineAt = this.requestStallDeadline(pending.operation);
        try {
          pending.onPartial?.(response.partial);
        } catch (error) {
          this.deletePendingRequest(requestId);
          pending.reject(asError(error));
        }
        return;
      }
      this.deletePendingRequest(requestId);
      if (response.type === 'failed') {
        pending.reject(new ClearraWasmRuntimeError(response.code, response.message));
      } else {
        pending.resolve(response);
      }
    };
    worker.onerror = (event) => {
      const error = new VerifierTransportError(
        event.message || 'distributed verifier worker failed'
      );
      this.release(error);
    };
    worker.onmessageerror = () => {
      this.release(
        new VerifierTransportError('distributed verifier worker returned an invalid message')
      );
    };
    return worker;
  }

  private requestStallDeadline(operation: PendingRequest['operation']): number {
    return performance.now() + this.requestStallTimeout(operation);
  }

  private requestStallTimeout(operation: PendingRequest['operation']): number {
    return operation === 'finish'
      ? this.finishStallTimeoutMs
      : this.requestStallTimeoutMs;
  }

  private ensureRequestWatchdogScan() {
    if (this.requestWatchdogScan !== null) return;
    this.requestWatchdogScan = setInterval(() => {
      if (this.pending.size === 0) return;
      const now = performance.now();
      for (const pending of this.pending.values()) {
        if (now < pending.stallDeadlineAt) continue;
        const timeoutMs = this.requestStallTimeout(pending.operation);
        this.release(
          new VerifierTransportError(
            `distributed verifier ${pending.operation} stalled for ${timeoutMs} ms`
          )
        );
        return;
      }
    }, this.requestWatchdogScanIntervalMs);
    const nodeTimer = this.requestWatchdogScan as unknown as { unref?: () => void };
    nodeTimer.unref?.();
  }

  private deletePendingRequest(requestId: number) {
    this.pending.delete(requestId);
  }

  private release(error: Error) {
    if (this.requestWatchdogScan !== null) {
      clearInterval(this.requestWatchdogScan);
      this.requestWatchdogScan = null;
    }
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
    this.candidatesVerifiedAvailable = true;
    this.candidatesVerifiedExact = true;
    this.progress = emptyVerifierProgress();
    this.batchStartedAt = null;
    this.initialized = false;
    this.lifecycleOwnerId = '';
    this.busy = false;
  }
}

export class ClearraVerifierPool {
  private clients: VerifierClient[] = [];
  private waiters: PoolWaiter[] = [];
  private inFlight = new Set<Promise<void>>();
  private leasedClients = new Set<VerifierClient>();
  private histories = new Map<VerifierClient, ArrayBuffer[]>();
  private initialization: string | ArrayBuffer | null = null;
  private compiledModule: WebAssembly.Module | undefined;
  private hostCapabilities: ClearraWasmHostCapabilities | undefined;
  private lifecycleOwnerId = '';
  private recoveryMode: ClearraVerifierRecoveryMode = 'atomic-task';
  private targetWorkerCount = 0;
  private generation = 0;
  private active = false;
  private failure: Error | null = null;

  private readonly initializationTimeoutMs: number;
  private readonly requestStallTimeoutMs: number;
  private readonly finishStallTimeoutMs: number;

  constructor(
    private readonly workerFactory: VerifierWorkerFactory = createVerifierWorker,
    options: ClearraVerifierPoolOptions = {}
  ) {
    this.initializationTimeoutMs = positiveTimeout(
      options.initializationTimeoutMs,
      VERIFIER_INITIALIZATION_TIMEOUT_MS
    );
    this.requestStallTimeoutMs = positiveTimeout(
      options.requestStallTimeoutMs,
      VERIFIER_REQUEST_STALL_TIMEOUT_MS
    );
    this.finishStallTimeoutMs = positiveTimeout(
      options.finishStallTimeoutMs,
      VERIFIER_FINISH_STALL_TIMEOUT_MS
    );
  }

  async prewarm(
    size: number,
    compiledModule?: WebAssembly.Module,
    lifecycleOwnerId = '',
    hostCapabilities?: ClearraWasmHostCapabilities
  ) {
    const generation = ++this.generation;
    try {
      while (this.clients.length < size) {
        this.clients.push(
          new VerifierClient(
            this.workerFactory,
            this.requestStallTimeoutMs,
            this.finishStallTimeoutMs
          )
        );
      }
      while (this.clients.length > size) this.clients.pop()?.dispose();
      await Promise.all(
        this.clients.map((client) =>
          client.prewarm(compiledModule, lifecycleOwnerId, hostCapabilities)
        )
      );
      if (generation !== this.generation) return;
    } catch (error) {
      if (generation !== this.generation) return;
      this.fail(error);
      throw this.failure;
    }
  }

  async initialize(
    initialization: string | ArrayBuffer,
    size: number,
    compiledModule?: WebAssembly.Module,
    lifecycleOwnerId = '',
    recoveryMode: ClearraVerifierRecoveryMode = 'atomic-task',
    hostCapabilities?: ClearraWasmHostCapabilities
  ) {
    const generation = ++this.generation;
    this.active = true;
    this.failure = null;
    this.initialization = cloneInitialization(initialization);
    this.compiledModule = compiledModule;
    this.hostCapabilities = hostCapabilities;
    this.lifecycleOwnerId = lifecycleOwnerId;
    this.recoveryMode = recoveryMode;
    this.targetWorkerCount = size;
    this.histories.clear();
    this.leasedClients.clear();
    try {
      if (size < 1) throw new Error('distributed verifier pool requires a worker');
      while (this.clients.length > size) this.clients.pop()?.dispose();
      while (this.clients.length < size) {
        this.clients.push(
          new VerifierClient(
            this.workerFactory,
            this.requestStallTimeoutMs,
            this.finishStallTimeoutMs
          )
        );
      }
      await Promise.all(
        this.clients.map((client) =>
          withTimeout(
            client.initialize(
              initialization,
              compiledModule,
              lifecycleOwnerId,
              hostCapabilities
            ),
            this.initializationTimeoutMs,
            'distributed verifier initialization'
          )
        )
      );
      for (const client of this.clients) this.histories.set(client, []);
      this.assertActive(generation);
    } catch (error) {
      this.fail(error);
      throw this.failure;
    }
  }

  async enqueue(batch: ArrayBuffer, consumePartial: (partial: ArrayBuffer) => void) {
    const generation = this.generation;
    this.assertActive(generation);
    let client = this.findAvailableClient();
    while (!client) {
      await new Promise<void>((resolve, reject) =>
        this.waiters.push({ generation, resolve, reject })
      );
      this.assertActive(generation);
      client = this.findAvailableClient();
    }
    this.leasedClients.add(client);
    const operation = this.consumeLease(client, batch, consumePartial, generation);
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

  async finish(consumePartial: (partial: ArrayBuffer) => void): Promise<number> {
    const generation = this.generation;
    await this.waitForIdle();
    this.assertActive(generation);
    const finished = await Promise.all(
      this.clients.map((client) => this.finishClient(client, generation))
    );
    for (const value of finished) {
      if (value.byteLength > 0) consumePartial(value);
    }
    this.assertActive(generation);
    this.active = false;
    this.histories.clear();
    this.initialization = null;
    return finished.length;
  }

  async waitForIdle(): Promise<void> {
    const generation = this.generation;
    await Promise.allSettled([...this.inFlight]);
    this.assertActive(generation);
  }

  progressSnapshot(now = performance.now()): ClearraVerifierPoolProgress {
    if (!this.active) return emptyPoolProgress();
    const snapshots = this.clients.map((client) => client.progressSnapshot(now));
    const readySnapshots = snapshots.filter((snapshot) => snapshot.ready);
    const candidatesVerified = aggregateProgressCounts(
      snapshots.map((snapshot) => ({
        value: snapshot.candidatesVerified,
        available: snapshot.availability.candidatesVerified,
        exact: snapshot.exactness.candidatesVerified
      }))
    );
    const buildNodes = aggregateProgressCounts(
      snapshots.map((snapshot) => ({
        value: snapshot.buildNodes,
        available: snapshot.availability.buildNodes,
        exact: snapshot.exactness.buildNodes
      }))
    );
    const coverageChecks = aggregateProgressCounts(
      snapshots.map((snapshot) => ({
        value: snapshot.coverageChecks,
        available: snapshot.availability.coverageChecks,
        exact: snapshot.exactness.coverageChecks
      }))
    );
    return {
      candidatesVerified: candidatesVerified.value,
      buildNodes: buildNodes.value,
      coverageChecks: coverageChecks.value,
      availability: {
        candidatesVerified: candidatesVerified.available,
        buildNodes: buildNodes.available,
        coverageChecks: coverageChecks.available
      },
      exactness: {
        candidatesVerified: candidatesVerified.exact,
        buildNodes: buildNodes.exact,
        coverageChecks: coverageChecks.exact
      },
      readyWorkers: readySnapshots.length,
      activeWorkers: readySnapshots.filter((snapshot) => snapshot.active).length,
      workerCount: this.targetWorkerCount,
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
    this.leasedClients.clear();
    this.histories.clear();
    this.initialization = null;
    this.targetWorkerCount = 0;
    const error = new Error('distributed verifier pool cancelled');
    for (const waiter of this.waiters.splice(0)) waiter.reject(error);
  }

  private findAvailableClient(): VerifierClient | undefined {
    return this.clients.find(
      (candidate) =>
        candidate.isReady() && !candidate.busy && !this.leasedClients.has(candidate)
    );
  }

  private async consumeLease(
    initialClient: VerifierClient,
    batch: ArrayBuffer,
    consumePartial: (partial: ArrayBuffer) => void,
    generation: number
  ): Promise<void> {
    let client = initialClient;
    let recoveryAttempted = false;
    const bufferedPartials: ArrayBuffer[] = [];
    try {
      for (;;) {
        try {
          const result = await client.consume(batch, (partial) => {
            if (this.recoveryMode === 'streaming') {
              commitPartial(consumePartial, partial);
            } else {
              bufferedPartials.push(partial);
            }
          });
          this.assertActive(generation);
          if (result.partial && result.partial.byteLength > 0) {
            if (this.recoveryMode === 'streaming') {
              commitPartial(consumePartial, result.partial);
            } else {
              bufferedPartials.push(result.partial);
            }
          }
          if (this.recoveryMode === 'replay-state') {
            this.histories.get(client)?.push(batch);
          }
          for (const partial of bufferedPartials) commitPartial(consumePartial, partial);
          return;
        } catch (error) {
          this.assertActive(generation);
          if (
            recoveryAttempted || !isRetryableVerifierFailure(error)
          ) {
            throw error;
          }
          recoveryAttempted = true;
          bufferedPartials.length = 0;
          client = await this.replaceAndReplayClient(client, generation);
        }
      }
    } finally {
      this.leasedClients.delete(client);
    }
  }

  private async finishClient(
    initialClient: VerifierClient,
    generation: number
  ): Promise<ArrayBuffer> {
    try {
      return await initialClient.finish();
    } catch (error) {
      this.assertActive(generation);
      if (!isRetryableVerifierFailure(error)) throw error;
      const replacement = await this.replaceAndReplayClient(initialClient, generation);
      try {
        return await replacement.finish();
      } finally {
        this.leasedClients.delete(replacement);
      }
    }
  }

  private async replaceAndReplayClient(
    failedClient: VerifierClient,
    generation: number
  ): Promise<VerifierClient> {
    const index = this.clients.indexOf(failedClient);
    if (index < 0 || this.initialization === null) {
      throw new Error('distributed verifier recovery state is unavailable');
    }
    const history = this.histories.get(failedClient) ?? [];
    this.clients.splice(index, 1);
    this.leasedClients.delete(failedClient);
    this.histories.delete(failedClient);
    failedClient.terminate();

    const replacement = new VerifierClient(
      this.workerFactory,
      this.requestStallTimeoutMs,
      this.finishStallTimeoutMs
    );
    this.leasedClients.add(replacement);
    try {
      await withTimeout(
        replacement.initialize(
          this.initialization,
          this.compiledModule,
          this.lifecycleOwnerId,
          this.hostCapabilities
        ),
        this.initializationTimeoutMs,
        'distributed verifier replacement initialization'
      );
      this.assertActive(generation);
      if (this.recoveryMode === 'replay-state') {
        for (const committedBatch of history) {
          await replacement.consume(committedBatch);
          this.assertActive(generation);
        }
      }
      this.clients.splice(Math.min(index, this.clients.length), 0, replacement);
      this.histories.set(replacement, history);
      return replacement;
    } catch (error) {
      this.leasedClients.delete(replacement);
      replacement.terminate();
      throw error;
    }
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
    this.leasedClients.clear();
    this.histories.clear();
    this.initialization = null;
    this.targetWorkerCount = 0;
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
  return {
    candidateCount: 0,
    buildNodes: 0,
    coverageChecks: 0,
    availability: { candidateCount: false, buildNodes: false, coverageChecks: false },
    exactness: { candidateCount: false, buildNodes: false, coverageChecks: false }
  };
}

function emptyPoolProgress(): ClearraVerifierPoolProgress {
  return {
    candidatesVerified: 0,
    buildNodes: 0,
    coverageChecks: 0,
    availability: {
      candidatesVerified: false,
      buildNodes: false,
      coverageChecks: false
    },
    exactness: {
      candidatesVerified: false,
      buildNodes: false,
      coverageChecks: false
    },
    readyWorkers: 0,
    activeWorkers: 0,
    workerCount: 0,
    oldestBatchMs: 0
  };
}

function addProgressCounts(
  left: number,
  right: number
): { value: number; exact: boolean } {
  if (!Number.isSafeInteger(left) || left < 0) {
    return { value: Number.MAX_SAFE_INTEGER, exact: false };
  }
  if (!Number.isSafeInteger(right) || right < 0 || left > Number.MAX_SAFE_INTEGER - right) {
    return { value: Number.MAX_SAFE_INTEGER, exact: false };
  }
  return { value: left + right, exact: true };
}

function aggregateProgressCounts(
  values: Array<{ value: number; available: boolean; exact: boolean }>
): { value: number; available: boolean; exact: boolean } {
  if (values.length === 0) return { value: 0, available: false, exact: false };
  let value = 0;
  let arithmeticExact = true;
  for (const current of values) {
    const sum = addProgressCounts(value, current.value);
    value = sum.value;
    arithmeticExact &&= sum.exact;
  }
  const available = values.every((current) => current.available);
  return {
    value,
    available,
    exact:
      available &&
      arithmeticExact &&
      values.every((current) => current.exact)
  };
}

function createVerifierWorker(): Worker {
  return new Worker(new URL('./clearraVerifierWorker.ts', import.meta.url), {
    type: 'module'
  });
}

function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

function cloneInitialization(value: string | ArrayBuffer): string | ArrayBuffer {
  return typeof value === 'string' ? value : value.slice(0);
}

function positiveTimeout(value: number | undefined, fallback: number): number {
  if (value === undefined || !Number.isFinite(value) || value <= 0) return fallback;
  return Math.max(1, Math.floor(value));
}

function watchdogScanInterval(
  requestStallTimeoutMs: number,
  finishStallTimeoutMs: number
): number {
  const shortestTimeoutMs = Math.min(requestStallTimeoutMs, finishStallTimeoutMs);
  return Math.min(
    VERIFIER_WATCHDOG_MAX_SCAN_INTERVAL_MS,
    Math.max(1, Math.floor(shortestTimeoutMs / 4))
  );
}

function withTimeout<T>(operation: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  return Promise.race([
    operation,
    new Promise<never>((_, reject) => {
      timeout = setTimeout(
        () => reject(new Error(`${label} timed out after ${timeoutMs} ms`)),
        timeoutMs
      );
    })
  ]).finally(() => {
    if (timeout !== undefined) clearTimeout(timeout);
  });
}

function isRetryableVerifierFailure(error: unknown): boolean {
  if (error instanceof VerifierCommitError) return false;
  if (error instanceof VerifierTransportError) return true;
  if (!(error instanceof ClearraWasmRuntimeError)) return false;
  return (
    error.diagnosticCode === 'E_WASM_MODULE_LOAD_FAILED' ||
    error.diagnosticCode === 'E_WASM_VERIFIER_FAILED'
  );
}

function commitPartial(
  consumePartial: (partial: ArrayBuffer) => void,
  partial: ArrayBuffer
) {
  try {
    consumePartial(partial);
  } catch (error) {
    throw new VerifierCommitError(error);
  }
}

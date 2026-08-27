// SRP rationale: this module has one behavior-level change reason: coordinating verifier
// workers and aggregating their bounded availability and exactness telemetry.
import {
  ClearraWasmRuntimeError,
  type ClearraDistributedVerifierProgress,
  type ClearraWasmHostCapabilities
} from './clearraWasmRuntime';
import {
  createBrowserDelegationAuthority,
  DurableDelegationAuthority,
  OFFER_ACCEPT_TIMEOUT_MS,
  sha256Hex,
  type DelegationAcceptance,
  type DelegationOffer,
  type DelegationToken,
  type ExecutableDelegationPermit
} from './DurableDelegationJournal';

type VerifierResponse =
  | { type: 'prewarmed' }
  | { type: 'delegation-accepted'; acceptance: DelegationAcceptance }
  | {
      type: 'delegation-started';
      taskId: string;
      fencingTokenDecimal: string;
    }
  | {
      type: 'delegation-rejected';
      taskId: string;
      fencingTokenDecimal: string;
      code: string;
      message: string;
    }
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
  resolve: (response: DelegatedVerifierResponse) => void;
  reject: (error: Error) => void;
  partials: ArrayBuffer[];
  operation: 'consume' | 'finish';
  stallDeadlineAt: number;
  delegation: ActiveDelegation;
};

type PendingOffer = {
  resolve: (acceptance: DelegationAcceptance) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
  authority: DurableDelegationAuthority;
  token: DelegationToken;
};

type PendingStart = {
  delegation: ActiveDelegation;
};

type ActiveDelegation = {
  authority: DurableDelegationAuthority;
  token: DelegationToken;
  permit: ExecutableDelegationPermit;
};

type DelegatedVerifierResponse = {
  response: VerifierResponse;
  delegation: ActiveDelegation;
  partials: readonly ArrayBuffer[];
  resultSha256: string;
  workerReplySha256: string;
};

type VerifierConsumeResult = {
  candidateCount: number;
  candidateCountAvailable: boolean;
  candidateCountExact: boolean;
  sealed: DelegatedVerifierResponse;
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
  delegationAuthority?: DurableDelegationAuthority | Promise<DurableDelegationAuthority>;
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
  private pendingOffers = new Map<string, PendingOffer>();
  private pendingStarts = new Map<string, PendingStart>();
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
  private rootRequestSha256: string | null = null;
  private initializationDelegation: ActiveDelegation | null = null;
  private nextDelegationTask = 1;
  private requestWatchdogScan: ReturnType<typeof setInterval> | null = null;
  private readonly requestWatchdogScanIntervalMs: number;
  busy = false;

  constructor(
    private readonly workerFactory: VerifierWorkerFactory,
    private readonly requestStallTimeoutMs: number,
    private readonly finishStallTimeoutMs: number,
    private readonly delegationAuthority: Promise<DurableDelegationAuthority>,
    private readonly coordinatorId: string,
    private readonly clientId: string,
    private readonly jobId: string
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
          hostCapabilities,
          workerId: this.clientId
        });
      } catch (error) {
        if (compiledModule) {
          try {
            worker.postMessage({
              type: 'prewarm',
              lifecycleOwnerId: this.lifecycleOwnerId,
              hostCapabilities,
              workerId: this.clientId
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
    const workerInitialization =
      typeof initialization === 'string' ? initialization : initialization.slice(0);
    const delegation = await this.publishExecutableDelegation(
      'initialize',
      workerInitialization,
      byteLength(workerInitialization)
    );
    this.rootRequestSha256 = delegation.permit.payloadSha256;
    this.initializationDelegation = delegation;
    this.ready = new Promise<void>((resolve, reject) => {
      const rejectAndCleanup = (error: Error) => {
        cleanup();
        void this.failDelegation(delegation, error.message);
        reject(error);
      };
      const cleanup = () => {
        worker.removeEventListener('message', onMessage);
        worker.removeEventListener('error', onError);
        if (this.readyReject === rejectAndCleanup) this.readyReject = null;
      };
      const onMessage = (event: MessageEvent<VerifierResponse>) => {
        if (event.data.type === 'ready') {
          void this.completeDelegation(delegation)
            .then(() => {
              if (this.initializationDelegation === delegation) {
                this.initializationDelegation = null;
              }
              cleanup();
              resolve();
            })
            .catch((error) => rejectAndCleanup(asError(error)));
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
      try {
        this.pendingStarts.set(delegation.token.taskId, { delegation });
        worker.postMessage(
          {
            type: 'initialize',
            initialization: workerInitialization,
            lifecycleOwnerId: this.lifecycleOwnerId,
            hostCapabilities,
            delegation: delegation.permit
          },
          workerInitialization instanceof ArrayBuffer ? [workerInitialization] : []
        );
      } catch (error) {
        this.pendingStarts.delete(delegation.token.taskId);
        rejectAndCleanup(asError(error));
      }
    });
    return this.ready;
  }

  async consume(
    batch: ArrayBuffer,
    onPartial?: (partial: ArrayBuffer) => void
  ): Promise<VerifierConsumeResult> {
    this.busy = false;
    this.batchStartedAt = null;
    let activeDelegation: ActiveDelegation | null = null;
    try {
      await this.ready;
      const workerBatch = batch.slice(0);
      const delegated = await this.request(
        { type: 'consume', batch: workerBatch },
        [workerBatch],
        onPartial,
        () => {
          this.busy = true;
          this.batchStartedAt = performance.now();
        }
      );
      activeDelegation = delegated.delegation;
      const response = delegated.response;
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
      const result = {
        candidateCount: response.candidateCount,
        candidateCountAvailable: response.candidateCountAvailable,
        candidateCountExact: response.candidateCountExact,
        sealed: delegated
      };
      activeDelegation = null;
      return result;
    } catch (error) {
      if (activeDelegation) {
        await this.failDelegation(activeDelegation, asError(error).message);
      }
      throw error;
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

  async finish(): Promise<DelegatedVerifierResponse> {
    this.busy = false;
    this.batchStartedAt = null;
    let activeDelegation: ActiveDelegation | null = null;
    try {
      await this.ready;
      const delegated = await this.request({ type: 'finish' }, [], undefined, () => {
        this.busy = true;
        this.batchStartedAt = performance.now();
      });
      activeDelegation = delegated.delegation;
      const response = delegated.response;
      if (response.type !== 'finished') throw new Error('invalid verifier finish response');
      activeDelegation = null;
      return delegated;
    } catch (error) {
      if (activeDelegation) {
        await this.failDelegation(activeDelegation, asError(error).message);
      }
      throw error;
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

  private async request(
    message: { type: 'consume'; batch: ArrayBuffer } | { type: 'finish' },
    transfer: Transferable[] = [],
    _onPartial?: (partial: ArrayBuffer) => void,
    onExecutablePosted?: () => void
  ): Promise<DelegatedVerifierResponse> {
    const requestId = this.nextRequestId++;
    const payload = message.type === 'consume' ? message.batch : 'clearra-verifier-finish-v1';
    const delegation = await this.publishExecutableDelegation(
      message.type,
      payload,
      message.type === 'consume' ? message.batch.byteLength : 0
    );
    return new Promise((resolve, reject) => {
      const worker = this.worker;
      if (!worker) {
        reject(new Error('distributed verifier is not initialized'));
        return;
      }
      const pending: PendingRequest = {
        resolve,
        reject,
        partials: [],
        operation: message.type,
        stallDeadlineAt: this.requestStallDeadline(message.type),
        delegation
      };
      this.pending.set(requestId, pending);
      this.pendingStarts.set(delegation.token.taskId, { delegation });
      this.ensureRequestWatchdogScan();
      try {
        worker.postMessage({ ...message, requestId, delegation: delegation.permit }, transfer);
        onExecutablePosted?.();
      } catch (error) {
        this.pendingStarts.delete(delegation.token.taskId);
        this.deletePendingRequest(requestId);
        void this.failDelegation(delegation, 'worker executable transport failed');
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
      if (response.type === 'delegation-accepted') {
        const pending = this.pendingOffers.get(response.acceptance.taskId);
        if (!pending) return;
        if (
          response.acceptance.fencingTokenDecimal !== pending.token.fencingTokenDecimal
        ) {
          this.rejectPendingOffer(
            response.acceptance.taskId,
            new Error('distributed verifier returned a stale delegation fence')
          );
          return;
        }
        this.resolvePendingOffer(response.acceptance.taskId, response.acceptance);
        return;
      }
      if (response.type === 'delegation-rejected') {
        this.rejectPendingOffer(
          response.taskId,
          new ClearraWasmRuntimeError(response.code, response.message)
        );
        return;
      }
      if (response.type === 'delegation-started') {
        this.acknowledgeExecutableStart(response.taskId, response.fencingTokenDecimal);
        return;
      }
      if (response.type === 'ready') return;
      if (!('requestId' in response)) return;
      const requestId = response.requestId;
      if (requestId === undefined) return;
      const pending = this.pending.get(requestId);
      if (!pending) return;
      if (response.type === 'heartbeat') {
        this.progress = response.progress;
        pending.stallDeadlineAt = this.requestStallDeadline(pending.operation);
        void pending.delegation.authority
          .heartbeat(pending.delegation.token)
          .catch((error) => this.release(asError(error)));
        return;
      }
      if (response.type === 'partial') {
        pending.stallDeadlineAt = this.requestStallDeadline(pending.operation);
        pending.partials.push(response.partial);
        return;
      }
      if (response.type === 'failed') {
        this.deletePendingRequest(requestId);
        void this.failDelegation(pending.delegation, response.message);
        pending.reject(new ClearraWasmRuntimeError(response.code, response.message));
      } else {
        void sealVerifierResponse(pending.operation, pending.delegation, pending.partials, response)
          .then((sealed) => {
            if (this.pending.get(requestId) !== pending) {
              void this.failDelegation(pending.delegation, 'result was cancelled before apply');
              return;
            }
            this.deletePendingRequest(requestId);
            pending.resolve(sealed);
          })
          .catch((error) => {
            if (this.pending.get(requestId) === pending) this.deletePendingRequest(requestId);
            void this.failDelegation(pending.delegation, asError(error).message);
            pending.reject(asError(error));
          });
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

  private async publishExecutableDelegation(
    operation: 'initialize' | 'consume' | 'finish',
    payload: string | ArrayBuffer,
    memoryBytes: number
  ): Promise<ActiveDelegation> {
    const authority = await this.delegationAuthority;
    const payloadSha256 = await sha256Hex(payload);
    const requestSha256 = this.rootRequestSha256 ?? payloadSha256;
    const taskId = `${this.jobId}:${this.clientId}:${this.nextDelegationTask++}:${operation}`;
    const token = await authority.prepare(
      {
        jobId: this.jobId,
        taskId,
        coordinatorId: this.coordinatorId,
        payloadSha256,
        requestSha256
      },
      {
        computeUnitsDecimal: '1',
        memoryBytesDecimal: String(memoryBytes)
      }
    );
    try {
      const offer = await authority.offered(token);
      const acceptance = await this.sendDelegationOffer(authority, token, offer);
      await authority.accepted(token, acceptance);
      const permit = await authority.publish(token);
      return { authority, token, permit };
    } catch (error) {
      try {
        await authority.failedClosed(token, asError(error).message);
      } catch {
        // The original journal/transport failure is the authoritative error.
      }
      throw error;
    }
  }

  private sendDelegationOffer(
    authority: DurableDelegationAuthority,
    token: DelegationToken,
    offer: DelegationOffer
  ): Promise<DelegationAcceptance> {
    return new Promise((resolve, reject) => {
      const worker = this.worker;
      if (!worker) {
        reject(new Error('distributed verifier is unavailable for delegation'));
        return;
      }
      const timeout = setTimeout(() => {
        this.rejectPendingOffer(
          token.taskId,
          new Error(`distributed verifier offer timed out after ${OFFER_ACCEPT_TIMEOUT_MS} ms`)
        );
      }, OFFER_ACCEPT_TIMEOUT_MS);
      const nodeTimer = timeout as unknown as { unref?: () => void };
      nodeTimer.unref?.();
      this.pendingOffers.set(token.taskId, {
        resolve,
        reject,
        timeout,
        authority,
        token
      });
      try {
        // The offer deliberately contains no executable initialization, batch,
        // command, or finish payload.
        worker.postMessage({ type: 'delegation-offer', offer });
      } catch (error) {
        this.rejectPendingOffer(token.taskId, asError(error));
      }
    });
  }

  private resolvePendingOffer(taskId: string, acceptance: DelegationAcceptance): void {
    const pending = this.pendingOffers.get(taskId);
    if (!pending) return;
    this.pendingOffers.delete(taskId);
    clearTimeout(pending.timeout);
    pending.resolve(acceptance);
  }

  private rejectPendingOffer(taskId: string, error: Error): void {
    const pending = this.pendingOffers.get(taskId);
    if (!pending) return;
    this.pendingOffers.delete(taskId);
    clearTimeout(pending.timeout);
    void pending.authority.failedClosed(pending.token, error.message);
    pending.reject(error);
  }

  private acknowledgeExecutableStart(taskId: string, fencingTokenDecimal: string): void {
    const pending = this.pendingStarts.get(taskId);
    if (!pending) return;
    if (pending.delegation.token.fencingTokenDecimal !== fencingTokenDecimal) {
      this.pendingStarts.delete(taskId);
      this.release(new Error('distributed verifier returned a stale executable start fence'));
      return;
    }
    void pending.delegation.authority
      .running(pending.delegation.token)
      .then(() => {
        if (this.pendingStarts.get(taskId) !== pending) return;
        const worker = this.worker;
        if (!worker) throw new Error('distributed verifier disappeared before start ACK');
        worker.postMessage({
          type: 'delegation-run',
          taskId,
          fencingTokenDecimal
        });
        this.pendingStarts.delete(taskId);
      })
      .catch((error) => {
        if (this.pendingStarts.get(taskId) === pending) this.pendingStarts.delete(taskId);
        this.release(asError(error));
      });
  }

  private async completeDelegation(delegation: ActiveDelegation): Promise<void> {
    const sealed = await sealVerifierResponse('initialize', delegation, [], { type: 'ready' });
    const decision = delegation.authority.resultApplicationDecision(
      delegation.token,
      sealed.resultSha256
    );
    if (decision === 'apply-once') {
      this.initialized = true;
      await delegation.authority.resultApplied(delegation.token);
    } else {
      this.initialized = true;
    }
    await delegation.authority.completed(delegation.token);
  }

  private async failDelegation(
    delegation: ActiveDelegation,
    reason: string
  ): Promise<void> {
    try {
      await delegation.authority.failedClosed(delegation.token, reason);
    } catch {
      // Cleanup must not replace the worker/journal failure that caused it.
    }
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
    if (this.initializationDelegation) {
      void this.failDelegation(this.initializationDelegation, error.message);
      this.initializationDelegation = null;
    }
    for (const pending of this.pendingOffers.values()) {
      clearTimeout(pending.timeout);
      void pending.authority.failedClosed(pending.token, error.message);
      pending.reject(error);
    }
    this.pendingOffers.clear();
    for (const pending of this.pendingStarts.values()) {
      void this.failDelegation(pending.delegation, error.message);
    }
    this.pendingStarts.clear();
    for (const pending of this.pending.values()) {
      void this.failDelegation(pending.delegation, error.message);
      pending.reject(error);
    }
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
    this.rootRequestSha256 = null;
    this.busy = false;
  }
}

export class ClearraVerifierPool {
  private clients: VerifierClient[] = [];
  private waiters: PoolWaiter[] = [];
  private inFlight = new Set<Promise<void>>();
  private leasedClients = new Set<VerifierClient>();
  private targetWorkerCount = 0;
  private generation = 0;
  private active = false;
  private failure: Error | null = null;
  private readonly jobId = uniqueDelegationUuid();
  private readonly bootId = uniqueDelegationUuid();
  private readonly delegationAuthority: Promise<DurableDelegationAuthority>;
  private readonly coordinatorId = `unverified-local-build:${this.bootId}`;
  private nextClientId = 1;

  private readonly initializationTimeoutMs: number;
  private readonly requestStallTimeoutMs: number;
  private readonly finishStallTimeoutMs: number;

  constructor(
    private readonly workerFactory: VerifierWorkerFactory = createVerifierWorker,
    options: ClearraVerifierPoolOptions = {}
  ) {
    this.delegationAuthority = Promise.resolve(
      options.delegationAuthority ??
        createBrowserDelegationAuthority(() => Date.now(), this.jobId)
    );
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
        this.clients.push(this.createClient());
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
    // All durable v0.8 modes seal an immutable task result before merger
    // application. The legacy `streaming` label is accepted as an input
    // compatibility spelling but does not re-enable streaming application.
    void recoveryMode;
    this.targetWorkerCount = size;
    this.leasedClients.clear();
    try {
      if (size < 1) throw new Error('distributed verifier pool requires a worker');
      while (this.clients.length > size) this.clients.pop()?.dispose();
      while (this.clients.length < size) {
        this.clients.push(this.createClient());
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
    for (const value of finished) await applySealedVerifierResult(value, consumePartial);
    this.assertActive(generation);
    this.active = false;
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
    try {
      const result = await client.consume(batch);
      this.assertActive(generation);
      await applySealedVerifierResult(result.sealed, consumePartial);
    } finally {
      this.leasedClients.delete(client);
    }
  }

  private async finishClient(
    initialClient: VerifierClient,
    generation: number
  ): Promise<DelegatedVerifierResponse> {
    const result = await initialClient.finish();
    this.assertActive(generation);
    return result;
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
    this.targetWorkerCount = 0;
    for (const waiter of this.waiters.splice(0)) waiter.reject(this.failure);
  }

  private createClient(): VerifierClient {
    return new VerifierClient(
      this.workerFactory,
      this.requestStallTimeoutMs,
      this.finishStallTimeoutMs,
      this.delegationAuthority,
      this.coordinatorId,
      String(this.nextClientId++),
      this.jobId
    );
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

function byteLength(value: string | ArrayBuffer): number {
  return typeof value === 'string' ? new TextEncoder().encode(value).byteLength : value.byteLength;
}

function uniqueDelegationUuid(): string {
  const randomId = globalThis.crypto?.randomUUID?.();
  if (randomId) return randomId;
  // Contract harnesses without Web Crypto get a canonical process-local UUID.
  const ordinal = nextFallbackDelegationId++;
  return `00000000-0000-4000-8000-${ordinal.toString(16).padStart(12, '0')}`;
}

let nextFallbackDelegationId = 1;

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

async function sealVerifierResponse(
  operation: 'initialize' | 'consume' | 'finish',
  delegation: ActiveDelegation,
  streamedPartials: readonly ArrayBuffer[],
  response: VerifierResponse
): Promise<DelegatedVerifierResponse> {
  const partials = [...streamedPartials];
  if (response.type === 'consumed' && response.partial && response.partial.byteLength > 0) {
    partials.push(response.partial);
  }
  if (response.type === 'finished' && response.partial.byteLength > 0) {
    partials.push(response.partial);
  }
  const partialDescriptors = await Promise.all(
    partials.map(async (partial) => ({
      byte_length: partial.byteLength,
      sha256: await sha256Hex(partial)
    }))
  );
  const resultSha256 = await sha256Hex(
    JSON.stringify({
      schema: 'clearra.verifier-merger-ready-result.v1',
      operation,
      partials: partialDescriptors
    })
  );
  const workerReplySha256 = await sha256Hex(
    JSON.stringify({
      schema: 'clearra.verifier-worker-reply.v1',
      operation,
      reply: canonicalVerifierReply(response),
      partials: partialDescriptors
    })
  );
  await delegation.authority.resultSealed(
    delegation.token,
    resultSha256,
    workerReplySha256
  );
  return Object.freeze({
    response,
    delegation,
    partials: Object.freeze(partials),
    resultSha256,
    workerReplySha256
  });
}

function canonicalVerifierReply(response: VerifierResponse): unknown {
  if (response.type === 'ready') return { type: 'ready' };
  if (response.type === 'consumed') {
    return {
      type: 'consumed',
      request_id: response.requestId,
      candidate_count: response.candidateCount,
      candidate_count_available: response.candidateCountAvailable,
      candidate_count_exact: response.candidateCountExact,
      progress: response.progress
    };
  }
  if (response.type === 'finished') {
    return { type: 'finished', request_id: response.requestId };
  }
  throw new Error(`response ${response.type} cannot be sealed as a final verifier result`);
}

async function applySealedVerifierResult(
  sealed: DelegatedVerifierResponse,
  consumePartial: (partial: ArrayBuffer) => void
): Promise<void> {
  const { authority, token } = sealed.delegation;
  const decision = authority.resultApplicationDecision(token, sealed.resultSha256);
  if (decision === 'already-applied') return;
  try {
    for (const partial of sealed.partials) commitPartial(consumePartial, partial);
    await authority.resultApplied(token);
    await authority.completed(token);
  } catch (error) {
    try {
      await authority.failedClosed(token, `immutable result application failed: ${asError(error).message}`);
    } catch {
      // The original merger or journal failure remains authoritative.
    }
    throw error;
  }
}

import type {
  ClearraSolutionPageWorkerEvent,
  ClearraWasmWorkerEvent
} from './wasmCommandClient';
import {
  isSolutionPageWorkerEvent,
  postLoadSolutionPage,
  postPrewarmRuntime
} from './wasmCommandClient';
import {
  DEFAULT_RUNTIME_WARMUP_POLICY,
  normalizeRuntimeWarmupPolicy,
  resolveWorkerAuthority,
  sharedBrowserHostCapabilitySnapshot,
  type HostCapabilitySnapshot,
  type RuntimeWarmupPolicy,
  type WorkerAuthorityReport
} from './hostCapabilitySnapshot';
import {
  ensureWasmWorkerOwnerId,
  terminateOwnedWasmWorker,
  type ClearraWasmForcedTerminationReason
} from './wasmWorkerLifecycle';
import {
  applyTablebaseWarmupEvent,
  applyWasmWorkerEvent,
  cancelWasmCommand,
  runWasmCommand,
  type TablebaseWarmupWorkerEvent
} from './wasmWorkerStore';

const COOPERATIVE_CANCEL_GRACE_MS = 100;

type RuntimePrewarmWorkerEvent = {
  type: 'runtime_prewarm';
  phase: 'started' | 'finished';
  workerCount: number;
};

export class WasmTerminalWorkerController {
  private worker: Worker | null = null;
  private cancellingWorker: Worker | null = null;
  private prewarmingWorker: Worker | null = null;
  private prewarmWorkerCount = 1;
  private hostCapabilitySnapshot: HostCapabilitySnapshot;
  private workerAuthority: WorkerAuthorityReport;
  private warmupPolicy: RuntimeWarmupPolicy = DEFAULT_RUNTIME_WARMUP_POLICY;
  private tablebaseRequested = false;
  private runInFlight = false;
  private prewarmDeferred = false;
  private cancelFallback: ReturnType<typeof setTimeout> | null = null;
  private cancellingJobId: number | null = null;
  private nextSolutionPageRequestId = 1;
  private solutionPageRequests = new Map<
    number,
    {
      resolve: (value: { keys: string[]; total: number }) => void;
      reject: (reason: Error) => void;
      offset: number;
      limit: number;
    }
  >();

  constructor(
    private workerFactory: (() => Worker) | null,
    hostCapabilitySnapshot: HostCapabilitySnapshot = sharedBrowserHostCapabilitySnapshot()
  ) {
    this.hostCapabilitySnapshot = hostCapabilitySnapshot;
    this.workerAuthority = resolveWorkerAuthority(hostCapabilitySnapshot, 1);
  }

  setWorkerFactory(workerFactory: (() => Worker) | null) {
    if (this.workerFactory === workerFactory) return;
    this.dispose();
    this.workerFactory = workerFactory;
  }

  setHostCapabilitySnapshot(hostCapabilitySnapshot: HostCapabilitySnapshot) {
    if (this.hostCapabilitySnapshot.snapshotId === hostCapabilitySnapshot.snapshotId) return;
    this.dispose();
    this.hostCapabilitySnapshot = hostCapabilitySnapshot;
    this.workerAuthority = resolveWorkerAuthority(
      hostCapabilitySnapshot,
      this.workerAuthority.workersRequested
    );
    this.prewarmWorkerCount = this.workerAuthority.workersEffective;
  }

  currentWorkerAuthority(): WorkerAuthorityReport {
    return this.workerAuthority;
  }

  run(): boolean {
    if (
      this.runInFlight ||
      this.cancellingWorker !== null ||
      this.cancelFallback !== null
    ) {
      return false;
    }
    let worker: Worker | null;
    try {
      worker = this.ensureWorker();
    } catch (error) {
      const ownedWorker = this.worker;
      if (ownedWorker) {
        this.failClosedWorker(
          ownedWorker,
          'E_WASM_WORKER_CREATE_FAILED',
          errorMessage(error)
        );
      } else {
        this.emitFailure('E_WASM_WORKER_CREATE_FAILED', errorMessage(error));
      }
      return false;
    }
    if (!worker) {
      this.emitFailure(
        'E_WASM_WORKER_UNAVAILABLE',
        'A browser worker factory is required to start the WASM runtime.'
      );
      return false;
    }
    this.rejectSolutionPages(new Error('a new search replaced the previous solution pages'));
    if (this.worker && this.prewarmingWorker === this.worker) {
      // Foreground execution interrupts incomplete optional warmup inside the
      // worker; the compiled module itself remains reusable.
      this.prewarmingWorker = null;
    }
    try {
      this.runInFlight = true;
      runWasmCommand(
        worker,
        this.prewarmWorkerCount,
        this.tablebaseRequested,
        ensureWasmWorkerOwnerId(worker),
        this.runtimeAuthority()
      );
      return true;
    } catch (error) {
      this.runInFlight = false;
      this.failClosedWorker(worker, 'E_WASM_WORKER_MESSAGE_FAILED', errorMessage(error));
      return false;
    }
  }

  prewarm(
    workerCount: number,
    tablebaseRequested = false,
    warmupPolicy: RuntimeWarmupPolicy = DEFAULT_RUNTIME_WARMUP_POLICY,
    authority?: WorkerAuthorityReport
  ) {
    this.workerAuthority = authorityForPrewarm(
      this.hostCapabilitySnapshot,
      workerCount,
      authority
    );
    this.prewarmWorkerCount = this.workerAuthority.workersEffective;
    this.warmupPolicy = normalizeRuntimeWarmupPolicy(warmupPolicy);
    const tablebaseChanged = this.tablebaseRequested !== tablebaseRequested;
    if (tablebaseChanged) {
      applyTablebaseWarmupEvent({
        type: 'tablebase_warmup',
        phase: tablebaseRequested ? 'loading' : 'disabled',
        artifactSha256: '',
        byteLength: 0
      });
    }
    this.tablebaseRequested = tablebaseRequested;
    if (this.runInFlight) {
      this.prewarmDeferred = true;
      return;
    }
    this.prewarmDeferred = false;
    if (tablebaseChanged && this.worker && this.prewarmingWorker === this.worker) {
      this.disposeOwnedWorker(this.worker);
    }
    const worker = this.ensureWorker();
    if (worker) this.prewarmWorker(worker, this.prewarmWorkerCount);
  }

  cancel() {
    const worker = this.worker;
    if (!worker || this.cancelFallback !== null) return;
    let jobId: number | null | undefined;
    try {
      jobId = cancelWasmCommand(worker);
    } catch (error) {
      this.failClosedWorker(worker, 'E_WASM_WORKER_CANCEL_FAILED', errorMessage(error));
      return;
    }
    if (jobId === undefined) return;
    this.cancellingWorker = worker;
    this.cancellingJobId = jobId ?? null;
    this.cancelFallback = setTimeout(() => {
      this.terminateCancelledWorker(worker, this.cancellingJobId ?? jobId ?? 0);
    }, COOPERATIVE_CANCEL_GRACE_MS);
  }

  loadSolutionPage(
    offset: number,
    limit: number,
    signal?: AbortSignal
  ): Promise<{ keys: string[]; total: number }> {
    const worker = this.worker;
    if (!worker || this.runInFlight || this.cancellingWorker !== null) {
      return Promise.reject(new Error('solution page runtime is not available'));
    }
    if (signal?.aborted) return Promise.reject(solutionPageAbortError(signal));
    const requestId = this.nextSolutionPageRequestId++;
    return new Promise((resolve, reject) => {
      const cleanup = () => signal?.removeEventListener('abort', onAbort);
      const settleResolve = (value: { keys: string[]; total: number }) => {
        cleanup();
        resolve(value);
      };
      const settleReject = (reason: Error) => {
        cleanup();
        reject(reason);
      };
      const onAbort = () => {
        if (!this.solutionPageRequests.delete(requestId)) return;
        settleReject(solutionPageAbortError(signal));
      };
      this.solutionPageRequests.set(requestId, {
        resolve: settleResolve,
        reject: settleReject,
        offset,
        limit
      });
      signal?.addEventListener('abort', onAbort, { once: true });
      try {
        postLoadSolutionPage(worker, requestId, offset, limit);
      } catch (error) {
        this.solutionPageRequests.delete(requestId);
        settleReject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  takeIdleWorker(): Worker | null {
    if (
      !this.worker ||
      this.cancellingWorker !== null ||
      this.prewarmingWorker !== null ||
      this.runInFlight ||
      this.cancelFallback !== null ||
      this.solutionPageRequests.size > 0
    ) {
      return null;
    }
    const worker = this.worker;
    this.worker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    return worker;
  }

  dispose() {
    this.runInFlight = false;
    this.prewarmDeferred = false;
    const worker = this.worker;
    if (!worker) {
      this.clearCancelFallback();
      return;
    }
    let jobId: number | null | undefined;
    try {
      jobId = cancelWasmCommand(worker);
    } catch {
      jobId = 0;
    }
    this.disposeOwnedWorker(worker);
    if (jobId !== undefined) {
      this.emitForcedTermination(
        jobId ?? 0,
        'owner-disposed',
        'E_WASM_OWNER_DISPOSED',
        'The WASM runtime owner was disposed while a search was active; the worker tree was force-terminated.'
      );
    }
  }

  private ensureWorker() {
    if (!this.worker && this.workerFactory) {
      const worker = this.workerFactory();
      this.worker = worker;
      worker.onmessage = (
        message: MessageEvent<
          | ClearraWasmWorkerEvent
          | RuntimePrewarmWorkerEvent
          | TablebaseWarmupWorkerEvent
          | ClearraSolutionPageWorkerEvent
        >
      ) => {
        if (this.worker !== worker) return;
        if (isSolutionPageWorkerEvent(message.data)) {
          this.resolveSolutionPage(message.data);
          return;
        }
        if (isRuntimePrewarmWorkerEvent(message.data)) {
          this.prewarmingWorker = message.data.phase === 'started' ? worker : null;
          return;
        }
        if (isTablebaseWarmupWorkerEvent(message.data)) {
          applyTablebaseWarmupEvent(message.data);
          return;
        }
        if (this.cancellingWorker === worker) {
          if (message.data.event === 'started') {
            this.cancellingJobId = message.data.job_id;
          }
          if (message.data.event === 'cancelled') {
            this.runInFlight = false;
            applyWasmWorkerEvent(message.data);
            this.releaseWorker(worker, 'owner-disposed');
            this.flushDeferredPrewarm();
            return;
          }
          if (isTerminalWorkerEvent(message.data)) {
            this.runInFlight = false;
            applyWasmWorkerEvent(message.data);
            if (
              message.data.event === 'failed' ||
              message.data.event === 'terminated' ||
              (message.data.event === 'final_response' &&
                message.data.response.status !== 'success')
            ) {
              this.releaseWorker(worker, 'worker-failure');
            } else {
              this.clearCancelFallback();
            }
            this.flushDeferredPrewarm();
            return;
          }
        }
        const terminal = isTerminalWorkerEvent(message.data);
        if (terminal) this.runInFlight = false;
        applyWasmWorkerEvent(message.data);
        if (
          message.data.event === 'failed' ||
          message.data.event === 'cancelled' ||
          message.data.event === 'terminated' ||
          (message.data.event === 'final_response' && message.data.response.status !== 'success')
        ) {
          this.releaseWorker(
            worker,
            message.data.event === 'failed' || message.data.event === 'terminated'
              ? 'worker-failure'
              : 'owner-disposed'
          );
        }
        if (terminal) this.flushDeferredPrewarm();
      };
      worker.onerror = (event) => {
        event.preventDefault();
        const message = event.message || 'WASM worker crashed';
        const location = event.filename
          ? ` (${event.filename}:${event.lineno}:${event.colno})`
          : '';
        this.failClosedWorker(worker, 'E_WASM_WORKER_CRASH', `${message}${location}`);
      };
      worker.onmessageerror = () => {
        this.failClosedWorker(
          worker,
          'E_WASM_WORKER_MESSAGE_INVALID',
          'WASM worker returned an invalid message'
        );
      };
    }
    return this.worker;
  }

  private terminateCancelledWorker(worker: Worker, jobId: number | null) {
    if (this.worker !== worker || this.cancellingWorker !== worker) return;
    this.runInFlight = false;
    this.releaseWorker(worker, 'cancel-timeout');
    this.emitForcedTermination(
      jobId ?? 0,
      'cancel-timeout',
      'E_WASM_FORCED_TERMINATION',
      'The search did not acknowledge cooperative cancellation before the deadline; the worker tree was force-terminated.'
    );
    this.flushDeferredPrewarm();
  }

  private failClosedWorker(worker: Worker, code: string, message: string) {
    if (this.worker !== worker) return;
    this.runInFlight = false;
    this.releaseWorker(worker, 'worker-failure');
    this.emitFailure(code, message);
    this.flushDeferredPrewarm();
  }

  private emitFailure(code: string, message: string) {
    applyWasmWorkerEvent({
      schema_version: 1,
      runtime: 'clearra-wasm',
      event: 'failed',
      job_id: 0,
      diagnostics: {
        diagnostics: [{ code, severity: 'error', message }]
      }
    });
  }

  private emitForcedTermination(
    jobId: number,
    reason: ClearraWasmForcedTerminationReason,
    code: string,
    message: string
  ) {
    applyWasmWorkerEvent({
      schema_version: 1,
      runtime: 'clearra-wasm',
      event: 'terminated',
      job_id: jobId,
      reason,
      scope_released: true,
      diagnostics: {
        diagnostics: [{ code, severity: 'error', message }]
      }
    });
  }

  private prewarmWorker(worker: Worker, workerCount: number) {
    try {
      this.prewarmingWorker = worker;
      postPrewarmRuntime(
        worker,
        workerCount,
        this.tablebaseRequested,
        ensureWasmWorkerOwnerId(worker),
        this.runtimeAuthority()
      );
    } catch (error) {
      this.failClosedWorker(worker, 'E_WASM_WORKER_PREWARM_FAILED', errorMessage(error));
    }
  }

  private flushDeferredPrewarm() {
    if (!this.prewarmDeferred || this.runInFlight) return;
    this.prewarmDeferred = false;
    const worker = this.ensureWorker();
    if (worker) this.prewarmWorker(worker, this.prewarmWorkerCount);
  }

  private releaseWorker(worker: Worker, reason: ClearraWasmForcedTerminationReason) {
    this.rejectSolutionPages(new Error('solution page runtime was released'));
    if (this.worker !== worker) return;
    this.clearCancelFallback();
    if (this.prewarmingWorker === worker) this.prewarmingWorker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    terminateOwnedWasmWorker(worker, reason);
    this.worker = null;
  }

  private disposeOwnedWorker(worker: Worker) {
    this.rejectSolutionPages(new Error('solution page runtime was disposed'));
    if (this.worker !== worker) return;
    this.clearCancelFallback();
    if (this.prewarmingWorker === worker) this.prewarmingWorker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    this.worker = null;
    try {
      worker.postMessage({ type: 'dispose_runtime' });
    } catch {}
    terminateOwnedWasmWorker(worker, 'owner-disposed');
  }

  private runtimeAuthority() {
    return {
      hostCapabilitySnapshot: this.hostCapabilitySnapshot,
      workerAuthority: this.workerAuthority,
      warmupPolicy: this.warmupPolicy
    };
  }

  private resolveSolutionPage(event: ClearraSolutionPageWorkerEvent) {
    const pending = this.solutionPageRequests.get(event.request_id);
    if (!pending) return;
    this.solutionPageRequests.delete(event.request_id);
    if (event.type === 'solution_page_failed') {
      pending.reject(new Error(event.message));
    } else if (
      event.offset !== pending.offset ||
      !Array.isArray(event.keys) ||
      event.keys.length > pending.limit
    ) {
      pending.reject(new Error('solution page response does not match its request'));
    } else {
      pending.resolve({ keys: event.keys, total: event.total });
    }
  }

  private rejectSolutionPages(error: Error) {
    for (const pending of this.solutionPageRequests.values()) pending.reject(error);
    this.solutionPageRequests.clear();
  }

  private clearCancelFallback() {
    if (this.cancelFallback !== null) clearTimeout(this.cancelFallback);
    this.cancelFallback = null;
    this.cancellingWorker = null;
    this.cancellingJobId = null;
  }
}

function solutionPageAbortError(signal: AbortSignal | undefined): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error('Solution page load was aborted.');
  error.name = 'AbortError';
  return error;
}

function authorityForPrewarm(
  snapshot: HostCapabilitySnapshot,
  workerCount: number,
  authority: WorkerAuthorityReport | undefined
): WorkerAuthorityReport {
  if (
    authority?.snapshotId === snapshot.snapshotId &&
    authority.workersEffective === workerCount &&
    authority.reportedLogicalProcessors === snapshot.reportedLogicalProcessors
  ) {
    return authority;
  }
  return resolveWorkerAuthority(snapshot, workerCount);
}

function isTerminalWorkerEvent(event: ClearraWasmWorkerEvent): boolean {
  return (
    event.event === 'failed' ||
    event.event === 'cancelled' ||
    event.event === 'terminated' ||
    event.event === 'final_response'
  );
}

function isRuntimePrewarmWorkerEvent(
  event: ClearraWasmWorkerEvent | RuntimePrewarmWorkerEvent | TablebaseWarmupWorkerEvent
): event is RuntimePrewarmWorkerEvent {
  return 'type' in event && event.type === 'runtime_prewarm';
}

function isTablebaseWarmupWorkerEvent(
  event: ClearraWasmWorkerEvent | RuntimePrewarmWorkerEvent | TablebaseWarmupWorkerEvent
): event is TablebaseWarmupWorkerEvent {
  return 'type' in event && event.type === 'tablebase_warmup';
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

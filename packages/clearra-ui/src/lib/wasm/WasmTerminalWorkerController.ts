import type {
  ClearraProductPageWorkerEvent,
  ClearraProductPageWorkerPayload,
  ClearraSolutionPageWorkerEvent,
  ClearraWasmWorkerEvent
} from './wasmCommandClient';
import {
  isProductPageWorkerEvent,
  isSolutionPageWorkerEvent,
  postLoadNextProductPage,
  postLoadProductMemberPage,
  postLoadSolutionPage,
  postPrewarmRuntime,
  postReleaseProductPages
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
  currentWasmArtifactGeneration,
  isCurrentWasmArtifactGeneration
} from './wasmArtifactGeneration';
import {
  applyTablebaseWarmupEvent,
  applyWasmWorkerEvent,
  cancelWasmCommand,
  runWasmCommand,
  type TablebaseWarmupWorkerEvent
} from './wasmWorkerStore';

const COOPERATIVE_CANCEL_GRACE_MS = 100;
// Local searches and lazy portfolio pages can legitimately take minutes.
// Deadlines are opt-in for bounded fixtures; cancellation remains available.
const PREPARATION_PROGRESS_STALL_TIMEOUT_MS = 0;
const SEARCH_PROGRESS_STALL_TIMEOUT_MS = 0;
const PRODUCT_PAGE_STALL_TIMEOUT_MS = 0;

export type WasmTerminalWorkerControllerOptions = {
  preparationProgressStallTimeoutMs?: number;
  searchProgressStallTimeoutMs?: number;
  productPageStallTimeoutMs?: number;
};

type RuntimePrewarmWorkerEvent = {
  type: 'runtime_prewarm';
  phase: 'started' | 'finished';
  workerCount: number;
};

export class WasmTerminalWorkerController {
  private worker: Worker | null = null;
  private workerArtifactGeneration: string | null = null;
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
  private progressWatchdog: ReturnType<typeof setTimeout> | null = null;
  private progressWorker: Worker | null = null;
  private progressJobId: number | null = null;
  private progressFingerprint: string | null = null;
  private progressPhaseKind: 'preparation' | 'search' | null = null;
  private runningJobId: number | null = null;
  private readonly preparationProgressStallTimeoutMs: number;
  private readonly searchProgressStallTimeoutMs: number;
  private readonly productPageStallTimeoutMs: number;
  private productPageGeneration = 0;
  private nextSolutionPageRequestId = 1;
  private nextProductPageRequestId = 1;
  private solutionPageRequests = new Map<
    number,
    {
      resolve: (value: { keys: string[]; total: number }) => void;
      reject: (reason: Error) => void;
      offset: number;
      limit: number;
    }
  >();
  private productPageRequests = new Map<
    number,
    {
      resolve: (value: ClearraProductPageWorkerPayload) => void;
      reject: (reason: Error) => void;
      generation: number;
    }
  >();

  constructor(
    private workerFactory: (() => Worker) | null,
    hostCapabilitySnapshot: HostCapabilitySnapshot = sharedBrowserHostCapabilitySnapshot(),
    options: WasmTerminalWorkerControllerOptions = {}
  ) {
    this.hostCapabilitySnapshot = hostCapabilitySnapshot;
    this.workerAuthority = resolveWorkerAuthority(hostCapabilitySnapshot, 1);
    this.preparationProgressStallTimeoutMs = positiveTimeout(
      options.preparationProgressStallTimeoutMs,
      PREPARATION_PROGRESS_STALL_TIMEOUT_MS
    );
    this.searchProgressStallTimeoutMs = positiveTimeout(
      options.searchProgressStallTimeoutMs,
      SEARCH_PROGRESS_STALL_TIMEOUT_MS
    );
    this.productPageStallTimeoutMs = positiveTimeout(
      options.productPageStallTimeoutMs,
      PRODUCT_PAGE_STALL_TIMEOUT_MS
    );
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
    this.replaceStaleWorkerForNewRun();
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
    this.productPageGeneration += 1;
    this.rejectProductPages(new Error('a new search replaced the previous product pages'));
    this.clearProgressWatchdog();
    this.runningJobId = null;
    try {
      postReleaseProductPages(worker);
    } catch {}
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
    if (!this.runInFlight && this.productPageRequests.size > 0) {
      this.rejectProductPages(new Error('product page runtime was cancelled'));
      this.releaseWorker(worker, 'owner-disposed');
      return;
    }
    this.rejectProductPages(new Error('product page runtime was cancelled'));
    this.clearProgressWatchdog();
    try {
      postReleaseProductPages(worker);
    } catch {}
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

  loadNextProductPage(
    signal?: AbortSignal,
    maximumWorkSteps = 10_000
  ): Promise<ClearraProductPageWorkerPayload> {
    return this.requestProductPage(
      (worker, requestId) =>
        postLoadNextProductPage(worker, requestId, maximumWorkSteps),
      signal
    );
  }

  loadProductMemberPage(
    alternativeIndex: string,
    memberPageNumber: string,
    signal?: AbortSignal,
    maximumWorkSteps = 10_000
  ): Promise<ClearraProductPageWorkerPayload> {
    return this.requestProductPage(
      (worker, requestId) =>
        postLoadProductMemberPage(
          worker,
          requestId,
          alternativeIndex,
          memberPageNumber,
          maximumWorkSteps
        ),
      signal
    );
  }

  releaseProductPages() {
    const worker = this.worker;
    this.productPageGeneration += 1;
    if (worker && this.productPageRequests.size > 0) {
      this.releaseWorker(worker, 'owner-disposed');
      return;
    }
    this.rejectProductPages(new Error('product page runtime was released'));
    if (!worker) return;
    postReleaseProductPages(worker);
  }

  private requestProductPage(
    post: (worker: Worker, requestId: number) => void,
    signal?: AbortSignal
  ): Promise<ClearraProductPageWorkerPayload> {
    const worker = this.worker;
    if (
      !worker ||
      this.runInFlight ||
      this.cancellingWorker !== null
    ) {
      return Promise.reject(new Error('product page runtime is not available'));
    }
    if (signal?.aborted) return Promise.reject(productPageAbortError(signal));
    const requestId = this.nextProductPageRequestId++;
    const generation = this.productPageGeneration;
    return new Promise((resolve, reject) => {
      let stallTimer: ReturnType<typeof setTimeout> | null = null;
      const cleanup = () => {
        signal?.removeEventListener('abort', onAbort);
        if (stallTimer !== null) clearTimeout(stallTimer);
        stallTimer = null;
      };
      const onAbort = () => {
        if (!this.productPageRequests.delete(requestId)) return;
        cleanup();
        reject(productPageAbortError(signal));
        if (this.worker === worker) this.releaseWorker(worker, 'owner-disposed');
      };
      this.productPageRequests.set(requestId, {
        resolve: (value) => {
          cleanup();
          resolve(value);
        },
        reject: (reason) => {
          cleanup();
          reject(reason);
        },
        generation
      });
      signal?.addEventListener('abort', onAbort, { once: true });
      if (this.productPageStallTimeoutMs > 0) {
        stallTimer = setTimeout(() => {
        const pending = this.productPageRequests.get(requestId);
        if (
          !pending ||
          pending.generation !== generation ||
          this.productPageGeneration !== generation ||
          this.worker !== worker
        ) {
          return;
        }
        this.productPageRequests.delete(requestId);
        pending.reject(
          new Error(
            `Product page work did not return within ${this.productPageStallTimeoutMs} ms.`
          )
        );
        this.releaseWorker(worker, 'worker-failure');
      }, this.productPageStallTimeoutMs);
        const nodeTimer = stallTimer as unknown as { unref?: () => void };
        nodeTimer.unref?.();
      }
      try {
        post(worker, requestId);
      } catch (error) {
        this.productPageRequests.delete(requestId);
        cleanup();
        reject(error instanceof Error ? error : new Error(String(error)));
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
      this.solutionPageRequests.size > 0 ||
      this.productPageRequests.size > 0
    ) {
      return null;
    }
    if (!isCurrentWasmArtifactGeneration(this.workerArtifactGeneration)) {
      this.disposeOwnedWorker(this.worker);
      return null;
    }
    const worker = this.worker;
    this.worker = null;
    this.workerArtifactGeneration = null;
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
      this.workerArtifactGeneration = currentWasmArtifactGeneration();
      worker.onmessage = (
        message: MessageEvent<
          | ClearraWasmWorkerEvent
          | RuntimePrewarmWorkerEvent
          | TablebaseWarmupWorkerEvent
          | ClearraSolutionPageWorkerEvent
          | ClearraProductPageWorkerEvent
        >
      ) => {
        if (this.worker !== worker) return;
        if (isSolutionPageWorkerEvent(message.data)) {
          this.resolveSolutionPage(message.data);
          return;
        }
        if (isProductPageWorkerEvent(message.data)) {
          this.resolveProductPage(message.data);
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
        this.observeBoundedProgress(worker, message.data);
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

  /**
   * Arms when the foreground runtime starts, then follows its preparation and
   * concrete serial/distributed execution progress. A preparation deadline is
   * independent from the long-running search deadline, so module/catalog or
   * worker initialization cannot leave the GUI loading forever. Only changed
   * bounded-work evidence renews a deadline; periodic identical heartbeats
   * cannot hide a synchronous WASM call that no longer returns to the host.
   */
  private observeBoundedProgress(worker: Worker, event: ClearraWasmWorkerEvent) {
    if (isTerminalWorkerEvent(event)) {
      this.clearProgressWatchdog();
      this.runningJobId = null;
      return;
    }
    if (!this.runInFlight || this.cancellingWorker === worker) return;
    if (event.event === 'started') this.runningJobId = event.job_id;
    if (event.event !== 'started' && event.event !== 'progress') return;
    if (this.runningJobId !== event.job_id) return;
    const telemetry = event.event === 'progress' ? event.progress.telemetry : undefined;
    const phaseKind =
      event.event === 'started' ||
      telemetry?.phase === 'preparing' ||
      telemetry?.phase === 'initializing'
        ? 'preparation'
        : 'search';
    const explicitlyBounded =
      event.event === 'started' ||
      phaseKind === 'preparation' ||
      telemetry?.execution_mode === 'serial' ||
      telemetry?.execution_mode === 'distributed';
    const ownsActiveRun =
      this.progressWorker === worker && this.progressJobId === event.job_id;
    if (!explicitlyBounded && !ownsActiveRun) return;

    const fingerprint = event.event === 'started'
      ? 'runtime-started'
      : boundedProgressFingerprint(event);
    if (
      ownsActiveRun &&
      phaseKind === this.progressPhaseKind &&
      fingerprint === this.progressFingerprint
    ) {
      return;
    }
    this.clearProgressWatchdog();
    this.progressWorker = worker;
    this.progressJobId = event.job_id;
    this.progressFingerprint = fingerprint;
    this.progressPhaseKind = phaseKind;
    const timeoutMs = phaseKind === 'preparation'
      ? this.preparationProgressStallTimeoutMs
      : this.searchProgressStallTimeoutMs;
    if (timeoutMs === 0) return;
    this.progressWatchdog = setTimeout(() => {
      if (
        this.worker !== worker ||
        this.progressWorker !== worker ||
        this.progressJobId !== event.job_id ||
        this.progressPhaseKind !== phaseKind ||
        !this.runInFlight
      ) {
        return;
      }
      const jobId = event.job_id;
      this.runInFlight = false;
      this.releaseWorker(worker, 'worker-failure');
      this.emitForcedTermination(
        jobId,
        'worker-failure',
        phaseKind === 'preparation'
          ? 'E_WASM_PREPARATION_PROGRESS_STALLED'
          : 'E_WASM_SEARCH_PROGRESS_STALLED',
        phaseKind === 'preparation'
          ? `WASM preparation did not complete within ${timeoutMs} ms; the worker tree was force-terminated.`
          : `The WASM search made no bounded progress for ${timeoutMs} ms; the worker tree was force-terminated.`
      );
      this.flushDeferredPrewarm();
    }, timeoutMs);
    const nodeTimer = this.progressWatchdog as unknown as { unref?: () => void };
    nodeTimer.unref?.();
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
    this.productPageGeneration += 1;
    this.rejectProductPages(new Error('product page runtime was released'));
    if (this.worker !== worker) return;
    this.clearCancelFallback();
    this.clearProgressWatchdog();
    if (this.prewarmingWorker === worker) this.prewarmingWorker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    terminateOwnedWasmWorker(worker, reason);
    this.worker = null;
    this.workerArtifactGeneration = null;
  }

  private disposeOwnedWorker(worker: Worker) {
    this.rejectSolutionPages(new Error('solution page runtime was disposed'));
    this.productPageGeneration += 1;
    this.rejectProductPages(new Error('product page runtime was disposed'));
    if (this.worker !== worker) return;
    this.clearCancelFallback();
    this.clearProgressWatchdog();
    if (this.prewarmingWorker === worker) this.prewarmingWorker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    this.worker = null;
    this.workerArtifactGeneration = null;
    try {
      worker.postMessage({ type: 'dispose_runtime' });
    } catch {}
    terminateOwnedWasmWorker(worker, 'owner-disposed');
  }

  private replaceStaleWorkerForNewRun() {
    const worker = this.worker;
    if (
      !worker ||
      isCurrentWasmArtifactGeneration(this.workerArtifactGeneration)
    ) {
      return;
    }
    // A new run already invalidates retained solution/product pages. Rotate at
    // this boundary instead of on the update event so a result that is being
    // inspected or copied is never destroyed underneath the user.
    this.disposeOwnedWorker(worker);
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

  private resolveProductPage(event: ClearraProductPageWorkerEvent) {
    const pending = this.productPageRequests.get(event.request_id);
    if (!pending) return;
    this.productPageRequests.delete(event.request_id);
    if (pending.generation !== this.productPageGeneration) {
      pending.reject(new Error('stale product page generation was discarded'));
      return;
    }
    if (event.type === 'product_page_failed') {
      pending.reject(new Error(event.message));
    } else {
      pending.resolve(event.payload);
    }
  }

  private rejectProductPages(error: Error) {
    for (const pending of this.productPageRequests.values()) pending.reject(error);
    this.productPageRequests.clear();
  }

  private clearCancelFallback() {
    if (this.cancelFallback !== null) clearTimeout(this.cancelFallback);
    this.cancelFallback = null;
    this.cancellingWorker = null;
    this.cancellingJobId = null;
  }

  private clearProgressWatchdog() {
    if (this.progressWatchdog !== null) clearTimeout(this.progressWatchdog);
    this.progressWatchdog = null;
    this.progressWorker = null;
    this.progressJobId = null;
    this.progressFingerprint = null;
    this.progressPhaseKind = null;
    if (!this.runInFlight) this.runningJobId = null;
  }
}

function solutionPageAbortError(signal: AbortSignal | undefined): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error('Solution page load was aborted.');
  error.name = 'AbortError';
  return error;
}

function productPageAbortError(signal: AbortSignal | undefined): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error('Product page load was aborted.');
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

function positiveTimeout(value: number | undefined, fallback: number): number {
  if (value === undefined || !Number.isFinite(value) || value < 0) return fallback;
  return Math.floor(value);
}

function boundedProgressFingerprint(
  event: Extract<ClearraWasmWorkerEvent, { event: 'progress' }>
): string {
  const telemetry = event.progress.telemetry;
  return JSON.stringify([
    event.progress.done,
    event.progress.total,
    event.progress.label,
    telemetry?.execution_mode ?? null,
    telemetry?.phase ?? null,
    telemetry?.producer_complete ?? null,
    telemetry?.geometry_nodes ?? null,
    telemetry?.candidates_emitted ?? null,
    telemetry?.geometry_family_count ?? null,
    telemetry?.candidates_verified ?? null,
    telemetry?.producer_build_nodes ?? null,
    telemetry?.producer_coverage_checks ?? null,
    telemetry?.build_nodes ?? null,
    telemetry?.coverage_checks ?? null,
    telemetry?.ready_workers ?? null,
    telemetry?.active_workers ?? null,
    telemetry?.worker_count ?? null,
    telemetry?.pass_index ?? null,
    telemetry?.pass_count ?? null,
    telemetry?.layer_index ?? null,
    telemetry?.layer_count ?? null,
    telemetry?.layer_done ?? null,
    telemetry?.layer_total ?? null
  ]);
}

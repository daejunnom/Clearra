import {
  createHostCapabilitySnapshot,
  isHostCapabilitySnapshot,
  normalizeRuntimeWarmupPolicy,
  resolveWorkerAuthority,
  wasmProductRetentionByteCap,
  type HostCapabilitySnapshot,
  type RuntimeWarmupPolicy,
  type WorkerAuthorityReport
} from '@clearra/ui/wasm-host';
import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';
import { isLocalSearchProfileMode } from '../lib/localSearchProfile';
import { withHostExecutionTiming } from './HostExecutionProfile';

import { ClearraProductJobRunner } from './ClearraProductJobRunner';
import {
  disposeDistributedWorkers,
  prewarmDistributedWorkers
} from './DistributedWasmJobRunner';
import { SharedExecutionAvailabilityError } from './SharedExecutionResourceAuthority';
import {
  ClearraWasmRuntimeError,
  loadClearraWasmModule,
  type ClearraWasmFailureDiagnostics,
  type ClearraWasmHostCapabilities,
  type ClearraWasmModule
} from './clearraWasmRuntime';
import {
  pc4TablebaseArtifactSha256,
  prewarmPc4TablebaseAssets,
  releasePc4TablebaseAssets
} from './pc4TablebaseAssets';

const MAX_EAGER_PREWARM_TOTAL_WORKERS = 9;
const RUNTIME_PREWARM_TIMEOUT_MS = 15_000;
const TABLEBASE_WARMUP_TIMEOUT_MS = 30_000;

type ClearraWorkerMessage =
  | {
      type: 'prewarm_runtime';
      workerCount: number;
      tablebaseRequested?: boolean;
      lifecycleOwnerId?: string;
      hostCapabilitySnapshot?: HostCapabilitySnapshot;
      workerAuthority?: WorkerAuthorityReport;
      warmupPolicy?: RuntimeWarmupPolicy;
    }
  | {
      type: 'run_command_text';
      commandText?: string;
      prewarmWorkerCount?: number;
      tablebaseRequested?: boolean;
      lifecycleOwnerId?: string;
      hostCapabilitySnapshot?: HostCapabilitySnapshot;
      workerAuthority?: WorkerAuthorityReport;
      warmupPolicy?: RuntimeWarmupPolicy;
    }
  | {
      type: 'load_solution_page';
      requestId: number;
      offset: number;
      limit: number;
    }
  | {
      type: 'load_product_page';
      requestId: number;
      action: 'next' | 'get';
      maximumWorkSteps?: number;
      alternativeIndex?: string;
      memberPageNumber?: string;
    }
  | { type: 'release_product_pages' }
  | { type: 'cancel_job'; jobId?: number }
  | { type: 'dispose_runtime' };

type ActiveJob = {
  id: number;
  runner: ClearraProductJobRunner | null;
  cancelled: boolean;
  terminalPosted: boolean;
};

let nextJobId = 1;
let active: ActiveJob | null = null;
let runtimePrewarm: Promise<void> | null = null;
let runtimePrewarmGeneration = 0;
let requestedPrewarmWorkerCount = 1;
let completedPrewarmWorkerCount = 0;
let loadedWasm: ClearraWasmModule | null = null;
let gpuWarmup: Promise<void> | null = null;
let gpuWarmupGeneration = 0;
let gpuWarmupCompleted = false;
let tablebaseRequested = false;
let deferredTablebaseRequested = false;
let tablebaseWarmup: Promise<void> | null = null;
let tablebaseWarmupGeneration = 0;
let tablebaseWarmupAttempted = false;
let failClosed = false;
let lifecycleOwnerId = '';
let hostCapabilitySnapshot = createHostCapabilitySnapshot({
  snapshotId: 'root-worker-conservative-fallback',
  source: 'conservative-fallback',
  reportedLogicalProcessors: 1,
  webGpuAvailable: false,
  crossOriginIsolated: false
});
let workerAuthority = resolveWorkerAuthority(hostCapabilitySnapshot, 1);
let warmupPolicy = normalizeRuntimeWarmupPolicy();

self.onmessage = (message: MessageEvent<ClearraWorkerMessage>) => {
  if (message.data.type === 'load_solution_page') {
    loadSolutionPage(message.data.requestId, message.data.offset, message.data.limit);
    return;
  }
  if (message.data.type === 'load_product_page') {
    loadProductPage(message.data);
    return;
  }
  if (message.data.type === 'release_product_pages') {
    releaseProductPages();
    return;
  }
  if (message.data.type === 'dispose_runtime') {
    disposeRuntime();
    return;
  }
  if (message.data.type === 'prewarm_runtime') {
    updateLifecycleOwner(message.data.lifecycleOwnerId);
    updateRuntimeAuthority(message.data, message.data.workerCount);
    startRuntimePrewarm(
      workerAuthority.workersEffective,
      message.data.tablebaseRequested ?? false,
      warmupPolicy
    );
    return;
  }
  if (message.data.type === 'cancel_job') {
    cancelActiveJob(message.data.jobId);
    return;
  }
  updateLifecycleOwner(message.data.lifecycleOwnerId);
  updateRuntimeAuthority(
    message.data,
    message.data.prewarmWorkerCount ?? workerAuthority.workersRequested
  );
  void runCommandText(
    message.data.commandText ?? '',
    workerAuthority.workersEffective,
    message.data.tablebaseRequested ?? false,
    warmupPolicy
  );
};

function loadSolutionPage(requestId: number, offset: number, limit: number) {
  try {
    if (!loadedWasm) throw new Error('WASM runtime is not loaded');
    const keys = loadedWasm.tiling_solution_page(offset, limit);
    self.postMessage({
      type: 'solution_page',
      request_id: requestId,
      offset,
      total: loadedWasm.tiling_solution_count(),
      keys
    });
  } catch (error) {
    self.postMessage({
      type: 'solution_page_failed',
      request_id: requestId,
      message: error instanceof Error ? error.message : String(error)
    });
  }
}

function loadProductPage(
  request: Extract<ClearraWorkerMessage, { type: 'load_product_page' }>
) {
  try {
    if (!loadedWasm) throw new Error('WASM runtime is not loaded');
    if (!loadedWasm.product_page_available()) {
      throw new Error('product page handle is not available');
    }
    const payload =
      request.action === 'next'
        ? loadedWasm.product_page_next(request.maximumWorkSteps ?? 10_000)
        : loadedWasm.product_page_get(
            request.alternativeIndex ?? '',
            request.memberPageNumber ?? '',
            request.maximumWorkSteps ?? 10_000
          );
    self.postMessage({
      type: 'product_page',
      request_id: request.requestId,
      payload
    });
  } catch (error) {
    self.postMessage({
      type: 'product_page_failed',
      request_id: request.requestId,
      message: error instanceof Error ? error.message : String(error)
    });
  }
}

function releaseProductPages() {
  try {
    if (loadedWasm?.product_page_available()) loadedWasm.product_page_release();
  } catch {
    // A running job owns its source until cancellation/termination completes.
  }
}

self.addEventListener('error', (event) => {
  event.preventDefault();
  failCloseUnhandled(event.error ?? new Error(event.message || 'WASM worker crashed'));
});

self.addEventListener('unhandledrejection', (event) => {
  event.preventDefault();
  failCloseUnhandled(event.reason);
});

async function runCommandText(
  commandText: string,
  prewarmWorkerCount: number,
  requestedTablebase: boolean,
  requestedWarmupPolicy: RuntimeWarmupPolicy
) {
  const profileStarted = isLocalSearchProfileMode(import.meta.env.MODE) ? performance.now() : null;
  if (active) {
    postRuntimeFailure(active.id, 'E_WASM_JOB_ALREADY_RUNNING', 'a WASM job is already active');
    return;
  }
  requestedPrewarmWorkerCount = Math.max(1, Math.floor(prewarmWorkerCount));
  deferredTablebaseRequested = requestedTablebase;
  setTablebaseRequested(requestedTablebase);
  const jobId = nextJobId++;
  const job: ActiveJob = {
    id: jobId,
    runner: null,
    cancelled: false,
    terminalPosted: false
  };
  active = job;
  postStarted(job.id);
  let failureCode = 'E_WASM_MODULE_LOAD_FAILED';
  let wasm: ClearraWasmModule | null = null;
  try {
    // Entry warmup is opportunistic. A slow optional worker or GPU adapter
    // must never become a correctness barrier for a foreground command.
    interruptIncompleteRuntimePrewarm();
    wasm = loadedWasm ?? (await loadClearraWasmModule(
      undefined,
      wasmHostCapabilities(hostCapabilitySnapshot)
    ));
    wasm.configure_host(wasmHostCapabilities(hostCapabilitySnapshot));
    loadedWasm = wasm;
    releaseProductPages();
    await startTablebaseWarmupAfterWasm(wasm);
    if (job.cancelled) {
      releaseJobResources(job);
      emitCancelled(job);
      closeFailClosedWorker();
      return;
    }
    failureCode = 'E_WASM_EXECUTION_FAILED';
    job.runner = new ClearraProductJobRunner(
      wasm,
      jobId,
      lifecycleOwnerId,
      wasmHostCapabilities(hostCapabilitySnapshot)
    );
    const modulePrepareMs = profileStarted === null ? 0 : performance.now() - profileStarted;
    const terminal = await job.runner.run(commandText, (event) => {
      if (event.event === 'started') return;
      const emitted = withHostExecutionTiming(withJobId(event, job.id), profileStarted === null ? null : {
        module_prepare_ms: modulePrepareMs,
        worker_elapsed_to_terminal_ms: performance.now() - profileStarted
      });
      if (isTerminal(emitted)) job.terminalPosted = true;
      postWorkerEvent(emitted);
    }, { transportProfile: isLocalSearchProfileMode(import.meta.env.MODE) });
    if (requiresFailClosedRelease(terminal)) {
      releaseJobResources(job);
      closeFailClosedWorker();
    }
  } catch (error) {
    const diagnostics = wasm?.failure_diagnostics();
    releaseJobResources(job);
    if (job.cancelled) {
      emitCancelled(job);
    } else {
      job.terminalPosted = true;
      postRuntimeFailure(job.id, failureCode, error, diagnostics);
    }
    closeFailClosedWorker();
  } finally {
    if (active === job) active = null;
    if (!failClosed) {
      startRuntimePrewarm(
        requestedPrewarmWorkerCount,
        deferredTablebaseRequested,
        requestedWarmupPolicy
      );
    }
  }
}

function startRuntimePrewarm(
  workerCount: number,
  requestedTablebase = tablebaseRequested,
  requestedWarmupPolicy: RuntimeWarmupPolicy = warmupPolicy
) {
  const normalizedWarmupPolicy = normalizeRuntimeWarmupPolicy(requestedWarmupPolicy);
  const boundedWorkerCount = resolveWorkerAuthority(
    hostCapabilitySnapshot,
    workerCount
  ).workersEffective;
  const eagerWorkerCount = Math.min(
    boundedWorkerCount,
    MAX_EAGER_PREWARM_TOTAL_WORKERS
  );
  requestedPrewarmWorkerCount = boundedWorkerCount;
  deferredTablebaseRequested = requestedTablebase;
  if (active) return;
  setTablebaseRequested(requestedTablebase);
  if (
    !normalizedWarmupPolicy.cpuWarmup &&
    !normalizedWarmupPolicy.gpuWarmup &&
    !requestedTablebase
  ) {
    postRuntimePrewarmPhase('finished', 0);
    return;
  }
  if (runtimePrewarm || completedPrewarmWorkerCount >= eagerWorkerCount) {
    if (loadedWasm) {
      if (normalizedWarmupPolicy.gpuWarmup) void startGpuWarmupAfterWasm(loadedWasm);
      void startTablebaseWarmupAfterWasm(loadedWasm);
    }
    return;
  }
  const generation = ++runtimePrewarmGeneration;
  postRuntimePrewarmPhase('started', eagerWorkerCount);
  runtimePrewarm = loadClearraWasmModule(
    undefined,
    wasmHostCapabilities(hostCapabilitySnapshot)
  )
    .then(async (wasm) => {
      loadedWasm = wasm;
      if (generation !== runtimePrewarmGeneration) return;
      if (normalizedWarmupPolicy.gpuWarmup) void startGpuWarmupAfterWasm(wasm);
      void startTablebaseWarmupAfterWasm(wasm);
      if (normalizedWarmupPolicy.cpuWarmup) {
        await withTimeout(
          prewarmDistributedWorkers(
            eagerWorkerCount,
            wasm.compiled_module(),
            lifecycleOwnerId,
            wasmHostCapabilities(hostCapabilitySnapshot)
          ),
          RUNTIME_PREWARM_TIMEOUT_MS,
          'distributed runtime warmup'
        );
      }
      if (generation === runtimePrewarmGeneration) {
        completedPrewarmWorkerCount = eagerWorkerCount;
      }
    })
    .then(() => undefined)
    .catch((error) => {
      if (generation !== runtimePrewarmGeneration) return;
      disposeDistributedWorkers();
      console.warn('Clearra browser runtime warmup was incomplete', error);
    })
    .finally(() => {
      if (generation === runtimePrewarmGeneration) {
        runtimePrewarm = null;
        postRuntimePrewarmPhase('finished', eagerWorkerCount);
      }
    });
}

function startGpuWarmupAfterWasm(wasm: ClearraWasmModule): Promise<void> {
  if (gpuWarmupCompleted) return Promise.resolve();
  if (gpuWarmup) return gpuWarmup;
  const generation = ++gpuWarmupGeneration;
  gpuWarmup = wasm.prewarm_gpu(null)
    .then(() => {
      if (generation === gpuWarmupGeneration) gpuWarmupCompleted = true;
    })
    .catch((error) => {
      if (generation === gpuWarmupGeneration) {
        console.warn('Clearra GPU warmup was unavailable', error);
      }
    })
    .finally(() => {
      if (generation === gpuWarmupGeneration) gpuWarmup = null;
    });
  return gpuWarmup;
}

function interruptIncompleteRuntimePrewarm() {
  if (gpuWarmup) {
    gpuWarmupGeneration += 1;
    gpuWarmup = null;
    gpuWarmupCompleted = false;
    const wasm = loadedWasm;
    if (!wasm) {
      throw new Error('GPU warmup is active without an owned WASM runtime');
    }
    wasm.cancel_gpu_warmup();
  }
  if (!runtimePrewarm) return;
  runtimePrewarmGeneration += 1;
  runtimePrewarm = null;
  completedPrewarmWorkerCount = 0;
  // Keep already-ready clients and their in-flight prewarm promises. The
  // foreground pool initialization can reuse each client independently and
  // schedule its first batch without joining the slowest speculative worker.
}

function setTablebaseRequested(requested: boolean) {
  if (tablebaseRequested === requested) return;
  tablebaseRequested = requested;
  tablebaseWarmupGeneration += 1;
  tablebaseWarmup = null;
  tablebaseWarmupAttempted = false;
  if (requested) return;
  try {
    loadedWasm?.release_tablebase();
  } catch (error) {
    console.warn('Clearra tablebase release was incomplete', error);
  }
  releasePc4TablebaseAssets();
  postTablebaseWarmupPhase('disabled', 0);
}

function updateLifecycleOwner(ownerId: string | undefined) {
  if (ownerId) lifecycleOwnerId = ownerId;
}

function updateRuntimeAuthority(
  message: {
    hostCapabilitySnapshot?: HostCapabilitySnapshot;
    workerAuthority?: WorkerAuthorityReport;
    warmupPolicy?: RuntimeWarmupPolicy;
  },
  fallbackRequestedWorkers: number
) {
  if (isHostCapabilitySnapshot(message.hostCapabilitySnapshot)) {
    hostCapabilitySnapshot = createHostCapabilitySnapshot({
      snapshotId: message.hostCapabilitySnapshot.snapshotId,
      source: message.hostCapabilitySnapshot.source,
      reportedLogicalProcessors:
        message.hostCapabilitySnapshot.reportedLogicalProcessors,
      reportedDeviceMemoryGiB:
        message.hostCapabilitySnapshot.reportedDeviceMemoryGiB,
      webGpuAvailable: message.hostCapabilitySnapshot.webGpuAvailable,
      crossOriginIsolated: message.hostCapabilitySnapshot.crossOriginIsolated
    });
  }
  const requestedReason =
    message.workerAuthority?.reason === 'reserved-main-thread' ||
    message.workerAuthority?.reason === 'all-logical-processors'
      ? message.workerAuthority.reason
      : 'explicit-request';
  workerAuthority = resolveWorkerAuthority(
    hostCapabilitySnapshot,
    message.workerAuthority?.workersRequested ?? fallbackRequestedWorkers,
    requestedReason
  );
  warmupPolicy = normalizeRuntimeWarmupPolicy(
    message.warmupPolicy ?? warmupPolicy
  );
  loadedWasm?.configure_host(wasmHostCapabilities(hostCapabilitySnapshot));
}

function wasmHostCapabilities(
  snapshot: HostCapabilitySnapshot
): ClearraWasmHostCapabilities {
  return {
    logicalProcessorCount: snapshot.reportedLogicalProcessors,
    transferByteCap: snapshot.wasmTransferByteCap,
    productRetentionByteCap: wasmProductRetentionByteCap(snapshot),
    webGpuAvailable: snapshot.webGpuAvailable,
    crossOriginIsolated: snapshot.crossOriginIsolated
  };
}

function startTablebaseWarmupAfterWasm(wasm: ClearraWasmModule): Promise<void> {
  if (!tablebaseRequested) return Promise.resolve();
  if (tablebaseWarmup) return tablebaseWarmup;
  if (tablebaseWarmupAttempted) return Promise.resolve();
  tablebaseWarmupAttempted = true;
  const generation = ++tablebaseWarmupGeneration;
  postTablebaseWarmupPhase('loading', 0);
  tablebaseWarmup = withTimeout(
    prewarmPc4TablebaseAssets(),
    TABLEBASE_WARMUP_TIMEOUT_MS,
    'tablebase warmup',
    releasePc4TablebaseAssets
  )
    .then((bundle) => {
      if (generation !== tablebaseWarmupGeneration || !tablebaseRequested) return;
      postTablebaseWarmupPhase('loading', bundle.byteLength);
      const report = wasm.install_tablebase(bundle.artifact);
      if (report.artifact_bytes !== bundle.byteLength) {
        throw new Error('WASM tablebase install reported an unexpected artifact size');
      }
      postTablebaseWarmupPhase('ready', bundle.byteLength);
    })
    .catch((error) => {
      if (generation !== tablebaseWarmupGeneration || !tablebaseRequested) return;
      const message = error instanceof Error ? error.message : String(error);
      console.warn('Clearra tablebase data warmup was unavailable', error);
      postTablebaseWarmupPhase('unavailable', 0, message);
    })
    .finally(() => {
      if (generation === tablebaseWarmupGeneration) tablebaseWarmup = null;
    });
  return tablebaseWarmup;
}

function withTimeout<T>(
  operation: Promise<T>,
  timeoutMs: number,
  label: string,
  onTimeout?: () => void
): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  return Promise.race([
    operation,
    new Promise<never>((_, reject) => {
      timeout = setTimeout(() => {
        onTimeout?.();
        reject(new Error(`${label} timed out after ${timeoutMs} ms`));
      }, timeoutMs);
    })
  ]).finally(() => {
    if (timeout !== undefined) clearTimeout(timeout);
  });
}

function cancelActiveJob(jobId: number | undefined) {
  const job = active;
  if (!job || job.terminalPosted || (jobId !== undefined && jobId !== job.id)) return;
  job.cancelled = true;
  try {
    job.runner?.cancel();
    releaseProductPages();
  } catch (error) {
    releaseJobResources(job);
    emitCancelled(job);
    closeFailClosedWorker();
    console.error('Clearra cancellation cleanup failed', error);
  }
}

function disposeRuntime() {
  runtimePrewarmGeneration++;
  runtimePrewarm = null;
  completedPrewarmWorkerCount = 0;
  gpuWarmupGeneration++;
  gpuWarmup = null;
  gpuWarmupCompleted = false;
  try {
    loadedWasm?.cancel_gpu_warmup();
  } catch {
    // Closing the worker releases a trapped GPU warmup state.
  }
  tablebaseRequested = false;
  deferredTablebaseRequested = false;
  tablebaseWarmupGeneration += 1;
  tablebaseWarmup = null;
  tablebaseWarmupAttempted = false;
  try {
    loadedWasm?.release_tablebase();
  } catch {
    // Closing the worker releases a trapped runtime's tablebase memory.
  }
  releasePc4TablebaseAssets();
  const job = active;
  if (job) releaseJobResources(job);
  else {
    disposeDistributedWorkers();
    try {
      loadedWasm?.distributed_reset();
    } catch {
      // Closing the worker releases a trapped runtime's linear memory.
    }
  }
  active = null;
  closeFailClosedWorker();
}

function postStarted(jobId: number) {
  postWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'started',
    job_id: jobId
  });
}

function emitCancelled(job: ActiveJob) {
  if (job.terminalPosted) return;
  job.terminalPosted = true;
  postCancelled(job.id);
}

function releaseJobResources(job: ActiveJob) {
  try {
    job.runner?.dispose();
  } catch {
    // Worker termination below is the final fail-closed boundary.
  }
  releaseProductPages();
  job.runner = null;
}

function failCloseUnhandled(error: unknown) {
  if (failClosed) return;
  const job = active;
  if (job) {
    releaseJobResources(job);
    if (!job.terminalPosted) {
      job.terminalPosted = true;
      postRuntimeFailure(
        job.id,
        'E_WASM_WORKER_UNHANDLED_FAILURE',
        error,
        loadedWasm?.failure_diagnostics()
      );
    }
  } else {
    disposeDistributedWorkers();
  }
  active = null;
  closeFailClosedWorker();
}

function closeFailClosedWorker() {
  if (failClosed) return;
  failClosed = true;
  runtimePrewarmGeneration++;
  runtimePrewarm = null;
  completedPrewarmWorkerCount = 0;
  gpuWarmupGeneration++;
  gpuWarmup = null;
  gpuWarmupCompleted = false;
  try {
    loadedWasm?.cancel_gpu_warmup();
  } catch {
    // Worker termination is the final fail-closed release boundary.
  }
  deferredTablebaseRequested = false;
  tablebaseWarmupGeneration += 1;
  tablebaseWarmup = null;
  try {
    loadedWasm?.release_tablebase();
  } catch {
    // Worker termination is the final fail-closed release boundary.
  }
  releasePc4TablebaseAssets();
  loadedWasm = null;
  self.close();
}

function isTerminal(event: ClearraWasmWorkerEvent) {
  return (
    event.event === 'final_response' ||
    event.event === 'failed' ||
    event.event === 'cancelled' ||
    event.event === 'terminated'
  );
}

function requiresFailClosedRelease(event: ClearraWasmWorkerEvent) {
  return (
    event.event === 'failed' ||
    event.event === 'cancelled' ||
    event.event === 'terminated' ||
    (event.event === 'final_response' && event.response.status !== 'success')
  );
}

function withJobId(event: ClearraWasmWorkerEvent, jobId: number): ClearraWasmWorkerEvent {
  return { ...event, job_id: jobId } as ClearraWasmWorkerEvent;
}

function postCancelled(jobId: number) {
  postWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'cancelled',
    job_id: jobId,
    scope_released: true,
    execution_availability: {
      state: 'cancelled',
      reason: 'cancelled-by-caller',
      surface: 'browser-wasm32',
      descriptor_pattern_count: null,
      dense_pattern_count: null,
      required_dense_bytes: null,
      required_memory_bytes: null
    },
    result_completeness: 'incomplete'
  });
}

function postRuntimeFailure(
  jobId: number,
  code: string,
  error: unknown,
  wasmDiagnostics?: ClearraWasmFailureDiagnostics
) {
  console.error('Clearra WASM worker failure', error);
  const linearMemoryExhausted =
    error instanceof WebAssembly.RuntimeError &&
    error.message.toLowerCase().includes('unreachable') &&
    !wasmDiagnostics?.rustPanic &&
    (wasmDiagnostics?.linearMemoryBytes ?? 0) >= 3 * 1024 * 1024 * 1024;
  const diagnosticCode =
    error instanceof ClearraWasmRuntimeError
      ? error.diagnosticCode
      : error instanceof WebAssembly.RuntimeError
        ? linearMemoryExhausted
          ? 'E_WASM_LINEAR_MEMORY_EXHAUSTED'
          : 'E_WASM_RUNTIME_TRAP'
        : code;
  const baseMessage = error instanceof Error ? error.message : String(error);
  const context = wasmDiagnostics
    ? `WASM linear memory: ${formatByteCount(wasmDiagnostics.linearMemoryBytes)}` +
      (wasmDiagnostics.rustPanic ? `; Rust panic: ${wasmDiagnostics.rustPanic}` : '')
    : null;
  const message = context ? `${baseMessage} (${context})` : baseMessage;
  const runtimeResourceReport =
    error instanceof ClearraWasmRuntimeError ? error.resourceReport : null;
  const typedAvailability = runtimeResourceReport
    ? runtimeResourceReport.execution_availability
    : error instanceof SharedExecutionAvailabilityError
      ? error.availability
      : {
        state: linearMemoryExhausted ? 'unavailable' : 'incomplete',
        reason: linearMemoryExhausted ? 'capability-unavailable' : 'partial-execution',
        surface: 'browser-wasm32',
        descriptor_pattern_count: null,
        dense_pattern_count: null,
        required_dense_bytes: null,
        required_memory_bytes: null
      } as const;
  postWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'failed',
    job_id: jobId,
    ...(runtimeResourceReport ? { resource_report: runtimeResourceReport } : {}),
    execution_availability: typedAvailability,
    result_completeness: runtimeResourceReport
      ? runtimeResourceReport.result_completeness
      : error instanceof SharedExecutionAvailabilityError
        ? 'not-executed'
        : 'incomplete',
    diagnostics: {
      diagnostics: [
        {
          code: diagnosticCode,
          severity: 'error',
          message
        }
      ]
    }
  });
}

function formatByteCount(bytes: number): string {
  const gibibytes = bytes / (1024 * 1024 * 1024);
  return gibibytes >= 1 ? `${gibibytes.toFixed(2)} GiB` : `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function postWorkerEvent(event: ClearraWasmWorkerEvent) {
  self.postMessage(event);
}

function postRuntimePrewarmPhase(phase: 'started' | 'finished', workerCount: number) {
  self.postMessage({
    type: 'runtime_prewarm',
    phase,
    workerCount
  });
}

function postTablebaseWarmupPhase(
  phase: 'disabled' | 'loading' | 'ready' | 'unavailable',
  byteLength: number,
  message?: string
) {
  self.postMessage({
    type: 'tablebase_warmup',
    phase,
    artifactSha256: pc4TablebaseArtifactSha256(),
    byteLength,
    ...(message ? { message } : {})
  });
}

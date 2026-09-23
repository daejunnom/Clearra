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
  type AcceleratorRequestPolicy,
  type ClearraWasmFailureDiagnostics,
  type ClearraWasmHostCapabilities,
  type ClearraWasmModule
} from './clearraWasmRuntime';
import {
  pc4TablebaseArtifactSha256,
  getPc4OnlineGeneration,
  prewarmPc4TablebaseAssets,
  releasePc4TablebaseAssets
} from './pc4TablebaseAssets';
import { currentQualifiedAcceleratorIdentity, readQualifiedAccelerator } from './acceleratorLocalStore';

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
    // The root WASM owner installs at most one immutable profile generation
    // per product. Distributed peers receive no asset copy and retain their
    // exact fallback; no network request is made on the solver hot path.
    await activateLocalAccelerators(wasm, commandText, hostCapabilitySnapshot.wasmTransferByteCap);
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
    }, { transportProfile: isLocalSearchProfileMode(import.meta.env.MODE),
      onlinePc4: tablebaseRequested ? getPc4OnlineGeneration() : null, tablebaseRequested });
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

const ACCELERATOR_PROFILES = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
let acceleratorOwner: ClearraWasmModule | null = null;
const activeAcceleratorIdentities = new Map<string, string>();

async function activateLocalAccelerators(
  wasm: ClearraWasmModule,
  commandText: string,
  transferByteCap: number
) {
  if (!wasm.accelerator_catalog || !wasm.accelerator_request_policy ||
      !wasm.accelerator_admit || !wasm.accelerator_remove) return;
  if (acceleratorOwner !== wasm) {
    acceleratorOwner = wasm;
    activeAcceleratorIdentities.clear();
  }
  let requestPolicy: AcceleratorRequestPolicy | null = null;
  try {
    requestPolicy = wasm.accelerator_request_policy(commandText);
  } catch {
    // The command runner owns the canonical parser error. Clear any prior
    // asset before it handles this request; a parse failure is never a reason
    // to retain a previous negative-proof snapshot.
  }
  const profile = requestPolicy && requestPolicy.profile !== null &&
    Number.isInteger(requestPolicy.profile) && requestPolicy.profile >= 0 &&
    requestPolicy.profile < ACCELERATOR_PROFILES.length
      ? requestPolicy.profile : -1;
  // A custom/unknown rule cannot inherit the previous request's accelerator.
  // Clear its owner slots before the exact path begins.
  // The product holds at most one profile per accelerator in this WASM
  // owner. A prior request for another kick table cannot retain memory or
  // leak its proof into a new request.
  for (let stale = 0; stale < ACCELERATOR_PROFILES.length; stale += 1) {
    if (stale === profile) continue;
    for (const kind of [0, 1]) {
      const key = `${kind}:${stale}`;
      if (activeAcceleratorIdentities.has(key)) {
        wasm.accelerator_remove(kind, stale);
        activeAcceleratorIdentities.delete(key);
      }
    }
  }
  if (profile < 0) return;
  for (const kind of [0, 1]) {
    // A previously loaded generation must never survive a failed local read
    // or a new catalog. If an in-flight lease prevents removal, fail closed
    // instead of allowing a stale negative proof into this request.
    const disabled = kind === 0 ? !requestPolicy?.legal_board
      : !requestPolicy?.conditioned_reachability;
    const key = `${kind}:${profile}`;
    if (disabled) {
      if (activeAcceleratorIdentities.has(key)) {
        wasm.accelerator_remove(kind, profile);
        activeAcceleratorIdentities.delete(key);
      }
      continue;
    }
    try {
      const plan = wasm.accelerator_catalog(kind, profile);
      if (plan.state !== 'qualified' || !plan.payload_bytes || plan.payload_bytes > transferByteCap) {
        if (activeAcceleratorIdentities.has(key)) {
          wasm.accelerator_remove(kind, profile);
          activeAcceleratorIdentities.delete(key);
        }
        continue;
      }
      const identity = await currentQualifiedAcceleratorIdentity(plan);
      if (activeAcceleratorIdentities.get(key) === identity) continue;
      if (activeAcceleratorIdentities.has(key)) {
        wasm.accelerator_remove(kind, profile);
        activeAcceleratorIdentities.delete(key);
      }
      if (!identity) continue;
      const bytes = await readQualifiedAccelerator(plan);
      if (bytes) {
        wasm.accelerator_admit(kind, profile, bytes, true);
        activeAcceleratorIdentities.set(key, identity);
      }
    } catch (error) {
      // Invalid/missing local assets are Unknown, not negative evidence.
      // A previously admitted asset cannot retain negative authority after
      // the local pointer check fails; clear it before exact fallback.
      if (activeAcceleratorIdentities.has(key)) {
        wasm.accelerator_remove(kind, profile);
        activeAcceleratorIdentities.delete(key);
      }
      console.warn('Clearra local accelerator unavailable; using exact search', error);
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
  if (active) {
    // A running non-TB search still leaves the network handshake on the
    // critical path of the next explicit TB request. Starting transport
    // preparation is safe because it owns no WASM/search state; defer only
    // the feature-off transition so an active TB job cannot lose its runtime
    // tables underneath it.
    if (requestedTablebase && !tablebaseRequested) setTablebaseRequested(true);
    return;
  }
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
  if (requested) {
    // Begin revision discovery and the bounded qualification reads as soon as
    // the host expresses intent. This intentionally overlaps DNS/TCP/TLS/ALPN
    // establishment with WASM compilation and worker preparation. The later
    // WASM join still owns the runtime-capability check.
    void startTablebaseTransportWarmup();
    return;
  }
  // The online generation preparation is owned by this worker lifetime, not
  // by the visible feature toggle. Let an
  // already-started qualification finish and retain its immutable revision so
  // re-enabling TB does not repeat discovery/TLS setup. The generation guard
  // below prevents the completed promise from publishing a ready UI state
  // while the feature is disabled. Runtime disposal and fail-closed shutdown
  // remain the only owners that abort and clear this cache. The retired
  // static-beta installer has no v0.9 runtime state to release here.
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

async function startTablebaseWarmupAfterWasm(wasm: ClearraWasmModule): Promise<void> {
  await startTablebaseTransportWarmup();
  if (tablebaseRequested && !wasm.configure_online_pc4) {
    throw new Error('pc4_online_wasm_update_required');
  }
}

function startTablebaseTransportWarmup(): Promise<void> {
  if (!tablebaseRequested) return Promise.resolve();
  if (tablebaseWarmup) return tablebaseWarmup;
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
      postTablebaseWarmupPhase(bundle.generation.profiles.some(slot => slot.status === 'ready') ? 'ready' : 'unavailable', bundle.byteLength);
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
  const runtimeMemoryBytes = currentRuntimeMemoryBytes();
  self.postMessage(
    isTerminal(event) && runtimeMemoryBytes !== undefined
      ? { ...event, runtime_memory_bytes: runtimeMemoryBytes }
      : event
  );
}

function postRuntimePrewarmPhase(phase: 'started' | 'finished', workerCount: number) {
  const runtimeMemoryBytes = phase === 'finished' ? currentRuntimeMemoryBytes() : undefined;
  self.postMessage({
    type: 'runtime_prewarm',
    phase,
    workerCount,
    ...(runtimeMemoryBytes === undefined ? {} : { runtimeMemoryBytes })
  });
}

function currentRuntimeMemoryBytes(): number | undefined {
  try {
    const bytes = loadedWasm?.linear_memory_bytes();
    return typeof bytes === 'number' && Number.isSafeInteger(bytes) && bytes >= 0
      ? bytes
      : undefined;
  } catch {
    // Memory accounting is advisory at this boundary. A detached or trapped
    // runtime must still be able to publish its original terminal event.
    return undefined;
  }
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
    profiles: getPc4OnlineGeneration()?.profiles.map(({
      profile,
      status,
      reason,
      pc_search_target_lines,
      setup_search_target_lines
    }) => ({
      profile,
      status,
      reason,
      pcSearchTargetLines: pc_search_target_lines ?? [],
      setupSearchTargetLines: setup_search_target_lines ?? []
    })) ?? [],
    ...(message ? { message } : {})
  });
}

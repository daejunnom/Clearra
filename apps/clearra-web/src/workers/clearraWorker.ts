import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

import { ClearraProductJobRunner } from './ClearraProductJobRunner';
import {
  disposeDistributedWorkers,
  prewarmDistributedWorkers
} from './DistributedWasmJobRunner';
import {
  ClearraWasmRuntimeError,
  loadClearraWasmModule,
  type ClearraWasmFailureDiagnostics,
  type ClearraWasmModule
} from './clearraWasmRuntime';

type ClearraWorkerMessage =
  | { type: 'prewarm_runtime'; workerCount: number }
  | { type: 'run_command_text'; commandText?: string; prewarmWorkerCount?: number }
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
let failClosed = false;

self.onmessage = (message: MessageEvent<ClearraWorkerMessage>) => {
  if (message.data.type === 'dispose_runtime') {
    disposeRuntime();
    return;
  }
  if (message.data.type === 'prewarm_runtime') {
    startRuntimePrewarm(message.data.workerCount);
    return;
  }
  if (message.data.type === 'cancel_job') {
    cancelActiveJob(message.data.jobId);
    return;
  }
  void runCommandText(
    message.data.commandText ?? '',
    message.data.prewarmWorkerCount ?? requestedPrewarmWorkerCount
  );
};

self.addEventListener('error', (event) => {
  event.preventDefault();
  failCloseUnhandled(event.error ?? new Error(event.message || 'WASM worker crashed'));
});

self.addEventListener('unhandledrejection', (event) => {
  event.preventDefault();
  failCloseUnhandled(event.reason);
});

async function runCommandText(commandText: string, prewarmWorkerCount: number) {
  if (active) {
    postRuntimeFailure(active.id, 'E_WASM_JOB_ALREADY_RUNNING', 'a WASM job is already active');
    return;
  }
  requestedPrewarmWorkerCount = Math.max(1, Math.floor(prewarmWorkerCount));
  interruptIncompleteRuntimePrewarm();
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
    // Entry prewarm is opportunistic. A serial plan must not wait for every
    // distributed verifier; distributed plans await their pool in run().
    wasm = await loadClearraWasmModule();
    loadedWasm = wasm;
    if (job.cancelled) {
      releaseJobResources(job, wasm);
      emitCancelled(job);
      closeFailClosedWorker();
      return;
    }
    failureCode = 'E_WASM_EXECUTION_FAILED';
    job.runner = new ClearraProductJobRunner(wasm, jobId);
    const terminal = await job.runner.run(commandText, (event) => {
      if (event.event === 'started') return;
      const emitted = withJobId(event, job.id);
      if (isTerminal(emitted)) job.terminalPosted = true;
      postWorkerEvent(emitted);
    });
    if (requiresFailClosedRelease(terminal)) {
      releaseJobResources(job, wasm);
      closeFailClosedWorker();
    }
  } catch (error) {
    releaseJobResources(job, wasm);
    if (job.cancelled) {
      emitCancelled(job);
    } else {
      job.terminalPosted = true;
      postRuntimeFailure(job.id, failureCode, error, wasm?.failure_diagnostics());
    }
    closeFailClosedWorker();
  } finally {
    if (active === job) active = null;
    if (!failClosed) startRuntimePrewarm(requestedPrewarmWorkerCount);
  }
}

function startRuntimePrewarm(workerCount: number) {
  const boundedWorkerCount = Math.max(1, Math.floor(workerCount));
  requestedPrewarmWorkerCount = boundedWorkerCount;
  if (runtimePrewarm || completedPrewarmWorkerCount >= boundedWorkerCount) return;
  const generation = ++runtimePrewarmGeneration;
  postRuntimePrewarmPhase('started', boundedWorkerCount);
  runtimePrewarm = loadClearraWasmModule()
    .then(async (wasm) => {
      loadedWasm = wasm;
      if (generation !== runtimePrewarmGeneration) return;
      await wasm.prewarm_gpu(null);
      if (generation !== runtimePrewarmGeneration) return;
      await prewarmDistributedWorkers(boundedWorkerCount, wasm.compiled_module());
      if (generation === runtimePrewarmGeneration) {
        completedPrewarmWorkerCount = boundedWorkerCount;
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
        postRuntimePrewarmPhase('finished', boundedWorkerCount);
      }
    });
}

function interruptIncompleteRuntimePrewarm() {
  if (!runtimePrewarm || completedPrewarmWorkerCount >= requestedPrewarmWorkerCount) return;
  runtimePrewarmGeneration++;
  runtimePrewarm = null;
  completedPrewarmWorkerCount = 0;
  disposeDistributedWorkers();
}

function cancelActiveJob(jobId: number | undefined) {
  const job = active;
  if (!job || job.terminalPosted || (jobId !== undefined && jobId !== job.id)) return;
  job.cancelled = true;
  try {
    job.runner?.cancel();
  } catch (error) {
    releaseJobResources(job, loadedWasm);
    emitCancelled(job);
    closeFailClosedWorker();
    console.error('Clearra cancellation cleanup failed', error);
  }
}

function disposeRuntime() {
  runtimePrewarmGeneration++;
  runtimePrewarm = null;
  completedPrewarmWorkerCount = 0;
  const job = active;
  if (job) releaseJobResources(job, loadedWasm);
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

function releaseJobResources(job: ActiveJob, wasm: ClearraWasmModule | null) {
  try {
    job.runner?.dispose();
  } catch {
    // Worker termination below is the final fail-closed boundary.
  }
  job.runner = null;
  disposeDistributedWorkers();
  try {
    wasm?.distributed_reset();
  } catch {
    // A trapped module is released when this worker closes.
  }
}

function failCloseUnhandled(error: unknown) {
  if (failClosed) return;
  const job = active;
  if (job) {
    releaseJobResources(job, loadedWasm);
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
  loadedWasm = null;
  self.close();
}

function isTerminal(event: ClearraWasmWorkerEvent) {
  return event.event === 'final_response' || event.event === 'failed' || event.event === 'cancelled';
}

function requiresFailClosedRelease(event: ClearraWasmWorkerEvent) {
  return (
    event.event === 'failed' ||
    event.event === 'cancelled' ||
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
    scope_released: true
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
  postWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'failed',
    job_id: jobId,
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

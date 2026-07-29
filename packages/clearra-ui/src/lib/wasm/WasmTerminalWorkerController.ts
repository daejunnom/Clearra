import type { ClearraWasmWorkerEvent } from './wasmCommandClient';
import { postPrewarmRuntime } from './wasmCommandClient';
import {
  applyTablebaseWarmupEvent,
  applyWasmWorkerEvent,
  cancelWasmCommand,
  runWasmCommand,
  type TablebaseWarmupWorkerEvent
} from './wasmWorkerStore';

const COOPERATIVE_CANCEL_GRACE_MS = 100;
const OWNER_DISPOSE_GRACE_MS = 100;

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
  private tablebaseRequested = false;
  private cancelFallback: ReturnType<typeof setTimeout> | null = null;

  constructor(private workerFactory: (() => Worker) | null) {}

  setWorkerFactory(workerFactory: (() => Worker) | null) {
    if (this.workerFactory === workerFactory) return;
    this.dispose();
    this.workerFactory = workerFactory;
  }

  run() {
    if (this.worker && this.prewarmingWorker === this.worker) {
      // The worker awaits in-flight prewarm and preserves its compiled
      // coordinator module and verifier pool for the requested job.
      this.prewarmingWorker = null;
    }
    const worker = this.ensureWorker();
    if (!worker) return;
    try {
      runWasmCommand(worker, this.prewarmWorkerCount, this.tablebaseRequested);
    } catch (error) {
      this.failClosedWorker(worker, 'E_WASM_WORKER_MESSAGE_FAILED', errorMessage(error));
    }
  }

  prewarm(workerCount: number, tablebaseRequested = false) {
    this.prewarmWorkerCount = Math.max(1, Math.floor(workerCount));
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
    this.cancelFallback = setTimeout(() => {
      this.terminateCancelledWorker(worker, jobId);
    }, COOPERATIVE_CANCEL_GRACE_MS);
  }

  takeIdleWorker(): Worker | null {
    if (
      !this.worker ||
      this.cancellingWorker !== null ||
      this.prewarmingWorker !== null ||
      this.cancelFallback !== null
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
    if (jobId !== undefined) this.emitReleasedCancellation(jobId ?? 0);
  }

  private ensureWorker() {
    if (!this.worker && this.workerFactory) {
      const worker = this.workerFactory();
      this.worker = worker;
      worker.onmessage = (
        message: MessageEvent<
          ClearraWasmWorkerEvent | RuntimePrewarmWorkerEvent | TablebaseWarmupWorkerEvent
        >
      ) => {
        if (this.worker !== worker) return;
        if (isRuntimePrewarmWorkerEvent(message.data)) {
          this.prewarmingWorker = message.data.phase === 'started' ? worker : null;
          return;
        }
        if (isTablebaseWarmupWorkerEvent(message.data)) {
          applyTablebaseWarmupEvent(message.data);
          return;
        }
        if (this.cancellingWorker === worker) {
          if (message.data.event === 'cancelled') {
            applyWasmWorkerEvent(message.data);
            this.releaseWorker(worker);
            return;
          } else if (message.data.event === 'final_response' || message.data.event === 'failed') {
            this.terminateCancelledWorker(worker, message.data.job_id);
            return;
          }
        }
        applyWasmWorkerEvent(message.data);
        if (
          message.data.event === 'failed' ||
          message.data.event === 'cancelled' ||
          (message.data.event === 'final_response' && message.data.response.status !== 'success')
        ) {
          this.releaseWorker(worker);
        }
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
    this.releaseWorker(worker);
    this.emitReleasedCancellation(jobId ?? 0);
  }

  private failClosedWorker(worker: Worker, code: string, message: string) {
    if (this.worker !== worker) return;
    this.releaseWorker(worker);
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

  private emitReleasedCancellation(jobId: number) {
    applyWasmWorkerEvent({
      schema_version: 1,
      runtime: 'clearra-wasm',
      event: 'cancelled',
      job_id: jobId,
      scope_released: true
    });
  }

  private prewarmWorker(worker: Worker, workerCount: number) {
    try {
      this.prewarmingWorker = worker;
      postPrewarmRuntime(worker, workerCount, this.tablebaseRequested);
    } catch (error) {
      this.failClosedWorker(worker, 'E_WASM_WORKER_PREWARM_FAILED', errorMessage(error));
    }
  }

  private releaseWorker(worker: Worker) {
    if (this.worker !== worker) return;
    this.clearCancelFallback();
    if (this.prewarmingWorker === worker) this.prewarmingWorker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    worker.terminate();
    this.worker = null;
  }

  private disposeOwnedWorker(worker: Worker) {
    if (this.worker !== worker) return;
    this.clearCancelFallback();
    if (this.prewarmingWorker === worker) this.prewarmingWorker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    this.worker = null;
    try {
      worker.postMessage({ type: 'dispose_runtime' });
      setTimeout(() => worker.terminate(), OWNER_DISPOSE_GRACE_MS);
    } catch {
      worker.terminate();
    }
  }

  private clearCancelFallback() {
    if (this.cancelFallback !== null) clearTimeout(this.cancelFallback);
    this.cancelFallback = null;
    this.cancellingWorker = null;
  }
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

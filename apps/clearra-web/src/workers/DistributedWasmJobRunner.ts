import type {
  ClearraSearchProgressTelemetry,
  ClearraWasmWorkerEvent
} from '@clearra/ui/wasm';

import { ClearraVerifierPool } from './ClearraVerifierPool';
import type { ClearraDistributedPlan, ClearraWasmModule } from './clearraWasmRuntime';

const PRODUCER_WORK_BUDGET = 32768;
const CANDIDATE_BATCH_SIZE = 256;
const HOST_YIELD_BUDGET_MS = 8;
const PROGRESS_REFRESH_MS = 50;
const yieldToWorkerHost = createWorkerHostYield();
const sharedVerifierPool = new ClearraVerifierPool();

export function prewarmDistributedWorkers(
  totalWorkerCount: number,
  compiledModule: WebAssembly.Module
): Promise<void> {
  return sharedVerifierPool.prewarm(
    Math.max(0, Math.floor(totalWorkerCount) - 1),
    compiledModule
  );
}

export function disposeDistributedWorkers() {
  sharedVerifierPool.cancel();
}

export class DistributedWasmJobRunner {
  private cancelled = false;
  private released = false;
  private pool = sharedVerifierPool;

  constructor(
    private readonly wasm: ClearraWasmModule,
    private readonly jobId: number
  ) {}

  prepare(commandText: string): ClearraDistributedPlan {
    return this.wasm.distributed_prepare(commandText);
  }

  async run(
    commandText: string,
    plan: ClearraDistributedPlan,
    onEvent: (event: ClearraWasmWorkerEvent) => void
  ): Promise<ClearraWasmWorkerEvent> {
    this.cancelled = false;
    this.released = false;
    let profilingActive = false;
    let searchProfile: unknown = null;
    if (this.wasm.profile_start) {
      this.wasm.profile_start();
      profilingActive = true;
    }
    const verifierCount = Math.max(1, plan.workerCount - 1);
    let lastHostYield = performance.now();
    let progressPhase: ClearraSearchProgressTelemetry['phase'] = 'initializing';
    let producerComplete = false;
    const emitProgress = () => {
      if (this.cancelled) return;
      const producer = this.wasm.distributed_progress();
      const verifier =
        progressPhase === 'initializing'
          ? {
              candidatesVerified: 0,
              buildNodes: 0,
              coverageChecks: 0,
              activeWorkers: 0,
              workerCount: verifierCount,
              oldestBatchMs: 0
            }
          : this.pool.progressSnapshot();
      onEvent(
        progressEvent(
          this.jobId,
          plan,
          progressStep(progressPhase),
          5,
          progressLabel(progressPhase),
          {
            phase: progressPhase,
            producer_complete: producerComplete,
            geometry_nodes: producer.geometryNodes,
            candidates_emitted: producer.candidateCount,
            geometry_family_count: producer.candidateFamilyCount,
            candidates_verified: verifier.candidatesVerified,
            build_nodes: verifier.buildNodes,
            coverage_checks: verifier.coverageChecks,
            active_workers: verifier.activeWorkers,
            worker_count: verifier.workerCount,
            oldest_batch_ms: Math.floor(verifier.oldestBatchMs),
            pass_index: producer.passIndex,
            pass_count: producer.passCount
          }
        )
      );
    };
    const progressTimer = setInterval(emitProgress, PROGRESS_REFRESH_MS);
    try {
      onEvent({
        schema_version: 1,
        runtime: 'clearra-wasm',
        event: 'started',
        job_id: this.jobId
      });
      emitProgress();
      let verifierInitialization: Promise<void> | null = null;
      if (!plan.deferredInitialization) {
        verifierInitialization = this.pool.initialize(
          plan.workerInitialization ?? commandText,
          verifierCount,
          this.wasm.compiled_module()
        );
        void verifierInitialization.catch(() => undefined);
      }
      await yieldToWorkerHost();
      this.requireActive();
      lastHostYield = performance.now();
      progressPhase = 'searching';
      emitProgress();

      while (!this.cancelled) {
        const produced = this.wasm.distributed_produce(
          PRODUCER_WORK_BUDGET,
          CANDIDATE_BATCH_SIZE
        );
        if (produced.status === 'initialization') {
          if (verifierInitialization) {
            throw new Error('distributed worker initialization was produced more than once');
          }
          verifierInitialization = this.pool.initialize(
            produced.initialization,
            verifierCount,
            this.wasm.compiled_module()
          );
          void verifierInitialization.catch(() => undefined);
          await yieldToWorkerHost();
          lastHostYield = performance.now();
          this.requireActive();
          continue;
        }
        if (produced.status === 'batch') {
          if (!verifierInitialization) {
            throw new Error('distributed task arrived before worker initialization');
          }
          await verifierInitialization;
          this.requireActive();
          await this.pool.enqueue(produced.batch, (partial) =>
            this.wasm.distributed_merge_partial(partial)
          );
          if (performance.now() - lastHostYield >= HOST_YIELD_BUDGET_MS) {
            await yieldToWorkerHost();
            lastHostYield = performance.now();
            this.requireActive();
          }
          continue;
        }
        if (produced.status === 'completed') break;
        if (produced.status === 'cancelled') throw new Error('distributed search cancelled');
        await yieldToWorkerHost();
        lastHostYield = performance.now();
        this.requireActive();
      }
      this.requireActive();
      if (!verifierInitialization) {
        throw new Error('distributed producer completed without worker initialization');
      }
      await verifierInitialization;
      this.requireActive();

      producerComplete = true;
      progressPhase = 'draining';
      emitProgress();
      await this.pool.waitForIdle();
      this.requireActive();
      progressPhase = 'merging';
      emitProgress();
      await this.pool.finish((partial) => this.wasm.distributed_merge_partial(partial));
      this.requireActive();
      const events = JSON.parse(
        this.wasm.distributed_finish(this.jobId, plan.workerCount)
      ) as ClearraWasmWorkerEvent[];
      if (profilingActive && this.wasm.profile_finish) {
        searchProfile = this.wasm.profile_finish();
        profilingActive = false;
      }
      let terminal: ClearraWasmWorkerEvent | null = null;
      for (const event of events) {
        const emittedEvent =
          (event.event === 'final_response' || event.event === 'failed') &&
          searchProfile !== null
            ? ({ ...event, search_profile: searchProfile } as ClearraWasmWorkerEvent)
            : event;
        onEvent(emittedEvent);
        if (event.event === 'final_response' || event.event === 'failed') terminal = emittedEvent;
      }
      if (!terminal) throw new Error('distributed search completed without a terminal event');
      return terminal;
    } catch (error) {
      this.releaseFailedRun();
      throw error;
    } finally {
      if (profilingActive && this.wasm.profile_finish) {
        try {
          this.wasm.profile_finish();
        } catch {
          // Coordinator reset remains the final ownership boundary after failure.
        }
      }
      clearInterval(progressTimer);
      this.resetCoordinator();
    }
  }

  cancel() {
    if (this.cancelled && this.released) return;
    this.cancelled = true;
    this.releaseFailedRun();
  }

  dispose() {
    this.cancelled = true;
    this.releaseFailedRun();
  }

  private requireActive() {
    if (this.cancelled) throw new Error('distributed search cancelled');
  }

  private releaseFailedRun() {
    if (this.released) return;
    this.released = true;
    try {
      this.wasm.distributed_cancel();
    } catch {
      // Reset and worker termination below are the fail-closed fallback.
    }
    this.pool.cancel();
    this.resetCoordinator();
  }

  private resetCoordinator() {
    try {
      this.wasm.distributed_reset();
    } catch {
      // The main worker is terminated after failure or cancellation.
    }
  }
}

function progressEvent(
  jobId: number,
  plan: ClearraDistributedPlan,
  done: number,
  total: number,
  label: string,
  telemetry?: ClearraSearchProgressTelemetry
): ClearraWasmWorkerEvent {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'progress',
    job_id: jobId,
    progress: {
      done,
      total,
      label,
      budget_status: { state: 'within-budget', used: 0, limit: null },
      backend_status: {
        backend_requested: plan.requestedBackend,
        backend_selected: plan.selectedBackend,
        fallback_used: plan.fallbackUsed,
        fallback_reason: plan.fallbackReason
      },
      memory_status: {
        state: 'wasm-computation-scope-active',
        raw_pointer_exposed: false
      },
      telemetry
    }
  } as ClearraWasmWorkerEvent;
}

function progressStep(phase: ClearraSearchProgressTelemetry['phase']): number {
  return { preparing: 0, initializing: 1, searching: 2, draining: 3, merging: 4 }[phase];
}

function progressLabel(phase: ClearraSearchProgressTelemetry['phase']): string {
  return {
    preparing: 'Search catalog preparing',
    initializing: 'Distributed workers initializing',
    searching: 'Geometry and exact verification running',
    draining: 'Remaining exact verification draining',
    merging: 'Exact results merging'
  }[phase];
}

function createWorkerHostYield(): () => Promise<void> {
  const channel = new MessageChannel();
  const pending: Array<() => void> = [];
  channel.port1.onmessage = () => pending.shift()?.();
  return () =>
    new Promise<void>((resolve) => {
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
}

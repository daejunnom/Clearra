import type {
  ClearraSearchProgressTelemetry,
  ClearraSearchProgressTelemetryFlags,
  ClearraWasmWorkerEvent
} from '@clearra/ui/wasm';

import {
  ClearraVerifierPool,
  type ClearraVerifierPoolProgress,
  type ClearraVerifierRecoveryMode
} from './ClearraVerifierPool';
import type {
  ClearraDistributedCoreProgress,
  ClearraDistributedPlan,
  ClearraWasmHostCapabilities,
  ClearraWasmModule
} from './clearraWasmRuntime';

const PRODUCER_WORK_BUDGET = 32768;
const CANDIDATE_BATCH_SIZE = 256;
const HOST_YIELD_BUDGET_MS = 8;
const PROGRESS_REFRESH_MS = 50;
const yieldToWorkerHost = createWorkerHostYield();
const sharedVerifierPool = new ClearraVerifierPool();

export function prewarmDistributedWorkers(
  totalWorkerCount: number,
  compiledModule: WebAssembly.Module,
  lifecycleOwnerId = '',
  hostCapabilities?: ClearraWasmHostCapabilities
): Promise<void> {
  return sharedVerifierPool.prewarm(
    Math.max(0, Math.floor(totalWorkerCount) - 1),
    compiledModule,
    lifecycleOwnerId,
    hostCapabilities
  );
}

export function disposeDistributedWorkers() {
  sharedVerifierPool.cancel();
}

export class DistributedWasmJobRunner {
  private cancelled = false;
  private released = false;
  private readonly pool: ClearraVerifierPool;

  constructor(
    private readonly wasm: ClearraWasmModule,
    private readonly jobId: number,
    private readonly lifecycleOwnerId: string,
    private readonly hostCapabilities: ClearraWasmHostCapabilities,
    pool: ClearraVerifierPool = sharedVerifierPool
  ) {
    this.pool = pool;
  }

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
    const verifierCount = plan.verificationRequired
      ? Math.max(1, plan.workerCount - 1)
      : 0;
    let effectiveVerifierCount = verifierCount;
    let finishedVerifierCount = 0;
    let lastHostYield = performance.now();
    let progressPhase: ClearraSearchProgressTelemetry['phase'] = 'initializing';
    let producerComplete = false;
    let lastVerifierProgress: ClearraVerifierPoolProgress = {
      candidatesVerified: 0,
      buildNodes: 0,
      coverageChecks: 0,
      availability: emptyVerifierProgressFlags(),
      exactness: emptyVerifierProgressFlags(),
      readyWorkers: 0,
      activeWorkers: 0,
      workerCount: verifierCount,
      oldestBatchMs: 0
    };
    const emitProgress = () => {
      if (this.cancelled) return;
      const producer = this.wasm.distributed_progress();
      let verifier = lastVerifierProgress;
      if (progressPhase !== 'initializing' && progressPhase !== 'merging') {
        const snapshot = this.pool.progressSnapshot();
        if (snapshot.workerCount > 0) lastVerifierProgress = snapshot;
        verifier = snapshot.workerCount > 0 ? snapshot : lastVerifierProgress;
      }
      onEvent(
        progressEvent(
          this.jobId,
          plan,
          progressStep(progressPhase, plan.verificationRequired),
          progressTotal(plan.verificationRequired),
          progressLabel(
            progressPhase,
            plan.verificationRequired,
            plan.tilingGeometryParallel
          ),
          {
            phase: progressPhase,
            producer_complete: producerComplete,
            geometry_nodes: producer.geometryNodes,
            candidates_emitted: producer.candidateCount,
            geometry_family_count: producer.candidateFamilyCount,
            candidates_verified: verifier.candidatesVerified,
            producer_build_nodes: producer.buildNodes,
            producer_coverage_checks: producer.coverageChecks,
            build_nodes: verifier.buildNodes,
            coverage_checks: verifier.coverageChecks,
            ready_workers: verifier.readyWorkers,
            active_workers: verifier.activeWorkers,
            worker_count: verifier.workerCount,
            oldest_batch_ms: Math.floor(verifier.oldestBatchMs),
            pass_index: producer.passIndex,
            pass_count: producer.passCount,
            layer_index: producer.layerIndex,
            layer_count: producer.layerCount,
            layer_done: producer.layerDone,
            layer_total: producer.layerTotal,
            availability: telemetryFlags(producer, verifier, 'availability'),
            exactness: telemetryFlags(producer, verifier, 'exactness')
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
      if (plan.verificationRequired && !plan.deferredInitialization) {
        verifierInitialization = this.pool.initialize(
          plan.workerInitialization ?? commandText,
          verifierCount,
          this.wasm.compiled_module(),
          this.lifecycleOwnerId,
          verifierRecoveryMode(plan),
          this.hostCapabilities
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
          if (!plan.verificationRequired) {
            throw new Error('distributed producer requested an unavailable verifier');
          }
          if (verifierInitialization) {
            throw new Error('distributed worker initialization was produced more than once');
          }
          effectiveVerifierCount = plan.deferredInitialization
            ? boundedVerifierCount(
                verifierCount,
                this.wasm.distributed_progress().candidateFamilyCount
              )
            : verifierCount;
          verifierInitialization = this.pool.initialize(
            produced.initialization,
            effectiveVerifierCount,
            this.wasm.compiled_module(),
            this.lifecycleOwnerId,
            verifierRecoveryMode(plan),
            this.hostCapabilities
          );
          void verifierInitialization.catch(() => undefined);
          await yieldToWorkerHost();
          lastHostYield = performance.now();
          this.requireActive();
          continue;
        }
        if (produced.status === 'batch') {
          if (!plan.verificationRequired) {
            throw new Error('distributed producer emitted an unavailable worker batch');
          }
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
      producerComplete = true;
      if (plan.verificationRequired) {
        if (!verifierInitialization) {
          throw new Error('distributed producer completed without worker initialization');
        }
        await verifierInitialization;
        this.requireActive();
        progressPhase = 'draining';
        emitProgress();
        await this.pool.waitForIdle();
        this.requireActive();
      }
      if (plan.verificationRequired) {
        progressPhase = 'postprocessing';
        emitProgress();
        finishedVerifierCount = await this.pool.finish((partial) =>
          this.wasm.distributed_merge_partial(partial)
        );
        lastVerifierProgress = {
          ...lastVerifierProgress,
          readyWorkers: finishedVerifierCount,
          activeWorkers: 0,
          workerCount: finishedVerifierCount,
          oldestBatchMs: 0
        };
        this.requireActive();
      }
      progressPhase = 'merging';
      emitProgress();
      const events = JSON.parse(
        this.wasm.distributed_finish(
          this.jobId,
          plan.verificationRequired ? finishedVerifierCount + 1 : 1
        )
      ) as ClearraWasmWorkerEvent[];
      if (profilingActive && this.wasm.profile_finish) {
        searchProfile = this.wasm.profile_finish();
        profilingActive = false;
      }
      let terminal: ClearraWasmWorkerEvent | null = null;
      for (const event of events) {
        const emittedEvent = withSearchProfile(event, searchProfile);
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

function emptyVerifierProgressFlags() {
  return {
    candidatesVerified: false,
    buildNodes: false,
    coverageChecks: false
  };
}

function telemetryFlags(
  producer: ClearraDistributedCoreProgress,
  verifier: ClearraVerifierPoolProgress,
  kind: 'availability' | 'exactness'
): ClearraSearchProgressTelemetryFlags {
  const producerFlags = producer[kind];
  const verifierFlags = verifier[kind];
  return {
    geometry_nodes: producerFlags.geometryNodes,
    candidates_emitted: producerFlags.candidateCount,
    geometry_family_count: producerFlags.candidateFamilyCount,
    candidates_verified: verifierFlags.candidatesVerified,
    producer_build_nodes: producerFlags.buildNodes,
    producer_coverage_checks: producerFlags.coverageChecks,
    build_nodes: verifierFlags.buildNodes,
    coverage_checks: verifierFlags.coverageChecks,
    ready_workers: true,
    active_workers: true,
    worker_count: true,
    oldest_batch_ms: true,
    pass_index: producerFlags.passIndex,
    pass_count: producerFlags.passCount,
    layer_index: producerFlags.layerIndex,
    layer_count: producerFlags.layerCount,
    layer_done: producerFlags.layerDone,
    layer_total: producerFlags.layerTotal
  };
}

function withSearchProfile(
  event: ClearraWasmWorkerEvent,
  searchProfile: unknown
): ClearraWasmWorkerEvent {
  if (
    searchProfile === null ||
    (event.event !== 'final_response' && event.event !== 'failed')
  ) {
    return event;
  }
  return { ...event, search_profile: searchProfile } as unknown as ClearraWasmWorkerEvent;
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

function progressStep(
  phase: ClearraSearchProgressTelemetry['phase'],
  verificationRequired: boolean
): number {
  if (!verificationRequired) {
    return {
      preparing: 0,
      initializing: 0,
      searching: 1,
      draining: 1,
      postprocessing: 2,
      merging: 2
    }[phase];
  }
  return {
    preparing: 0,
    initializing: 1,
    searching: 2,
    draining: 3,
    postprocessing: 4,
    merging: 5
  }[phase];
}

function progressTotal(verificationRequired: boolean): number {
  return verificationRequired ? 6 : 3;
}

function progressLabel(
  phase: ClearraSearchProgressTelemetry['phase'],
  verificationRequired: boolean,
  tilingGeometryParallel: boolean
): string {
  if (tilingGeometryParallel) {
    return {
      preparing: 'Runtime preparing',
      initializing: 'Geometry workers preparing',
      searching: 'Geometry roots searching',
      draining: 'Remaining geometry roots finishing',
      postprocessing: 'Exact tiling results preparing',
      merging: 'Exact tiling results merging'
    }[phase];
  }
  if (!verificationRequired) {
    return {
      preparing: 'Runtime preparing',
      initializing: 'Runtime preparing',
      searching: 'Geometry and candidates generating',
      draining: 'Geometry and candidates generating',
      postprocessing: 'Exact tiling results preparing',
      merging: 'Exact tiling results merging'
    }[phase];
  }
  return {
    preparing: 'Search catalog preparing',
    initializing: 'Distributed workers initializing',
    searching: 'Geometry and exact verification running',
    draining: 'Remaining exact verification draining',
    postprocessing: 'Verified results postprocessing',
    merging: 'Exact results merging'
  }[phase];
}

function createWorkerHostYield(): () => Promise<void> {
  const channel = new MessageChannel();
  const nodePort1 = channel.port1 as MessagePort & { unref?: () => void };
  const nodePort2 = channel.port2 as MessagePort & { unref?: () => void };
  const pending: Array<() => void> = [];
  channel.port1.onmessage = () => pending.shift()?.();
  nodePort1.unref?.();
  nodePort2.unref?.();
  return () =>
    new Promise<void>((resolve) => {
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
}

function verifierRecoveryMode(plan: ClearraDistributedPlan): ClearraVerifierRecoveryMode {
  if (plan.tilingGeometryParallel) return 'streaming';
  if (plan.workerInitialization !== null || plan.deferredInitialization) {
    return 'atomic-task';
  }
  return 'replay-state';
}

function boundedVerifierCount(requested: number, taskCount: string | null): number {
  if (taskCount === null) return requested;
  try {
    const tasks = BigInt(taskCount);
    if (tasks <= 0n) return 1;
    return Math.max(1, Math.min(requested, Number(tasks)));
  } catch {
    return requested;
  }
}

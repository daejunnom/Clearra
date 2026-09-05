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
import {
  authorityForVerifierPool,
  browserExecutionResourceCapacity,
  SharedExecutionAvailabilityError,
  type SharedExecutionResourceAuthority,
  type SharedExecutionResourceLease
} from './SharedExecutionResourceAuthority';

// The core applies its own latency bound to PC geometry transactions. Keep a
// large logical budget here so cheap resumable states can be drained inside a
// single host quantum; the loop below still yields after this wall-time budget
// for cancellation, timers, and verifier messages.
const PRODUCER_WORK_BUDGET = 2_048;
const MAX_CANDIDATE_BATCH_SIZE = 256;
const TARGET_BATCHES_PER_VERIFIER = 4;
const HOST_YIELD_BUDGET_MS = 8;
const PROGRESS_REFRESH_MS = 50;
const SHARED_RESOURCE_WAIT_TIMEOUT_MS = 5_000;
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
  private coordinatorOwned = false;
  private poolOwned = false;
  private acquireGeneration = 0;
  private acquireController: AbortController | null = null;
  private resourceLease: SharedExecutionResourceLease | null = null;
  private readonly pool: ClearraVerifierPool;
  private readonly resourceAuthority: SharedExecutionResourceAuthority;

  constructor(
    private readonly wasm: ClearraWasmModule,
    private readonly jobId: number,
    private readonly lifecycleOwnerId: string,
    private readonly hostCapabilities: ClearraWasmHostCapabilities,
    pool: ClearraVerifierPool = sharedVerifierPool,
    resourceAuthority?: SharedExecutionResourceAuthority,
    private readonly resourceWaitTimeoutMs = SHARED_RESOURCE_WAIT_TIMEOUT_MS
  ) {
    this.pool = pool;
    this.resourceAuthority = resourceAuthority ?? authorityForVerifierPool(
      pool,
      browserExecutionResourceCapacity(
        hostCapabilities.logicalProcessorCount,
        hostCapabilities.transferByteCap
      )
    );
  }

  async acquire(): Promise<void> {
    if (this.resourceLease && !this.resourceLease.isReleased()) return;
    const generation = ++this.acquireGeneration;
    this.cancelled = false;
    this.released = false;
    this.coordinatorOwned = false;
    this.poolOwned = false;
    const controller = new AbortController();
    this.acquireController = controller;
    let lease: SharedExecutionResourceLease;
    try {
      lease = await this.resourceAuthority.acquireBounded(
        `${this.lifecycleOwnerId || 'anonymous'}:${this.jobId}`,
        this.resourceAuthority.capacity(),
        {
          timeoutMs: this.resourceWaitTimeoutMs,
          signal: controller.signal
        }
      );
    } catch (error) {
      if (generation === this.acquireGeneration) {
        this.acquireController = null;
        this.released = true;
      }
      throw error;
    }
    if (
      generation !== this.acquireGeneration ||
      controller.signal.aborted ||
      this.cancelled ||
      this.released
    ) {
      if (!lease.isReleased()) lease.release();
      if (generation === this.acquireGeneration) this.acquireController = null;
      const capacity = this.resourceAuthority.capacity();
      throw new SharedExecutionAvailabilityError(
        Object.freeze({
          state: 'cancelled',
          reason: 'cancelled-by-caller',
          surface: 'browser-wasm32',
          descriptor_pattern_count: null,
          dense_pattern_count: null,
          required_dense_bytes: null,
          required_memory_bytes: null
        }),
        capacity,
        this.resourceAuthority.snapshot().available
      );
    }
    this.resourceLease = lease;
    this.acquireController = null;
  }

  prepare(commandText: string): ClearraDistributedPlan {
    if (!this.resourceLease || this.resourceLease.isReleased()) {
      throw new Error('distributed coordinator preparation requires an execution lease');
    }
    // distributed_prepare may mutate coordinator state before throwing. Mark
    // ownership first because the shared lease has already been acquired.
    this.coordinatorOwned = true;
    return this.wasm.distributed_prepare(commandText);
  }

  finishPreparedResult(
    onEvent: (event: ClearraWasmWorkerEvent) => void
  ): ClearraWasmWorkerEvent {
    if (!this.resourceLease || this.resourceLease.isReleased() || !this.coordinatorOwned) {
      throw new Error('prepared terminal result requires an owned execution lease');
    }
    try {
      this.requireActive();
      onEvent({
        schema_version: 1,
        runtime: 'clearra-wasm',
        event: 'started',
        job_id: this.jobId
      });
      this.requireActive();
      const decoded = JSON.parse(this.wasm.distributed_finish(this.jobId, 0)) as unknown;
      if (!Array.isArray(decoded)) {
        throw new Error('prepared terminal result returned a non-array event payload');
      }
      let terminal: ClearraWasmWorkerEvent | null = null;
      for (const event of decoded as ClearraWasmWorkerEvent[]) {
        onEvent(event);
        if (
          event.event === 'final_response' ||
          event.event === 'failed' ||
          event.event === 'cancelled' ||
          event.event === 'terminated'
        ) {
          terminal = event;
        }
      }
      if (!terminal) {
        throw new Error('prepared App response completed without a terminal event');
      }
      this.releaseCompletedRun();
      return terminal;
    } catch (error) {
      this.releaseFailedRun();
      throw error;
    }
  }

  /**
   * Clears preparation-only coordinator state while retaining the shared
   * execution lease for a serial fallback owned by the same product run.
   */
  resetPreparedCoordinatorForSerial(): void {
    if (!this.resourceLease || this.resourceLease.isReleased()) {
      throw new Error('serial handoff requires an execution lease');
    }
    this.resetOwnedCoordinator();
  }

  async run(
    commandText: string,
    plan: ClearraDistributedPlan,
    onEvent: (event: ClearraWasmWorkerEvent) => void
  ): Promise<ClearraWasmWorkerEvent> {
    await this.acquire();
    // A directly supplied plan is a compatibility path for hosts that prepared
    // the same coordinator immediately before run(). The lease makes taking
    // ownership safe; no coordinator state is touched before acquisition.
    this.coordinatorOwned = true;
    let profilingActive = false;
    let searchProfile: unknown = null;
    if (this.wasm.profile_start) {
      try {
        this.wasm.profile_start();
        profilingActive = true;
      } catch (error) {
        this.releaseFailedRun();
        throw error;
      }
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
      workerCount: 0,
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
      const coordinatorActive = distributedCoordinatorIsActive(
        progressPhase,
        producerComplete
      );
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
            execution_mode: 'distributed',
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
            ready_workers: verifier.readyWorkers + 1,
            active_workers: verifier.activeWorkers + coordinatorActive,
            worker_count: verifier.workerCount + 1,
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
        this.poolOwned = true;
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
        const candidateBatchSize = distributedCandidateBatchSize(
          this.wasm.distributed_progress().candidateFamilyCount,
          effectiveVerifierCount
        );
        const produced = this.wasm.distributed_produce(
          PRODUCER_WORK_BUDGET,
          candidateBatchSize
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
          this.poolOwned = true;
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
          // enqueue() waits for any ready verifier. Do not join the complete
          // initialization set here: a fast worker must start consuming while
          // slower workers are still loading the same immutable executable.
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
        if (performance.now() - lastHostYield < HOST_YIELD_BUDGET_MS) continue;
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
        progressPhase = 'draining';
        emitProgress();
        await this.pool.waitForIdle();
        this.requireActive();
      }
      if (plan.verificationRequired) {
        progressPhase = 'postprocessing';
        emitProgress();
        finishedVerifierCount = await this.pool.finish(
          (partial) => this.wasm.distributed_merge_partial(partial),
          { readySubset: true }
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
      this.releaseCompletedRun();
    }
  }

  cancel() {
    if (this.cancelled && this.released) return;
    this.cancelled = true;
    this.acquireGeneration += 1;
    this.acquireController?.abort();
    this.releaseFailedRun();
  }

  dispose() {
    this.cancelled = true;
    this.acquireGeneration += 1;
    this.acquireController?.abort();
    this.releaseFailedRun();
  }

  private requireActive() {
    if (this.cancelled) throw new Error('distributed search cancelled');
  }

  private releaseFailedRun() {
    if (this.released) return;
    this.released = true;
    if (this.poolOwned) {
      try {
        this.wasm.distributed_cancel();
      } catch {
        // Reset and worker termination below are the fail-closed fallback.
      }
      this.pool.cancel();
    }
    this.resetOwnedCoordinator();
    this.releaseExecutionLease();
  }

  private releaseCompletedRun() {
    if (this.released) return;
    this.released = true;
    this.resetOwnedCoordinator();
    this.releaseExecutionLease();
  }

  private releaseExecutionLease() {
    this.acquireController = null;
    this.poolOwned = false;
    const lease = this.resourceLease;
    this.resourceLease = null;
    if (!lease || lease.isReleased()) return;
    lease.release();
  }

  private resetOwnedCoordinator() {
    if (!this.coordinatorOwned) return;
    this.coordinatorOwned = false;
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

function distributedCoordinatorIsActive(
  phase: ClearraSearchProgressTelemetry['phase'],
  producerComplete: boolean
): number {
  if (phase === 'initializing') return 1;
  if (phase === 'searching' && !producerComplete) return 1;
  if (phase === 'merging') return 1;
  return 0;
}

function distributedCandidateBatchSize(
  candidateFamilyCount: string | null,
  verifierCount: number
): number {
  if (verifierCount <= 1) return MAX_CANDIDATE_BATCH_SIZE;
  if (candidateFamilyCount === null) return 1;
  try {
    const candidates = BigInt(candidateFamilyCount);
    if (candidates <= 0n) return 1;
    const targetBatches = BigInt(verifierCount * TARGET_BATCHES_PER_VERIFIER);
    const balanced = (candidates + targetBatches - 1n) / targetBatches;
    return Number(
      balanced > BigInt(MAX_CANDIDATE_BATCH_SIZE)
        ? BigInt(MAX_CANDIDATE_BATCH_SIZE)
        : balanced
    );
  } catch {
    return 1;
  }
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

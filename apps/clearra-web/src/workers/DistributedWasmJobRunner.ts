// SRP rationale: this host's single change reason is a distributed product job's lifecycle. Source
// dispatch, exact-cover continuations, progress, and terminal drain stay here
// because each transition must use the same admitted lease and cancellation
// owner; verifier execution, durable commits, and solver proofs are delegated.
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
import { createWorkerHostYield } from './workerHostYield';
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
const MAX_CANDIDATE_BATCH_SIZE = 2_048;
const STREAMING_CANDIDATE_BATCH_SIZE = 1_024;
const STREAMING_STARTUP_BATCH_SIZE = 64;
const TARGET_BATCHES_PER_VERIFIER = 4;
const HOST_YIELD_BUDGET_MS = 8;
const PROGRESS_REFRESH_MS = 50;
const SHARED_RESOURCE_WAIT_TIMEOUT_MS = 5_000;
const MAX_MINIMUM_PROFILE_WAVES = 128;
// Unlike remote compute-only verifiers, this worker owns the serialized durable
// journal. Yield its timer lane every quantum so remote dispatch/ACK work does
// not wait behind eight self-refilling posted-message compute continuations.
const yieldToWorkerHost = createWorkerHostYield('timer');
const sharedVerifierPool = new ClearraVerifierPool();

export type MinimumManagerPolicy = 'auto' | 'shared' | 'dedicated';

/** Pure topology choice; neither this decision nor a ready worker is a memory
 * admission. The guarded ABI must admit the actual whole-job lease first. */
export function minimumManagerTopology(
  computeWorkers: number, logicalProcessors: number, guarded: boolean,
  localAvailable: boolean, policy: MinimumManagerPolicy = 'auto'
): { controlOnly: boolean; remoteWorkers: number; partitions: number } {
  const workers = Math.max(1, Math.floor(computeWorkers));
  const logical = Math.max(1, Math.floor(logicalProcessors));
  const controlOnly = guarded && localAvailable && policy !== 'shared' && workers > 1 && workers < logical;
  return {
    controlOnly,
    remoteWorkers: Math.max(1, workers - Number(!controlOnly)),
    partitions: Math.max(1, workers - Number(!localAvailable)) * 4
  };
}

interface MinimumWaveProfile {
  wave: number;
  query_prepare_ms: number;
  upstream_gap_ms: number;
  initialize_all_ready_ms: number | null;
  first_receipt_ms: number | null;
  last_receipt_ms: number | null;
  remote_admission_wait_ms: number;
  remote_drain_ms: number;
  task_round_trip_max_ms: number;
  coordinator_compute_ms: number;
  coordinator_tasks: number;
  coordinator_slices: number;
  remote_tasks: number;
  sampled_active_min: number | null;
  sampled_active_max: number | null;
  elapsed_ms: number;
}

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
    private readonly resourceWaitTimeoutMs = SHARED_RESOURCE_WAIT_TIMEOUT_MS,
    private readonly minimumManagerPolicy: MinimumManagerPolicy = 'auto'
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
    onEvent: (event: ClearraWasmWorkerEvent) => void,
    options: { transportProfile?: boolean } = {}
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
    let transportProfiling = profilingActive || options.transportProfile === true;
    const captureSchedulingProfile = transportProfiling;
    const hostStarted = transportProfiling ? performance.now() : 0;
    const hostTiming: Record<string, number> = {
      source_ms: 0, drain_ms: 0, verifier_finish_ms: 0, finalize_ms: 0, parse_ms: 0,
      source_produce_ms: 0, source_produce_calls: 0, source_enqueue_ms: 0,
      source_merge_ms: 0, source_merge_calls: 0
    };
    let stageStarted = hostStarted;
    const mergePartial = (partial: ArrayBuffer) => {
      if (!captureSchedulingProfile) return this.wasm.distributed_merge_partial(partial);
      const started = performance.now();
      try { return this.wasm.distributed_merge_partial(partial); }
      finally {
        hostTiming.source_merge_calls += 1;
        hostTiming.source_merge_ms += performance.now() - started;
      }
    };
    if (transportProfiling) this.pool.beginTransportProfile();
    const verifierCount = plan.verificationRequired
      ? Math.max(1, plan.workerCount - 1)
      : 0;
    let effectiveVerifierCount = verifierCount;
    let dispatchedBatches = 0;
    let finishedVerifierCount = 0;
    let lastHostYield = performance.now();
    let progressPhase: ClearraSearchProgressTelemetry['phase'] = 'initializing';
    let producerComplete = false;
    let finalizingProducer: ClearraDistributedCoreProgress | null = null;
    let parallelCompletionActive = false;
    let parallelCoordinatorActive = false;
    let parallelCoordinatorComputes = true;
    const minimumWaves: MinimumWaveProfile[] = [];
    let minimumWaveCount = 0;
    let activeMinimumWave: MinimumWaveProfile | null = null;
    let previousMinimumWaveEnd: number | null = null;
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
      const producer = finalizingProducer ?? this.wasm.distributed_progress();
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
      const proof = parallelCompletionActive ? this.pool.progressSnapshot() : null;
      if (proof && activeMinimumWave) {
        const active = proof.activeWorkers + Number(parallelCoordinatorActive);
        activeMinimumWave.sampled_active_min = Math.min(activeMinimumWave.sampled_active_min ?? active, active);
        activeMinimumWave.sampled_active_max = Math.max(activeMinimumWave.sampled_active_max ?? active, active);
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
            ready_workers: proof ? proof.readyWorkers + Number(parallelCoordinatorComputes) : verifier.readyWorkers + 1,
            active_workers: proof
              ? proof.activeWorkers + Number(parallelCoordinatorActive)
              : verifier.activeWorkers + coordinatorActive,
            // This is admitted capacity, not the temporarily ready/finalizing
            // subset. Keep the denominator stable through every phase.
            worker_count: plan.workerCount,
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
          effectiveVerifierCount,
          dispatchedBatches
        );
        const produceStarted = captureSchedulingProfile ? performance.now() : 0;
        const produced = this.wasm.distributed_produce(
          PRODUCER_WORK_BUDGET,
          candidateBatchSize
        );
        if (captureSchedulingProfile) {
          hostTiming.source_produce_calls += 1;
          hostTiming.source_produce_ms += performance.now() - produceStarted;
        }
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
          const enqueueStarted = captureSchedulingProfile ? performance.now() : 0;
          await this.pool.enqueue(produced.batch, mergePartial);
          if (captureSchedulingProfile) hostTiming.source_enqueue_ms += performance.now() - enqueueStarted;
          dispatchedBatches += 1;
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
      if (captureSchedulingProfile) {
        const now = performance.now();
        hostTiming.source_ms = now - stageStarted;
        stageStarted = now;
      }
      if (plan.verificationRequired) {
        if (!verifierInitialization) {
          throw new Error('distributed producer completed without worker initialization');
        }
        progressPhase = 'draining';
        emitProgress();
        await this.pool.waitForIdle();
        this.requireActive();
      }
      if (captureSchedulingProfile) {
        const now = performance.now();
        hostTiming.drain_ms = now - stageStarted;
        stageStarted = now;
      }
      if (plan.verificationRequired) {
        progressPhase = 'postprocessing';
        emitProgress();
        finishedVerifierCount = await this.pool.finish(
          mergePartial,
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
      if (captureSchedulingProfile) {
        const now = performance.now();
        hostTiming.verifier_finish_ms = now - stageStarted;
        stageStarted = now;
      }
      progressPhase = 'merging';
      // Starting the product continuation moves the coordinator out of the
      // producer ABI slot. Preserve its exact final counters for this phase.
      finalizingProducer = this.wasm.distributed_progress();
      emitProgress();
      const workersUsed = plan.verificationRequired ? finishedVerifierCount + 1 : 1;
      let finalEvents: string | null;
      if (this.wasm.distributed_finish_start && this.wasm.distributed_finish_advance) {
        let lastCompletionYield = performance.now();
        const localExactAvailable = Boolean(this.wasm.distributed_finish_parallel_local_start &&
          this.wasm.distributed_finish_parallel_local_advance);
        const idleAssistAvailable = Boolean(this.wasm.distributed_finish_parallel_assist &&
          this.wasm.distributed_finish_parallel_last_task_key && this.wasm.distributed_finish_parallel_redundant);
        finalEvents = this.wasm.distributed_finish_start(this.jobId, workersUsed);
        const guardedExact = Boolean(this.wasm.distributed_finish_parallel_configure &&
          this.wasm.distributed_finish_parallel_admit && this.wasm.distributed_finish_parallel_guarded_query);
        let minimumTopology = minimumManagerTopology(plan.workerCount,
          this.hostCapabilities.logicalProcessorCount, guardedExact, localExactAvailable, this.minimumManagerPolicy);
        let minimumAdmitted = false;
        const minimumGrant = this.resourceLease!.token.grant;
        if (guardedExact && plan.workerCount > 1 && finalEvents === null) {
          this.wasm.distributed_finish_parallel_configure!(this.jobId,
            minimumGrant.computeUnits, minimumGrant.memoryBytes);
        }
        while (finalEvents === null) {
          if (plan.workerCount > 1 &&
            this.wasm.distributed_finish_parallel_prepare &&
            this.wasm.distributed_finish_parallel_task &&
            this.wasm.distributed_finish_parallel_merge) {
            const preparationStarted = captureSchedulingProfile ? performance.now() : 0;
            let query = this.wasm.distributed_finish_parallel_prepare(
              this.jobId, minimumTopology.partitions
            );
            if (query !== null) {
              if (guardedExact) {
                if (!minimumAdmitted) {
                  minimumAdmitted = this.wasm.distributed_finish_parallel_admit!(this.jobId,
                    minimumTopology.remoteWorkers, minimumTopology.controlOnly, minimumGrant.computeUnits, minimumGrant.memoryBytes);
                  if (!minimumAdmitted && minimumTopology.controlOnly) {
                    minimumTopology = minimumManagerTopology(plan.workerCount,
                      this.hostCapabilities.logicalProcessorCount, true, localExactAvailable, 'shared');
                    minimumAdmitted = this.wasm.distributed_finish_parallel_admit!(this.jobId,
                      minimumTopology.remoteWorkers, false, minimumGrant.computeUnits, minimumGrant.memoryBytes);
                  }
                  if (!minimumAdmitted) throw new Error('minimum parallel whole-job memory admission unavailable');
                }
                // A late remote initialization failure is fail-closed. Never
                // replace a query after ready-first dispatch has issued tasks.
                query = this.wasm.distributed_finish_parallel_guarded_query!(this.jobId);
              }
              const waveStarted = captureSchedulingProfile ? performance.now() : 0;
              minimumWaveCount += 1;
              const wave: MinimumWaveProfile | null = captureSchedulingProfile && minimumWaves.length < MAX_MINIMUM_PROFILE_WAVES ? {
                wave: minimumWaveCount, query_prepare_ms: waveStarted - preparationStarted,
                upstream_gap_ms: previousMinimumWaveEnd === null ? 0 : preparationStarted - previousMinimumWaveEnd,
                initialize_all_ready_ms: null, first_receipt_ms: null, last_receipt_ms: null,
                remote_admission_wait_ms: 0, remote_drain_ms: 0, task_round_trip_max_ms: 0,
                coordinator_compute_ms: 0, coordinator_tasks: 0, coordinator_slices: 0,
                remote_tasks: 0, sampled_active_min: null, sampled_active_max: null, elapsed_ms: 0
              } : null;
              if (wave) minimumWaves.push(wave);
              activeMinimumWave = wave;
              // The same durable offer/start/result journal protects Geometry
              // and exact proof tasks. TypeScript neither decides minimum k
              // nor turns transport completion into an UNSAT proof.
              this.poolOwned = true;
              parallelCompletionActive = true;
              parallelCoordinatorComputes = localExactAvailable && !minimumTopology.controlOnly;
              const initialization = this.pool.initialize(
                query, minimumTopology.remoteWorkers,
                this.wasm.compiled_module(), this.lifecycleOwnerId,
                'atomic-task', this.hostCapabilities, 'exact-at-most'
              );
              void initialization.catch(() => undefined);
              if (wave) void initialization.then(() => {
                wave.initialize_all_ready_ms = performance.now() - waveStarted;
              }, () => undefined);
              emitProgress();
              let proofCancellationRequested = false;
              let releaseLocalDrain!: () => void;
              const localDrained = new Promise<void>((resolve) => { releaseLocalDrain = resolve; });
              const cancelSatisfiedSiblings = () => {
                if (this.wasm.distributed_finish_parallel_found?.(this.jobId)) {
                  if (!proofCancellationRequested) {
                    proofCancellationRequested = true;
                    this.pool.cancelExactTasks();
                  }
                  return;
                }
                if (idleAssistAvailable) this.pool.cancelRedundantExactTasks((key) =>
                  this.wasm.distributed_finish_parallel_redundant!(this.jobId, key));
              };
              const dispatchRemoteTasks = async () => {
                for (;;) {
                  this.requireActive();
                  const admissionStarted = wave ? performance.now() : 0;
                  let taskIssued = admissionStarted;
                  const dispatched = await this.pool.enqueueFromSource(() => {
                    this.requireActive();
                    let task = this.wasm.distributed_finish_parallel_task!(this.jobId);
                    if (task === null && idleAssistAvailable &&
                      this.wasm.distributed_finish_parallel_assist!(this.jobId, 64)) {
                      task = this.wasm.distributed_finish_parallel_task!(this.jobId);
                      if (task === null) throw new Error('core committed assistance without an issued child');
                    }
                    if (wave && task !== null) {
                      wave.remote_tasks += 1;
                      taskIssued = performance.now();
                    }
                    return task;
                  }, (receipt) => {
                    if (wave) {
                      const now = performance.now();
                      wave.first_receipt_ms ??= now - waveStarted;
                      wave.last_receipt_ms = now - waveStarted;
                      wave.task_round_trip_max_ms = Math.max(wave.task_round_trip_max_ms, now - taskIssued);
                    }
                    this.wasm.distributed_finish_parallel_merge!(this.jobId, receipt);
                    cancelSatisfiedSiblings();
                  }, idleAssistAvailable ? () => this.wasm.distributed_finish_parallel_last_task_key!(this.jobId) : undefined);
                  if (wave) wave.remote_admission_wait_ms += performance.now() - admissionStarted;
                  if (!dispatched) break;
                }
              };
              const remoteTasks = (async () => {
                await dispatchRemoteTasks();
                // A growing local shard can decline its memory admission and
                // return the same issued task. Keep the pool alive, then drain
                // that retry before closing the source-bound query frontier.
                await localDrained;
                this.requireActive();
                await dispatchRemoteTasks();
                const drainStarted = wave ? performance.now() : 0;
                await this.pool.completeAtomicTasks();
                if (wave) wave.remote_drain_ms = performance.now() - drainStarted;
              })();
              const localTasks = (async () => {
                let lastLocalYield = performance.now();
                try {
                  if (!localExactAvailable || minimumTopology.controlOnly) return;
                  for (;;) {
                    this.requireActive();
                    if (!parallelCoordinatorActive) {
                      const started = wave ? performance.now() : 0;
                      parallelCoordinatorActive = this.wasm.distributed_finish_parallel_local_start!(this.jobId);
                      if (!parallelCoordinatorActive && idleAssistAvailable &&
                        this.wasm.distributed_finish_parallel_assist!(this.jobId, 64)) {
                        parallelCoordinatorActive = this.wasm.distributed_finish_parallel_local_start!(this.jobId);
                      }
                      if (wave) {
                        wave.coordinator_compute_ms += performance.now() - started;
                        wave.coordinator_tasks += Number(parallelCoordinatorActive);
                      }
                      if (!parallelCoordinatorActive) break;
                    }
                    const sliceStarted = wave ? performance.now() : 0;
                    const drained = this.wasm.distributed_finish_parallel_local_advance!(this.jobId, 128);
                    if (wave) {
                      wave.coordinator_compute_ms += performance.now() - sliceStarted;
                      wave.coordinator_slices += 1;
                    }
                    if (drained) {
                      parallelCoordinatorActive = false;
                      cancelSatisfiedSiblings();
                    }
                    // The coordinator computes while remote workers initialize,
                    // consume, or seal receipts, but yields a host quantum so
                    // ready verifiers are dispatched without waiting for it.
                    if (performance.now() - lastLocalYield >= HOST_YIELD_BUDGET_MS) {
                      await yieldToWorkerHost();
                      lastLocalYield = performance.now();
                    }
                  }
                } finally {
                  parallelCoordinatorActive = false;
                  releaseLocalDrain();
                }
              })();
              await Promise.all([remoteTasks, localTasks]);
              this.requireActive();
              parallelCompletionActive = false;
              activeMinimumWave = null;
              if (captureSchedulingProfile) {
                previousMinimumWaveEnd = performance.now();
                if (wave) wave.elapsed_ms = previousMinimumWaveEnd - waveStarted;
              }
            }
          }
          // Keep the merged source in the shared product continuation, yielding
          // between exact proof/portfolio slices for cancellation and progress.
          if (performance.now() - lastCompletionYield >= HOST_YIELD_BUDGET_MS) {
            await yieldToWorkerHost();
            lastCompletionYield = performance.now();
          }
          this.requireActive();
          finalEvents = this.wasm.distributed_finish_advance(this.jobId, 128);
        }
      } else {
        // Compatibility for pre-continuation adapters and explicit fixtures.
        finalEvents = this.wasm.distributed_finish(this.jobId, workersUsed);
      }
      this.requireActive();
      const parseStarted = captureSchedulingProfile ? performance.now() : 0;
      if (captureSchedulingProfile) hostTiming.finalize_ms = parseStarted - stageStarted;
      const events = JSON.parse(finalEvents) as ClearraWasmWorkerEvent[];
      if (captureSchedulingProfile) hostTiming.parse_ms = performance.now() - parseStarted;
      if (profilingActive && this.wasm.profile_finish) {
        searchProfile = this.wasm.profile_finish();
        profilingActive = false;
      }
      if (minimumWaves.length > 0) {
        searchProfile = {
          ...(searchProfile !== null && typeof searchProfile === 'object' && !Array.isArray(searchProfile)
            ? searchProfile : { core_profile: searchProfile }),
          minimum_parallel: {
            wave_count: minimumWaveCount,
            omitted_wave_count: minimumWaveCount - minimumWaves.length,
            waves: minimumWaves
          }
        };
      }
      if (transportProfiling) {
        const transport = this.pool.finishTransportProfile();
        transportProfiling = false;
        searchProfile = {
          ...(searchProfile !== null && typeof searchProfile === 'object' && !Array.isArray(searchProfile)
            ? searchProfile : { core_profile: searchProfile }),
          verifier_transport: transport
        };
      }
      if (captureSchedulingProfile) {
        searchProfile = {
          ...(searchProfile !== null && typeof searchProfile === 'object' && !Array.isArray(searchProfile)
            ? searchProfile : { core_profile: searchProfile }),
          host_execution: { ...hostTiming, run_to_emit_ms: performance.now() - hostStarted }
        };
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
      if (transportProfiling) this.pool.finishTransportProfile();
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
    if (this.cancelled || this.released) throw new Error('distributed search cancelled');
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
  verifierCount: number,
  dispatchedBatches: number
): number {
  if (verifierCount <= 1) return MAX_CANDIDATE_BATCH_SIZE;
  // Each batch carries a durable publish/start/result protocol. A streaming
  // unknown family must not turn every single candidate into a disk-backed
  // delegation. The producer still yields after its bounded geometry quantum
  // and may return a shorter batch, so this does not wait for a full catalog.
  // Large streams need thousands, not millions, of durable transactions; tiny
  // known families retain four dispatch waves per verifier below.
  // Fill four modest waves before using throughput-sized packets. An unknown
  // stream of a few hundred candidates used to fit in one 1,024-item packet,
  // leaving all other ready verifiers idle. One wave also left medium streams
  // in only a few large trailing packets. The bounded startup keeps those
  // streams divisible while long streams still amortize durable delegation
  // with the original large packets. Exact family sizes take precedence below.
  const streamingBatchSize = dispatchedBatches < verifierCount * TARGET_BATCHES_PER_VERIFIER
    ? STREAMING_STARTUP_BATCH_SIZE
    : STREAMING_CANDIDATE_BATCH_SIZE;
  if (candidateFamilyCount === null) return streamingBatchSize;
  try {
    const candidates = BigInt(candidateFamilyCount);
    if (candidates <= 0n) return streamingBatchSize;
    const targetBatches = BigInt(verifierCount * TARGET_BATCHES_PER_VERIFIER);
    const balanced = (candidates + targetBatches - 1n) / targetBatches;
    return Number(
      balanced > BigInt(MAX_CANDIDATE_BATCH_SIZE)
        ? BigInt(MAX_CANDIDATE_BATCH_SIZE)
        : balanced
    );
  } catch {
    return streamingBatchSize;
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

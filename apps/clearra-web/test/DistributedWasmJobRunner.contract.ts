// SRP rationale: these contracts have one change reason: the distributed product
// coordinator's lifecycle against controlled workers. Scheduling, telemetry, cancellation,
// and completion assertions protect the same job/lease transition contract;
// mathematical solver and durable-journal correctness are tested by their owners.
import assert from 'node:assert/strict';

import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';
import { workspaceProgressDetail } from '../../../packages/clearra-ui/src/lib/workspace/workspaceI18n.ts';
import { buildWorkspaceProgressModel } from '../../../packages/clearra-ui/src/lib/workspace/workspaceProgressModel.ts';

import type { ClearraVerifierPoolProgress } from '../src/workers/ClearraVerifierPool.ts';
import { DistributedWasmJobRunner, minimumManagerTopology, type MinimumManagerPolicy } from '../src/workers/DistributedWasmJobRunner.ts';
import { ClearraProductJobRunner } from '../src/workers/ClearraProductJobRunner.ts';
import {
  SharedExecutionAvailabilityError,
  SharedExecutionResourceAuthority,
  type SharedExecutionResourceLease
} from '../src/workers/SharedExecutionResourceAuthority.ts';
import {
  normalizeWasmU32,
  type ClearraDistributedPlan,
  type ClearraWasmModule
} from '../src/workers/clearraWasmRuntime.ts';

const completedProgress: ClearraVerifierPoolProgress = {
  candidatesVerified: 12,
  buildNodes: 34,
  coverageChecks: 56,
  availability: verifierFlags(true),
  exactness: verifierFlags(true),
  readyWorkers: 1,
  activeWorkers: 1,
  workerCount: 1,
  oldestBatchMs: 78
};
const emptyProgress: ClearraVerifierPoolProgress = {
  candidatesVerified: 0,
  buildNodes: 0,
  coverageChecks: 0,
  availability: verifierFlags(false),
  exactness: verifierFlags(false),
  readyWorkers: 0,
  activeWorkers: 0,
  workerCount: 0,
  oldestBatchMs: 0
};
let poolFinished = false;
const pool = {
  beginTransportProfile() {},
  finishTransportProfile() { return { schema: 'clearra.verifier-transport-profile.v1', timings: {} }; },
  async initialize() {},
  async enqueue() {},
  async enqueueFromSource(takeTask: () => ArrayBuffer | null, consume: (receipt: ArrayBuffer) => void) {
    const task = takeTask();
    if (task === null) return false;
    await (this.enqueue as (task: ArrayBuffer, consume: (receipt: ArrayBuffer) => void) => Promise<void>)(task, consume);
    return true;
  },
  async waitForIdle() {},
  async finish() {
    poolFinished = true;
    return 1;
  },
  progressSnapshot() {
    return poolFinished ? emptyProgress : completedProgress;
  },
  cancel() {}
};

let workersUsed = 0;
const distributedProducerCalls: Array<{ workBudget: number; batchSize: number }> = [];
const wasm = {
  compiled_module: () => ({}) as WebAssembly.Module,
  distributed_produce: (workBudget: number, batchSize: number) => {
    distributedProducerCalls.push({ workBudget, batchSize });
    return { status: 'completed' as const };
  },
  distributed_progress: () => ({
    geometryNodes: 1,
    candidateCount: 2,
    candidateFamilyCount: '3',
    buildNodes: 4,
    coverageChecks: 5,
    passIndex: 1,
    passCount: 1,
    layerIndex: 0,
    layerCount: 0,
    layerDone: 0,
    layerTotal: 0,
    availability: coreFlags(true),
    exactness: coreFlags(true)
  }),
  distributed_finish: (jobId: number, count: number) => {
    workersUsed = count;
    return JSON.stringify([
      {
        schema_version: 1,
        runtime: 'clearra-wasm',
        event: 'failed',
        job_id: jobId,
        diagnostics: { diagnostics: [] }
      }
    ]);
  },
  distributed_merge_partial() {},
  distributed_cancel() {},
  distributed_reset() {}
} as unknown as ClearraWasmModule;
const plan: ClearraDistributedPlan = {
  mode: 'cpu-multi',
  workerCount: 2,
  requestedBackend: 'cpu',
  selectedBackend: 'wasm-cpu',
  fallbackUsed: false,
  fallbackReason: null,
  workerInitialization: new ArrayBuffer(0),
  deferredInitialization: false,
  verificationRequired: true,
  tilingGeometryParallel: false
};
const events: ClearraWasmWorkerEvent[] = [];

await new DistributedWasmJobRunner(
  wasm,
  41,
  'merging-progress-owner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  pool as never
).run(
  'clearra pc minimals --lines 4 --backend cpu --workers 2',
  plan,
  (event) => events.push(event)
);

const distributedSearching = events.find(
  (event) => event.event === 'progress' && event.progress.telemetry?.phase === 'searching'
);
assert.ok(distributedSearching && distributedSearching.event === 'progress');
assert.equal(
  distributedSearching.progress.telemetry?.execution_mode,
  'distributed',
  'distributed search progress must identify its execution mode for the shared UI watchdog'
);

const merging = events.find(
  (event) => event.event === 'progress' && event.progress.telemetry?.phase === 'merging'
);
assert.ok(merging && merging.event === 'progress');
assert.equal(merging.progress.telemetry?.candidates_verified, 12);
assert.equal(merging.progress.telemetry?.build_nodes, 34);
assert.equal(merging.progress.telemetry?.coverage_checks, 56);
assert.equal(merging.progress.telemetry?.availability.candidates_verified, true);
assert.equal(merging.progress.telemetry?.exactness.candidates_verified, true);
assert.equal(merging.progress.telemetry?.active_workers, 1);
assert.equal(merging.progress.telemetry?.worker_count, 2);
assert.equal(workersUsed, 2);
assert.deepEqual(
  distributedProducerCalls,
  [{ workBudget: 2_048, batchSize: 2_048 }],
  'the browser coordinator must return to the host between bounded geometry slices'
);

// Draining and finalizing are distinct worker tasks: a finalizer wave can
// legitimately become busy after all candidate-consume leases have drained.
// Preserve that transition, and never count stale verifiers in coordinator merge.
{
  const lifecycleEvents: ClearraWasmWorkerEvent[] = [];
  let snapshot = { ...completedProgress, readyWorkers: 10, activeWorkers: 10, workerCount: 10 };
  let observeFinalizerWave!: () => void;
  const finalizerWaveObserved = new Promise<void>((resolve) => { observeFinalizerWave = resolve; });
  let timeout: ReturnType<typeof setTimeout> | undefined;
  const lifecycleRunner = new DistributedWasmJobRunner({
    ...wasm,
    distributed_produce: () => {
      snapshot = { ...snapshot, activeWorkers: 2 };
      return { status: 'completed' as const };
    }
  } as ClearraWasmModule, 412, 'finalizer-lifecycle-owner', {
    logicalProcessorCount: 12,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  }, {
    ...pool,
    async waitForIdle() { snapshot = { ...snapshot, activeWorkers: 0 }; },
    async finish() {
      snapshot = { ...snapshot, activeWorkers: 10 };
      await finalizerWaveObserved;
      // Deliberately leave the old pool snapshot busy. The runner must clear
      // finalizer activity at the completed ownership boundary before merging.
      return 10;
    },
    progressSnapshot: () => snapshot
  } as never);
  try {
    await Promise.race([
      lifecycleRunner.run(
        'clearra build-probability --patterns P7 --workers 11',
        { ...plan, workerCount: 11 },
        (event) => {
          lifecycleEvents.push(event);
          if (event.event === 'progress' && event.progress.telemetry?.phase === 'postprocessing' &&
            event.progress.telemetry.active_workers === 10) observeFinalizerWave();
        }
      ),
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error('finalizer lifecycle progress did not settle')), 2_000);
      })
    ]);
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
    lifecycleRunner.cancel();
  }
  const phaseCounts = lifecycleEvents.flatMap((event) =>
    event.event === 'progress' && event.progress.telemetry
      ? [[event.progress.telemetry.phase, event.progress.telemetry.active_workers]]
      : []
  );
  for (const expected of [['searching', 11], ['draining', 2], ['postprocessing', 0], ['postprocessing', 10], ['merging', 1]]) {
    assert.ok(phaseCounts.some((entry) => entry[0] === expected[0] && entry[1] === expected[1]),
      `worker lifecycle must report ${expected.join(':')}`);
  }
  assert.ok(lifecycleEvents.every((event) => event.event !== 'progress' ||
    event.progress.telemetry?.worker_count === 11), 'admitted maximum remains eleven across every phase');
}

const streamingProducerCalls: number[] = [];
await new DistributedWasmJobRunner(
  {
    ...wasm,
    distributed_progress: () => ({ ...wasm.distributed_progress(), candidateFamilyCount: null }),
    distributed_produce: (_workBudget: number, batchSize: number) => {
      streamingProducerCalls.push(batchSize);
      return { status: 'completed' as const };
    }
  } as ClearraWasmModule,
  410,
  'streaming-batch-owner',
  {
    logicalProcessorCount: 4,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  { ...pool } as never
).run('clearra pc --lines 4 --workers 4', { ...plan, workerCount: 4 }, () => undefined);
assert.deepEqual(streamingProducerCalls, [64], 'unknown streaming Geometry must fill the first worker wave');

const streamingWaveBatchSizes: number[] = [];
await new DistributedWasmJobRunner({
  ...wasm,
  distributed_progress: () => ({ ...wasm.distributed_progress(), candidateFamilyCount: null }),
  distributed_produce: (_workBudget: number, batchSize: number) => {
    streamingWaveBatchSizes.push(batchSize);
    return streamingWaveBatchSizes.length <= 42
      ? { status: 'batch' as const, batch: new ArrayBuffer(0) }
      : { status: 'completed' as const };
  }
} as ClearraWasmModule, 411, 'eleven-worker-streaming-wave', {
  logicalProcessorCount: 12,
  webGpuAvailable: false,
  crossOriginIsolated: false,
  transferByteCap: 32 * 1024 * 1024
}, { ...pool } as never).run(
  'clearra pc --lines 4 --workers 11', { ...plan, workerCount: 11 }, () => undefined
);
assert.deepEqual(
  streamingWaveBatchSizes,
  [...Array(40).fill(64), 1_024, 1_024, 1_024],
  'ten verifiers share four modest startup waves, switching to large packets only after forty dispatches'
);

for (const initiallyKnown of [false, true]) {
  const knownFamilyBatchSizes: number[] = [];
  await new DistributedWasmJobRunner({
    ...wasm,
    distributed_progress: () => ({
      ...wasm.distributed_progress(),
      candidateFamilyCount: initiallyKnown || knownFamilyBatchSizes.length > 0 ? '2260' : null
    }),
    distributed_produce: (_workBudget: number, batchSize: number) => {
      knownFamilyBatchSizes.push(batchSize);
      return knownFamilyBatchSizes.length <= 42
        ? { status: 'batch' as const, batch: new ArrayBuffer(0) }
        : { status: 'completed' as const };
    }
  } as ClearraWasmModule, initiallyKnown ? 413 : 412, `known-family-${initiallyKnown}`, {
    logicalProcessorCount: 12,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  }, { ...pool } as never).run(
    'clearra build-probability --height 4 --patterns P7 --workers 11',
    { ...plan, workerCount: 11 },
    () => undefined
  );
  assert.deepEqual(
    knownFamilyBatchSizes,
    [initiallyKnown ? 57 : 64, ...Array(42).fill(57)],
    'an exact 2,260-candidate family immediately uses its balanced 57-item cap regardless of the streaming threshold'
  );
}

const progressiveInitializationGate = new Promise<void>(() => undefined);
let observeProgressiveEnqueue: () => void = () => undefined;
const progressiveEnqueueObserved = new Promise<void>((resolve) => {
  observeProgressiveEnqueue = resolve;
});
let progressiveProducerStep = 0;
const progressivePool = {
  initialize() {
    return progressiveInitializationGate;
  },
  async enqueue() {
    observeProgressiveEnqueue();
  },
  async waitForIdle() {},
  async finish(
    _consumePartial: (partial: ArrayBuffer) => void,
    options?: { readySubset?: boolean }
  ) {
    assert.equal(
      options?.readySubset,
      true,
      'producer completion must finish the ready verifier subset'
    );
    return 1;
  },
  progressSnapshot() {
    return completedProgress;
  },
  cancel() {}
};
const progressiveWasm = {
  ...wasm,
  distributed_produce() {
    progressiveProducerStep += 1;
    return progressiveProducerStep === 1
      ? { status: 'batch' as const, batch: new ArrayBuffer(0) }
      : { status: 'completed' as const };
  }
} as unknown as ClearraWasmModule;
const progressiveRun = new DistributedWasmJobRunner(
  progressiveWasm,
  42,
  'progressive-initialization-owner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  progressivePool as never
).run(
  'clearra pc --lines 4 --backend cpu --workers 2',
  plan,
  () => undefined
);
let progressiveEnqueueTimeout: ReturnType<typeof setTimeout> | undefined;
try {
  await Promise.race([
    progressiveEnqueueObserved,
    new Promise<never>((_, reject) => {
      progressiveEnqueueTimeout = setTimeout(
        () => reject(new Error('ready verifier did not receive work during pool initialization')),
        2_000
      );
    })
  ]);
} finally {
  if (progressiveEnqueueTimeout !== undefined) clearTimeout(progressiveEnqueueTimeout);
}
await Promise.race([
  progressiveRun,
  new Promise<never>((_, reject) => {
    const timeout = setTimeout(
      () => reject(new Error('a never-ready verifier extended the terminal tail')),
      2_000
    );
    (timeout as unknown as { unref?: () => void }).unref?.();
  })
]);

const U32_MAX = normalizeWasmU32(-1);
assert.equal(U32_MAX, 0xffff_ffff);
let saturatedPoolFinished = false;
const saturatedProgress: ClearraVerifierPoolProgress = {
  candidatesVerified: U32_MAX,
  buildNodes: U32_MAX,
  coverageChecks: U32_MAX,
  availability: verifierFlags(false),
  exactness: verifierFlags(false),
  readyWorkers: 1,
  activeWorkers: 1,
  workerCount: 1,
  oldestBatchMs: 0
};
const saturatedPool = {
  async initialize() {},
  async enqueue() {},
  async waitForIdle() {},
  async finish() {
    saturatedPoolFinished = true;
    return 1;
  },
  progressSnapshot() {
    return saturatedPoolFinished ? emptyProgress : saturatedProgress;
  },
  cancel() {}
};
const saturatedWasm = {
  ...wasm,
  distributed_progress: () => ({
    geometryNodes: U32_MAX,
    candidateCount: U32_MAX,
    candidateFamilyCount: null,
    buildNodes: U32_MAX,
    coverageChecks: U32_MAX,
    passIndex: U32_MAX,
    passCount: U32_MAX,
    layerIndex: U32_MAX,
    layerCount: U32_MAX,
    layerDone: U32_MAX,
    layerTotal: U32_MAX,
    availability: coreFlags(false),
    exactness: coreFlags(false)
  })
} as unknown as ClearraWasmModule;
const saturatedEvents: ClearraWasmWorkerEvent[] = [];

await new DistributedWasmJobRunner(
  saturatedWasm,
  42,
  'saturated-progress-owner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  saturatedPool as never
).run('clearra pc --lines 1', plan, (event) => saturatedEvents.push(event));

const saturatedMerging = saturatedEvents.find(
  (event) => event.event === 'progress' && event.progress.telemetry?.phase === 'merging'
);
assert.ok(saturatedMerging && saturatedMerging.event === 'progress');
const saturatedTelemetry = saturatedMerging.progress.telemetry!;
assert.equal(saturatedTelemetry.geometry_nodes, U32_MAX);
assert.equal(saturatedTelemetry.candidates_verified, U32_MAX);
assert.equal(saturatedTelemetry.availability.geometry_nodes, false);
assert.equal(saturatedTelemetry.exactness.geometry_nodes, false);
assert.equal(saturatedTelemetry.availability.candidates_verified, false);
assert.equal(saturatedTelemetry.exactness.candidates_verified, false);
const unavailableDetail = workspaceProgressDetail('en', saturatedTelemetry);
assert.match(unavailableDetail, /—/u);
assert.doesNotMatch(unavailableDetail, /4[,.]?294[,.]?967[,.]?295/u);
const unavailableModel = buildWorkspaceProgressModel({
  profile: 'pc',
  status: 'running',
  progressLabel: '',
  progressDone: 0,
  progressTotal: 0,
  telemetry: {
    ...saturatedTelemetry,
    phase: 'searching',
    producer_complete: false
  }
});
assert.equal(unavailableModel.stages.find((stage) => stage.id === 'geometry')?.done, null);
assert.equal(unavailableModel.stages.find((stage) => stage.id === 'geometry')?.percent, null);
assert.equal(unavailableModel.stages.find((stage) => stage.id === 'verify')?.status, 'pending');

const approximateTelemetry = {
  ...saturatedTelemetry,
  phase: 'searching' as const,
  producer_complete: false,
  availability: telemetryFlags(true),
  exactness: telemetryFlags(false)
};
const approximateDetail = workspaceProgressDetail('en', approximateTelemetry);
assert.match(approximateDetail, /≈/u);
const approximateModel = buildWorkspaceProgressModel({
  profile: 'pc',
  status: 'running',
  progressLabel: '',
  progressDone: 0,
  progressTotal: 0,
  telemetry: approximateTelemetry
});
assert.equal(
  approximateModel.stages.find((stage) => stage.id === 'geometry')?.status,
  'running'
);
assert.equal(
  approximateModel.stages.find((stage) => stage.id === 'geometry')?.percent,
  null
);
assert.match(
  approximateModel.stages.find((stage) => stage.id === 'geometry')?.done ?? '',
  /^≈/u
);

const sharedCapacity = { computeUnits: 2, memoryBytes: 64n * 1024n * 1024n };
const sharedAuthority = new SharedExecutionResourceAuthority(sharedCapacity);
const existingOwner = sharedAuthority.tryAcquire('existing-pool-owner', sharedCapacity);
let deferredInitializeCount = 0;
let deferredPoolCancelCount = 0;
let deferredCoordinatorCancelCount = 0;
let deferredCoordinatorResetCount = 0;
const deferredPool = {
  async initialize() {
    deferredInitializeCount += 1;
  },
  async enqueue() {},
  async waitForIdle() {},
  async finish() { return 0; },
  progressSnapshot() { return emptyProgress; },
  cancel() {
    deferredPoolCancelCount += 1;
  }
};
const deferredWasm = {
  ...wasm,
  distributed_cancel() {
    deferredCoordinatorCancelCount += 1;
  },
  distributed_reset() {
    deferredCoordinatorResetCount += 1;
  }
} as unknown as ClearraWasmModule;
const deferredRunner = new DistributedWasmJobRunner(
  deferredWasm,
  43,
  'deferred-runner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  deferredPool as never,
  sharedAuthority,
  100
);
const deferredRun = deferredRunner.run(
  'clearra pc --lines 1',
  plan,
  () => undefined
);
deferredRunner.cancel();
await assert.rejects(
  deferredRun,
  (error: unknown) => error instanceof SharedExecutionAvailabilityError &&
    error.availability.state === 'cancelled' &&
    error.availability.reason === 'cancelled-by-caller'
);
assert.equal(deferredInitializeCount, 0);
assert.equal(deferredPoolCancelCount, 0);
assert.equal(deferredCoordinatorCancelCount, 0);
assert.equal(deferredCoordinatorResetCount, 0);
assert.deepEqual(sharedAuthority.snapshot().used, sharedCapacity);

const timedOutRunner = new DistributedWasmJobRunner(
  deferredWasm,
  44,
  'timed-out-runner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  deferredPool as never,
  sharedAuthority,
  1
);
await assert.rejects(
  timedOutRunner.run('clearra pc --lines 1', plan, () => undefined),
  (error: unknown) => error instanceof SharedExecutionAvailabilityError &&
    error.availability.state === 'deferred' &&
    error.availability.reason === 'shared-resource-contention'
);
assert.equal(deferredInitializeCount, 0);
assert.equal(deferredPoolCancelCount, 0);
assert.equal(deferredCoordinatorCancelCount, 0);
assert.equal(deferredCoordinatorResetCount, 0);
assert.deepEqual(sharedAuthority.snapshot().used, sharedCapacity);
existingOwner.release();
assert.deepEqual(sharedAuthority.snapshot().available, sharedCapacity);

const productAuthority = new SharedExecutionResourceAuthority(sharedCapacity);
const incumbentProductOwner = productAuthority.tryAcquire('incumbent-product-owner', sharedCapacity);
let productPrepareCount = 0;
let productResetCount = 0;
const productWasm = {
  ...wasm,
  distributed_prepare() {
    productPrepareCount += 1;
    return plan;
  },
  distributed_reset() {
    productResetCount += 1;
  }
} as unknown as ClearraWasmModule;
await assert.rejects(
  new ClearraProductJobRunner(
    productWasm,
    45,
    'deferred-product-runner',
    {
      logicalProcessorCount: 2,
      webGpuAvailable: false,
      crossOriginIsolated: false,
      transferByteCap: 32 * 1024 * 1024
    },
    productAuthority,
    1
  ).run('clearra pc --lines 1', () => undefined),
  (error: unknown) => error instanceof SharedExecutionAvailabilityError &&
    error.availability.state === 'deferred'
);
assert.equal(productPrepareCount, 0);
assert.equal(productResetCount, 0);
assert.deepEqual(productAuthority.snapshot().used, sharedCapacity);
incumbentProductOwner.release();

const serialAuthority = new SharedExecutionResourceAuthority(sharedCapacity);
let serialResetCount = 0;
let serialDrainCount = 0;
const serialPlan: ClearraDistributedPlan = {
  ...plan,
  mode: 'serial',
  workerCount: 1,
  workerInitialization: null,
  verificationRequired: false
};
const serialWasm = {
  ...wasm,
  distributed_prepare() {
    return serialPlan;
  },
  distributed_reset() {
    serialResetCount += 1;
  },
  start_job() {
    assert.deepEqual(
      serialAuthority.snapshot().used,
      sharedCapacity,
      'serial start must remain covered by the preparation lease'
    );
    return 46;
  },
  advance_job(jobId: number, workBudget: number) {
    assert.equal(jobId, 46);
    assert.equal(
      workBudget,
      2_048,
      'the serial browser fallback must use the same bounded host-sized work slice'
    );
    assert.deepEqual(
      serialAuthority.snapshot().used,
      sharedCapacity,
      'serial advance must remain covered by the preparation lease'
    );
    return 'failed' as const;
  },
  drain_job_events_json() {
    serialDrainCount += 1;
    if (serialDrainCount === 1) return '[]';
    return JSON.stringify([{
      schema_version: 1,
      runtime: 'clearra-wasm',
      event: 'failed',
      job_id: 46,
      diagnostics: { diagnostics: [] }
    }]);
  },
  cancel_job() {}
} as unknown as ClearraWasmModule;
const serialEvents: ClearraWasmWorkerEvent[] = [];
const serialTerminal = await new ClearraProductJobRunner(
  serialWasm,
  46,
  'serial-product-runner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  serialAuthority,
  100
).run('clearra pc --lines 1', (event) => serialEvents.push(event));
assert.equal(serialTerminal.event, 'failed');
assert.equal(serialResetCount, 1, 'preparation coordinator resets exactly once');
assert.deepEqual(
  serialEvents
    .filter((event) => event.event === 'progress')
    .map((event) => event.progress.telemetry?.phase),
  ['preparing', 'searching'],
  'serial handoff must leave preparing before the synchronous exact search starts'
);
const serialSearching = serialEvents.find(
  (event) => event.event === 'progress' && event.progress.telemetry?.phase === 'searching'
);
assert.ok(serialSearching && serialSearching.event === 'progress');
assert.equal(serialSearching.progress.telemetry?.active_workers, 1);
assert.equal(serialSearching.progress.telemetry?.worker_count, 1);
assert.equal(serialEvents.at(-1)?.event, 'failed', 'serial execution must converge on a terminal event');
assert.deepEqual(serialAuthority.snapshot().available, sharedCapacity);

const readyAuthority = new SharedExecutionResourceAuthority(sharedCapacity);
let readyFinishCount = 0;
let readyResetCount = 0;
let readySerialStartCount = 0;
const readyPlan: ClearraDistributedPlan = {
  ...serialPlan,
  mode: 'ready'
};
const readyWasm = {
  ...wasm,
  distributed_prepare() {
    return readyPlan;
  },
  distributed_finish(jobId: number, workersUsed: number) {
    readyFinishCount += 1;
    assert.equal(jobId, 47);
    assert.equal(workersUsed, 0, 'an App-terminal preparation did not run distributed workers');
    return JSON.stringify([
      {
        schema_version: 1,
        runtime: 'clearra-wasm',
        event: 'progress',
        job_id: jobId,
        progress: { done: 2, total: 2, label: 'AppResponse completed' }
      },
      {
        schema_version: 1,
        runtime: 'clearra-wasm',
        event: 'final_response',
        job_id: jobId,
        response: { status: 'validation-failed' }
      }
    ]);
  },
  distributed_reset() {
    readyResetCount += 1;
  },
  start_job() {
    readySerialStartCount += 1;
    throw new Error('a prepared App response must not be replayed serially');
  }
} as unknown as ClearraWasmModule;
const readyEvents: ClearraWasmWorkerEvent[] = [];
const readyTerminal = await new ClearraProductJobRunner(
  readyWasm,
  47,
  'ready-product-runner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  readyAuthority,
  100
).run('clearra pc --lines 4 --workers 2', (event) => readyEvents.push(event));
assert.equal(readyTerminal.event, 'final_response');
assert.equal(readyFinishCount, 1, 'the prepared App response has one terminal consumer');
assert.equal(readySerialStartCount, 0, 'the CLI command must not execute a second time');
assert.equal(readyResetCount, 1, 'the prepared terminal owner resets exactly once');
assert.deepEqual(
  readyEvents.map((event) => event.event),
  ['progress', 'started', 'progress', 'final_response'],
  'preparation, lifecycle start, and the authoritative terminal envelope are emitted once in order'
);
assert.deepEqual(readyAuthority.snapshot().available, sharedCapacity);

const serialCancellationAuthority = new SharedExecutionResourceAuthority(sharedCapacity);
let observeSerialAdvance: () => void = () => undefined;
const serialAdvanceObserved = new Promise<void>((resolve) => {
  observeSerialAdvance = resolve;
});
let serialCancellationRequested = false;
let serialCancellationTerminalEmitted = false;
const serialCancellationWasm = {
  ...serialWasm,
  start_job() {
    return 48;
  },
  advance_job() {
    observeSerialAdvance();
    return 'pending' as const;
  },
  cancel_job(jobId: number) {
    assert.equal(jobId, 48);
    serialCancellationRequested = true;
  },
  drain_job_events_json() {
    if (!serialCancellationRequested || serialCancellationTerminalEmitted) return '[]';
    serialCancellationTerminalEmitted = true;
    return JSON.stringify([{
      schema_version: 1,
      runtime: 'clearra-wasm',
      event: 'cancelled',
      job_id: 48,
      reason: 'cancelled-by-caller'
    }]);
  }
} as unknown as ClearraWasmModule;
const serialCancellationRunner = new ClearraProductJobRunner(
  serialCancellationWasm,
  48,
  'serial-product-cancellation-runner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  serialCancellationAuthority,
  100
);
const serialCancellationEvents: ClearraWasmWorkerEvent[] = [];
const serialCancellationRun = serialCancellationRunner.run(
  'clearra pc --lines 1',
  (event) => serialCancellationEvents.push(event)
);
await serialAdvanceObserved;
// The production worker itself keeps the event loop alive. This Node contract
// uses an unref'd MessageChannel, so retain one bounded handle while the
// cancellation turn crosses that host-yield boundary.
const serialCancellationKeepAlive = setTimeout(() => undefined, 1_000);
serialCancellationRunner.cancel();
const serialCancellationTerminal = await serialCancellationRun;
clearTimeout(serialCancellationKeepAlive);
assert.equal(serialCancellationTerminal.event, 'cancelled');
assert.equal(serialCancellationEvents.at(-1)?.event, 'cancelled');
assert.ok(
  serialCancellationEvents.some(
    (event) => event.event === 'progress' && event.progress.telemetry?.phase === 'searching'
  ),
  'the serial handoff exposes a cancellable running phase before terminal convergence'
);
assert.deepEqual(serialCancellationAuthority.snapshot().available, sharedCapacity);

const lateGrantAuthority = new SharedExecutionResourceAuthority(sharedCapacity);
let settleLateGrant: () => void = () => undefined;
const lateGrant = new Promise<SharedExecutionResourceLease>((resolve) => {
  settleLateGrant = () => resolve(
    lateGrantAuthority.tryAcquire('late-grant-owner', sharedCapacity)
  );
});
lateGrantAuthority.acquireBounded = async () => lateGrant;
let lateGrantPoolCancelCount = 0;
let lateGrantCoordinatorCancelCount = 0;
let lateGrantCoordinatorResetCount = 0;
const lateGrantRunner = new DistributedWasmJobRunner(
  {
    ...wasm,
    distributed_cancel() {
      lateGrantCoordinatorCancelCount += 1;
    },
    distributed_reset() {
      lateGrantCoordinatorResetCount += 1;
    }
  } as unknown as ClearraWasmModule,
  47,
  'late-grant-runner',
  {
    logicalProcessorCount: 2,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  {
    ...deferredPool,
    cancel() {
      lateGrantPoolCancelCount += 1;
    }
  } as never,
  lateGrantAuthority,
  100
);
const pendingLateGrant = lateGrantRunner.acquire();
lateGrantRunner.cancel();
settleLateGrant();
await assert.rejects(
  pendingLateGrant,
  (error: unknown) => error instanceof SharedExecutionAvailabilityError &&
    error.availability.state === 'cancelled' &&
    error.availability.reason === 'cancelled-by-caller'
);
lateGrantRunner.dispose();
assert.deepEqual(lateGrantAuthority.snapshot().used, {
  computeUnits: 0,
  memoryBytes: 0n
});
assert.equal(lateGrantAuthority.snapshot().activeLeaseCount, 0);
assert.equal(lateGrantPoolCancelCount, 0);
assert.equal(lateGrantCoordinatorCancelCount, 0);
assert.equal(lateGrantCoordinatorResetCount, 0);

// Product completion must use the cooperative API when it is available.
let finishSlices = 0;
const finishHost = {
  logicalProcessorCount: 2, webGpuAvailable: false,
  crossOriginIsolated: false, transferByteCap: 32 * 1024 * 1024
};
await new DistributedWasmJobRunner({
  ...wasm,
  distributed_finish() { throw new Error('blocking finish must not run'); },
  distributed_finish_start() { return null; },
  distributed_finish_advance(jobId: number, budget: number) {
    assert.equal(budget, 128);
    return ++finishSlices < 3 ? null : wasm.distributed_finish(jobId, 2);
  }
}, 901, 'cooperative-finish-owner', finishHost, { ...pool } as never).run(
  'clearra pc minimals --lines 4 --backend cpu --workers 2', plan, () => {}
);
assert.equal(finishSlices, 3);

// Proof tasks reuse the admitted durable pool; each query is initialized once,
// every receipt is merged before continuation, and no host-provided minimum is
// accepted as a result. Do not join all initializers before dispatching work.
let proofQueryIssued = false;
let proofTasksIssued = 0;
let proofReceiptsMerged = 0;
let proofMode = false;
let proofTransportComplete = false;
let negativeProofCancellations = 0;
const proofKinds: unknown[] = [];
const proofPool = {
  ...pool,
  async initialize(...args: unknown[]) {
    proofMode = args[6] === 'exact-at-most';
    if (proofMode) {
      proofKinds.push(args[6]);
      assert.equal(args[1], 1);
    }
  },
  async enqueue(task: ArrayBuffer, merge: (value: ArrayBuffer) => void) {
    assert.equal(proofMode, true);
    assert.equal(new Uint8Array(task)[0], proofTasksIssued);
    merge(task);
  },
  async completeAtomicTasks() {
    assert.equal(proofReceiptsMerged, 4);
    proofTransportComplete = true;
    return 1;
  },
  cancelExactTasks() {
    negativeProofCancellations += 1;
  }
};
await new DistributedWasmJobRunner({
  ...wasm,
  distributed_finish_start() { return null; },
  distributed_finish_parallel_prepare(_jobId: number, partitions: number) {
    assert.equal(partitions, 4);
    if (proofQueryIssued) return null;
    proofQueryIssued = true;
    return new Uint8Array([99]).buffer;
  },
  distributed_finish_parallel_task() {
    if (proofTasksIssued === 4) return null;
    return new Uint8Array([++proofTasksIssued]).buffer;
  },
  distributed_finish_parallel_merge(_jobId: number, receipt: ArrayBuffer) {
    assert.equal(new Uint8Array(receipt)[0], ++proofReceiptsMerged);
  },
  distributed_finish_parallel_found() { return false; },
  distributed_finish_advance(jobId: number) {
    assert.equal(proofTransportComplete, true);
    return wasm.distributed_finish(jobId, 2);
  }
}, 903, 'parallel-exact-finish-owner', finishHost, proofPool as never).run(
  'clearra pc minimals --lines 4 --backend cpu --workers 2', plan, () => {}
);
assert.deepEqual(proofKinds, ['exact-at-most']);
assert.equal(proofReceiptsMerged, 4);
assert.equal(negativeProofCancellations, 0, 'transport completion alone never cancels negative proof siblings');

{
  const singleEvents: ClearraWasmWorkerEvent[] = [];
  await new DistributedWasmJobRunner({
    ...wasm,
    distributed_finish_start: () => null,
    distributed_finish_advance: (jobId) => wasm.distributed_finish(jobId, 1),
    distributed_finish_parallel_prepare() { throw new Error('one admitted worker has no remote proof pool'); },
    distributed_finish_parallel_task() { throw new Error('one worker cannot create parallel tasks'); },
    distributed_finish_parallel_merge() { throw new Error('one worker has no remote receipts'); },
    distributed_finish_parallel_local_start() { throw new Error('one worker uses the existing serial exact continuation'); },
    distributed_finish_parallel_local_advance() { throw new Error('no duplicate serial proof'); }
  }, 909, 'single-coordinator-budget', finishHost, {
    ...pool, async initialize() { throw new Error('one worker must not create verifier threads'); }
  } as never).run('clearra pc minimals --workers 1', {
    ...plan, workerCount: 1, verificationRequired: false
  }, (event) => singleEvents.push(event));
  assert.ok(singleEvents.every((event) => event.event !== 'progress' ||
    (event.progress.telemetry?.worker_count === 1 && event.progress.telemetry.active_workers <= 1)));
}

for (const workerCount of [2, 11, 32]) {
  let tasksIssued = 0;
  let localActive = false;
  let localCompleted = false;
  let remoteCompleted = false;
  let maximumObserved = false;
  let localAdvances = 0;
  let queryIssued = false;
  let localStarted!: () => void;
  let localFinished!: () => void;
  const started = new Promise<void>((resolve) => { localStarted = resolve; });
  const finished = new Promise<void>((resolve) => { localFinished = resolve; });
  const remote: number[] = [];
  const coordinatorTasks: number[] = [];
  const proofSamples: Array<[string, number]> = [];
  let minimumProfile: { wave_count: number; waves: Array<Record<string, number>> } | null = null;
  let timeout: ReturnType<typeof setTimeout> | undefined;
  const runner = new DistributedWasmJobRunner({
    ...wasm,
    profile_start() {},
    profile_finish: () => ({ core_marker: true }),
    distributed_finish_start: () => null,
    distributed_finish_parallel_prepare(_jobId, partitions) {
      assert.equal(partitions, workerCount * 4, 'frontier includes the computing coordinator');
      if (queryIssued) return null;
      queryIssued = true;
      return new ArrayBuffer(0);
    },
    distributed_finish_parallel_task() {
      return tasksIssued < 4 ? Uint8Array.of(++tasksIssued).buffer : null;
    },
    distributed_finish_parallel_local_start() {
      assert.equal(localActive, false);
      if (tasksIssued === 4) return false;
      coordinatorTasks.push(++tasksIssued);
      localActive = true;
      localStarted();
      return true;
    },
    distributed_finish_parallel_local_advance(_jobId, budget) {
      localAdvances += 1;
      assert.equal(budget, 128);
      assert.equal(localActive, true);
      if (!maximumObserved) return false;
      localActive = false;
      localCompleted = true;
      localFinished();
      return true;
    },
    distributed_finish_parallel_merge(_jobId, receipt) { remote.push(new Uint8Array(receipt)[0]); },
    distributed_finish_parallel_found: () => false,
    distributed_finish_advance(jobId) {
      assert.ok(localCompleted && remoteCompleted, 'both issued owners drain before the next query');
      return wasm.distributed_finish(jobId, workerCount);
    }
  }, 910 + workerCount, 'coordinator-participates', {
    ...finishHost, logicalProcessorCount: workerCount + 1
  }, {
    ...pool,
    async enqueue(task: ArrayBuffer, merge: (receipt: ArrayBuffer) => void) {
      // The first ready worker need not wait for a coordinator shard to finish.
      await started;
      merge(task);
    },
    async completeAtomicTasks() {
      await finished;
      remoteCompleted = true;
      return workerCount - 1;
    },
    progressSnapshot() {
      return { ...completedProgress, activeWorkers: workerCount - 1,
        readyWorkers: workerCount - 1, workerCount: workerCount - 1 };
    },
    cancelExactTasks() { throw new Error('a negative query cannot cancel siblings'); }
  } as never);
  try {
    await Promise.race([
      runner.run('clearra pc minimals --patterns P7', { ...plan, workerCount }, (event) => {
        if (event.event === 'final_response' || event.event === 'failed') {
          const profile = (event as unknown as { search_profile: {
            core_marker: boolean; minimum_parallel: NonNullable<typeof minimumProfile>
          } }).search_profile;
          assert.equal(profile.core_marker, true, 'core profiling fields are preserved');
          minimumProfile = profile.minimum_parallel;
        }
        if (event.event === 'progress' && event.progress.telemetry) {
          proofSamples.push([event.progress.telemetry.phase, event.progress.telemetry.active_workers]);
        }
        if (event.event === 'progress' && event.progress.telemetry?.phase === 'merging' &&
          localActive && event.progress.telemetry.active_workers === workerCount) maximumObserved = true;
      }),
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error(`coordinator/remote progress stalled ${JSON.stringify({ workerCount, tasksIssued, localActive, localCompleted, remoteCompleted, maximumObserved, localAdvances, proofSamples })}`)), 2_000);
      })
    ]);
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
    runner.cancel();
  }
  assert.ok(maximumObserved, 'only the actually running coordinator fills the admitted count');
  assert.ok(coordinatorTasks.length > 0 && remote.length > 0);
  assert.deepEqual([...coordinatorTasks, ...remote].sort(), [1, 2, 3, 4],
    'local and remote owners consume one shared, exact-once core frontier');
  assert.ok(minimumProfile);
  const measured = minimumProfile as { wave_count: number; waves: Array<Record<string, number>> };
  assert.equal(measured.wave_count, 1);
  assert.equal(measured.waves[0].coordinator_tasks, coordinatorTasks.length);
  assert.equal(measured.waves[0].remote_tasks, remote.length);
  assert.equal(measured.waves[0].sampled_active_max, workerCount);
  assert.ok(measured.waves[0].coordinator_compute_ms >= 0);
  assert.ok(measured.waves[0].initialize_all_ready_ms >= 0);
  assert.ok(measured.waves[0].query_prepare_ms >= 0);
  assert.ok(measured.waves[0].elapsed_ms >= measured.waves[0].coordinator_compute_ms);
}

{
  let waves = 0;
  const terminal = await new DistributedWasmJobRunner({
    ...wasm,
    profile_start() {},
    profile_finish: () => ({ core_marker: true }),
    distributed_finish_start: () => null,
    distributed_finish_parallel_prepare() { waves += 1; return new ArrayBuffer(0); },
    distributed_finish_parallel_task: () => null,
    distributed_finish_parallel_merge() {},
    distributed_finish_advance: (jobId) => waves < 130 ? null : wasm.distributed_finish(jobId, 2)
  }, 958, 'bounded-proof-profiling', finishHost, {
    ...pool, async completeAtomicTasks() { return 1; }
  } as never).run('clearra pc minimals --patterns P7', plan, () => undefined);
  const profile = (terminal as unknown as { search_profile: { minimum_parallel: {
    wave_count: number; omitted_wave_count: number; waves: unknown[]
  } } }).search_profile.minimum_parallel;
  assert.equal(profile.wave_count, 130);
  assert.equal(profile.waves.length, 128, 'profiling cannot retain an unbounded history of exact queries');
  assert.equal(profile.omitted_wave_count, 2);
}

{
  let queryIssued = false;
  let lastIssued = 0;
  let assisted = false;
  let childrenMerged = 0;
  let parentDrained = false;
  const frontier = [1];
  const pending = new Map<number, { key: ArrayBuffer; merge: (receipt: ArrayBuffer) => void }>();
  const merged: number[] = [];
  const key = (id: number) => { const bytes = new Uint8Array(56); bytes[55] = id; return bytes.buffer; };
  await new DistributedWasmJobRunner({
    ...wasm,
    distributed_finish_start: () => null,
    distributed_finish_parallel_prepare() {
      if (queryIssued) return null;
      queryIssued = true; return new ArrayBuffer(0);
    },
    distributed_finish_parallel_task() {
      lastIssued = frontier.shift() ?? 0;
      return lastIssued === 0 ? null : Uint8Array.of(lastIssued).buffer;
    },
    distributed_finish_parallel_last_task_key: () => key(lastIssued),
    distributed_finish_parallel_assist(_job, maxChildren) {
      assert.equal(maxChildren, 64);
      assert.equal(frontier.length, 0, 'assistance only starts after original unissued work is empty');
      if (assisted) return false;
      assert.ok(pending.has(1), 'the existing parent cursor is retained, not restarted');
      assisted = true; frontier.push(2, 3); return true;
    },
    distributed_finish_parallel_redundant(_job, taskKey) {
      return childrenMerged === 2 && new Uint8Array(taskKey)[55] === 1;
    },
    distributed_finish_parallel_found: () => false,
    distributed_finish_parallel_merge(_job, receipt) {
      const id = new Uint8Array(receipt)[0]; merged.push(id);
      if (id === 1) { assert.equal(childrenMerged, 2); parentDrained = true; }
      else childrenMerged += 1;
    },
    distributed_finish_advance: (job) => {
      assert.ok(parentDrained, 'redundant original receipt drains before advancing the query');
      return wasm.distributed_finish(job, 3);
    }
  }, 959, 'idle-parent-assistance', finishHost, {
    ...pool,
    async enqueueFromSource(take: () => ArrayBuffer | null, merge: (receipt: ArrayBuffer) => void, readKey: () => ArrayBuffer) {
      const task = take(); if (task === null) return false;
      const id = new Uint8Array(task)[0]; pending.set(id, { key: readKey(), merge });
      if (id !== 1) queueMicrotask(() => { pending.delete(id); merge(task); });
      return true;
    },
    cancelRedundantExactTasks(redundant: (key: ArrayBuffer) => boolean) {
      for (const [id, owner] of pending) if (redundant(owner.key)) {
        pending.delete(id); owner.merge(Uint8Array.of(id).buffer);
      }
    },
    async completeAtomicTasks() { assert.equal(pending.size, 0); return 2; },
    cancelExactTasks() { throw new Error('a negative child closure must not cancel unrelated proof roots'); }
  } as never).run('clearra pc minimals --patterns P7', { ...plan, workerCount: 3 }, () => undefined);
  assert.deepEqual(merged, [2, 3, 1]);
}

let positiveTasksIssued = 0;
for (const enabled of [false, true]) {
  let began = 0;
  const terminal = await new DistributedWasmJobRunner(wasm, 962 + Number(enabled), 'typescript-only-profile', finishHost, {
    ...pool,
    beginTransportProfile() { began += 1; },
    finishTransportProfile() { return { schema: 'clearra.verifier-transport-profile.v1', timings: {} }; }
  } as never).run('clearra pc all --patterns P7', plan, () => undefined, { transportProfile: enabled });
  assert.equal(wasm.profile_start, undefined, 'transport observation does not require a profiled WASM');
  assert.equal(began, Number(enabled));
  assert.equal('search_profile' in terminal, enabled, 'normal Pages execution remains unprofiled');
}
{
  let issued = 0;
  let remoteInitiallyDrained = false;
  let retryAvailable = false;
  let localStarted = false;
  let localDeclined = false;
  const merged: number[] = [];
  await new DistributedWasmJobRunner({
    ...wasm,
    distributed_finish_start: () => null,
    distributed_finish_parallel_prepare: () => new ArrayBuffer(0),
    distributed_finish_parallel_task() {
      if (retryAvailable) { retryAvailable = false; return Uint8Array.of(2).buffer; }
      if (issued === 0) { issued = 1; return Uint8Array.of(1).buffer; }
      remoteInitiallyDrained = true;
      return null;
    },
    distributed_finish_parallel_local_start() {
      if (localStarted) return false;
      localStarted = true;
      issued = 2;
      return true;
    },
    distributed_finish_parallel_local_advance() {
      if (!remoteInitiallyDrained) return false;
      localDeclined = true;
      retryAvailable = true;
      return true;
    },
    distributed_finish_parallel_merge(_jobId, task) { merged.push(new Uint8Array(task)[0]); },
    distributed_finish_parallel_found: () => false,
    distributed_finish_advance(jobId) {
      assert.ok(localDeclined);
      assert.deepEqual(merged, [1, 2], 'late local admission decline returns the exact pending task to remote workers');
      return wasm.distributed_finish(jobId, 2);
    }
  }, 950, 'local-admission-retry-owner', finishHost, {
    ...pool,
    async enqueue(task: ArrayBuffer, merge: (receipt: ArrayBuffer) => void) { merge(task); },
    async completeAtomicTasks() {
      assert.deepEqual(merged, [1, 2], 'the pool stays available until local retries have drained');
      return 1;
    },
    cancelExactTasks() { throw new Error('admission failure is not a positive witness'); }
  } as never).run('clearra pc minimals --workers 2', plan, () => {});
}

let positiveReceiptsMerged = 0;
let positiveCancellationWaves = 0;
let positiveTransportComplete = false;
const positiveProofPool = {
  ...pool,
  async enqueue(task: ArrayBuffer, merge: (value: ArrayBuffer) => void) { merge(task); },
  cancelExactTasks() { positiveCancellationWaves += 1; },
  async completeAtomicTasks() {
    assert.equal(positiveReceiptsMerged, 8, 'positive witness still drains every issued receipt');
    positiveTransportComplete = true;
    return 1;
  }
};
await new DistributedWasmJobRunner({
  ...wasm,
  distributed_finish_start() { return null; },
  distributed_finish_parallel_prepare() { return new Uint8Array([99]).buffer; },
  distributed_finish_parallel_task() {
    return positiveTasksIssued < 8 ? new Uint8Array([++positiveTasksIssued]).buffer : null;
  },
  distributed_finish_parallel_merge() { positiveReceiptsMerged += 1; },
  distributed_finish_parallel_found() { return positiveReceiptsMerged > 0; },
  distributed_finish_advance(jobId: number) {
    assert.equal(positiveTransportComplete, true);
    return wasm.distributed_finish(jobId, 2);
  }
} as ClearraWasmModule, 904, 'positive-proof-cancellation-owner', finishHost, positiveProofPool as never)
  .run('clearra pc minimals --lines 4 --workers 2', plan, () => {});
assert.equal(positiveCancellationWaves, 1, 'one validated witness broadcasts one cancellation wave per query');

let rejectedReceiptCleanup = 0;
let rejectedReceiptAdvanced = false;
await assert.rejects(new DistributedWasmJobRunner({
  ...wasm,
  distributed_finish_start() { return null; },
  distributed_finish_parallel_prepare() { return new Uint8Array([99]).buffer; },
  distributed_finish_parallel_task() { return new Uint8Array([1]).buffer; },
  distributed_finish_parallel_merge() { throw new Error('stale exact receipt rejected'); },
  distributed_finish_advance() { rejectedReceiptAdvanced = true; return null; },
  distributed_cancel() { rejectedReceiptCleanup += 1; }
} as ClearraWasmModule, 905, 'rejected-proof-receipt-owner', finishHost, positiveProofPool as never)
  .run('clearra pc minimals --lines 4 --workers 2', plan, () => {}), /stale exact receipt rejected/);
assert.equal(rejectedReceiptCleanup, 1);
assert.equal(rejectedReceiptAdvanced, false, 'rejected receipt cannot become a completed negative proof');

let cancelledFinishSlices = 0;
let cancelledFinishStarted = 0;
const cancelledFinishRunner = new DistributedWasmJobRunner({
  ...wasm,
  distributed_finish() { throw new Error('blocking finish must not run'); },
  distributed_finish_start() {
    cancelledFinishStarted = performance.now();
    setTimeout(() => cancelledFinishRunner.cancel(), 0);
    return null;
  },
  distributed_finish_advance() { cancelledFinishSlices++; return null; }
}, 902, 'cancelled-finish-owner', finishHost, { ...pool } as never);
await assert.rejects(cancelledFinishRunner.run(
  'clearra pc minimals --lines 4 --backend cpu --workers 2', plan, () => {}
), /cancelled/u);
assert.ok(cancelledFinishSlices > 0, 'cheap completion slices can run in one host quantum');
assert.ok(performance.now() - cancelledFinishStarted < 500,
  'timer-based cancellation interrupts a self-refilling proof loop within bounded host quanta');

{
  const previousNow = Object.getOwnPropertyDescriptor(performance, 'now');
  const previousTimer = globalThis.setTimeout;
  let now = 0;
  let slices = 0;
  let completionStarted = false;
  const slicesAtTimerYield: number[] = [];
  Object.defineProperty(performance, 'now', { configurable: true, value: () => now });
  globalThis.setTimeout = ((callback: (...args: unknown[]) => void, delay?: number, ...args: unknown[]) => {
    if (completionStarted && delay === 0) slicesAtTimerYield.push(slices);
    return previousTimer(callback, delay, ...args);
  }) as typeof setTimeout;
  try {
    await new DistributedWasmJobRunner({
      ...wasm,
      distributed_finish_start() { completionStarted = true; return null; },
      distributed_finish_advance(jobId: number) {
        slices += 1;
        now += 2;
        return slices < 9 ? null : wasm.distributed_finish(jobId, 2);
      }
    }, 966, 'coordinator-timer-quantum', finishHost, { ...pool } as never).run(
      'clearra pc minimals --patterns P7', plan, () => {}
    );
    assert.equal(slices, 9);
    assert.deepEqual(slicesAtTimerYield, [4, 8],
      'coordinator uses timer lane each 8ms quantum, not each cheap ABI slice or every eighth quantum');
  } finally {
    globalThis.setTimeout = previousTimer;
    if (previousNow) Object.defineProperty(performance, 'now', previousNow);
    else delete (performance as unknown as { now?: () => number }).now;
  }
}

for (const [workers, logical] of [[1, 1], [1, 2], [2, 3], [11, 12], [32, 33], [12, 12], [32, 32]]) {
  const eligible = workers > 1 && workers < logical;
  for (const policy of ['auto', 'shared', 'dedicated'] as const) {
    const topology = minimumManagerTopology(workers, logical, true, true, policy);
    assert.equal(topology.controlOnly, eligible && policy !== 'shared');
    assert.equal(topology.partitions, workers * 4, 'topology never changes solver frontier factor');
    if (workers > 1) assert.equal(topology.remoteWorkers + Number(!topology.controlOnly), workers);
    assert.ok(topology.remoteWorkers + 1 <= Math.max(2, logical), 'no extra full-CPU instance');
  }
  assert.equal(minimumManagerTopology(workers, logical, false, true).controlOnly, false,
    'an old ABI cannot acquire an unguarded dedicated replica');
}

for (const sample of [
  { workers: 2, logical: 3, policy: 'auto', decline: false, expected: 2 },
  { workers: 11, logical: 12, policy: 'auto', decline: false, expected: 11 },
  { workers: 32, logical: 33, policy: 'auto', decline: false, expected: 32 },
  { workers: 12, logical: 12, policy: 'auto', decline: false, expected: 11 },
  { workers: 11, logical: 12, policy: 'shared', decline: false, expected: 10 },
  { workers: 11, logical: 12, policy: 'auto', decline: true, expected: 10 }
] as const) {
  let configured = 0;
  const admitted: Array<[number, boolean]> = [];
  let preparedWave = 0;
  let completedWave = 0;
  let issued = 0;
  let merged = 0;
  let localStarts = 0;
  const pools: number[] = [];
  let proofPool = false;
  let actualGrant = 0n;
  const guardWasm = {
    ...wasm,
    distributed_finish_start: () => null,
    distributed_finish_parallel_configure(_job: number, cpus: number, bytes: bigint) {
      assert.equal(cpus, sample.logical);
      assert.ok(bytes > 0n);
      actualGrant = bytes;
      configured += 1;
    },
    distributed_finish_parallel_admit(_job: number, count: number, only: boolean, cpus: number, bytes: bigint) {
      assert.equal(configured, 1);
      assert.equal(issued, 0, 'fallback occurs strictly before task issuance');
      assert.equal(cpus, sample.logical);
      assert.equal(bytes, actualGrant, 'pass the actual outer lease, not result retention cap');
      admitted.push([count, only]);
      return !(sample.decline && only);
    },
    distributed_finish_parallel_prepare(_job: number, partitions: number) {
      assert.equal(partitions, sample.workers * 4);
      if (preparedWave > completedWave) return null;
      preparedWave += 1;
      issued = 0;
      return Uint8Array.of(preparedWave).buffer;
    },
    distributed_finish_parallel_guarded_query() { return Uint8Array.of(44, preparedWave).buffer; },
    distributed_finish_parallel_task() { return issued++ === 0 ? Uint8Array.of(preparedWave).buffer : null; },
    distributed_finish_parallel_local_start() { localStarts += 1; return false; },
    distributed_finish_parallel_local_advance() { throw new Error('fixture issued no local shard'); },
    distributed_finish_parallel_merge() { merged += 1; },
    distributed_finish_advance(job: number) {
      assert.equal(merged, preparedWave, 'every actual receipt drains before advancing the query');
      completedWave += 1;
      return completedWave === 2 ? wasm.distributed_finish(job, sample.workers) : null;
    }
  } as ClearraWasmModule;
  const guardPool = {
    ...pool,
    async initialize(query: ArrayBuffer, count: number, _module: unknown, _owner: unknown,
      _recovery: unknown, _host: unknown, kind?: string) {
      proofPool = kind === 'exact-at-most';
      if (proofPool) {
        assert.equal(new Uint8Array(query)[0], 44, 'only source-bound cap packets reach replicas');
        pools.push(count);
      }
    },
    async enqueue(task: ArrayBuffer, merge: (receipt: ArrayBuffer) => void) { merge(task); },
    async completeAtomicTasks() { return sample.expected; },
    progressSnapshot() { return proofPool ? { ...emptyProgress, activeWorkers: sample.expected,
      readyWorkers: sample.expected, workerCount: sample.expected } : completedProgress; }
  };
  await new DistributedWasmJobRunner(guardWasm, 990, 'guarded-topology', {
    ...finishHost, logicalProcessorCount: sample.logical
  }, guardPool as never, undefined, undefined, sample.policy as MinimumManagerPolicy).run(
    'clearra pc minimals --patterns P7', { ...plan, workerCount: sample.workers }, (event) => {
      if (event.event === 'progress') {
        const telemetry = (event as unknown as { telemetry?: { active_workers: number; ready_workers: number } }).telemetry;
        if (proofPool && telemetry) {
          assert.ok(telemetry.active_workers <= sample.workers);
          assert.ok(telemetry.ready_workers <= sample.workers, 'control-only manager is not a ready compute worker');
        }
      }
    }
  );
  assert.equal(configured, 1);
  assert.deepEqual(pools, [sample.expected, sample.expected]);
  assert.equal(admitted.length, sample.decline ? 2 : 1, 'fixed completion slices survive query transitions');
  assert.equal(localStarts, sample.expected === sample.workers ? 0 : 2);
}

{
  let attempts = 0;
  let issued = 0;
  let exactInitializations = 0;
  const runner = new DistributedWasmJobRunner({
    ...wasm,
    distributed_finish_start: () => null,
    distributed_finish_parallel_prepare: () => new ArrayBuffer(0),
    distributed_finish_parallel_configure() {},
    distributed_finish_parallel_admit() { attempts += 1; return false; },
    distributed_finish_parallel_guarded_query() { throw new Error('declined memory has no packet authority'); },
    distributed_finish_parallel_task() { issued += 1; return null; },
    distributed_finish_parallel_merge() {},
    distributed_finish_parallel_local_start: () => false,
    distributed_finish_parallel_local_advance: () => true,
    distributed_finish_advance: () => null
  }, 991, 'guarded-memory-decline', { ...finishHost, logicalProcessorCount: 12 }, {
    ...pool,
    async initialize(_q: unknown, _c: unknown, _m: unknown, _o: unknown, _r: unknown, _h: unknown, kind?: string) {
      exactInitializations += Number(kind === 'exact-at-most');
    }
  } as never);
  await assert.rejects(runner.run('clearra pc minimals --patterns P7', { ...plan, workerCount: 11 }, () => {}),
    /whole-job memory admission unavailable/);
  assert.equal(attempts, 2, 'one dedicated attempt plus one admitted shared fallback, never repeated waves');
  assert.equal(issued, 0);
  assert.equal(exactInitializations, 0);
}

function verifierFlags(value: boolean) {
  return {
    candidatesVerified: value,
    buildNodes: value,
    coverageChecks: value
  };
}

function coreFlags(value: boolean) {
  return {
    geometryNodes: value,
    candidateCount: value,
    candidateFamilyCount: value,
    buildNodes: value,
    coverageChecks: value,
    passIndex: value,
    passCount: value,
    layerIndex: value,
    layerCount: value,
    layerDone: value,
    layerTotal: value
  };
}

function telemetryFlags(value: boolean) {
  return {
    geometry_nodes: value,
    candidates_emitted: value,
    geometry_family_count: value,
    candidates_verified: value,
    producer_build_nodes: value,
    producer_coverage_checks: value,
    build_nodes: value,
    coverage_checks: value,
    ready_workers: value,
    active_workers: value,
    worker_count: value,
    oldest_batch_ms: value,
    pass_index: value,
    pass_count: value,
    layer_index: value,
    layer_count: value,
    layer_done: value,
    layer_total: value
  };
}

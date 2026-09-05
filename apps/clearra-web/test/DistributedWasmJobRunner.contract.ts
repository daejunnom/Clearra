import assert from 'node:assert/strict';

import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';
import { workspaceProgressDetail } from '../../../packages/clearra-ui/src/lib/workspace/workspaceI18n.ts';
import { buildWorkspaceProgressModel } from '../../../packages/clearra-ui/src/lib/workspace/workspaceProgressModel.ts';

import type { ClearraVerifierPoolProgress } from '../src/workers/ClearraVerifierPool.ts';
import { DistributedWasmJobRunner } from '../src/workers/DistributedWasmJobRunner.ts';
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
  async initialize() {},
  async enqueue() {},
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
assert.equal(merging.progress.telemetry?.active_workers, 0);
assert.equal(merging.progress.telemetry?.worker_count, 1);
assert.equal(workersUsed, 2);
assert.deepEqual(
  distributedProducerCalls,
  [{ workBudget: 2_048, batchSize: 256 }],
  'the browser coordinator must return to the host between bounded geometry slices'
);

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

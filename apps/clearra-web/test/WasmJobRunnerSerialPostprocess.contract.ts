import assert from 'node:assert/strict';

import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';
import { buildWorkspaceProgressModel } from '../../../packages/clearra-ui/src/lib/workspace/workspaceProgressModel.ts';

import { ClearraProductJobRunner } from '../src/workers/ClearraProductJobRunner.ts';
import { SerialSearchProgress } from '../src/workers/SerialSearchProgress.ts';
import { SharedExecutionResourceAuthority } from '../src/workers/SharedExecutionResourceAuthority.ts';
import type {
  ClearraDistributedPlan,
  ClearraWasmModule
} from '../src/workers/clearraWasmRuntime.ts';

const capacity = { computeUnits: 1, memoryBytes: 64n * 1024n * 1024n };
const authority = new SharedExecutionResourceAuthority(capacity);
const serialPlan: ClearraDistributedPlan = {
  mode: 'serial',
  workerCount: 1,
  requestedBackend: 'cpu',
  selectedBackend: 'wasm-cpu',
  fallbackUsed: false,
  fallbackReason: null,
  workerInitialization: null,
  deferredInitialization: false,
  verificationRequired: false,
  tilingGeometryParallel: false
};

const jobId = 91;
let advanceCount = 0;
let resetCount = 0;
let hostTurnAfterPostprocess = false;
const pendingEvents: ClearraWasmWorkerEvent[] = [];
const order: string[] = [];

const wasm = {
  distributed_prepare(commandText: string) {
    assert.match(commandText, /^clearra pc minimals\b/u);
    return serialPlan;
  },
  distributed_reset() {
    resetCount += 1;
  },
  start_job(commandText: string) {
    assert.match(commandText, /^clearra pc minimals\b/u);
    return jobId;
  },
  advance_job(actualJobId: number, workBudget: number) {
    assert.equal(actualJobId, jobId);
    assert.equal(workBudget, 2_048);
    advanceCount += 1;
    if (advanceCount === 1) {
      order.push('search-completed');
      pendingEvents.push({
        schema_version: 1,
        runtime: 'clearra-wasm',
        event: 'progress',
        job_id: jobId,
        progress: {
          done: 0,
          total: 1,
          label: 'postprocess',
          budget_status: { state: 'within-budget', used: 0, limit: null },
          backend_status: {
            backend_requested: 'cpu',
            backend_selected: 'wasm-cpu',
            fallback_used: false,
            fallback_reason: null
          },
          memory_status: {
            state: 'wasm-computation-scope-active',
            raw_pointer_exposed: false
          }
        }
      });
      return 'progress' as const;
    }
    assert.equal(advanceCount, 2);
    assert.equal(
      hostTurnAfterPostprocess,
      true,
      'the runner must yield to its worker host after publishing postprocess and before exact App finalization'
    );
    order.push('exact-app-finalization');
    pendingEvents.push({
      schema_version: 1,
      runtime: 'clearra-wasm',
      event: 'final_response',
      job_id: jobId,
      response: { status: 'success' }
    } as ClearraWasmWorkerEvent);
    return 'completed' as const;
  },
  drain_job_events_json(actualJobId: number) {
    assert.equal(actualJobId, jobId);
    return JSON.stringify(pendingEvents.splice(0));
  },
  cancel_job() {}
} as unknown as ClearraWasmModule;

const events: ClearraWasmWorkerEvent[] = [];
const keepAlive = setTimeout(() => undefined, 1_000);
const terminal = await new ClearraProductJobRunner(
  wasm,
  jobId,
  'serial-pc-minimals-postprocess-owner',
  {
    logicalProcessorCount: 1,
    webGpuAvailable: false,
    crossOriginIsolated: false,
    transferByteCap: 32 * 1024 * 1024
  },
  authority,
  100
).run('clearra pc minimals --lines 4 --backend cpu --workers 1', (event) => {
  events.push(event);
  if (event.event === 'progress' && event.progress.label === 'postprocess') {
    order.push('postprocess-host-visible');
    queueMicrotask(() => {
      hostTurnAfterPostprocess = true;
    });
  }
});
clearTimeout(keepAlive);

assert.equal(terminal.event, 'final_response');
assert.equal(resetCount, 1, 'serial handoff releases its prepared coordinator once');
assert.deepEqual(order, [
  'search-completed',
  'postprocess-host-visible',
  'exact-app-finalization'
]);
assert.deepEqual(authority.snapshot().available, capacity);

const postprocess = events.find(
  (event) => event.event === 'progress' && event.progress.label === 'postprocess'
);
assert.ok(postprocess && postprocess.event === 'progress');
const finalizing =
  postprocess.progress.telemetry?.phase === 'postprocessing' ||
  postprocess.progress.telemetry?.phase === 'merging' ||
  postprocess.progress.label === 'postprocess';
const producerComplete =
  postprocess.progress.telemetry?.producer_complete ??
  postprocess.progress.label === 'postprocess';
assert.equal(finalizing, true, 'serial postprocess is the canonical finalizing marker');
assert.equal(
  producerComplete,
  true,
  'the postprocess transition means the serial Geometry producer is complete'
);

const model = buildWorkspaceProgressModel({
  profile: 'pc',
  mode: 'pc-minimum-cover',
  status: 'running',
  progressLabel: postprocess.progress.label,
  progressDone: postprocess.progress.done,
  progressTotal: postprocess.progress.total,
  telemetry: postprocess.progress.telemetry ?? null
});
assert.equal(model.stages.find((stage) => stage.id === 'geometry')?.status, 'complete');
assert.equal(model.stages.find((stage) => stage.id === 'verify')?.status, 'complete');
assert.equal(model.stages.find((stage) => stage.id === 'finalize')?.status, 'running');
assert.equal(
  model.stages.find((stage) => stage.id === 'finalize')?.labelKey,
  'progressStageMinimumCover'
);

const serialStart = events.find(
  (event) => event.event === 'progress' && event.progress.telemetry?.execution_mode === 'serial'
);
assert.ok(serialStart && serialStart.event === 'progress');
const initialSerialProgress = serialStart;
const projection = new SerialSearchProgress(initialSerialProgress);
function buildProgress(label: string, done: number) {
  return projection.project({
    ...initialSerialProgress,
    event: 'progress',
    progress: { ...initialSerialProgress.progress, label, done, total: 0, telemetry: undefined }
  });
}
for (const geometryNodes of [1, 128, 1_024]) {
  const geometryEvent = buildProgress('build-geometry', geometryNodes);
  assert.ok(geometryEvent.event === 'progress');
  const liveTelemetry = geometryEvent.progress.telemetry;
  assert.equal(liveTelemetry?.geometry_nodes, geometryNodes);
  assert.equal(liveTelemetry?.availability.geometry_nodes, true);
  assert.equal(liveTelemetry?.exactness.geometry_nodes, true);
  const buildModel = buildWorkspaceProgressModel({
    profile: 'build', status: 'running', progressLabel: 'build-geometry',
    progressDone: geometryNodes, progressTotal: 0, telemetry: liveTelemetry ?? null
  });
  assert.equal(buildModel.stages.find((stage) => stage.id === 'geometry')?.metrics
    .find((metric) => metric.labelKey === 'progressMetricNodes')?.value, String(geometryNodes));
}
const verified = buildProgress('build-candidates', 12);
assert.ok(verified.event === 'progress');
assert.equal(verified.progress.telemetry?.candidates_emitted, 12);
assert.equal(verified.progress.telemetry?.candidates_verified, 12);
const buildNodes = buildProgress('build-verification', 48);
assert.ok(buildNodes.event === 'progress');
assert.equal(buildNodes.progress.telemetry?.build_nodes, 48);
assert.equal(buildNodes.progress.telemetry?.geometry_nodes, 1_024);
const finalized = buildProgress('postprocess', 0);
assert.ok(finalized.event === 'progress');
assert.equal(finalized.progress.telemetry?.producer_complete, true);
assert.equal(finalized.progress.telemetry?.phase, 'postprocessing');
assert.equal(finalized.progress.telemetry?.build_nodes, 48);
const saturated = buildProgress('build-geometry', 0xffff_ffff);
assert.ok(saturated.event === 'progress');
assert.equal(saturated.progress.telemetry?.exactness.geometry_nodes, false);
assert.equal(serialStart.progress.telemetry?.availability.geometry_nodes, false,
  'prior snapshots must not mutate after a later progress event');

const replayManifest = buildProgress('complete-replay-pattern', 64);
assert.ok(replayManifest.event === 'progress');
assert.equal(replayManifest.progress.telemetry?.producer_complete, true);
assert.equal(replayManifest.progress.telemetry?.phase, 'postprocessing');
const replayModel = buildWorkspaceProgressModel({
  profile: 'pc', status: 'running', progressLabel: 'complete-replay-pattern',
  progressDone: 64, progressTotal: 1_240, telemetry: replayManifest.progress.telemetry ?? null
});
assert.equal(replayModel.stages.find((stage) => stage.id === 'geometry')?.status, 'complete');
assert.equal(replayModel.stages.find((stage) => stage.id === 'verify')?.status, 'complete');
assert.equal(replayModel.stages.find((stage) => stage.id === 'finalize')?.status, 'running');

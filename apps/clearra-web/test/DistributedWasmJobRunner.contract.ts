import assert from 'node:assert/strict';

import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

import type { ClearraVerifierPoolProgress } from '../src/workers/ClearraVerifierPool.ts';
import { DistributedWasmJobRunner } from '../src/workers/DistributedWasmJobRunner.ts';
import type {
  ClearraDistributedPlan,
  ClearraWasmModule
} from '../src/workers/clearraWasmRuntime.ts';

const completedProgress: ClearraVerifierPoolProgress = {
  candidatesVerified: 12,
  buildNodes: 34,
  coverageChecks: 56,
  readyWorkers: 1,
  activeWorkers: 1,
  workerCount: 1,
  oldestBatchMs: 78
};
const emptyProgress: ClearraVerifierPoolProgress = {
  candidatesVerified: 0,
  buildNodes: 0,
  coverageChecks: 0,
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
const wasm = {
  compiled_module: () => ({}) as WebAssembly.Module,
  distributed_produce: () => ({ status: 'completed' as const }),
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
    layerTotal: 0
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
  pool as never
).run('clearra pc --lines 1', plan, (event) => events.push(event));

const merging = events.find(
  (event) => event.event === 'progress' && event.progress.telemetry?.phase === 'merging'
);
assert.ok(merging && merging.event === 'progress');
assert.equal(merging.progress.telemetry?.candidates_verified, 12);
assert.equal(merging.progress.telemetry?.build_nodes, 34);
assert.equal(merging.progress.telemetry?.coverage_checks, 56);
assert.equal(merging.progress.telemetry?.active_workers, 0);
assert.equal(merging.progress.telemetry?.worker_count, 1);
assert.equal(workersUsed, 2);

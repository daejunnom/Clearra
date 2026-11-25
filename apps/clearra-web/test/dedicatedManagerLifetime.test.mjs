import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import test from 'node:test';
import { transform } from 'esbuild';
import {
  createHostCapabilitySnapshot,
  normalizeRuntimeWarmupPolicy,
  resolveWorkerAuthority
} from '../../../packages/clearra-ui/src/lib/wasm/hostCapabilitySnapshot.ts';

// Exercise the production worker's cache/warmup owner without booting its
// unrelated asset/network entrypoint. Manager transport is covered separately.
const workerUrl = new URL('../src/workers/clearraWorker.ts', import.meta.url);
const source = await readFile(workerUrl, 'utf8');
function section(begin, end) {
  const start = source.indexOf(begin);
  const stop = source.indexOf(end, start + begin.length);
  assert.ok(start >= 0 && stop > start, `missing worker section ${begin}`);
  return source.slice(start, stop);
}
const compiled = (await transform([
  section('function createSourceManager(', '\nconst ACCELERATOR_PROFILES'),
  section('function startRuntimePrewarm(', '\nfunction startGpuWarmupAfterWasm(')
].join('\n'), {
  loader: 'ts', target: 'es2022', define: { 'import.meta.url': JSON.stringify(workerUrl.href) }
})).code;

function fixture() {
  const created = [];
  const calls = { pools: [], gpu: 0, tablebase: 0, retiredSourcePools: 0 };
  const wasm = { compiled_module: () => ({}) };
  class ManagerWorker {
    terminated = 0;
    messages = [];
    constructor() { created.push(this); }
    terminate() { this.terminated += 1; }
    postMessage(message) { this.messages.push(message); }
  }
  const context = vm.createContext({
    URL, console, Worker: ManagerWorker,
    DedicatedDistributedManager: class {
      constructor(_options, acquire, release) { this.acquire = acquire; this.release = release; }
    },
    createHostCapabilitySnapshot, normalizeRuntimeWarmupPolicy, resolveWorkerAuthority,
    hostCapabilitySnapshot: createHostCapabilitySnapshot({ snapshotId: 'lifetime-contract',
      source: 'browser', reportedLogicalProcessors: 8, webGpuAvailable: true, crossOriginIsolated: false }),
    warmupPolicy: normalizeRuntimeWarmupPolicy(),
    active: null, failClosed: false, dedicatedManagerWorker: null,
    lastExecutionUsedDedicatedManager: false, lifecycleOwnerId: 'test-owner',
    completedPrewarmWorkerCount: 0, completedRootOnlyPrewarm: false,
    requestedPrewarmWorkerCount: 1, deferredTablebaseRequested: false,
    runtimePrewarm: null, pendingRuntimePrewarm: null, runtimePrewarmGeneration: 0,
    loadedWasm: null, tablebaseRequested: false,
    MAX_EAGER_PREWARM_TOTAL_WORKERS: 9, RUNTIME_PREWARM_TIMEOUT_MS: 15_000,
    disposeDistributedWorkers: () => { calls.retiredSourcePools += 1; },
    loadClearraWasmModule: async () => wasm,
    wasmHostCapabilities: () => ({}),
    prewarmDistributedWorkers: async count => { calls.pools.push(count); },
    startGpuWarmupAfterWasm: async () => { calls.gpu += 1; },
    startTablebaseWarmupAfterWasm: async () => { calls.tablebase += 1; },
    postRuntimePrewarmPhase: () => {}, withTimeout: promise => promise,
    setTablebaseRequested: requested => { context.tablebaseRequested = requested; }
  });
  vm.runInContext(compiled, context);
  return { api: context, calls, created };
}

test('idle manager cache suppresses duplicate verifier warmup without reducing the requested workers', async () => {
  const { api, calls } = fixture();
  const manager = api.createSourceManager({});
  const worker = manager.acquire();
  manager.release(worker, true);
  const policy = { cpuWarmup: true, gpuWarmup: true, verifierWarmup: true };
  api.startRuntimePrewarm(6, true, policy);
  await api.runtimePrewarm;
  assert.deepEqual(calls.pools, [], 'the retained manager already owns the verifier cache');
  assert.equal(api.dedicatedManagerWorker, worker);
  assert.equal(api.requestedPrewarmWorkerCount, 6);
  assert.equal(api.completedRootOnlyPrewarm, true);
  assert.equal(api.tablebaseRequested, true, 'online preparation remains available');
  assert.equal(calls.gpu, 1);
  assert.equal(calls.tablebase, 1);

  api.startRuntimePrewarm(6, true, policy);
  await api.runtimePrewarm;
  assert.deepEqual(calls.pools, [], 'a repeated UI prewarm cannot create a second pool');
  api.releaseSourceManager();
  assert.equal(worker.messages[0].type, 'dispose');
  api.startRuntimePrewarm(6, false, policy);
  await api.runtimePrewarm;
  assert.deepEqual(calls.pools, [6], 'source warmup resumes after manager ownership ends');
  assert.equal(api.requestedPrewarmWorkerCount, 6);
});

for (const failure of ['onerror', 'onmessageerror']) {
  test(`idle manager ${failure} invalidates only its cache so the next job creates a live realm`, () => {
    const { api, created } = fixture();
    const first = api.createSourceManager({});
    const failedWorker = first.acquire();
    first.release(failedWorker, true);
    const idleFailure = failedWorker[failure];
    let prevented = 0;
    idleFailure({ preventDefault: () => { prevented += 1; } });
    assert.equal(prevented, 1, 'idle errors do not escape into the source owner');
    assert.equal(api.dedicatedManagerWorker, null);
    assert.equal(failedWorker.terminated, 1);

    const second = api.createSourceManager({});
    const liveWorker = second.acquire();
    assert.notEqual(liveWorker, failedWorker);
    second.release(liveWorker, true);
    idleFailure({ preventDefault() {} });
    assert.equal(api.dedicatedManagerWorker, liveWorker, 'late old errors cannot evict the new cache');
    const third = api.createSourceManager({});
    assert.equal(third.acquire(), liveWorker, 'healthy idle workers are reused');
    assert.equal(created.length, 2);
    third.release(liveWorker, false);
    assert.equal(api.dedicatedManagerWorker, null);
    assert.equal(liveWorker.terminated, 1);
  });
}

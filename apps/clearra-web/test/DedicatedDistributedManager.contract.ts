import assert from 'node:assert/strict';
import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';
import { ClearraProductJobRunner } from '../src/workers/ClearraProductJobRunner';
import type { ClearraVerifierPool } from '../src/workers/ClearraVerifierPool';
import { DedicatedDistributedManager, installDedicatedDistributedManager }
  from '../src/workers/DedicatedDistributedManager';
import { SharedExecutionAvailabilityError, SharedExecutionResourceAuthority }
  from '../src/workers/SharedExecutionResourceAuthority';
import { ClearraWasmRuntimeError, type ClearraDistributedPlan, type ClearraWasmModule }
  from '../src/workers/clearraWasmRuntime';

type Scope = Parameters<typeof installDedicatedDistributedManager>[0];
type PartialConsumer = (value: ArrayBuffer) => void | Promise<void>;
const compiled = new WebAssembly.Module(Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0));
const host = { logicalProcessorCount: 8, webGpuAvailable: false,
  crossOriginIsolated: false, transferByteCap: 32 * 1024 * 1024 };
const grant = { computeUnits: 4, memoryBytes: 128n * 1024n * 1024n };
const flags = { geometryNodes: true, candidateCount: true, buildNodes: true,
  coverageChecks: true, candidateFamilyCount: true, passIndex: true, passCount: true,
  layerIndex: true, layerCount: true, layerDone: true, layerTotal: true };
const coreProgress = { geometryNodes: 2, candidateCount: 1, candidateFamilyCount: '2',
  buildNodes: 0, coverageChecks: 0, passIndex: 1, passCount: 1,
  layerIndex: 0, layerCount: 0, layerDone: 0, layerTotal: 0,
  availability: flags, exactness: flags };
const plan = (): ClearraDistributedPlan => ({ mode: 'cpu-multi', workerCount: 3,
  requestedBackend: 'cpu', selectedBackend: 'wasm-cpu', fallbackUsed: false,
  fallbackReason: null, workerInitialization: Uint8Array.of(41).buffer,
  deferredInitialization: false, verificationRequired: true,
  tilingGeometryParallel: false, rootTaskParallel: false });
const terminal = (jobId: number) => ({ schema_version: 1, runtime: 'clearra-wasm',
  event: 'final_response', job_id: jobId, response: { status: 'success' } }) as ClearraWasmWorkerEvent;

class TestPool {
  readonly sizes: number[] = [];
  readonly inFlight = new Set<Promise<void>>();
  count = 0;
  cancelled = 0;
  initializeRelation: ((relation: unknown) => Promise<void>) | null = null;
  beginTransportProfile() {}
  finishTransportProfile() { return {}; }
  async initialize(_packet: unknown, count: number, _compiled: unknown, _owner: unknown,
    _recovery: unknown, _host: unknown, _kind: unknown, _synopsis: unknown, relation: unknown) {
    this.count = count;
    this.sizes.push(count);
    if (this.initializeRelation) await this.initializeRelation(relation);
  }
  async enqueue(batch: ArrayBuffer, consume: PartialConsumer) {
    const operation = Promise.resolve().then(() => consume(batch));
    this.inFlight.add(operation);
    void operation.then(() => this.inFlight.delete(operation), () => this.inFlight.delete(operation));
  }
  async enqueueFromSource(take: () => ArrayBuffer | null | Promise<ArrayBuffer | null>,
    consume: PartialConsumer, readKey?: () => ArrayBuffer | Promise<ArrayBuffer>) {
    const task = await take();
    if (task === null) return false;
    if (readKey) {
      const key = await readKey();
      assert.equal(new Uint8Array(key)[55], new Uint8Array(task)[0],
        'every issued task retains its own atomic source routing key');
    }
    await this.enqueue(task, consume);
    return true;
  }
  async waitForIdle() { await Promise.all([...this.inFlight]); }
  async waitForTaskProgress() { await Promise.race([...this.inFlight]); }
  async finish(consume: PartialConsumer) {
    await this.waitForIdle();
    await consume(Uint8Array.of(9).buffer);
    return this.count;
  }
  async completeAtomicTasks() { await this.waitForIdle(); return this.count; }
  cancelExactTasks() {}
  async cancelRedundantExactTasks() {}
  cancel() { this.cancelled += 1; }
  progressSnapshot() {
    const telemetry = { geometryNodes: true, candidatesVerified: true, buildNodes: true, coverageChecks: true };
    return { geometryNodes: 0, candidatesVerified: 1, buildNodes: 0, coverageChecks: 0,
      readyWorkers: this.count, activeWorkers: this.inFlight.size, workerCount: this.count,
      oldestBatchMs: 0, availability: telemetry, exactness: telemetry };
  }
}

/** Real asynchronous ports enforce structured cloning and transferable ownership.
 * The actual manager implementation runs on the far endpoint; only solver and
 * verifier computation are bounded fixtures. */
class ManagerRealm {
  onmessage: Worker['onmessage'] = null;
  onerror: Worker['onerror'] = null;
  onmessageerror: Worker['onmessageerror'] = null;
  readonly channel = new MessageChannel();
  readonly sourceMethods: string[] = [];
  lastDone: unknown;
  closed = false;
  readonly scope: Scope;
  constructor(readonly pool: TestPool) {
    this.scope = {
      onmessage: null,
      postMessage: (message, transfer = []) => {
        if (message.type === 'source') this.sourceMethods.push(message.method);
        if (message.type === 'done') this.lastDone = message;
        this.channel.port2.postMessage(message, transfer);
      },
      close: () => { this.closed = true; this.channel.port1.close(); this.channel.port2.close(); }
    };
    this.channel.port1.onmessage = event => this.onmessage?.call(this as unknown as Worker, event);
    this.channel.port2.onmessage = event => this.scope.onmessage?.(event);
    installDedicatedDistributedManager(this.scope, () => pool as unknown as ClearraVerifierPool);
  }
  postMessage(message: unknown, transfer: Transferable[] = []) { this.channel.port1.postMessage(message, transfer); }
  terminate() { this.scope.onmessage?.({ data: { type: 'dispose' } } as MessageEvent); }
}

function sourceFixture() {
  const state = { preparations: 0, produced: 0, merged: [] as number[], resets: 0,
    cancelled: 0, completedWorkers: 0 };
  const wasm = {
    compiled_module: () => compiled,
    distributed_prepare: () => { state.preparations += 1; state.produced = 0; return plan(); },
    distributed_progress: () => coreProgress,
    distributed_produce: () => state.produced++ === 0
      ? { status: 'batch', batch: Uint8Array.of(7).buffer }
      : { status: 'completed' },
    distributed_merge_partial: (wire: ArrayBuffer) => state.merged.push(new Uint8Array(wire)[0]),
    distributed_finish: (jobId: number, count: number) => {
      state.completedWorkers = count;
      assert.deepEqual(state.merged.slice(-2), [7, 9], 'source merges drain before final response');
      return JSON.stringify([terminal(jobId)]);
    },
    distributed_cancel: () => { state.cancelled += 1; },
    distributed_reset: () => { state.resets += 1; }
  } as unknown as ClearraWasmModule;
  return { wasm, state };
}

// Non-root production routing: the source is one compute lane, N-1 verifiers
// live under a distinct manager, and the source's existing lease stays held.
{
  const pool = new TestPool();
  const realm = new ManagerRealm(pool);
  const { wasm, state } = sourceFixture();
  const authority = new SharedExecutionResourceAuthority(grant);
  const events: ClearraWasmWorkerEvent[] = [];
  const seed = Uint8Array.of(10, 20).buffer;
  let relationCalls = 0;
  pool.initializeRelation = async value => {
    const relation = value as { answerQueries: (wire: ArrayBuffer) => Promise<ArrayBuffer> };
    const answer = await relation.answerQueries(Uint8Array.of(31).buffer);
    assert.deepEqual([...new Uint8Array(answer)], [32]);
  };
  const managerHost = {
    create: (options: ConstructorParameters<typeof DedicatedDistributedManager>[0]) =>
      new DedicatedDistributedManager(options, () => realm as unknown as Worker, (_worker, reusable) => {
        if (!reusable) realm.terminate();
      }),
    release: () => realm.terminate()
  };
  try {
    for (const jobId of [1, 2]) {
      const runner = new ClearraProductJobRunner(wasm, jobId, '', host, authority, 100,
        null, { profile: 1, seed, identity: 'source-held-relation', reservedBytes: 1024,
          maximumPeers: 2, answerQueries: async wire => {
            relationCalls += 1;
            assert.deepEqual(authority.snapshot().used, grant,
              'source lease stays live while manager and relation callbacks execute');
            return Uint8Array.of(new Uint8Array(wire)[0] + 1).buffer;
          } }, managerHost);
      const run = runner.run('clearra build-probability --workers 3', event => events.push(event));
      if (jobId === 2 && realm.lastDone) realm.channel.port2.postMessage(realm.lastDone);
      assert.equal((await run).event, 'final_response');
      assert.deepEqual(authority.snapshot().used, { computeUnits: 0, memoryBytes: 0n });
    }
    assert.deepEqual(pool.sizes, [2, 2]);
    assert.equal(state.preparations, 2, 'the command is prepared once per source instance run');
    assert.equal(state.resets, 2, 'manager handoff does not duplicate source cleanup');
    assert.equal(state.completedWorkers, 3);
    assert.equal(relationCalls, 2, 'conditioned relation callbacks keep their original source owner');
    assert.equal(seed.byteLength, 2, 'source-held immutable relation seed is never detached');
    assert.equal(events.filter(event => event.event === 'final_response').length, 2,
      'a stale previous-generation completion never publishes a second terminal');
    assert.ok(events.filter(event => event.event === 'progress' && event.progress.telemetry?.worker_count)
      .every(event => event.event === 'progress' && event.progress.telemetry!.worker_count === 3));
    assert.ok(realm.sourceMethods.includes('distributed_merge_partial'));
    assert.ok(realm.sourceMethods.includes('conditioned_queries'));
  } finally { realm.terminate(); }
}

// Completion reuses the same source instance and inherited grant. Once source
// production has drained, its proof work belongs to the full remote compute
// pool; the source only publishes queries and merges source-bound receipts.
{
  const pool = new TestPool();
  const realm = new ManagerRealm(pool);
  const { wasm, state } = sourceFixture();
  const authority = new SharedExecutionResourceAuthority(grant);
  let queryPublished = false;
  let issued = 0;
  let lastTask = 0;
  let admissions = 0;
  const proofs: number[] = [];
  wasm.distributed_finish_start = () => {
    assert.deepEqual(state.merged, [7, 9]);
    return null;
  };
  wasm.distributed_finish_parallel_configure = (_job, compute, bytes) => {
    assert.equal(compute, grant.computeUnits, 'manager inherits the actual lease, not the larger host report');
    assert.equal(bytes, grant.memoryBytes);
  };
  wasm.distributed_finish_parallel_admit = (_job, remote, controlOnly, compute, bytes) => {
    admissions += 1;
    assert.equal(remote, 3);
    assert.equal(controlOnly, true, 'dedicated JS manager owns no proof compute');
    assert.equal(compute, grant.computeUnits);
    assert.equal(bytes, grant.memoryBytes);
    return true;
  };
  wasm.distributed_finish_parallel_prepare = () => {
    if (queryPublished) return null;
    queryPublished = true;
    return Uint8Array.of(1).buffer;
  };
  wasm.distributed_finish_parallel_guarded_query = () => Uint8Array.of(2).buffer;
  wasm.distributed_finish_parallel_task = () => {
    lastTask = ++issued;
    return issued <= 2 ? Uint8Array.of(issued).buffer : null;
  };
  wasm.distributed_finish_parallel_last_task_key = () => {
    const key = new Uint8Array(56);
    key[55] = lastTask;
    lastTask = 99;
    return key.buffer;
  };
  wasm.distributed_finish_parallel_assist = () => false;
  wasm.distributed_finish_parallel_found = () => false;
  wasm.distributed_finish_parallel_redundant = () => false;
  wasm.distributed_finish_parallel_merge = (_job, wire) => { proofs.push(new Uint8Array(wire)[0]); };
  wasm.distributed_finish_advance = job => {
    assert.deepEqual(proofs, [1, 2]);
    return JSON.stringify([terminal(job)]);
  };
  const runner = new ClearraProductJobRunner(wasm, 5, '', host, authority, 100,
    null, null, { create: options => new DedicatedDistributedManager(options, () => realm as unknown as Worker),
      release: () => realm.terminate() });
  try {
    assert.equal((await runner.run('clearra build minimals --workers 3', () => {})).event, 'final_response');
    assert.deepEqual(pool.sizes, [2, 3]);
    assert.equal(admissions, 1);
    assert.equal(state.resets, 1);
    assert.ok(realm.sourceMethods.includes('take_exact_task'));
    assert.ok(!realm.sourceMethods.includes('distributed_finish_parallel_last_task_key'),
      'task and key never travel as independently interleavable source requests');
    assert.deepEqual(authority.snapshot().used, { computeUnits: 0, memoryBytes: 0n });
  } finally { realm.terminate(); }
}

// N=1 remains one serial compute owner. An impossible streaming multi plan
// cannot silently manufacture a second compute lane via max(1, N-1).
{
  const { wasm, state } = sourceFixture();
  const authority = new SharedExecutionResourceAuthority(grant);
  let managersCreated = 0;
  wasm.distributed_prepare = () => ({ ...plan(), mode: 'serial', workerCount: 1,
    workerInitialization: null, verificationRequired: false });
  wasm.start_job = () => 6;
  wasm.advance_job = () => 'completed';
  wasm.drain_job_events_json = () => JSON.stringify([terminal(6)]);
  const runner = new ClearraProductJobRunner(wasm, 6, '', host, authority, 100,
    null, null, { create: () => { managersCreated += 1; throw new Error('serial work cannot create a distributed manager'); },
      release: () => {} });
  assert.equal((await runner.run('clearra setup --workers 1', () => {})).event, 'final_response');
  assert.equal(managersCreated, 0);
  assert.equal(state.resets, 1);
  assert.deepEqual(authority.snapshot().used, { computeUnits: 0, memoryBytes: 0n });

  const pool = new TestPool();
  const realm = new ManagerRealm(pool);
  wasm.distributed_prepare = () => ({ ...plan(), workerCount: 1 });
  const malformed = new ClearraProductJobRunner(wasm, 7, '', host, authority, 100,
    null, null, { create: options => new DedicatedDistributedManager(options, () => realm as unknown as Worker),
      release: () => realm.terminate() });
  try {
    await assert.rejects(malformed.run('clearra setup --workers 1', () => {}), /one-worker source.*serial/);
    assert.deepEqual(pool.sizes, []);
    assert.equal(state.resets, 2);
    assert.deepEqual(authority.snapshot().used, { computeUnits: 0, memoryBytes: 0n });
  } finally { realm.terminate(); }
}

// Cancellation after source handoff drains exactly one generation and returns
// the outer lease even if a source RPC was already queued in the message port.
{
  const pool = new TestPool();
  const realm = new ManagerRealm(pool);
  const { wasm, state } = sourceFixture();
  const authority = new SharedExecutionResourceAuthority(grant);
  const runner = new ClearraProductJobRunner(wasm, 3, '', host, authority, 100,
    null, null, { create: options => new DedicatedDistributedManager(options, () => realm as unknown as Worker),
      release: () => realm.terminate() });
  let cancelled = false;
  try {
    await assert.rejects(runner.run('clearra setup --workers 3', event => {
      if (!cancelled && event.event === 'progress' && event.progress.telemetry?.phase === 'searching') {
        cancelled = true;
        runner.cancel();
      }
    }), /cancelled/);
    assert.equal(state.resets, 1);
    assert.ok(state.cancelled > 0);
    assert.ok(pool.cancelled > 0, 'verifier descendants are explicitly closed on cancellation');
    assert.deepEqual(authority.snapshot().used, { computeUnits: 0, memoryBytes: 0n });
  } finally { realm.terminate(); }
}

// Structured source failures preserve the public WASM diagnostic and release
// all ownership; they are not interpreted as a successful empty result.
{
  const resourceReport = {
    solver_executed: false, memory_status: 'not-executed', truncated: false,
    truncation_reason: null, peak_frontier_states: 0, peak_candidate_rows: 0,
    peak_hash_buckets: 0, peak_gpu_bytes: 0, peak_cpu_bytes: 0,
    build_worker_backlog_peak: 0, coverage_rows_emitted: 0, probability_complete: false,
    execution_availability: {
      state: 'unavailable', reason: 'dense-pattern-representation-unavailable',
      surface: 'browser-wasm32', descriptor_pattern_count: '35384428800',
      dense_pattern_count: '35384428800', required_dense_bytes: '4423053600',
      required_memory_bytes: '8846107200'
    }, result_completeness: 'not-executed'
  } as const;
  const availability = {
    state: 'exhausted', reason: 'memory-budget-exceeded', surface: 'browser-wasm32',
    descriptor_pattern_count: null, dense_pattern_count: null,
    required_dense_bytes: null, required_memory_bytes: null
  } as const;
  const errors = [
    new ClearraWasmRuntimeError('E_SOURCE_FIXTURE', 'bounded source failure'),
    new ClearraWasmRuntimeError('E_WASM_DISTRIBUTED_START', 'dense representation unavailable', resourceReport),
    new SharedExecutionAvailabilityError(availability, grant, { computeUnits: 1, memoryBytes: 0n }),
    new WebAssembly.RuntimeError('memory access out of bounds')
  ];
  for (const expected of errors) {
    const pool = new TestPool();
    const realm = new ManagerRealm(pool);
    const { wasm, state } = sourceFixture();
    wasm.distributed_produce = () => { throw expected; };
    const authority = new SharedExecutionResourceAuthority(grant);
    const runner = new ClearraProductJobRunner(wasm, 4, '', host, authority, 100,
      null, null, { create: options => new DedicatedDistributedManager(options, () => realm as unknown as Worker),
        release: () => realm.terminate() });
    try {
      await assert.rejects(runner.run('clearra pc failed-queue --workers 3', () => {}), error => {
        assert.ok(error instanceof expected.constructor, 'the public error class survives both manager RPC hops');
        assert.equal((error as Error).message, expected.message);
        if (expected instanceof ClearraWasmRuntimeError) {
          assert.ok(error instanceof ClearraWasmRuntimeError);
          assert.equal(error.diagnosticCode, expected.diagnosticCode);
          assert.deepEqual(error.resourceReport, expected.resourceReport,
            'not-executed and exact resource evidence must not become generic incomplete');
        } else if (expected instanceof SharedExecutionAvailabilityError) {
          assert.ok(error instanceof SharedExecutionAvailabilityError);
          assert.deepEqual(error.availability, expected.availability);
          assert.deepEqual(error.requested, expected.requested);
          assert.deepEqual(error.available, expected.available);
        }
        return true;
      });
      assert.equal(state.resets, 1);
      assert.ok(pool.cancelled > 0);
      assert.deepEqual(authority.snapshot().used, { computeUnits: 0, memoryBytes: 0n });
    } finally { realm.terminate(); }
  }
}

// SRP rationale: execute the production Web pool and verifier entrypoint with
// actual signed assets and WASM. Node supplies isolated message transports,
// not an alternate solver, browser/OPFS proof or performance instrumentation.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { join } from 'node:path';
import { Worker } from 'node:worker_threads';
import { assertNoBuildLinks } from '../../../scripts/tools/clearra-build-policy.mjs';
import { installFileArtifactSurface } from './helpers/nodeVerifierRealm.mjs';
import { ClearraVerifierPool } from '../src/workers/ClearraVerifierPool.ts';
import { DistributedWasmJobRunner } from '../src/workers/DistributedWasmJobRunner.ts';
import { WasmJobRunner } from '../src/workers/WasmJobRunner.ts';
import { loadClearraWasmModule } from '../src/workers/clearraWasmRuntime.ts';
import { DurableDelegationAuthority, MemoryDelegationJournal }
  from '../src/workers/DurableDelegationJournal.ts';

const profiles = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const counts = [245, 246, 289, 246, 175];
const caps = { logicalProcessorCount: 4, webGpuAvailable: false, crossOriginIsolated: false,
  transferByteCap: 64 * 1024 * 1024, productRetentionByteCap: 128 * 1024 * 1024 };
const wasmRoot = await realpath(process.env.CLEARRA_REAL_ACCELERATOR_WASM_DIR);
const assetRoot = await realpath(process.env.CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR);
assertNoBuildLinks(join(wasmRoot, 'clearra_wasm.manifest.json'));
const manifest = JSON.parse(await readFile(join(wasmRoot, 'clearra_wasm.manifest.json'), 'utf8'));
assert.match(process.env.CLEARRA_SOURCE_COMMIT ?? '', /^[0-9a-f]{40}$/u);
assert.equal(manifest.build.runtime_identity.source_commit, process.env.CLEARRA_SOURCE_COMMIT);
assert.equal(manifest.build.runtime_identity.engine_build_id, process.env.CLEARRA_SOURCE_COMMIT);
for (const [entry, suffix] of [[manifest.bindings, 'js'], [manifest.wasm, 'wasm']]) {
  assert.match(entry.path, new RegExp(`^clearra_wasm(?:_bg)?\\.[0-9a-f]{24}\\.${suffix}$`, 'u'));
  assertNoBuildLinks(join(wasmRoot, entry.path));
  const bytes = await readFile(join(wasmRoot, entry.path));
  assert.equal(bytes.length, entry.bytes);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), entry.sha256);
}
installFileArtifactSurface(wasmRoot, [manifest.bindings.path, manifest.wasm.path]);
const wasm = await loadClearraWasmModule(undefined, caps);
const transports = [];
let cancelOnConsume = null;
class NodeVerifierTransport {
  onmessage = null;
  onerror = null;
  listeners = new Map();
  sent = [];
  received = [];
  admitted = { legal: 0, relation: 0 };
  constructor() {
    this.node = new Worker(process.env.CLEARRA_REAL_VERIFIER_BOOT, { workerData: {
      wasmRoot, artifacts: [manifest.bindings.path, manifest.wasm.path]
    } });
    this.node.on('message', data => {
      this.received.push(data.type);
      if (data.type === 'accelerator-synopsis-ready' && data.applied) this.admitted.legal++;
      if (data.type === 'accelerator-pack-ready' && data.applied) this.admitted.relation++;
      const event = { data };
      this.onmessage?.(event);
      for (const listener of this.listeners.get('message') ?? []) listener(event);
    });
    this.node.on('error', error => {
      const event = { message: error.message, error };
      this.onerror?.(event);
      for (const listener of this.listeners.get('error') ?? []) listener(event);
    });
    this.exited = new Promise(resolve => this.node.once('exit', resolve));
    transports.push(this);
  }
  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(listener); this.listeners.set(type, listeners);
  }
  removeEventListener(type, listener) { this.listeners.get(type)?.delete(listener); }
  postMessage(message, transfer = []) {
    if (message.type === 'accelerator-pack') {
      assert.ok(!('bytes' in message), 'a verifier must never receive a full relation pack');
      assert.ok(!message.seed || message.seed.byteLength <= 256 * 1024);
    }
    if (message.type === 'accelerator-synopsis') {
      assert.ok(!message.wire || message.wire.byteLength <= 256 * 1024);
    }
    this.sent.push(message.type);
    this.node.postMessage(message, transfer);
    if (message.type === 'consume' && cancelOnConsume) {
      const cancel = cancelOnConsume; cancelOnConsume = null; queueMicrotask(cancel);
    }
  }
  terminate() { void this.node.terminate(); }
}

const pool = new ClearraVerifierPool(() => new NodeVerifierTransport(), {
  initializationTimeoutMs: 60_000, requestStallTimeoutMs: 60_000, finishStallTimeoutMs: 60_000,
  delegationAuthority: await DurableDelegationAuthority.recover(new MemoryDelegationJournal())
});
let jobId = 1, jobs = 0, exchanges = 0;
function result(terminal) {
  assert.equal(terminal.event, 'final_response', JSON.stringify(terminal));
  assert.equal(terminal.response.status, 'success');
  assert.deepEqual(terminal.response.runtime_identity, manifest.build.runtime_identity);
  const report = terminal.search_report;
  for (const field of ['solution_count_calculated', 'solution_set_materialized',
    'solution_keys_complete', 'coverage_calculated', 'probability_calculated',
    'probability_complete', 'count_complete']) assert.equal(report[field], true, field);
  assert.equal(report.normalized_solution_keys.length, report.unique_solution_count);
  assert.equal(new Set(report.normalized_solution_keys).size, report.unique_solution_count);
  return { keys: report.normalized_solution_keys, hash: report.normalized_solution_set_hash,
    covered: report.covered_pattern_count, total: report.total_possible_pattern_count,
    probability: report.coverage_probability, probabilities: report.solution_probabilities };
}
function command(profile, workers, legal, relation, input = 'existing') {
  const args = input === 'existing'
    ? '--board-mask 0x3c0f03c0f --pieces 6 --patterns P7 --hold empty'
    : '--board-mask 0 --pieces 10 --queue IIOOOIIOOO --no-hold';
  return `clearra pc --lines 4 --height 4 ${args} --objective unique --count unique ` +
    `--solution-probabilities --backend cpu --workers ${workers} --rule ${profile} --no-tablebase ` +
    (legal ? '--legal-board ' : '--no-legal-board ') +
    (relation ? '--conditioned-reachability' : '--no-conditioned-reachability');
}
async function assets(profile, legal, relation) {
  for (const kind of [0, 1]) wasm.accelerator_remove(kind, profile);
  let synopsis = null, pack = null;
  for (const [kind, enabled] of [[0, legal], [1, relation]]) {
    if (!enabled) continue;
    const plan = wasm.accelerator_catalog(kind, profile);
    assert.equal(plan.state, 'qualified');
    const name = kind === 0 ? `legal-board-${profiles[profile]}-v2.cllb`
      : `conditioned-${profiles[profile]}.cllr`;
    assertNoBuildLinks(join(assetRoot, name));
    const bytes = await readFile(join(assetRoot, name));
    assert.equal(bytes.length, plan.payload_bytes);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), plan.payload_identity);
    wasm.accelerator_admit(kind, profile,
      bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), true);
    if (kind === 0) synopsis = { profile, wire: wasm.accelerator_export_negative_synopsis(profile, 256 * 1024), maximumPeers: 3 };
    else pack = { profile, seed: wasm.accelerator_peer_seed(profile), reservedBytes: 1024 * 1024,
      maximumPeers: 3, identity: `${plan.catalog_identity}:${plan.generation}:${plan.payload_identity}`,
      answerQueries: wire => { exchanges++; return wasm.accelerator_peer_answer(profile, wire); } };
  }
  return { synopsis, pack };
}
async function distributed(text, activated, cancel = false) {
  const admissions = () => transports.reduce((sum, transport) => ({
    legal: sum.legal + transport.admitted.legal, relation: sum.relation + transport.admitted.relation
  }), { legal: 0, relation: 0 });
  const before = admissions();
  const runner = new DistributedWasmJobRunner(wasm, jobId++, '', caps, pool,
    undefined, undefined, 'auto', activated.synopsis, activated.pack);
  try {
    await runner.acquire();
    const plan = runner.prepare(text);
    assert.equal(plan.mode, 'cpu-multi', 'a serial or ready fallback is not a pool proof');
    assert.equal(plan.workerCount, 3);
    if (cancel) cancelOnConsume = () => runner.cancel();
    const execution = runner.run(text, plan, () => {});
    if (cancel) {
      // The production runner rejects on cancellation; clearraWorker owns the
      // public cancelled event. Do not fabricate that presenter event here.
      await assert.rejects(execution, /distributed.*(?:cancelled|disposed|terminated)/u);
      assert.equal(cancelOnConsume, null, 'cancel must happen after a real remote consume was posted');
      jobs++;
      return null;
    }
    const terminal = await execution;
    jobs++;
    const after = admissions();
    if (activated.synopsis) assert.ok(after.legal > before.legal, 'real synopsis admission must not silently fall back');
    if (activated.pack) assert.ok(after.relation > before.relation, 'real relation admission must not silently fall back');
    return result(terminal);
  } finally { cancelOnConsume = null; runner.dispose(); }
}
try {
  for (let profile = 0; profile < profiles.length; profile++) {
    for (const input of ['existing', 'empty']) {
      await assets(profile, false, false);
      const baseline = result(await new WasmJobRunner(wasm).run(
        command(profiles[profile], 1, false, false, input), () => {}));
      assert.equal(baseline.keys.length, input === 'existing' ? counts[profile] : 159);
      for (const [legal, relation] of [[false, false], [true, false], [false, true], [true, true]]) {
        const activated = await assets(profile, legal, relation);
        const actual = await distributed(command(profiles[profile], 3, legal, relation, input), activated);
        assert.deepEqual(actual, baseline, `${profiles[profile]}/${input}/${legal}/${relation}`);
        console.log(`real_web_verifier_case=passed profile=${profiles[profile]} input=${input} legal=${legal} relation=${relation}`);
      }
    }
    for (const kind of [0, 1]) wasm.accelerator_remove(kind, profile);
  }
  const activated = await assets(1, true, true);
  await distributed(command('srs-plus', 3, true, true), activated, true);
  // Browser Worker.terminate stops synchronously from the host's perspective;
  // Node's transport adapter has an asynchronous exit. Drain that owned boundary
  // before creating replacement transports, without altering solver scheduling.
  await Promise.all(transports.map(transport => transport.exited));
  for (const kind of [0, 1]) wasm.accelerator_remove(kind, 1);
  const restarted = await distributed(command('srs-plus', 3, false, false), { synopsis: null, pack: null });
  assert.equal(restarted.keys.length, 246);
  assert.ok(exchanges > 0, 'actual relation broker queries must be observed');
  const sent = transports.flatMap(transport => transport.sent);
  const received = transports.flatMap(transport => transport.received);
  for (const type of ['accelerator-pack', 'accelerator-synopsis', 'initialize', 'consume', 'finish', 'delegation-offer', 'delegation-run']) {
    assert.ok(sent.includes(type), `missing actual worker transport: ${type}`);
  }
  assert.ok(received.includes('conditioned-queries'));
  assert.ok(sent.includes('conditioned-reply'));
  assert.ok(received.includes('delegation-accepted'));
  console.log(`real_web_verifier_pool=passed profiles=5 completed=${jobs - 1} cancelled=1 relation_exchanges=${exchanges}`);
} finally {
  pool.cancel();
  for (const transport of transports) transport.terminate();
  await Promise.all(transports.map(transport => transport.exited));
  wasm.distributed_reset();
}

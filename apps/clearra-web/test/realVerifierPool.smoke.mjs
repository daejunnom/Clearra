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
import { buildWorkspaceCommand, createDefaultWorkspaceRequest }
  from '../../../packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts';
import { validateProductResultPayload, validateSolutionSetArtifactPayload }
  from '../../../packages/clearra-ui/src/lib/workspace/productResultPager.ts';
import { encodeCtkSolutionKeySegment }
  from '../../../packages/clearra-ui/src/lib/workspace/solutionExport.ts';

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
let nextPrewarmDelayMs = 0;
class NodeVerifierTransport {
  onmessage = null;
  onerror = null;
  listeners = new Map();
  sent = [];
  received = [];
  admitted = { legal: 0, relation: 0 };
  prewarmDelayMs = nextPrewarmDelayMs;
  prewarmTimer = null;
  constructor() {
    nextPrewarmDelayMs = 0;
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
    if (message.type === 'prewarm' && this.prewarmDelayMs > 0) {
      const delay = this.prewarmDelayMs;
      this.prewarmDelayMs = 0;
      // Delay only a transport's initialization, never the solver or a
      // resource grant. Faster peers must continue through the same pool.
      this.prewarmTimer = setTimeout(() => {
        this.prewarmTimer = null;
        this.node.postMessage(message, transfer);
      }, delay);
    } else {
      this.node.postMessage(message, transfer);
    }
    if (message.type === 'consume' && cancelOnConsume) {
      const cancel = cancelOnConsume; cancelOnConsume = null; queueMicrotask(cancel);
    }
  }
  terminate() {
    if (this.prewarmTimer !== null) clearTimeout(this.prewarmTimer);
    this.prewarmTimer = null;
    void this.node.terminate();
  }
}

const pool = new ClearraVerifierPool(() => new NodeVerifierTransport(), {
  initializationTimeoutMs: 60_000, requestStallTimeoutMs: 60_000, finishStallTimeoutMs: 60_000,
  delegationAuthority: await DurableDelegationAuthority.recover(new MemoryDelegationJournal())
});
let jobId = 1, jobs = 0, cancelledJobs = 0, exchanges = 0;
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
function assertVisibleSevenAuthority(terminal) {
  const fields = new Map(terminal.search_report.summary_fields);
  assert.equal(fields.get('queue_knowledge'), 'visible-7');
  assert.equal(fields.get('coverage_semantics'), 'visible-seven-policy');
  assert.equal(fields.get('observation_policy_evaluated'), 'true');
  const checks = fields.get('observation_policy_action_checks');
  assert.match(checks ?? '', /^[0-9]+$/u);
  assert.ok(BigInt(checks) > 0n, 'the global observation owner must evaluate the complete physical family');
}
function productProjection(terminal) {
  assert.equal(terminal.event, 'final_response', JSON.stringify(terminal));
  assert.equal(terminal.response.status, 'success', JSON.stringify(terminal.response.diagnostics));
  assert.deepEqual(terminal.response.runtime_identity, manifest.build.runtime_identity);
  const report = terminal.search_report;
  for (const field of ['count_complete', 'solution_keys_complete']) assert.equal(report[field], true, field);
  if (terminal.response.product_result_payload) {
    assert.equal(validateProductResultPayload(terminal.response.product_result_payload), null,
      'the exact runtime product must be accepted by its production GUI payload boundary');
  }
  if (terminal.response.solution_set_artifact) {
    assert.equal(validateSolutionSetArtifactPayload(terminal.response.solution_set_artifact), null,
      'the exact runtime copy artifact must be accepted by the production GUI');
  }
  return {
    kind: terminal.response.result?.kind,
    product: terminal.response.product_result_payload ?? null,
    artifact: terminal.response.solution_set_artifact ?? null,
    keys: report.normalized_solution_keys,
    hash: report.normalized_solution_set_hash,
    count: report.unique_solution_count,
    covered: report.covered_pattern_count,
    probability: report.coverage_probability
  };
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
async function distributed(text, activated, {
  cancel = false, expectedRootTasks = false, expectedFullHeight = false,
  expectedVisibleSeven = false, projectTerminal = result
} = {}) {
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
    if (expectedRootTasks) assert.equal(plan.rootTaskParallel, true,
      'the requested root-partition product must exercise its actual root producer');
    if (expectedRootTasks) assert.equal(plan.verificationRequired, true,
      'root-partition products must execute their actual remote worker');
    if (expectedFullHeight) {
      assert.equal(plan.rootTaskParallel, false, 'full-height streams are not compact natural-root packets');
      assert.equal(plan.verificationRequired, true, 'full-height BuildUp must use the actual remote verifier');
    }
    if (cancel) cancelOnConsume = () => runner.cancel();
    const execution = runner.run(text, plan, () => {});
    if (cancel) {
      // The production runner rejects on cancellation; clearraWorker owns the
      // public cancelled event. Do not fabricate that presenter event here.
      await assert.rejects(execution, /distributed.*(?:cancelled|disposed|terminated)/u);
      assert.equal(cancelOnConsume, null, 'cancel must happen after a real remote consume was posted');
      jobs++;
      cancelledJobs++;
      return null;
    }
    const terminal = await execution;
    jobs++;
    const after = admissions();
    if (activated.synopsis) assert.ok(after.legal > before.legal, 'real synopsis admission must not silently fall back');
    if (activated.pack) assert.ok(after.relation > before.relation, 'real relation admission must not silently fall back');
    if (expectedVisibleSeven) assertVisibleSevenAuthority(terminal);
    return projectTerminal(terminal);
  } finally { cancelOnConsume = null; runner.dispose(); }
}
try {
  // The older smoke always requested per-solution probabilities and therefore
  // bypassed the ordinary GUI's PcRoots producer entirely. Use real solver
  // results, reuse one pool, and vary readiness independently of input.
  const ordinaryText = command('srs-plus', 3, false, false)
    .replace('--solution-probabilities ', '');
  const ordinaryBaseline = result(await new WasmJobRunner(wasm).run(
    ordinaryText.replace('--workers 3', '--workers 1'), () => {}));
  assert.equal(ordinaryBaseline.keys.length, 246);
  for (let repeat = 0; repeat < 4; repeat++) {
    // Alternate a cold pool with warm reuse. The delayed peer can be retired
    // at the producer boundary; completed results may never be lost or hung.
    const delayedPrewarmMs = repeat === 2 ? 250 : 0;
    if (repeat % 2 === 0) {
      pool.cancel();
      await Promise.all(transports.map(transport => transport.exited));
      nextPrewarmDelayMs = delayedPrewarmMs;
    }
    const actual = await distributed(ordinaryText, { synopsis: null, pack: null }, { expectedRootTasks: true });
    assert.deepEqual(actual, ordinaryBaseline, `ordinary-root-repeat-${repeat}`);
    console.log(`real_web_pc_root_repeat=passed repeat=${repeat} delayed_prewarm=${delayedPrewarmMs}`);
  }
  nextPrewarmDelayMs = 0;
  await distributed(ordinaryText, { synopsis: null, pack: null }, { cancel: true, expectedRootTasks: true });
  await Promise.all(transports.map(transport => transport.exited));
  const restartedOrdinary = await distributed(ordinaryText, { synopsis: null, pack: null }, { expectedRootTasks: true });
  assert.deepEqual(restartedOrdinary, ordinaryBaseline, 'a cancelled ordinary root job cannot contaminate its replacement');
  console.log('real_web_pc_root_cancel_restart=passed');
  // Do not enumerate an enormous empty 24L field. These tracked fixtures fill
  // all but 2-6 pieces while retaining real occupancy above the compact frame.
  // Each profile uses its own primitive kicks and the production opaque batch
  // transport; identical serial results are not a grant from a source string.
  const extendedRows = (await readFile(new URL('../../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv', import.meta.url), 'utf8'))
    .split(/\r?\n/u).filter(line => line && !line.startsWith('#'));
  for (const row of extendedRows) {
    const [id, height, boardHex, pieces, queue] = row.split('\t');
    for (const profile of profiles) {
      const base = `clearra pc --lines ${height} --height ${height} --board-mask 0x${boardHex} ` +
        `--pieces ${pieces} --queue ${queue} --no-hold --count unique --backend cpu ` +
        `--rule ${profile} --no-tablebase --no-legal-board --no-conditioned-reachability`;
      const baseline = result(await new WasmJobRunner(wasm).run(`${base} --workers 1`, () => {}));
      assert.ok(baseline.keys.length > 0, `${id}/${profile} must execute the connected physical family`);
      assert.ok(baseline.keys.every(key => key.startsWith(`ctk2|height=${height}|initial=`)),
        'full-height keys retain all four words instead of using a compact truncation');
      const sentBefore = transports.reduce((count, transport) => count + transport.sent.filter(type => type === 'consume').length, 0);
      const actual = await distributed(`${base} --workers 3`, { synopsis: null, pack: null }, { expectedFullHeight: true });
      assert.deepEqual(actual, baseline, `${id}/${profile}/full-height Web family`);
      const sentAfter = transports.reduce((count, transport) => count + transport.sent.filter(type => type === 'consume').length, 0);
      assert.ok(sentAfter > sentBefore, 'an actual full-height verifier task must cross the Worker transport');
      console.log(`real_web_full_height_family=passed fixture=${id} profile=${profile} workers=3`);
      if (profile === 'srs-plus') {
        // All supplied 2-6 pieces fit within the visible window, so the policy
        // must retain the oracle family and probability. A summary authority
        // assertion still proves this was evaluated, not silently normalized
        // to oracle or ORed from separately reduced verifier coverages.
        const visible = `${base} --queue-knowledge visible-7`;
        const serialTerminal = await new WasmJobRunner(wasm).run(`${visible} --workers 1`, () => {});
        assertVisibleSevenAuthority(serialTerminal);
        assert.deepEqual(result(serialTerminal), baseline, `${id}/serial visible-seven family`);
        const visibleActual = await distributed(`${visible} --workers 3`,
          { synopsis: null, pack: null }, { expectedFullHeight: true, expectedVisibleSeven: true });
        assert.deepEqual(visibleActual, baseline, `${id}/distributed visible-seven family`);
        console.log(`real_web_full_height_visible_seven=passed fixture=${id} profile=${profile} workers=3`);
        // Reuse the GUI's drawing selection contract rather than trusting a
        // raw candidate ID. The complete compiled family remains unchanged;
        // required members constrain only the common minimum-cover reducer.
        const pinnedRequest = { ...createDefaultWorkspaceRequest(), lines: Number(height),
          boardMask: BigInt(`0x${boardHex}`), queue, holdEnabled: false,
          scoreMode: 'minimum-cover', backend: 'cpu', rule: profile,
          legalBoardEnabled: false, conditionedReachabilityEnabled: false,
          pinnedSolutionKeys: [baseline.keys[0]],
          pinnedSolutionDocument: encodeCtkSolutionKeySegment([baseline.keys[0]]),
          pinnedSourceSetHash: baseline.hash };
        const pinnedBaseline = productProjection(await new WasmJobRunner(wasm).run(
          buildWorkspaceCommand({ ...pinnedRequest, workers: 1 }), () => {}));
        assert.equal(pinnedBaseline.product.contract, 'pc.pinned-minimals');
        assert.equal(pinnedBaseline.product.result_kind, 'pc-pinned-minimum-cover.v1');
        assert.deepEqual(pinnedBaseline.product.content.payload.pinned_candidate_keys, pinnedRequest.pinnedSolutionKeys,
          'the finite transport must preserve source-bound full-height required keys');
        const pinnedActual = await distributed(buildWorkspaceCommand({ ...pinnedRequest, workers: 3 }),
          { synopsis: null, pack: null }, { expectedFullHeight: true, projectTerminal: productProjection });
        assert.deepEqual(pinnedActual, pinnedBaseline, `${id}/drawing-pinned full-height minimum`);
        console.log(`real_web_full_height_pinned_minimum=passed fixture=${id} workers=3`);
      }
    }
    // Reducers do not inherit coverage flags from geometry-only tiling. Compare
    // their exact product and copy projections, leaving resource/worker timing
    // outside result identity. Native/Desktop cover the full profile matrix.
    for (const [mode, scoreMode, kind] of [['tiling', 'tiling', 'pc-tiling-family.v1'],
      ['minimals', 'minimum-cover', 'pc-minimum-cover.v2'], ['failed-queue', 'failed-queue', 'pc-failed-queue.v2'],
      ['score', 'summary', 'pc-score-summary.v2'], ['score-finder', 'score-finder', 'pc-fixed-score-witness.v2'],
      ['score-minimals', 'score-minimals', 'pc-score-portfolio.v2'], ['path', 'path', 'pc-path-family.v2']]) {
      // Exercise the same canonical argv adapter used by the GUI. In
      // particular, tiling rejects rule/TB options and score-finder owns its
      // profile. The explicit occupied scenario plus --queue lowers to a
      // FixedSequence in WebCommandRequest, not a seven-bag opening preset.
      const request = { ...createDefaultWorkspaceRequest(), lines: Number(height),
        boardMask: BigInt(`0x${boardHex}`), queue, holdEnabled: false, scoreMode,
        backend: 'cpu', legalBoardEnabled: false, conditionedReachabilityEnabled: false };
      const serial = buildWorkspaceCommand({ ...request, workers: 1 });
      const parallel = buildWorkspaceCommand({ ...request, workers: 3 });
      const baseline = productProjection(await new WasmJobRunner(wasm).run(serial, () => {}));
      assert.equal(baseline.kind, kind, `${id}/${mode} must reach its exact typed product presenter`);
      if (mode !== 'tiling' && mode !== 'failed-queue') assert.ok(baseline.product,
        `${id}/${mode} must expose the product-owned GUI payload instead of a raw scalar-only result`);
      const actual = await distributed(parallel, { synopsis: null, pack: null },
        { expectedRootTasks: mode === 'tiling', expectedFullHeight: mode !== 'tiling',
          projectTerminal: productProjection });
      assert.deepEqual(actual, baseline, `${id}/${mode}/full-height Web product and export`);
      console.log(`real_web_full_height_product=passed fixture=${id} mode=${mode} workers=3`);
    }
    if (height === '7') {
      const request = { ...createDefaultWorkspaceRequest(), lines: 7, boardMask: BigInt(`0x${boardHex}`),
        queue, holdEnabled: false, scoreMode: 'minimum-cover', preserveB2B: true, spinProfile: 'all-spin',
        backend: 'cpu', legalBoardEnabled: false, conditionedReachabilityEnabled: false };
      const baseline = productProjection(await new WasmJobRunner(wasm).run(
        buildWorkspaceCommand({ ...request, workers: 1 }), () => {}));
      assert.equal(baseline.kind, 'pc-minimum-cover.v2');
      assert.equal(baseline.product.content.payload.optimal_cardinality, '0');
      assert.deepEqual(baseline.product.content.payload.members, [], 'an exact empty family is not a missing canonical solution');
      const actual = await distributed(buildWorkspaceCommand({ ...request, workers: 3 }),
        { synopsis: null, pack: null }, { expectedFullHeight: true, projectTerminal: productProjection });
      assert.deepEqual(actual, baseline, 'exact zero-cardinality minimum keeps its sealed metadata in the Web presenter');
      console.log('real_web_full_height_empty_minimum=passed height=7 preserve_b2b=true workers=3');
    }
  }
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
  await distributed(command('srs-plus', 3, true, true), activated, { cancel: true });
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
  console.log(`real_web_verifier_pool=passed profiles=5 completed=${jobs - cancelledJobs} cancelled=${cancelledJobs} relation_exchanges=${exchanges}`);
} finally {
  pool.cancel();
  for (const transport of transports) transport.terminate();
  await Promise.all(transports.map(transport => transport.exited));
  wasm.distributed_reset();
}

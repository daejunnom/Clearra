import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { availableParallelism, cpus } from 'node:os';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { COMPACT_REPLAY_PROBE_ARGS, compactReplayProbeIdentity, verifyCompactReplayProbeStages }
  from '../tools/paired-compact-replay-probe.mjs';
import { pairedP7P4StageArgs, P7P4_STAGE_IDENTITY, P7P4_STAGE_TIMEOUT_MS, requireLocalP7P4StageProbe }
  from '../tools/paired-p7p4-stage-probe.mjs';

// The large case is explicitly local-only; no diagnostic has release authority.
// Run the same product request on both hosts, without the research-only solver.
const options = process.argv.slice(2);
const compactReplay = options.length === 1 && options[0] === '--compact-replay';
const largeP7P4 = options.length === 1 && options[0] === '--p7p4';
assert.ok(!options.length || compactReplay || largeP7P4, 'expected --compact-replay or --p7p4');
const fixture = [
  'pc', '--board-mask', '0x3c0f03c0f', '--height', '4', '--lines', '4',
  '--pieces', '6', '--patterns', 'P7', '--hold', 'empty',
  '--queue-knowledge', 'oracle', '--count', 'unique', '--backend', 'cpu',
  '--rule', 'srs-plus', '--no-build-dependency-dag', '--no-tablebase',
  '--no-legal-board', '--no-conditioned-reachability'
];
const browserProbe = fileURLToPath(new URL('../tools/run-real-chromium-cpu-worker-smoke.mjs', import.meta.url));
const cli = process.env.CLEARRA_NATIVE_STAGE_CLI;
const source = process.env.CLEARRA_SOURCE_COMMIT;
assert.equal(process.env.CLEARRA_RUNTIME_SUPERVISED, '1');
assert.equal(process.env.CLEARRA_RUNTIME_PROFILE, 'benchmark-search');
assert.ok(cli, 'CLEARRA_NATIVE_STAGE_CLI is required');
assert.match(source ?? '', /^[a-f0-9]{40}$/u);
assert.equal(process.env.CLEARRA_ENGINE_BUILD_ID, source);
const logical = availableParallelism();
let nativeSha256 = null;
if (largeP7P4) {
  requireLocalP7P4StageProbe();
  assert.ok(logical >= 3, 'P7P4 needs at least two compute workers');
  const expectedSha = process.env.CLEARRA_NATIVE_STAGE_CLI_SHA256;
  assert.match(expectedSha ?? '', /^[0-9a-f]{64}$/u, 'bind the profiling CLI artifact digest');
  nativeSha256 = createHash('sha256').update(readFileSync(cli)).digest('hex');
  assert.equal(nativeSha256, expectedSha, 'profiling CLI artifact identity');
}
const widths = compactReplay ? [1] : largeP7P4 ? [Math.min(11, logical - 1)]
  : [...new Set([1, Math.min(8, Math.max(logical - 1, 1))])];
const sampleOrder = largeP7P4 ? ['native', 'chromium'] : ['native', 'chromium', 'chromium', 'native'];
const label = compactReplay ? 'compact-1l-typed-replay' : largeP7P4 ? 'empty-4l-p7p4' : 'existing-field-4l-p7';
console.log('native_chromium_cpu_probe=' + JSON.stringify({
  source, fixture: label, logical_processors: logical, cpu_model: cpus()[0]?.model ?? null,
  workers_requested: widths, sample_order: sampleOrder, native_sha256: nativeSha256,
  profiling_builds: true, release_authority: false,
}));

function jsonLine(text, prefix) {
  const lines = text.split(/\r?\n/u).filter(line => line.startsWith(prefix));
  assert.equal(lines.length, 1, prefix + ' receipt missing or duplicated');
  return JSON.parse(lines[0].slice(prefix.length));
}

function productIdentity(report, workers) {
  assert.ok(report, 'missing product report');
  assert.equal(report.count_complete, true);
  assert.equal(Number(report.workers_used), workers, 'actual worker count differed from the paired request');
  const identity = {
    solutions: Number(report.unique_solution_count),
    solution_set: report.normalized_solution_set_hash,
    patterns_total: String(report.total_possible_pattern_count),
    patterns_covered: String(report.covered_pattern_count),
  };
  assert.ok(Number.isSafeInteger(identity.solutions) && identity.solutions > 0);
  assert.match(identity.solution_set, /^cts1:[0-9a-f]+$/u);
  assert.equal(identity.patterns_total, largeP7P4 ? P7P4_STAGE_IDENTITY.patterns : '5040');
  if (largeP7P4) {
    assert.equal(identity.solutions, P7P4_STAGE_IDENTITY.solutions);
    assert.equal(identity.solution_set, P7P4_STAGE_IDENTITY.solutionSet);
  }
  assert.match(identity.patterns_covered, /^[1-9]\d*$/u);
  return identity;
}

function runNative(args) {
  // Process wall includes command ingress and stdout export. Rust stage
  // accumulation is sampled/internal, and must not be subtracted from it.
  const started = performance.now();
  const run = spawnSync(cli, ['--format', 'json', ...args], {
    env: { ...process.env, CLEARRA_NATIVE_CPU_STAGE_PROBE: '1' },
    encoding: 'utf8', windowsHide: true, timeout: largeP7P4 ? P7P4_STAGE_TIMEOUT_MS : 90_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const wall = performance.now() - started;
  if (run.error) throw run.error;
  assert.equal(run.status, 0, 'native probe failed: ' + run.stderr.slice(-1000));
  const result = JSON.parse(run.stdout);
  const stages = jsonLine(run.stderr, 'clearra_native_cpu_stages=');
  assert.ok(stages.stages.length > 0, 'native stage capture contains no search work');
  return { event: 'final_response', status: 'success', runtime_identity: result.runtime_identity,
    process_wall_ms: wall, search: compactReplay ? null : result.summary, search_profile: stages,
    replay: compactReplay ? { kind: result.kind, summary: result.summary } : null };
}

function runChromium(args, workers) {
  const browserArgs = largeP7P4 ? ['--paired-p7p4', '--workers', String(workers)] :
    ['--paired-probe', '--workers', String(workers), '--command', ['clearra', ...args].join(' ')];
  const run = spawnSync(process.execPath, [
    browserProbe, ...browserArgs,
  ], { encoding: 'utf8', windowsHide: true, maxBuffer: 2 * 1024 * 1024,
    ...(largeP7P4 ? { timeout: P7P4_STAGE_TIMEOUT_MS + 90_000 } : {}) });
  if (run.error) throw run.error;
  // Retain exact time-out observations; do not substitute a terminal receipt.
  if (run.status !== 0) {
    for (const line of run.stdout.split(/\r?\n/u)) {
      if (line.startsWith('chromium_cpu_paired_')) console.log(line);
    }
  }
  assert.equal(run.status, 0, 'Chromium probe failed: ' + run.stderr.slice(-1000));
  return jsonLine(run.stdout, 'chromium_cpu_paired_sample=');
}

function observedStages(sample) {
  const stages = sample.search_profile?.stages;
  assert.ok(Array.isArray(stages), 'missing Core stage profile');
  return stages.filter(stage => stage.invocation_count > 0);
}

function matchedSerialStages(native, chromium) {
  function aggregate(samples) {
    const totals = new Map();
    for (const stages of samples) {
      for (const row of stages) {
        const current = totals.get(row.name) ?? { duration_ns: 0, work_item_count: 0, observations: 0 };
        current.duration_ns += row.duration_ns;
        current.work_item_count += row.work_item_count;
        current.observations += 1;
        totals.set(row.name, current);
      }
    }
    return totals;
  }
  const baseline = aggregate(native);
  const candidate = aggregate(chromium);
  return Array.from(baseline, ([stage, n]) => {
    const c = candidate.get(stage);
    if (n.observations !== 2 || c?.observations !== 2) return null;
    return { stage, native_mean_ms: n.duration_ns / 2e6, chromium_mean_ms: c.duration_ns / 2e6,
      native_mean_work_items: n.work_item_count / 2, chromium_mean_work_items: c.work_item_count / 2 };
  }).filter(Boolean).sort((a, b) =>
    (b.chromium_mean_ms - b.native_mean_ms) - (a.chromium_mean_ms - a.native_mean_ms));
}

for (const workers of widths) {
  const args = compactReplay ? COMPACT_REPLAY_PROBE_ARGS :
    largeP7P4 ? pairedP7P4StageArgs(workers) : [...fixture, '--workers', String(workers)];
  let expected;
  let problems = 0;
  const serialStages = { native: [], chromium: [] };
  let nativeLargeStages;
  let chromiumLargeStages;
  for (const [index, host] of sampleOrder.entries()) {
    try {
      const sample = host === 'native' ? runNative(args) : runChromium(args, workers);
      assert.equal(sample.event, 'final_response');
      assert.equal(sample.status, 'success');
      assert.equal(sample.runtime_identity?.source_commit, source);
      assert.equal(sample.runtime_identity?.engine_build_id, source);
      const identity = compactReplay
        ? compactReplayProbeIdentity(sample.replay?.kind, sample.replay?.summary)
        : productIdentity(sample.search, workers);
      if (expected) assert.deepEqual(identity, expected, 'native/Chromium result differs');
      else expected = identity;
      if (largeP7P4 && host === 'native') nativeLargeStages = observedStages(sample);
      if (largeP7P4 && host === 'chromium') {
        const peer = sample.search_profile?.verifier_core;
        assert.equal(peer?.scope, 'finished-geometry-verifiers', 'missing browser peer Rust profile');
        assert.equal(peer.sampled_workers, workers, 'incomplete browser peer Rust profile');
        assert.equal(peer.expected_workers, workers, 'browser peer profile width mismatch');
        assert.ok(peer.stages.length > 0, 'empty browser peer Rust profile');
        chromiumLargeStages = peer.stages;
      }
      if (workers === 1) {
        const stages = observedStages(sample);
        assert.ok(stages.length > 0, 'no observed one-worker Core stages');
        if (compactReplay) verifyCompactReplayProbeStages(stages);
        serialStages[host].push(stages);
      }
      console.log('native_chromium_cpu_sample=' + JSON.stringify({
        host, sample: index + 1, workers_requested: workers,
        workers_used: sample.search?.workers_used == null ? null : Number(sample.search.workers_used),
        backend_selected: sample.search?.backend_selected ?? null,
        packing_candidate_count: sample.search?.packing_candidate_count ?? null,
        identity, process_wall_ms: sample.process_wall_ms ?? null,
        post_to_terminal_ms: sample.post_to_terminal_ms ?? null,
        phase_observations: sample.phases ?? null, last_progress: sample.last_progress ?? null,
        search_profile: sample.search_profile ?? null,
        browser: sample.browser ?? null,
        wasm_sha256: sample.wasm_sha256 ?? null,
      }));
    } catch (error) {
      problems += 1;
      console.error('native_chromium_cpu_sample_not_comparable=' + JSON.stringify({
        host, sample: index + 1, workers, reason: error.message,
      }));
    }
  }
  console.log('native_chromium_cpu_pair=' + JSON.stringify({
    workers, comparable: problems === 0, rejected_samples: problems, release_authority: false,
    ...(workers === 1 && problems === 0 ? { serial_accumulated_stage_means:
      matchedSerialStages(serialStages.native, serialStages.chromium) } : {}),
    ...(largeP7P4 && problems === 0 ? {
      shared_stage_diagnostics: nativeLargeStages.flatMap(native => {
        const peer = chromiumLargeStages.find(row => row.name === native.name);
        return peer ? [{
          name: native.name, native_accumulated_ms: native.duration_ns / 1e6,
          chromium_peer_accumulated_ms: peer.duration_ns / 1e6,
          native_work_items: native.work_item_count, chromium_peer_work_items: peer.work_item_count
        }] : [];
      }),
      timing_scope: 'nested stage intervals accumulate across threads/peers; this is not elapsed wall time',
    } : {}),
  }));
  if (problems) process.exitCode = 1;
}

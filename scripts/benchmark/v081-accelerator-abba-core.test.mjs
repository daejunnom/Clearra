import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
  abbaSchedule, assertIsolatedAcceleratorCounters, assertWorkerExecution, classifyExecution, commandArgs,
  compareWorkerReceipts, semanticIdentity,
  mergeBatchReceipts, summarizeSamples, validateCases, sampleFitsRemainingLease,
} from './v081-accelerator-abba-core.mjs';

const cases = validateCases(JSON.parse(readFileSync(
  new URL('./fixtures/v081-accelerator-cases.json', import.meta.url), 'utf8')));
const fixture = cases.find(entry => entry.id === 'pc-p7p4-srs-plus');

test('1800-second samples retain their timeout within an independently bounded outer lease', () => {
  assert.equal(sampleFitsRemainingLease(0, 1_800_000, 7_200_000, 120_000), true);
  assert.equal(sampleFitsRemainingLease(5_280_000, 1_800_000, 7_200_000, 120_000), true);
  assert.equal(sampleFitsRemainingLease(5_280_001, 1_800_000, 7_200_000, 120_000), false);
  assert.throws(() => sampleFitsRemainingLease(-1, 1_800_000, 7_200_000, 120_000));
});

test('fixture matrix covers all five kick profiles and a BuildUp product path', () => {
  assert.deepEqual(new Set(cases.filter(entry => entry.size === 'large').map(entry => entry.profile)),
    new Set(['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']));
  assert.ok(cases.some(entry => entry.args[0] === 'build'));
  assert.ok(cases.some(entry => entry.size === 'small'));
  for (const entry of cases.filter(candidate => candidate.size === 'large')) {
    assert.ok(entry.args.includes('P7P4'));
    assert.ok(entry.args.includes('--board-mask'));
    assert.ok(!entry.args.includes('--deterministic'));
    assert.equal(entry.expected['summary.count_complete'], true);
  }
  for (const entry of cases) {
    assert.ok(!entry.args.includes('--max-candidates'),
      `${entry.id}: an explicit candidate cap forces serial resource accounting`);
  }
});

test('candidate caps cannot silently turn a requested parallel benchmark into serial accounting', () => {
  for (const options of [['--max-candidates', '100000000'], ['--max-candidates=100000000']]) {
    assert.throws(() => validateCases({
      schema: 'clearra.v081.accelerator-benchmark-cases.v1',
      cases: [{ ...fixture, args: [...fixture.args, ...options] }],
    }), /benchmark-controlled command argument/);
  }
});

test('each ABBA round is symmetric and alternating rounds counterbalance first position', () => {
  assert.deepEqual(abbaSchedule(2).map(sample => sample.arm), [
    'baseline', 'treatment', 'treatment', 'baseline',
    'treatment', 'baseline', 'baseline', 'treatment',
  ]);
});

test('same-binary variants differ only by the two exact accelerator switches', () => {
  const baseline = commandArgs(fixture, 'combined', 'baseline', 11);
  const treatment = commandArgs(fixture, 'combined', 'treatment', 11);
  assert.deepEqual(baseline.slice(0, -2), treatment.slice(0, -2));
  assert.deepEqual(baseline.slice(-2), ['--no-legal-board', '--no-conditioned-reachability']);
  assert.deepEqual(treatment.slice(-2), ['--legal-board', '--conditioned-reachability']);
  assert.deepEqual(commandArgs(fixture, 'legal', 'treatment', 2).slice(-2),
    ['--legal-board', '--no-conditioned-reachability']);
  assert.deepEqual(commandArgs(fixture, 'conditioned', 'treatment', 1).slice(-2),
    ['--no-legal-board', '--conditioned-reachability']);
});

test('missing or incomplete exact semantics cannot masquerade as parity', () => {
  const summary = {
    backend_selected: 'wasm-cpu',
    unique_solution_count: 456923,
    normalized_solution_set_hash: fixture.expected['summary.normalized_solution_set_hash'],
    count_complete: true,
    packing_candidate_set_digest: 'candidate:test',
  };
  assert.deepEqual(semanticIdentity({ summary }, fixture), Object.fromEntries(
    Object.entries(summary).filter(([key]) => key !== 'backend_selected')));
  assert.deepEqual(semanticIdentity({ summary: { ...summary,
    solution_keys_complete: false, tiling_initial_page_covers_family: false } }, fixture),
  Object.fromEntries(Object.entries(summary).filter(([key]) => key !== 'backend_selected')));
  assert.throws(() => semanticIdentity({ summary: { ...summary,
    backend_selected: 'native-c-core' } }, fixture));
  assert.throws(() => semanticIdentity({ summary: { ...summary, count_complete: false } }, fixture));
  assert.throws(() => semanticIdentity({ summary: { ...summary,
    normalized_solution_set_hash: 'not-calculated' } }, fixture));
  assert.throws(() => semanticIdentity({ summary: { ...summary,
    unique_solution_count: 456922 } }, fixture));
});

test('timeouts and resource limits are censored, not completed times or UNSAT', () => {
  assert.deepEqual(classifyExecution({ timedOut: true }),
    { state: 'censored', reason: 'timeout' });
  assert.deepEqual(classifyExecution({ timedOut: false, overflow: false, exitCode: 1,
    stdout: '{"error":{"code":"E_PRODUCT_RESOURCE_LIMIT"}}' }),
    { state: 'censored', reason: 'E_PRODUCT_RESOURCE_LIMIT' });
  assert.equal(classifyExecution({ timedOut: false, overflow: true }).state, 'invalid');
  const summary = summarizeSamples([
    { arm: 'baseline', state: 'complete', wall_ms: 100 },
    { arm: 'treatment', state: 'censored', wall_ms: null },
  ]);
  assert.equal(summary.arms.treatment.complete, 0);
  assert.equal(summary.arms.treatment.p95_ms, null);
  assert.equal(summary.p95_improvement_fraction, null);
});

test('worker underuse is rejected instead of counted as an ABBA timing sample', () => {
  assert.doesNotThrow(() => assertWorkerExecution({ reported_workers_used: 11,
    cpu_parallel_execution: true, parallel_active_workers: 11 }, 11, true));
  assert.doesNotThrow(() => assertWorkerExecution({ reported_workers_used: 1,
    cpu_parallel_execution: false }, 1));
  assert.throws(() => assertWorkerExecution({ reported_workers_used: 1,
    cpu_parallel_execution: false }, 11), /worker underuse/);
  assert.throws(() => assertWorkerExecution({ reported_workers_used: 11,
    cpu_parallel_execution: false, parallel_active_workers: 11 }, 11), /did not report CPU parallel/);
  assert.throws(() => assertWorkerExecution({ reported_workers_used: 11,
    cpu_parallel_execution: true, parallel_active_workers: 4 }, 11, true), /active-worker underuse/);
  assert.throws(() => assertWorkerExecution({ reported_workers_used: null }, 11), /omitted/);
});

test('disabled asset activity cannot be mistaken for an isolated treatment', () => {
  const counters = {
    legal_board_verified_negative_prunes: 0,
    conditioned_complete_hits: 0,
    conditioned_misses: 0,
    conditioned_reachability_requested: true,
    conditioned_reachability_policy_enabled: true,
    conditioned_reachability_snapshot_active: true,
    conditioned_lookup_attempts: 1,
    conditioned_out_of_scope: 1,
    conditioned_no_query_context: 0,
    conditioned_empty_entry_sets: 0,
    conditioned_unknown: 0,
    conditioned_snapshot_mismatch: 0,
    conditioned_invalid_asset: 0,
  };
  assert.doesNotThrow(() => assertIsolatedAcceleratorCounters(counters,
    'legal', 'baseline', true));
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...counters,
    legal_board_verified_negative_prunes: 1 }, 'legal', 'baseline', true));
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...counters,
    conditioned_complete_hits: 1 }, 'legal', 'treatment', true));
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...counters,
    conditioned_complete_hits: null }, 'combined', 'treatment', true));
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...counters,
    conditioned_reachability_requested: false }, 'conditioned', 'treatment', true), /not propagated/);
});

test('zero conditioned hits are classified by activation, snapshot and lookup evidence', () => {
  const base = { legal_board_verified_negative_prunes: 0,
    conditioned_complete_hits: 0, conditioned_misses: 0,
    conditioned_reachability_requested: true,
    conditioned_reachability_policy_enabled: true,
    conditioned_reachability_snapshot_active: true,
    conditioned_lookup_attempts: 0, conditioned_no_query_context: 4,
    conditioned_cache_short_circuits: 0,
    conditioned_empty_entry_sets: 2, conditioned_out_of_scope: 0,
    conditioned_unknown: 0, conditioned_snapshot_mismatch: 0,
    conditioned_invalid_asset: 0 };
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...base,
    conditioned_reachability_requested: false }, 'conditioned', 'treatment', true), /not propagated/);
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...base,
    conditioned_reachability_policy_enabled: false }, 'conditioned', 'treatment', true), /policy gate/);
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...base,
    conditioned_reachability_snapshot_active: false }, 'conditioned', 'treatment', true), /no active/);
  assert.throws(() => assertIsolatedAcceleratorCounters(base,
    'conditioned', 'treatment', true), /no provider lookup occurred/);
  assert.throws(() => assertIsolatedAcceleratorCounters({ ...base,
    conditioned_lookup_attempts: 5, conditioned_out_of_scope: 5 },
  'conditioned', 'treatment', true), /all conditioned lookups.*OutOfScope/);
});

test('qualified empty-origin 4L legal treatment cannot be an installed no-op', () => {
  const noActivity = { legal_board_verified_negative_prunes: 0,
    conditioned_complete_hits: 0, conditioned_misses: 0 };
  assert.throws(() => assertIsolatedAcceleratorCounters(noActivity,
    'legal', 'treatment', true, true), /zero verified negative prunes/);
  assert.doesNotThrow(() => assertIsolatedAcceleratorCounters(noActivity,
    'legal', 'baseline', true, true));
  // Initial fields and non-4L requests legitimately have no exact legal domain.
  assert.doesNotThrow(() => assertIsolatedAcceleratorCounters(noActivity,
    'legal', 'treatment', true, false));
  assert.doesNotThrow(() => assertIsolatedAcceleratorCounters({ ...noActivity,
    legal_board_verified_negative_prunes: 1 }, 'legal', 'treatment', true, true));
});

test('local benchmark startup uses the App facade without a CLI to Core dependency', () => {
  const read = path => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');
  const cli = read('crates/clearra-cli/Cargo.toml');
  assert.match(cli, /local-search-ab = \["clearra-app\/local-search-ab"\]/u);
  assert.doesNotMatch(cli, /^clearra-core-executor\s*=/mu);
  assert.match(read('crates/clearra-app/Cargo.toml'),
    /local-search-ab = \["clearra-core-executor\/local-search-ab"\]/u);
  const entry = read('crates/clearra-cli/src/main.rs');
  assert.match(entry, /clearra_app::configure_local_product_search_benchmark\(\)/u);
  assert.doesNotMatch(entry, /clearra_core_executor::/u);
});

test('p95 gate review needs sufficient complete uncensored samples in both arms', () => {
  const samples = Array.from({ length: 20 }, (_, index) => [
    { arm: 'baseline', state: 'complete', wall_ms: 100 + index },
    { arm: 'treatment', state: 'complete', wall_ms: 80 + index },
  ]).flat();
  const summary = summarizeSamples(samples);
  assert.equal(summary.decision, 'eligible-for-gate-review');
  assert.equal(summary.arms.baseline.p95_ms, 118);
  assert.equal(summary.arms.treatment.p95_ms, 98);
  assert.ok(summary.p95_improvement_fraction > 0);
});

test('fixture rejects embedded toggles, tablebase, output paths and ambiguous oracles', () => {
  assert.throws(() => validateCases({ schema: 'clearra.v081.accelerator-benchmark-cases.v1',
    cases: [{ ...fixture, args: [...fixture.args, '--tablebase'] }] }));
  assert.throws(() => validateCases({ schema: 'clearra.v081.accelerator-benchmark-cases.v1',
    cases: [{ ...fixture, args: [...fixture.args, '--deterministic'] }] }));
  assert.throws(() => validateCases({ schema: 'clearra.v081.accelerator-benchmark-cases.v1',
    cases: [{ ...fixture, expected: { 'summary.not_compared': 1 } }] }));
});

test('Build portfolio must prove complete source, rows, and exact minimum', () => {
  const entry = cases.find(candidate => candidate.id === 'build-cover-p7p3-srs-plus');
  const summary = {
    source_candidate_count: 2, selected_candidate_count: 1,
    pattern_count: 4, required_pattern_count: 4,
    normalized_solution_set_hash: 'cts1:build', canonical_first_candidate_id: 'candidate',
    completeness: { source_universe_complete: true, coverage_rows_complete: true,
      exact_minimum_proven: true },
  };
  const identity = semanticIdentity({ summary }, entry);
  assert.equal(identity['completeness.exact_minimum_proven'], true);
  assert.throws(() => semanticIdentity({ summary: { ...summary,
    completeness: { ...summary.completeness, exact_minimum_proven: false } } }, entry));
  assert.throws(() => semanticIdentity({ summary: { ...summary, completeness: {} } }, entry));
});

test('batch merge rejects mixed generation and keeps release gate unclaimed', () => {
  const samples = Array.from({ length: 20 }, () => [
    { arm: 'baseline', state: 'complete', wall_ms: 100, reported_workers_used: 2, cpu_parallel_execution: true, parallel_active_workers: 2 },
    { arm: 'treatment', state: 'complete', wall_ms: 80, reported_workers_used: 2, cpu_parallel_execution: true, parallel_active_workers: 2 },
  ]).flat();
  const receipt = {
    manifest: {
      schema: 'clearra.v081.accelerator-abba-receipt.v2',
      executable_sha256: 'binary', fixture_sha256: 'fixture', case: fixture,
      pair: 'legal', workers_requested: 2,
      assets: { legal: { generation_identity: 'a' },
        conditioned: { generation_identity: 'b' } },
      timing_scope: 'fresh-native-process-start-through-exit',
      process_tree_peak_memory: { metric: 'working_set_bytes', interval_ms: 250 },
    },
    samples,
    semantic_identity: { unique_solution_count: 456923 },
    runtime_identity: { source_commit: 'commit' },
    result: 'complete-local-batch',
  };
  const analysis = mergeBatchReceipts([receipt]);
  assert.equal(analysis.overall_p95_result, 'pass');
  assert.equal(analysis.mechanism_observation.legal_board_verified_negative_prunes,
    'not-reported');
  assert.equal(analysis.release_gate, 'not-evaluated');
  assert.equal(mergeBatchReceipts([{ ...receipt, result: 'invalid-or-censored' }])
    .overall_p95_result, 'not-evaluable');
  assert.throws(() => mergeBatchReceipts([receipt, {
    ...receipt, manifest: { ...receipt.manifest,
      assets: { ...receipt.manifest.assets,
        legal: { generation_identity: 'different' } } },
  }]));
  const withOwnership = { ...receipt, manifest: { ...receipt.manifest,
    worker_memory_accounting: { scope: 'native-worker-exit-private-retained-payload-sum.v2' } } };
  assert.throws(() => mergeBatchReceipts([receipt, withOwnership]), /mixed/);
  assert.throws(() => compareWorkerReceipts([receipt, withOwnership]), /mixed/);
  const memoReference = { ...receipt, manifest: { ...receipt.manifest, memo_selection: { storage: 'reference', layout: 'flat' } } };
  const memoDifferent = { ...memoReference, manifest: { ...memoReference.manifest, memo_selection: { storage: 'reference', layout: 'state-major' } } };
  assert.throws(() => mergeBatchReceipts([memoReference, memoDifferent]), /mixed/);
  assert.throws(() => compareWorkerReceipts([memoReference, memoDifferent]), /mixed/);
  assert.equal(compareWorkerReceipts([receipt]).parity, 'incomplete-worker-matrix');
  assert.equal(compareWorkerReceipts([1, 2, 11].map(workers => ({
    ...receipt, manifest: { ...receipt.manifest, workers_requested: workers },
    samples: receipt.samples.map(sample => ({ ...sample, reported_workers_used: workers,
      cpu_parallel_execution: workers > 1, parallel_active_workers: workers })),
  }))).parity, 'complete-and-equal');
  assert.throws(() => compareWorkerReceipts([receipt, {
    ...receipt, manifest: { ...receipt.manifest, workers_requested: 11 },
    semantic_identity: { unique_solution_count: 1 },
  }]));
  assert.throws(() => mergeBatchReceipts([{ ...receipt, samples: receipt.samples.map(sample => ({
    ...sample, reported_workers_used: 1, cpu_parallel_execution: false,
  })) }]), /worker underuse/);
  const withCounters = mergeBatchReceipts([{ ...receipt,
    samples: samples.map(sample => ({ ...sample,
      legal_board_verified_negative_prunes: sample.arm === 'treatment' ? 2 : 0,
      conditioned_complete_hits: sample.arm === 'treatment' ? 3 : 0,
      conditioned_misses: 0 })) }]);
  assert.equal(withCounters.mechanism_observation.legal_board_verified_negative_prunes, 'observed');
  assert.equal(withCounters.mechanism_observation.conditioned_complete_hits, 'observed');
});

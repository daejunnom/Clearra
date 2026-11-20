import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { compilePlan, summarize, validateSample } from './run-native-rust-cpu-abba.mjs';

const C = 'native-c';
const R = 'rust-cpu';

function evidence(arm, workers) {
  const native = arm === C;
  return {
    schema: 'clearra.native-rust-cpu-abba.sample.v1',
    case: 'full-p7p4',
    arm,
    requested_workers: workers,
    available_parallelism: 11,
    cpu_warmup: false,
    solver_elapsed_ns: '987654',
    linux_process_rss_hwm_bytes: 128,
    semantic: {
      count_complete: true, resource_truncated: false, probability_complete: true,
      solution_count: 456923, solution_set_hash: 'cts1:98ebe8726537b29f',
      ordered_key_sha256: null, coverage_sha256: null,
      pattern_count: 4233600, covered_pattern_count: 4233600,
      build_variant_count: native ? '1234' : '5678',
    },
    backend: {
      selected: native ? 'cpu' : 'wasm-cpu',
      fallback: false,
      packing_execution_source: native ? 'native-cpu-packing' : null,
      buildup_execution_source: native ? 'native-cpu-buildup' : null,
      reported_workers: workers, reported_active_workers: native ? null : workers,
      native_geometry_dag_workers: null, native_candidate_workers: null, native_buildup_workers: null,
      parallel: native ? null : workers > 1,
    },
  };
}

test('plan binds exactly C/Rust/Rust/C, one timeout, and deliberate serial fixture', () => {
  const options = [
    '--manager', resolve('clearra-manage'), '--binary', resolve('native_rust_cpu_abba'),
    '--output', resolve('benchmark-test-placeholder'),
  ];
  const full = compilePlan([...options, '--case', 'full-p7p4', '--workers', '11', '--timeout-seconds', '1800']);
  assert.deepEqual(full.schedule, [C, R, R, C]);
  assert.equal(full.workers, 11);
  assert.equal(full.timeout, 1800);
  assert.throws(() => compilePlan([...options, '--case', 'fixed-2l', '--workers', '2', '--timeout-seconds', '60']), /one worker/);
  assert.throws(() => compilePlan([...options, '--case', 'full-p7p4', '--workers', '11', '--timeout-seconds', '1801']), /timeout/);
  assert.throws(() => compilePlan([...options, '--case', 'full-p7p4', '--workers', '11', '--max-candidates', '1000000']), /option/);
});

test('C packing width remains limited evidence, not proof of C BuildUp active workers', () => {
  const c = validateSample(evidence(C, 11), 'full-p7p4', C, 11);
  const rust = validateSample(evidence(R, 11), 'full-p7p4', R, 11);
  assert.equal(c.workerEvidence, 'native-packing-width-only');
  assert.equal(rust.workerEvidence, 'rust-useful-participation');
  assert.deepEqual(c.identity, rust.identity, 'incomparable diagnostic variant counts are not result identity');
});

test('CountUnique proves family parity even when only Rust emits coverage', () => {
  const native = evidence(C, 11);
  const rust = evidence(R, 11);
  native.semantic.probability_complete = false;
  native.semantic.covered_pattern_count = 0;
  rust.semantic.coverage_sha256 = 'a'.repeat(64);
  const c = validateSample(native, 'full-p7p4', C, 11).identity;
  const r = validateSample(rust, 'full-p7p4', R, 11).identity;
  assert.deepEqual(c, r);
  assert(!Object.hasOwn(c, 'covered_pattern_count'), 'diagnostic coverage is never promoted to parity');
  native.semantic.count_complete = false;
  assert.throws(() => validateSample(native, 'full-p7p4', C, 11), /complete count/);
});

test('positive fixed queue still requires verified complete coverage', () => {
  const sample = evidence(C, 1);
  sample.case = 'fixed-2l';
  sample.semantic.solution_count = 4;
  sample.semantic.pattern_count = 1;
  sample.semantic.covered_pattern_count = 1;
  assert.equal(validateSample(sample, 'fixed-2l', C, 1).identity.covered_pattern_count, 1);
  sample.semantic.probability_complete = false;
  assert.throws(() => validateSample(sample, 'fixed-2l', C, 1), /complete probability/);
  sample.semantic.probability_complete = true;
  sample.semantic.covered_pattern_count = 0;
  assert.throws(() => validateSample(sample, 'fixed-2l', C, 1));
});

test('semantic or actual runtime drift is a rejection rather than a fast observation', () => {
  for (const mutate of [
    sample => { sample.semantic.solution_count--; },
    sample => { sample.semantic.solution_set_hash = 'cts1:ffffffffffffffff'; },
    sample => { sample.semantic.count_complete = false; },
    sample => { sample.semantic.pattern_count = 5040; },
    sample => { sample.semantic.resource_truncated = true; },
    sample => { sample.backend.fallback = true; },
    sample => { sample.backend.selected = 'cpu'; },
    sample => { sample.backend.reported_workers = 1; },
    sample => { sample.backend.reported_active_workers = 1; },
    sample => { sample.available_parallelism = 2; },
  ]) {
    const sample = evidence(R, 11);
    mutate(sample);
    assert.throws(() => validateSample(sample, 'full-p7p4', R, 11));
  }
  const fakeC = evidence(C, 11);
  fakeC.backend.buildup_execution_source = null;
  assert.throws(() => validateSample(fakeC, 'full-p7p4', C, 11));
});

function observation(arm, timeNs, overrides) {
  return {
    arm, state: 'complete', performance_eligible: true, peak_memory_bytes: 128,
    solver: { solver_elapsed_ns: String(timeNs) },
    ...overrides,
  };
}

test('time ratios include only complete and undisturbed ABBA pairs', () => {
  const observations = [
    observation(C, 1000), observation(R, 700), observation(R, 800), observation(C, 1200),
  ];
  assert.equal(summarize(observations).no_regression_observed, true);
  assert.equal(summarize(observations).aggregate[C].peak_memory_bytes, 128);
  assert.equal(summarize(observations.slice(0, 3)).no_regression_observed, null);
  const regressed = [...observations];
  regressed[1] = observation(R, 1300);
  assert.equal(summarize(regressed).no_regression_observed, false, 'an observed tail regression is not masked by a better mean');
  regressed[1] = observation(R, 700, { performance_eligible: false });
  assert.equal(summarize(regressed).result, 'complete-disturbed-abba');
  assert.equal(summarize(regressed).solver_mean_ratio, null);
  regressed[1] = observation(R, 700, { state: 'censored' });
  assert.equal(summarize(regressed).result, 'incomplete-abba');
  assert.equal(summarize(regressed).aggregate[R].mean_solver_ms, null);
});

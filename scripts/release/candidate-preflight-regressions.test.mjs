import assert from 'node:assert/strict';
import test from 'node:test';
import { CANDIDATE_REF, CANDIDATE_RUST_REGRESSIONS, assertCandidateHost,
  candidateRegressionArguments, assertNonemptyRustSuccess, runCandidateRegressions,
} from './candidate-preflight-regressions.mjs';

const SHA = 'a'.repeat(40);
const ENV = { GITHUB_ACTIONS: 'true', GITHUB_REF: CANDIDATE_REF, GITHUB_SHA: SHA,
  CLEARRA_SOURCE_COMMIT: SHA, CLEARRA_ENGINE_BUILD_ID: SHA, RUST_MIN_STACK: '16777216' };
const PASSED = { status: 0, stdout: 'test result: ok. 1 passed; 0 failed; 0 ignored; 5 filtered out;\n', stderr: '' };

test('candidate filters are closed, cover nine observed failures and include enabled parallel Core regressions', () => {
  assert.equal(CANDIDATE_RUST_REGRESSIONS.filter((entry) => entry.exact).length, 9);
  assert.equal(new Set(CANDIDATE_RUST_REGRESSIONS.map((entry) => `${entry.package}:${entry.filter}`)).size, CANDIDATE_RUST_REGRESSIONS.length);
  for (const entry of CANDIDATE_RUST_REGRESSIONS) {
    assert.match(entry.filter, /^[a-z_][a-z_0-9:]*/u);
    const args = candidateRegressionArguments(entry);
    assert.ok(args.includes('--lib'));
    assert.ok(args.includes('--locked'));
    assert.ok(args.includes('--test-threads=1'));
    assert.ok(!args.some((arg) => ['--workspace', '--all', '--all-targets', '--ignored'].includes(arg)));
  }
  assert.ok(candidateRegressionArguments(CANDIDATE_RUST_REGRESSIONS.find((entry) => entry.package === 'clearra-core-executor')).includes('parallel'));
  assert.throws(() => candidateRegressionArguments({ package: 'clearra-app', filter: '' }), /Unknown/u);
});

test('local, main, unpaired, small-stack or acceptance environments cannot launch generated tests', () => {
  assertCandidateHost(ENV, 'win32');
  for (const invalid of [
    { ...ENV, GITHUB_ACTIONS: 'false' }, { ...ENV, GITHUB_REF: 'refs/heads/main' },
    { ...ENV, CLEARRA_SOURCE_COMMIT: 'b'.repeat(40) }, { ...ENV, RUST_MIN_STACK: '2097152' },
    { ...ENV, CLEARRA_ACCEPTED_RUN_ID: '1' },
  ]) {
    let calls = 0;
    assert.throws(() => runCandidateRegressions({ environment: invalid, platform: 'win32', spawnImplementation() { calls += 1; } }));
    assert.equal(calls, 0);
  }
  assert.throws(() => assertCandidateHost(ENV, 'linux'));
});

test('focused manifest includes guarded BuildCover results and moved PC score preparation contracts', () => {
  for (const filter of [
    'build_solution_probability_result::build_v2_result::tests::',
    'pc_score_minimum_cover_contract_tests::cooperative_score_minimum_',
  ]) {
    const matches = CANDIDATE_RUST_REGRESSIONS.filter((entry) => entry.filter === filter);
    assert.equal(matches.length, 1);
    assert.equal(matches[0].package, 'clearra-app');
    assert.equal(matches[0].exact, false);
    assert.equal(matches[0].parallel, true);
  }
});

test('zero tests, only ignored, malformed results and nonzero process outcomes fail closed', () => {
  assertNonemptyRustSuccess(PASSED);
  for (const invalid of [
    { ...PASSED, status: 1 }, { ...PASSED, signal: 'SIGTERM' }, { ...PASSED, error: new Error('spawn') },
    { ...PASSED, stdout: '' }, { ...PASSED, stdout: 'test result: ok. 0 passed; 0 failed; 1 ignored;\n' },
    { ...PASSED, stdout: PASSED.stdout + PASSED.stdout },
  ]) assert.throws(() => assertNonemptyRustSuccess(invalid));
});

test('all fixed selections execute once without a shell and failures remain non-accepting', () => {
  const calls = [];
  runCandidateRegressions({ environment: ENV, platform: 'win32', write() {},
    spawnImplementation(command, args, options) { calls.push({ command, args, options }); return PASSED; },
  });
  assert.equal(calls.length, CANDIDATE_RUST_REGRESSIONS.length);
  assert.ok(calls.every(({ command, options }) => command === 'cargo' && options.shell === false && options.windowsHide));
  let attempted = 0;
  assert.throws(() => runCandidateRegressions({ environment: ENV, platform: 'win32', write() {},
    spawnImplementation() { attempted += 1; return { ...PASSED, status: attempted === 1 ? 1 : 0 }; },
  }), /failed \(1\)/u);
  assert.equal(attempted, CANDIDATE_RUST_REGRESSIONS.length);
});

test('one compilation failure blocks only the same package and features, never marks skipped tests passed', () => {
  const calls = [];
  const messages = [];
  const expectedBlocked = CANDIDATE_RUST_REGRESSIONS.filter((entry) => entry.package === 'clearra-app' && entry.parallel).length - 1;
  assert.throws(() => runCandidateRegressions({ environment: ENV, platform: 'win32',
    write(message) { messages.push(message); },
    spawnImplementation(command, args) {
      calls.push(args);
      return calls.length === 1
        ? { status: 101, stdout: '', stderr: 'error: could not compile `clearra-app` (lib test) due to 1 previous error\n' }
        : PASSED;
    },
  }), new RegExp(`failed \\(1\\).*blocked \\(${expectedBlocked}\\)`));
  assert.equal(calls.filter((args) => args.includes('clearra-app')).length, 1);
  assert.equal(calls.length, CANDIDATE_RUST_REGRESSIONS.length - expectedBlocked);
  assert.equal(messages.filter((message) => message.startsWith('candidate_regression=blocked')).length, expectedBlocked);
  assert.ok(!messages.some((message) => message.includes('candidate_regressions=passed')));
});

test('replay DP, exact-zero equivalence, warm reuse and Desktop feature seams are included', () => {
  for (const [packageName, filter] of [
    ['clearra-postprocess', 'exact_replay_language::tests::'],
    ['clearra-app', 'cooperative_pc_replay_p7_ctk3_'],
    ['clearra-coverage', 'softmax_positive_zero_skip_'],
    ['clearra-coverage', 'zero_row_scatter_'],
    ['clearra-coverage', 'root_conditional_row_pruning_'],
    ['clearra-coverage', 'conditional_root_rows_filter_actual_pivot_'],
    ['clearra-wasm-abi', 'warm_minimum_to_geometry_'],
    ['clearra-wasm-abi', 'geometry_replacement_rejects_'],
    ['clearra-wasm-abi', 'exact_replacement_requires_geometry_'],
  ]) assert.ok(CANDIDATE_RUST_REGRESSIONS.some((entry) => entry.package === packageName && entry.filter === filter));
  const desktop = CANDIDATE_RUST_REGRESSIONS.find((entry) => entry.package === 'clearra-gui-host');
  assert.ok(candidateRegressionArguments(desktop).includes('wasm-cpu-runtime'));
});

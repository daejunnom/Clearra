import assert from 'node:assert/strict';
import test from 'node:test';
import { CANDIDATE_RUST_REGRESSIONS, assertCandidateHost,
  candidateRegressionArguments, assertNonemptyRustSuccess, runCandidateRegressions,
} from './candidate-preflight-regressions.mjs';

const SHA = 'a'.repeat(40);
const REF = 'refs/heads/codex/next-candidate';
const REPOSITORY = 'daejunnom/Clearra';
const workflowRef = (ref) => `${REPOSITORY}/.github/workflows/candidate-preflight.yml@${ref}`;
const ENV = { GITHUB_ACTIONS: 'true', GITHUB_REF: REF, GITHUB_SHA: SHA,
  GITHUB_REF_TYPE: 'branch', GITHUB_REPOSITORY: REPOSITORY, GITHUB_JOB: 'candidate-rust',
  GITHUB_EVENT_NAME: 'workflow_dispatch', GITHUB_WORKFLOW_REF: workflowRef(REF),
  CLEARRA_SOURCE_COMMIT: SHA, CLEARRA_ENGINE_BUILD_ID: SHA, RUST_MIN_STACK: '16777216' };
const PASSED = { status: 0, stdout: 'test result: ok. 1 passed; 0 failed; 0 ignored; 5 filtered out;\n', stderr: '' };

test('candidate filters are closed and include supported product and parallel Core regressions', () => {
  assert.equal(new Set(CANDIDATE_RUST_REGRESSIONS.map((entry) => `${entry.package}:${entry.filter}`)).size, CANDIDATE_RUST_REGRESSIONS.length);
  for (const entry of CANDIDATE_RUST_REGRESSIONS) {
    assert.match(entry.filter, /^[a-z_][a-z_0-9:]*/u);
    const args = candidateRegressionArguments(entry);
    assert.ok(args.includes('--lib'));
    assert.ok(args.includes('--locked'));
    assert.ok(args.includes('--test-threads=1'));
    assert.ok(!args.some((arg) => ['--workspace', '--all', '--all-targets', '--ignored'].includes(arg)));
  }
  const core = CANDIDATE_RUST_REGRESSIONS.filter((entry) => entry.package === 'clearra-core-executor');
  assert.ok(core.some((entry) => entry.filter === 'product_pc_placement_dependencies_match_concrete_p7_coverage'));
  const coreFeatures = core.map((entry) => {
    const args = candidateRegressionArguments(entry);
    return args[args.indexOf('--features') + 1];
  });
  assert.equal(new Set(coreFeatures).size, 1, 'Core focused cases share one test build');
  assert.deepEqual(coreFeatures[0].split(','), ['parallel', 'placement-dependency-pc']);
  const failedQueue = CANDIDATE_RUST_REGRESSIONS.find((entry) =>
    entry.filter === 'product_capability_contract_tests::compact_pc_failed_queue_');
  assert.ok(failedQueue);
  assert.equal(failedQueue.package, 'clearra-app');
  assert.ok(!candidateRegressionArguments(failedQueue).includes('--exact'));
  assert.throws(() => candidateRegressionArguments({ package: 'clearra-app', filter: '' }), /Unknown/u);
});

test('local, main, unpaired, small-stack or acceptance environments cannot launch generated tests', () => {
  assertCandidateHost(ENV, 'win32');
  const nextRef = 'refs/heads/codex/another-exact-source';
  assertCandidateHost({ ...ENV, GITHUB_REF: nextRef, GITHUB_WORKFLOW_REF: workflowRef(nextRef) }, 'win32');
  for (const invalid of [
    { ...ENV, GITHUB_ACTIONS: 'false' },
    { ...ENV, GITHUB_REF: 'refs/heads/main', GITHUB_WORKFLOW_REF: workflowRef('refs/heads/main') },
    { ...ENV, GITHUB_REF: nextRef },
    { ...ENV, GITHUB_JOB: 'candidate-cli' }, { ...ENV, GITHUB_REF_TYPE: 'tag' },
    { ...ENV, GITHUB_EVENT_NAME: 'push' },
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
    'native_build_probability_execution::durable::tests::',
    'native_build_probability_execution::system_provider::tests::',
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

test('ordered probability evidence runs through coverage, producer and strict App reducer', () => {
  const selected = CANDIDATE_RUST_REGRESSIONS.filter((entry) => entry.filter === 'ordered_solution_probability_');
  assert.deepEqual(selected.map((entry) => entry.package).sort(), ['clearra-app', 'clearra-core-executor', 'clearra-coverage']);
  assert.ok(selected.every((entry) => entry.exact === false));
});

test('replay page-source selects the real memory test module, not an empty former name', () => {
  assert.ok(CANDIDATE_RUST_REGRESSIONS.some((entry) => entry.package === 'clearra-app' && entry.filter === 'pc_replay_page_source::memory_tests::'));
  assert.ok(!CANDIDATE_RUST_REGRESSIONS.some((entry) => entry.filter === 'pc_replay_page_source::tests::'));
});

test('focused feedback includes first-canonical lazy ties and warm-seed authority regressions', () => {
  for (const [packageName, filter] of [
    ['clearra-app', 'canonical_ready_reads_do_not_enumerate_hidden_ties_and_explicit_pages_preserve_all_ties'],
    ['clearra-app', 'member_pages_are_fixed_at_one_hundred_without_losing_candidate_identity'],
    ['clearra-coverage', 'residual_warm_seed_'],
    ['clearra-coverage', 'parallel_first_page_handoff_resumes_serial_without_losing_query_or_frontier'],
  ]) assert.ok(CANDIDATE_RUST_REGRESSIONS.some((entry) => entry.package === packageName && entry.filter === filter));
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

test('candidate cancellation or missing shared process stops remaining selections', () => {
  for (const interrupted of [
    { status: null, signal: 'SIGTERM' }, { status: 1, error: new Error('spawn failed') },
    { status: null }, { status: -1 },
  ]) {
    let calls = 0;
    assert.throws(() => runCandidateRegressions({ environment: ENV, platform: 'win32', write() {},
      spawnImplementation() { calls += 1; return interrupted; },
    }), /runner interrupted/u);
    assert.equal(calls, 1);
  }
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

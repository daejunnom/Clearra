// Run related App contracts in one managed generation, preserving compiler
// reuse without retaining another independent experimental build.
import { spawn } from 'node:child_process';
import { assertManagedBuildTransaction } from './clearra-build-policy.mjs';
import { createRustTestEvidence } from './rust-test-evidence.mjs';

const transaction = assertManagedBuildTransaction();
const common = ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-app', '--features', 'online-pc4-tablebase', '--lib'];
const checks = [
  ['pc4-default', [...common, 'pc4_']],
  ['render-default', [...common, 'commands::render_app_command']],
  ['render-no-bitmap', [...common, '--no-default-features', 'commands::render_app_command']],
];
if (process.argv.includes('--row-normalization')) {
  checks.splice(0, checks.length,
    ['pc4-row-core', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-core-executor', '--lib', 'pc4_graph_materializer']],
    ['pc4-candidate-core', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-core-executor', '--lib', 'precomputed_geometry_tests']],
    ['pc4-row-tablebase', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-pc4-tablebase', '--lib']],
    ['pc4-suffix-dag-ab', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-pc4-tablebase', '--lib', 'pc4_suffix_dag_abba', '--', '--ignored', '--nocapture']],
    ['pc4-shared-prefix-ab', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-pc4-tablebase', '--lib', 'pc4_prefix_abba', '--', '--ignored', '--nocapture']],
    ['pc4-replay-core', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-replay', '--lib']],
    ['pc4-replay-products', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-postprocess', '--lib', 'score_batch::']],
    ['pc4-row-app', [...common, 'pc4_', '--', '--skip', 'pc4_compact_graph_union_']],
    ['pc4-compact-graph-union', [...common, 'pc4_compact_graph_union_', '--', '--nocapture']],
    ['pc4-host-wasm', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-wasm', '--lib', 'online_pc4']],
    ['pc4-local-cli', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-cli', '--features', 'wasm-cpu-runtime,online-pc4-tablebase', '--lib', 'tablebase_download']],
    ['pc4-compact-storage', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-supply', '--lib', 'compact_source_uses_real_storage']],
    ['pc4-compact-shape', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-supply', '--lib', 'factorized_shape_preserves_actual_order']],
    ['pc4-compact-large', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-supply', '--lib', 'p7_p7_p2_factorized_universe_retains_compact_expression_storage']],
    ['pc4-compact-union', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-supply', '--lib', 'compact_pattern_union_', '--', '--nocapture']],
    ['pc4-compact-input-ab', [...common, 'pc4_compact_input_admission_ab', '--', '--ignored', '--nocapture']],
    ['pc4-replay-app', [...common, 'pc_replay_']],
  );
}
if (process.argv.includes('--completion-proof')) {
  checks.push([
    'pc4-hf-completion-proof',
    [
      'test', '--locked', '--offline', '--quiet', '-j', '2',
      '-p', 'clearra-core-executor', '--lib',
      'classify_all_hf_omitted_pc4_targets_with_exact_completion_receipts',
      '--', '--ignored', '--nocapture', '--test-threads=1'
    ]
  ]);
}
// Even fetch probes rustc, so it must inherit this live managed owner.
// Keep fetch and offline tests in one generation rather than replacing it.
if (process.argv.includes('--fetch')) checks.unshift(['fetch-locked', ['fetch', '--locked']]);
// Existing A/B measurements are retained evidence, not ordinary regression
// tests. Do not repeat them on every source change; request them explicitly
// only when a new algorithm comparison requires fresh measurements.
const benchmarkChecks = new Set(['pc4-suffix-dag-ab', 'pc4-shared-prefix-ab', 'pc4-compact-input-ab']);
const selectedChecks = checks.filter(([name]) => process.argv.includes('--benchmarks') || !benchmarkChecks.has(name));
const failures = [];
for (const [name, args] of selectedChecks) {
  console.log(`app_contract_check=${name} status=started`);
  const command = args[0] === 'test'
    ? [...args, ...(args.includes('--') ? [] : ['--']), '--test-threads=2'] : args;
  const evidence = createRustTestEvidence();
  const result = await new Promise(resolve => {
    const child = spawn('cargo', command, {
      cwd: transaction.source_root,
      env: { ...process.env, CARGO_PROFILE_TEST_DEBUG: '0' },
      stdio: ['ignore', 'pipe', 'inherit'], windowsHide: true,
    });
    child.stdout.on('data', chunk => { evidence.observe(chunk); process.stdout.write(chunk); });
    child.once('error', error => resolve({ error, status: 1 }));
    child.once('close', status => resolve({ status }));
  });
  if (result.error || result.status !== 0) {
    console.error(`app_contract_check=${name} status=failed ${result.error?.message ?? ''}`);
    // Fetch is a prerequisite; test groups are independent and should still
    // report their failures in this same managed compiler generation.
    if (args[0] !== 'test') process.exit(result.status || 1);
    failures.push(name);
    continue;
  }
  if (args[0] === 'test' && !evidence.hasExecutedTests()) {
    console.error(`app_contract_check=${name} status=failed reason=no-tests-executed`);
    failures.push(name);
    continue;
  }
  console.log(`app_contract_check=${name} status=passed`);
}
if (failures.length) {
  console.error(`app_contracts=failed checks=${failures.join(',')}`);
  process.exitCode = 1;
}

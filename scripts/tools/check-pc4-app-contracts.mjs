// Run related App contracts in one managed generation, preserving compiler
// reuse without retaining another independent experimental build.
import { spawnSync } from 'node:child_process';
import { assertManagedBuildTransaction } from './clearra-build-policy.mjs';

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
    ['pc4-row-app', [...common, 'pc4_']],
    ['pc4-compact-storage', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-supply', '--lib', 'compact_source_uses_real_storage']],
    ['pc4-compact-shape', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-supply', '--lib', 'factorized_shape_preserves_actual_order']],
    ['pc4-compact-large', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-supply', '--lib', 'p7_p7_p2_factorized_universe_retains_compact_expression_storage']],
    ['pc4-compact-input-ab', [...common, 'pc4_compact_input_admission_ab', '--', '--ignored', '--nocapture']],
    ['pc4-replay-app', [...common, 'pc_replay_']],
  );
}
// Even fetch probes rustc, so it must inherit this live managed owner.
// Keep fetch and offline tests in one generation rather than replacing it.
if (process.argv.includes('--fetch')) checks.unshift(['fetch-locked', ['fetch', '--locked']]);
for (const [name, args] of checks) {
  console.log(`app_contract_check=${name} status=started`);
  const command = args[0] === 'test'
    ? [...args, ...(args.includes('--') ? [] : ['--']), '--test-threads=2'] : args;
  const result = spawnSync('cargo', command, {
    cwd: transaction.source_root,
    env: { ...process.env, CARGO_PROFILE_TEST_DEBUG: '0' },
    stdio: 'inherit', windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    console.error(`app_contract_check=${name} status=failed ${result.error?.message ?? ''}`);
    process.exit(result.status || 1);
  }
  console.log(`app_contract_check=${name} status=passed`);
}

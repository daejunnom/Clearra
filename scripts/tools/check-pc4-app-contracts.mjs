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
    ['pc4-row-tablebase', ['test', '--locked', '--offline', '--quiet', '-j', '2', '-p', 'clearra-pc4-tablebase', '--lib']],
    ['pc4-row-app', [...common, 'pc4_']],
  );
}
// Even fetch probes rustc, so it must inherit this live managed owner.
// Keep fetch and offline tests in one generation rather than replacing it.
if (process.argv.includes('--fetch')) checks.unshift(['fetch-locked', ['fetch', '--locked']]);
for (const [name, args] of checks) {
  console.log(`app_contract_check=${name} status=started`);
  const command = args[0] === 'test' ? [...args, '--', '--test-threads=2'] : args;
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

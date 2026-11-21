import assert from 'node:assert/strict';
import test from 'node:test';
import { pairedP7P4StageArgs, requireLocalP7P4StageProbe, requireP7P4ComputeStages }
  from './paired-p7p4-stage-probe.mjs';

test('P7P4 timing has an exact fixed workload and never runs in CI', () => {
  const managed = { CLEARRA_RUNTIME_SUPERVISED: '1', CLEARRA_RUNTIME_PROFILE: 'benchmark-search' };
  assert.doesNotThrow(() => requireLocalP7P4StageProbe(managed));
  for (const env of [{ ...managed, CI: 'true' }, { ...managed, GITHUB_ACTIONS: 'true' }, {}]) {
    assert.throws(() => requireLocalP7P4StageProbe(env));
  }
  assert.deepEqual(pairedP7P4StageArgs(11), [
    'pc', '--lines', '4', '--patterns', 'P7P4', '--count', 'unique',
    '--queue-knowledge', 'oracle', '--backend', 'cpu', '--no-backend-fallback',
    '--rule', 'srs-plus', '--no-build-dependency-dag', '--no-tablebase',
    '--no-legal-board', '--no-conditioned-reachability', '--workers', '11',
  ]);
  for (const workers of [1, 12, 2.5, NaN]) assert.throws(() => pairedP7P4StageArgs(workers));
});

test('a profile without actual comparable geometry and verification is not comparable', () => {
  const rows = ['wasm.geometry.advance', 'wasm.candidate.projection', 'wasm.candidate.build_order_reachability']
    .map(name => ({ name, invocation_count: 1, work_item_count: 2, duration_ns: 0 }));
  assert.doesNotThrow(() => requireP7P4ComputeStages(rows));
  assert.throws(() => requireP7P4ComputeStages(rows.slice(0, 2)), /required paired compute stage/u);
  assert.throws(() => requireP7P4ComputeStages([...rows, rows[0]]), /required paired compute stage/u);
  assert.throws(() => requireP7P4ComputeStages(rows.map(row => ({ ...row, work_item_count: 0 }))),
    /unobserved paired compute stage/u);
});

import assert from 'node:assert/strict';
import test from 'node:test';
import { pairedP7P4StageArgs, requireLocalP7P4StageProbe } from './paired-p7p4-stage-probe.mjs';

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

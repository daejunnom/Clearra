import assert from 'node:assert/strict';

export const P7P4_STAGE_TIMEOUT_MS = 0;
export const P7P4_STAGE_IDENTITY = Object.freeze({
  solutions: 456_923,
  solutionSet: 'cts1:98ebe8726537b29f',
  patterns: '4233600',
});
const P7P4_REQUIRED_STAGES = [
  'wasm.geometry.advance',
  'wasm.candidate.projection',
  'wasm.candidate.build_order_reachability',
];

// A nonempty profile could contain only setup/finalization work. Do not
// present it as evidence about the main search's bottleneck.
export function requireP7P4ComputeStages(rows) {
  assert.ok(Array.isArray(rows), 'missing P7P4 stage rows');
  for (const name of P7P4_REQUIRED_STAGES) {
    const found = rows.filter(row => row?.name === name);
    assert.equal(found.length, 1, 'required paired compute stage: ' + name);
    assert.ok(Number.isSafeInteger(found[0].invocation_count) && found[0].invocation_count > 0 &&
      Number.isSafeInteger(found[0].work_item_count) && found[0].work_item_count > 0,
      'unobserved paired compute stage: ' + name);
  }
}

export function requireLocalP7P4StageProbe(env = process.env) {
  assert.equal(env.CLEARRA_RUNTIME_SUPERVISED, '1');
  assert.equal(env.CLEARRA_RUNTIME_PROFILE, 'benchmark-search');
  assert.ok(!env.CI && !env.GITHUB_ACTIONS, 'large P7P4 timing is local-only');
}

export function pairedP7P4StageArgs(workers) {
  assert.ok(Number.isInteger(workers) && workers >= 2 && workers <= 11);
  return [
    'pc', '--lines', '4', '--patterns', 'P7P4', '--count', 'unique',
    '--queue-knowledge', 'oracle', '--backend', 'cpu', '--no-backend-fallback',
    '--rule', 'srs-plus', '--no-build-dependency-dag', '--no-tablebase',
    '--no-legal-board', '--no-conditioned-reachability', '--workers', String(workers),
  ];
}

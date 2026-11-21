import assert from 'node:assert/strict';

export const P7P4_STAGE_TIMEOUT_MS = 480_000;
export const P7P4_STAGE_IDENTITY = Object.freeze({
  solutions: 456_923,
  solutionSet: 'cts1:98ebe8726537b29f',
  patterns: '4233600',
});

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

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/candidate-preflight.yml', import.meta.url), 'utf8');
function assertIsolated(source) {
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b/u);
  assert.doesNotMatch(source, /CLEARRA_ACCEPTED_|accepted-wasm-build\.mjs|canonical-acceptance-evidence\.mjs|\bGH_TOKEN\b|\bGITHUB_TOKEN\b/u);
  assert.doesNotMatch(source, /\bgh\s|\bgcloud\s|\bssh\s|\bscp\s|git\s+(?:push|tag)|actions\/deploy-pages|\/dispatches/u);
  const actions = [...source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)].map((match) => match[1]);
  for (const action of actions) assert.ok(['actions/checkout@v4', 'actions/setup-node@v4',
    'pnpm/action-setup@v4',
    'actions/upload-artifact@v4', 'actions/cache/restore@v4'].includes(action), action);
  const checkouts = source.match(/uses: actions\/checkout@v4/gmu) ?? [];
  assert.equal((source.match(/persist-credentials: false/gmu) ?? []).length, checkouts.length);
  assert.match(source, /\[\[ "\$\(git rev-parse HEAD\)" == "\$GITHUB_SHA" \]\]/u);
}

test('candidate workflow has only isolated read-only preflight authority', () => assertIsolated(workflow));

for (const [name, mutate] of [
  ['write token', (source) => source.replace('contents: read', 'contents: write')],
  ['credential persistence', (source) => source.replace('persist-credentials: false', 'persist-credentials: true')],
  ['secret use', (source) => `${source}\n# secrets.ORACLE_SSH_PRIVATE_KEY_B64`],
  ['accepted receipt', (source) => `${source}\n# node scripts/release/canonical-acceptance-evidence.mjs seal`],
  ['deployment call', (source) => `${source}\n# gh workflow run release-cli.yml --ref main`],
  ['workflow chain', (source) => source.replace('  push:', '  workflow_run:')],
  ['canonical cache writer', (source) => source.replace('actions/cache/restore@v4', 'actions/cache/save@v4')],
  ['automatic cache writer', (source) => source.replace('actions/cache/restore@v4', 'actions/cache@v4')],
]) {
  test(`rejects mutation granting ${name}`, () => assert.throws(() => assertIsolated(mutate(workflow))));
}

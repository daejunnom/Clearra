import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/integration-contracts.yml', import.meta.url), 'utf8');
function isolated(source) {
  assert.match(source, /\[\[ "\$\(git rev-parse HEAD\)" == "\$GITHUB_SHA" \]\]/u);
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b|continue-on-error/u);
  assert.doesNotMatch(source, /\bgh\s|\bgcloud\s|\bssh\s|\bscp\s|git\s+(?:push|tag)|deploy-pages|\/dispatches|canonical-acceptance-evidence|CLEARRA_ACCEPTED_/u);
  const actions = [...source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)].map(m => m[1]);
  for (const action of actions) assert.ok(['actions/checkout@v4', 'actions/setup-node@v4', 'actions/upload-artifact@v4'].includes(action));
  assert.equal((source.match(/persist-credentials: false/gu) ?? []).length,
    actions.filter(a => a === 'actions/checkout@v4').length);
}

test('integration feedback has only read-only non-publishing authority', () => isolated(workflow));

for (const [name, mutation] of [
  ['write authority', s => s.replace('contents: read', 'contents: write')],
  ['deployment', s => s + '\n# gh workflow run release-cli.yml'],
  ['accepted artifact', s => s + '\n# canonical-acceptance-evidence.mjs'],
  ['cache writer', s => s + '\n      - uses: actions/cache@v4'],
]) test(`rejects ${name}`, () => assert.throws(() => isolated(mutation(workflow))));

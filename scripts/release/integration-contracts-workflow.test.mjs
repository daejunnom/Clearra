import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/integration-contracts.yml', import.meta.url), 'utf8');
const native = await readFile(new URL('../tools/check-native-cli-contracts.ps1', import.meta.url), 'utf8');
function isolated(source) {
  assert.match(source, /^name: Integration Contracts \(Non-publishing\)$/mu);
  assert.match(source, /^    branches: \["codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912"\]$/mu);
  assert.match(source, /if: github\.ref == 'refs\/heads\/codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912' && github\.ref_type == 'branch'/u);
  assert.match(source, /\[\[ "\$GITHUB_REF" == 'refs\/heads\/codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912' && "\$GITHUB_REF_TYPE" == 'branch' \]\]/u);
  assert.match(source, /\[\[ "\$\(git rev-parse HEAD\)" == "\$GITHUB_SHA" \]\]/u);
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b|continue-on-error/u);
  assert.doesNotMatch(source, /\bgh\s|\bgcloud\s|\bssh\s|\bscp\s|git\s+(?:push|tag)|deploy-pages|\/dispatches|canonical-acceptance-evidence|CLEARRA_ACCEPTED_/u);
  const actions = [...source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)].map(m => m[1]);
  for (const action of actions) assert.ok(['actions/checkout@v4', 'actions/setup-node@v4'].includes(action));
  assert.equal((source.match(/persist-credentials: false/gu) ?? []).length,
    actions.filter(a => a === 'actions/checkout@v4').length);
  for (const name of ['native-cli', 'pc4-contracts', 'surface-contracts']) {
    const job = source.split(`  ${name}:`)[1]?.split(/^  [a-z][a-z-]*:/mu)[0];
    assert.ok(job);
    assert.match(job, /needs: source/u);
    assert.match(job, /timeout-minutes: (?:20|60)/u);
    assert.match(job, /invoke-clearra-build\.ps1 -Purpose experiment/u);
  }
}
test('integration checks have only exact-branch read-only test authority', () => isolated(workflow));
for (const [name, mutation] of [
  ['main trigger', s => s.replace('branches: ["codex/v0.9.0-stacked-on-v0.8.1-20260912"]', 'branches: ["main"]')],
  ['broad job admission', s => s.replace("if: github.ref == 'refs/heads/codex/v0.9.0-stacked-on-v0.8.1-20260912' && github.ref_type == 'branch'", 'if: true')],
  ['write authority', s => s.replace('contents: read', 'contents: write')],
  ['deployment', s => s + '\n# gh workflow run release-cli.yml'],
  ['accepted artifact', s => s + '\n# canonical-acceptance-evidence.mjs'],
  ['cache writer', s => s + '\n      - uses: actions/cache@v4'],
]) test(`rejects ${name}`, () => assert.throws(() => isolated(mutation(workflow))));
test('native process checks preserve the local execution policy before archive building', () => {
  assert.ok(native.indexOf('Assert-ClearraTrustedExecutionSurface') < native.indexOf('$libraryDirectory = Resolve-ProductE2ENativeLibraryDir'));
  assert.match(native, /Sync-ClearraNativeCargoLinkState/u);
  assert.match(native, /--test process_e2e/u);
  assert.doesNotMatch(native, /Unblock-File|Set-AuthenticodeSignature|ExecutionPolicy|\bwsl\b/u);
});

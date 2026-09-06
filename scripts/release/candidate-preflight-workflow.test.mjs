import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/candidate-preflight.yml', import.meta.url), 'utf8');
const productionWorkflows = await Promise.all([
  'release-cli.yml', 'discord-deploy.yml', 'discord-deploy-recovery.yml',
  'pages.yml', 'pages-rollback.yml', 'finalize-release-publication.yml',
].map((name) => readFile(new URL(`../../.github/workflows/${name}`, import.meta.url), 'utf8')));

function assertIsolated(source) {
  assert.match(source, /^name: Candidate Preflight$/mu);
  assert.match(source, /^    branches: \["codex\/v0\.8\.0-preflight-20260906-rng"\]$/mu);
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b/u);
  assert.doesNotMatch(source, /CLEARRA_ACCEPTED_|accepted-wasm-build\.mjs|canonical-acceptance-evidence\.mjs|\bGH_TOKEN\b|\bGITHUB_TOKEN\b/u);
  assert.doesNotMatch(source, /\bgh\s|\bgcloud\s|\bssh\s|\bscp\s|git\s+(?:push|tag)|actions\/deploy-pages|\/dispatches/u);
  const actions = [...source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)].map((match) => match[1]);
  for (const action of actions) assert.ok(['actions/checkout@v4', 'actions/setup-node@v4', 'actions/upload-artifact@v4'].includes(action), action);
  const checkouts = source.match(/uses: actions\/checkout@v4/gmu) ?? [];
  assert.equal((source.match(/persist-credentials: false/gmu) ?? []).length, checkouts.length);
  assert.match(source, /if: github\.ref == 'refs\/heads\/codex\/v0\.8\.0-preflight-20260906-rng' && github\.ref_type == 'branch'/u);
  assert.match(source, /\[\[ "\$GITHUB_REF" == 'refs\/heads\/codex\/v0\.8\.0-preflight-20260906-rng'/u);
  assert.match(source, /\[\[ "\$\(git rev-parse HEAD\)" == "\$GITHUB_SHA" \]\]/u);
  for (const job of ['candidate-full-gate', 'candidate-cli', 'candidate-ui']) {
    const tail = source.slice(source.indexOf(`  ${job}:`) + `  ${job}:`.length).split(/^  [a-z][a-z-]*:/mu)[0];
    assert.match(tail, /needs: candidate-source/u, job);
    assert.match(tail, /if: needs\.candidate-source\.outputs\.full_gate == '(?:true|false)'/u, job);
  }
}

test('candidate workflow has only isolated read-only preflight authority', () => assertIsolated(workflow));

for (const [name, mutate] of [
  ['main branch', (source) => source.replace('branches: ["codex/v0.8.0-preflight-20260906-rng"]', 'branches: ["main"]')],
  ['canonical name', (source) => source.replace('name: Candidate Preflight', 'name: Publish Product Release')],
  ['write token', (source) => source.replace('contents: read', 'contents: write')],
  ['credential persistence', (source) => source.replace('persist-credentials: false', 'persist-credentials: true')],
  ['secret use', (source) => `${source}\n# secrets.ORACLE_SSH_PRIVATE_KEY_B64`],
  ['accepted receipt', (source) => `${source}\n# node scripts/release/canonical-acceptance-evidence.mjs seal`],
  ['deployment call', (source) => `${source}\n# gh workflow run release-cli.yml --ref main`],
  ['workflow chain', (source) => source.replace('  push:', '  workflow_run:')],
]) {
  test(`rejects mutation granting ${name}`, () => assert.throws(() => assertIsolated(mutate(workflow))));
}

test('full selection runs the unchanged eight-stage entry point once and skips light duplicates', () => {
  assert.equal((workflow.match(/-Task ReleaseAcceptance -ReleaseAcceptanceShard Full/gmu) ?? []).length, 1);
  assert.match(workflow, /push\) echo 'full_gate=true'/u);
  assert.match(workflow, /full_gate:[\s\S]*default: true[\s\S]*type: boolean/u);
  assert.equal((workflow.match(/outputs\.full_gate == 'false'/gu) ?? []).length, 2);
  assert.match(workflow, /runs-on: windows-latest/u);
  assert.match(workflow, /-ExecutionSurface Trusted -RuntimeEnvironment windows/u);
  assert.match(workflow, /RUST_MIN_STACK: "16777216"/u);
  assert.match(workflow, /wasm-bindgen-cli --version 0\.2\.126 --locked/u);
  assert.doesNotMatch(workflow, /-ExecutionPolicy|Unblock-File|Set-AuthenticodeSignature|\bwsl\b/u);
});

test('source identity is paired and WASM is uploaded only after independent five-file verification', () => {
  assert.match(workflow, /CLEARRA_SOURCE_COMMIT: \$\{\{ github\.sha \}\}/u);
  assert.match(workflow, /CLEARRA_ENGINE_BUILD_ID: \$\{\{ github\.sha \}\}/u);
  assert.match(workflow, /Preserve only source-verified candidate WASM if it was built\r?\n        if: always\(\)/u);
  assert.match(workflow, /if: always\(\) && steps\.candidate_wasm\.outputs\.ready == 'true'/u);
  assert.match(workflow, /name: unqualified-candidate-wasm-\$\{\{ github\.sha \}\}-run-/u);
  assert.match(workflow, /candidate-preflight-artifacts\.mjs --from \$candidateWasmSource --output \$candidateWasmOutput --source-commit \$env:GITHUB_SHA/u);
  assert.doesNotMatch(workflow, /continue-on-error: true/u);
});

test('existing production triggers cannot consume Candidate Preflight by name or branch', () => {
  for (const production of productionWorkflows) {
    assert.doesNotMatch(production, /Candidate Preflight|codex\/v0\.8\.0-preflight/u);
  }
});

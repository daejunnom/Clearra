import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/candidate-preflight.yml', import.meta.url), 'utf8');
function assertIsolated(source) {
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|^  push:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b/mu);
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

test('profiling artifacts are opt-in, build-only, and separate from normal candidates', () => {
  const source = workflow.replaceAll('\r\n', '\n');
  const jobs = new Map([...source.matchAll(/^  (candidate-[a-z-]+):\n([\s\S]*?)(?=^  candidate-[a-z-]+:\n|(?![\s\S]))/gmu)]
    .map(([, name, block]) => [name, block]));
  assert.match(source, /\n      profiling_artifacts:\n[^\n]*\n        default: false\n        type: boolean/u);
  assert.match(jobs.get('candidate-source'),
    /if \[\[ "\$REQUEST_PROFILING_ARTIFACTS" == 'false' \]\]; then\n\s+echo "full_gate=\$REQUEST_FULL_GATE"/u);
  const diagnostic = /needs\.candidate-source\.outputs\.profiling_artifacts == 'true'/u;
  assert.deepEqual([...jobs].filter(([, block]) => diagnostic.test(block)).map(([name]) => name),
    ['candidate-rust-wasm', 'candidate-cli']);
  const cli = jobs.get('candidate-cli');
  assert.match(cli, /\(needs\.candidate-source\.outputs\.profiling_artifacts == 'true' && !inputs\.profiling_wasm_only\)/u);
  assert.match(cli, /'windows-latest' \|\| 'ubuntu-latest'/u);
  const cliBuild = cli.split('      - name: Build exact-source Windows CLI for stage profiling\n')[1]
    .split(/^      - /mu)[0];
  assert.match(cliBuild, diagnostic);
  assert.match(cliBuild, /cargo build --locked --release --package clearra-cli --bin clearra --features wasm-cpu-runtime,stage-profiling/u);
  assert.match(cliBuild, /source_commit = \$env:GITHUB_SHA/u);
  assert.match(cliBuild, /release_authority = \$false/u);
  assert.match(cliBuild, /Get-FileHash -LiteralPath \$staged -Algorithm SHA256/u);
  assert.doesNotMatch(cliBuild, /cargo test|\bbenchmark(?:-search)?\b|& \$staged/u);
  const wasm = jobs.get('candidate-rust-wasm');
  assert.match(wasm, /\$buildArgs \+= '--stage-profiling'\n\s+\$preserveArgs \+= '--require-stage-profile'/u);
  assert.doesNotMatch(wasm, /--verify|cargo test|run-native-chromium/u);
  for (const [job, kind] of [[cli, 'cli'], [wasm, 'wasm']]) {
    const upload = job.split(`          name: unqualified-stage-profile-${kind}-`)[1];
    assert.ok(upload.startsWith('${{ github.sha }}-run-${{ github.run_id }}-attempt-${{ github.run_attempt }}\n'));
    assert.match(upload, /retention-days: 7/u);
  }
});

for (const [name, mutate] of [
  ['write token', (source) => source.replace('contents: read', 'contents: write')],
  ['credential persistence', (source) => source.replace('persist-credentials: false', 'persist-credentials: true')],
  ['secret use', (source) => `${source}\n# secrets.ORACLE_SSH_PRIVATE_KEY_B64`],
  ['accepted receipt', (source) => `${source}\n# node scripts/release/canonical-acceptance-evidence.mjs seal`],
  ['deployment call', (source) => `${source}\n# gh workflow run release-cli.yml --ref main`],
  ['workflow chain', (source) => source.replace('  workflow_dispatch:', '  workflow_run:')],
  ['automatic push gate', (source) => source.replace('  workflow_dispatch:', '  push:')],
  ['canonical cache writer', (source) => source.replace('actions/cache/restore@v4', 'actions/cache/save@v4')],
  ['automatic cache writer', (source) => source.replace('actions/cache/restore@v4', 'actions/cache@v4')],
]) {
  test(`rejects mutation granting ${name}`, () => assert.throws(() => assertIsolated(mutate(workflow))));
}

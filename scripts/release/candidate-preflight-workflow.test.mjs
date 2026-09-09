import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/candidate-preflight.yml', import.meta.url), 'utf8');
const productionWorkflows = await Promise.all([
  'release-cli.yml', 'discord-deploy.yml', 'discord-deploy-recovery.yml',
  'pages.yml', 'pages-rollback.yml', 'finalize-release-publication.yml',
].map((name) => readFile(new URL(`../../.github/workflows/${name}`, import.meta.url), 'utf8')));

test('focused CLI feedback checks publication fixture schema before native compilation', () => {
  const job = workflow.split('  candidate-cli:')[1].split('  candidate-minimum-diagnostic:')[0];
  const check = 'node --test scripts/release/release-publication-evidence.test.mjs scripts/release/final-source-stage-evidence.test.mjs';
  assert.ok(job.includes(check));
  assert.ok(job.indexOf(check) < job.indexOf('name: Compile candidate CLI'));
});

function candidateCliStep(name) {
  const job = workflow.split('  candidate-cli:')[1].split('  candidate-minimum-diagnostic:')[0];
  return job.split(`      - name: ${name}\n`)[1]?.split(/\r?\n      - (?:name|uses):/u)[0] ??
    job.split(`      - name: ${name}\r\n`)[1]?.split(/\r?\n      - (?:name|uses):/u)[0] ?? '';
}

test('candidate CLI continues only independent tests with successful setup or binary prerequisites', () => {
  for (const name of ['Check independent release smoke wiring', 'Check independent publication fixture contracts']) {
    assert.match(candidateCliStep(name), /if: \$\{\{ !cancelled\(\) && steps\.node\.outcome == 'success' \}\}/u);
  }
  const compile = candidateCliStep('Compile candidate CLI');
  assert.match(compile, /if: \$\{\{ !cancelled\(\) && steps\.metadata\.outcome == 'success' && steps\.smokes\.outcome == 'success' && steps\.fixtures\.outcome == 'success' \}\}/u);
  for (const name of ['Check candidate startup', 'Verify the current typed score renderer boundary',
    'Publish unqualified development binary']) {
    const step = candidateCliStep(name);
    assert.match(step, /if: \$\{\{ !cancelled\(\) && steps\.compile\.outcome == 'success' \}\}/u);
    assert.doesNotMatch(step, /continue-on-error|\|\| true/u);
  }
});

function assertIsolated(source) {
  assert.match(source, /^name: Candidate Preflight$/mu);
  assert.match(source, /^    branches: \["codex\/v0\.8\.0-preflight-20260906-rng"\]$/mu);
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b/u);
  assert.doesNotMatch(source, /CLEARRA_ACCEPTED_|accepted-wasm-build\.mjs|canonical-acceptance-evidence\.mjs|\bGH_TOKEN\b|\bGITHUB_TOKEN\b/u);
  assert.doesNotMatch(source, /\bgh\s|\bgcloud\s|\bssh\s|\bscp\s|git\s+(?:push|tag)|actions\/deploy-pages|\/dispatches/u);
  const actions = [...source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)].map((match) => match[1]);
  for (const action of actions) assert.ok(['actions/checkout@v4', 'actions/setup-node@v4',
    'actions/upload-artifact@v4', 'actions/cache/restore@v4'].includes(action), action);
  const checkouts = source.match(/uses: actions\/checkout@v4/gmu) ?? [];
  assert.equal((source.match(/persist-credentials: false/gmu) ?? []).length, checkouts.length);
  assert.match(source, /if: github\.ref == 'refs\/heads\/codex\/v0\.8\.0-preflight-20260906-rng' && github\.ref_type == 'branch'/u);
  assert.match(source, /\[\[ "\$GITHUB_REF" == 'refs\/heads\/codex\/v0\.8\.0-preflight-20260906-rng'/u);
  assert.match(source, /\[\[ "\$\(git rev-parse HEAD\)" == "\$GITHUB_SHA" \]\]/u);
  for (const job of ['candidate-full-gate', 'candidate-rust', 'candidate-rust-wasm', 'candidate-cli', 'candidate-minimum-diagnostic', 'candidate-ui']) {
    const tail = source.slice(source.indexOf(`  ${job}:`) + `  ${job}:`.length).split(/^  [a-z][a-z-]*:/mu)[0];
    assert.match(tail, /needs: candidate-source/u, job);
    assert.match(tail, /if: needs\.candidate-source\.outputs\.full_gate == '(?:true|false)'/u, job);
  }
}

test('candidate workflow has only isolated read-only preflight authority', () => assertIsolated(workflow));

test('new source feedback preserves an in-flight older source-bound build', () => {
  assert.match(workflow, /group: candidate-preflight-\$\{\{ github\.ref \}\}-\$\{\{ github\.sha \}\}/u);
  assert.match(workflow, /cancel-in-progress: true/u);
});

for (const [name, mutate] of [
  ['main branch', (source) => source.replace('branches: ["codex/v0.8.0-preflight-20260906-rng"]', 'branches: ["main"]')],
  ['canonical name', (source) => source.replace('name: Candidate Preflight', 'name: Publish Product Release')],
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

test('full selection runs the unchanged eight-stage entry point once and skips light duplicates', () => {
  assert.equal((workflow.match(/-Task ReleaseAcceptance -ReleaseAcceptanceShard Full/gmu) ?? []).length, 1);
  assert.match(workflow, /push\) echo 'full_gate=false'/u);
  assert.match(workflow, /full_gate:[\s\S]*default: true[\s\S]*type: boolean/u);
  assert.equal((workflow.match(/outputs\.full_gate == 'false'/gu) ?? []).length, 5);
  assert.match(workflow, /runs-on: windows-latest/u);
  assert.match(workflow, /-ExecutionSurface Trusted -RuntimeEnvironment windows/u);
  assert.match(workflow, /RUST_MIN_STACK: "16777216"/u);
  assert.match(workflow, /wasm-bindgen-cli --version 0\.2\.126 --locked/u);
  assert.doesNotMatch(workflow, /-ExecutionPolicy|Unblock-File|Set-AuthenticodeSignature|\bwsl\b/u);
});

test('focused Rust/WASM feedback is not another full gate and builds one independent generation', () => {
  const job = workflow.split('  candidate-rust-wasm:')[1].split('  candidate-cli:')[0];
  assert.match(job, /if: needs\.candidate-source\.outputs\.full_gate == 'false'/u);
  assert.match(job, /runs-on: ubuntu-latest/u);
  assert.match(job, /Assert-ClearraTrustedExecutionSurface -TaskName 'CandidateRustWasm' -ExecutionSurface Trusted -RuntimeEnvironment wasm/u);
  assert.match(job, /\. \.\/scripts\/lib\/clearra-path-helpers\.ps1/u);
  assert.match(job, /git rev-parse HEAD\)\.Trim\(\) -ne \$env:GITHUB_SHA/u);
  assert.match(job, /CARGO_BUILD_JOBS=\$cargoJobs/u);
  assert.match(job, /task_workers=1 cargo_jobs=\$cargoJobs/u);
  assert.doesNotMatch(job, /run: node scripts\/release\/candidate-preflight-regressions\.mjs/u);
  assert.doesNotMatch(job, /-Task ReleaseAcceptance|--workspace|--all-targets|--verify|--benchmark|npm ci/u);
  assert.equal((job.match(/node scripts\/tools\/build-clearra-wasm\.mjs --environment native --destination \$candidateWasmBuild/gu) ?? []).length, 1);
  assert.match(job, /Join-Path \$env:RUNNER_TEMP 'clearra-candidate-wasm-built'/u);
  assert.match(job, /!cancelled\(\) && steps\.toolchains\.outcome == 'success' && steps\.boundaries\.outcome == 'success'/u);
  assert.match(job, /!cancelled\(\) && steps\.wasm_build\.outcome == 'success'/u);
  assert.match(job, /if: always\(\) && steps\.candidate_wasm\.outputs\.ready == 'true'/u);
  assert.doesNotMatch(job, /continue-on-error:|actions\/cache(?:@|\/save)|download-artifact/u);
  assert.match(job, /"CARGO_TARGET_DIR=\$\(Get-ClearraCargoTargetDir\)" >> \$env:GITHUB_ENV/u);
});

test('native regressions and WASM are sibling leaves, not a serial critical path', () => {
  const job = workflow.split('  candidate-rust:')[1].split('  candidate-rust-wasm:')[0];
  assert.match(job, /needs: candidate-source/u);
  assert.match(job, /Assert-ClearraTrustedExecutionSurface/u);
  assert.match(job, /run: node scripts\/release\/candidate-preflight-regressions\.mjs/u);
  assert.doesNotMatch(job, /npm ci|cargo install wasm-bindgen|rustup target|build-clearra-wasm|candidate-rust-wasm|continue-on-error/u);
  assert.equal((workflow.match(/run: node scripts\/release\/candidate-preflight-regressions\.mjs/gu) ?? []).length, 1);
});

test('candidate build leaves only read matching canonical cache families and still rebuild', () => {
  const canonical = productionWorkflows[0].replaceAll('\r\n', '\n');
  for (const [name, family, version] of [
    ['candidate-full-gate', 'native', 3], ['candidate-rust-wasm', 'wasm', 4], ['candidate-rust', 'native', 3],
  ]) {
    const job = workflow.split(`  ${name}:`)[1].split(/^  [a-z][a-z-]*:/mu)[0].replaceAll('\r\n', '\n');
    const cache = job.match(/      - name: Restore verified (?:WASM|native) build inputs\n([\s\S]*?)(?=      - name:)/u)?.[1];
    assert.ok(cache, `${name} cache reader missing`);
    assert.ok(canonical.includes(cache.trimEnd()), `${name} must match the canonical producer cache paths and keys`);
    assert.ok(cache.includes(`key: release-acceptance-${family}-v${version}-`));
    assert.match(cache, /-\$\{\{ github.sha \}\}/u);
    assert.equal((job.match(/actions\/cache\/restore@v4/gu) ?? []).length, 1);
    assert.doesNotMatch(job, /cache-hit/u, 'a cache hit is not a reason to skip the source build or tests');
  }
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

test('Jstris diagnostic is bounded, same-binary and separate from release or GUI performance proof', () => {
  const job = workflow.split('  candidate-minimum-diagnostic:')[1].split('  candidate-ui:')[0];
  assert.match(job, /timeout-minutes: 45/u);
  assert.match(job, /ctk3_export_jstris_180_exact_cover_diagnostic_matrix -- --exact --ignored --nocapture/u);
  assert.match(job, /ctk3_jstris_180_residual_warm_seed_first_canonical_ab_probe -- --exact --ignored --nocapture/u);
  assert.match(job, /ctk3_diagnostic_ -- --nocapture/u);
  assert.equal((job.match(/--release --package clearra-wasm --test pc_minimals_ctk3_stage_probe/gu) ?? []).length, 3);
  assert.doesNotMatch(job, /continue-on-error:|--workspace|--all-targets|--no-default-features|--features|-Task ReleaseAcceptance/u);
  assert.match(job, /name: unqualified-jstris-matrix-/u);
  assert.doesNotMatch(workflow, /repository: Qnia28|benchmark-qnia|unqualified-qnia|qnia-cpsat-reference/u);
});

test('candidate CLI retains native startup and typed output checks without a duplicate timing benchmark', () => {
  const job = workflow.split('  candidate-cli:')[1].split('  candidate-minimum-diagnostic:')[0];
  assert.equal((job.match(/cargo build /gu) ?? []).length, 1);
  assert.match(job, /cargo build --locked --release --package clearra-cli --features wasm-cpu-runtime,webgpu-search/u);
  assert.match(job, /run: target\/release\/clearra --help/u);
  assert.match(job, /cargo test --locked --release --package clearra-cli --features wasm-cpu-runtime,webgpu-search --lib score_finder_renderer_ -- --nocapture/u);
  assert.match(job, /CLEARRA_SOURCE_COMMIT: \$\{\{ github\.sha \}\}/u);
  assert.match(job, /CLEARRA_ENGINE_BUILD_ID: \$\{\{ github\.sha \}\}/u);
  assert.match(job, /name: unqualified-candidate-cli-\$\{\{ github\.sha \}\}/u);
  assert.match(job, /steps\.compile\.outcome == 'success'/u);
  assert.doesNotMatch(job, /cli-parity|Compare direct CLI|target\/debug|continue-on-error:|\bgcloud\b|--workspace|ReleaseAcceptance/u);
});

test('existing production triggers cannot consume Candidate Preflight by name or branch', () => {
  for (const production of productionWorkflows) {
    assert.doesNotMatch(production, /Candidate Preflight|codex\/v0\.8\.0-preflight/u);
  }
});

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/integration-contracts.yml', import.meta.url), 'utf8');
const native = await readFile(new URL('../tools/check-native-cli-contracts.ps1', import.meta.url), 'utf8');
const nativeCMake = await readFile(new URL('../../core-c/CMakeLists.txt', import.meta.url), 'utf8');
function isolated(source) {
  assert.match(source, /^name: Integration Contracts \(Non-publishing\)$/mu);
  assert.match(source, /^    branches: \["codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912"\]$/mu);
  assert.match(source, /if: github\.ref == 'refs\/heads\/codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912' && github\.ref_type == 'branch'/u);
  assert.match(source, /\[\[ "\$GITHUB_REF" == 'refs\/heads\/codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912' && "\$GITHUB_REF_TYPE" == 'branch' \]\]/u);
  assert.match(source, /\[\[ "\$\(git rev-parse HEAD\)" == "\$GITHUB_SHA" \]\]/u);
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b|continue-on-error/u);
  assert.doesNotMatch(source, /\brun:\s*(?:&\s+)?cargo\b/u, 'even cargo fetch requires its live build owner');
  assert.doesNotMatch(source, /\bgh\s|\bgcloud\s|\bssh\s|\bscp\s|git\s+(?:push|tag)|deploy-pages|\/dispatches|canonical-acceptance-evidence|CLEARRA_ACCEPTED_/u);
  const actions = [...source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)].map(m => m[1]);
  for (const action of actions) assert.ok(['actions/checkout@v4', 'actions/setup-node@v4', 'actions/upload-artifact@v4'].includes(action));
  assert.equal((source.match(/persist-credentials: false/gu) ?? []).length,
    actions.filter(a => a === 'actions/checkout@v4').length);
  for (const name of ['native-cli', 'pc4-contracts', 'surface-contracts']) {
    const job = source.split(`  ${name}:`)[1]?.split(/^  [a-z][a-z-]*:/mu)[0];
    assert.ok(job);
    assert.match(job, /needs: source/u);
    assert.match(job, /timeout-minutes: (?:20|60)/u);
    assert.match(job, /invoke-clearra-build\.ps1 -Purpose experiment/u);
  }
  const preview = source.split('  preview-wasm:')[1];
  assert.ok(preview);
  assert.match(preview, /needs: source/u);
  assert.match(preview, /if: inputs\.preview_wasm == true \|\| \(github\.event_name == 'push' && contains\(github\.event\.head_commit\.message, '\[preview-wasm\]'\)\)/u);
  assert.match(preview, /timeout-minutes: 40/u);
  assert.match(preview, /invoke-clearra-build\.ps1 -Purpose experiment/u);
  const preparePreview = preview.split('      - name: Prepare isolated preview toolchain')[1]?.split('      - name: Build exact-source')[0];
  assert.ok(preparePreview?.includes('. ./scripts/lib/clearra-path-helpers.ps1'));
  assert.ok(preparePreview.indexOf('. ./scripts/lib/clearra-path-helpers.ps1') < preparePreview.indexOf('Assert-ClearraTrustedExecutionSurface'));
  assert.equal((source.match(/actions\/upload-artifact@v4/gu) ?? []).length, 1);
  assert.match(preview, /name: unqualified-integration-preview-wasm-\$\{\{ github\.sha \}\}-run-\$\{\{ github\.run_id \}\}-attempt-\$\{\{ github\.run_attempt \}\}/u);
  assert.match(preview, /if-no-files-found: error\s+retention-days: 2/u);
  assert.equal((preview.match(/node scripts\/tools\/build-clearra-wasm\.mjs/gu) ?? []).length, 1);
}
test('integration checks have only exact-branch read-only test authority', () => isolated(workflow));
test('online PC4 discovery and real host transport contracts stay in non-publishing checks', () => {
  const source = workflow.split('  source:')[1].split('  native-cli:')[0];
  assert.ok(source.includes('scripts/release/pc4/discover-upstream-generation.test.mjs'));
  assert.ok(source.includes('scripts/release/pc4/qualify-upstream-generation.test.mjs'));
  const surfaces = workflow.split('  surface-contracts:')[1].split('  preview-wasm:')[0];
  assert.ok(surfaces.includes('apps/clearra-web/test/onlinePc4Host.test.mjs'));
});
for (const [name, mutation] of [
  ['main trigger', s => s.replace('branches: ["codex/v0.9.0-stacked-on-v0.8.1-20260912"]', 'branches: ["main"]')],
  ['broad job admission', s => s.replace("if: github.ref == 'refs/heads/codex/v0.9.0-stacked-on-v0.8.1-20260912' && github.ref_type == 'branch'", 'if: true')],
  ['write authority', s => s.replace('contents: read', 'contents: write')],
  ['deployment', s => s + '\n# gh workflow run release-cli.yml'],
  ['accepted artifact', s => s + '\n# canonical-acceptance-evidence.mjs'],
  ['cache writer', s => s + '\n      - uses: actions/cache@v4'],
  ['unmanaged fetch', s => s + '\n      - run: cargo fetch --locked'],
  ['unconditional preview build', s => s.replace("if: inputs.preview_wasm == true || (github.event_name == 'push' && contains(github.event.head_commit.message, '[preview-wasm]'))", 'if: true')],
  ['ambiguous preview artifact', s => s.replace('name: unqualified-integration-preview-wasm-', 'name: runtime-wasm-')],
  ['extra artifact writer', s => s + '\n      - uses: actions/upload-artifact@v4'],
  ['missing preview platform helpers', s => s.replace('          . ./scripts/lib/clearra-path-helpers.ps1', '')],
]) test(`rejects ${name}`, () => assert.throws(() => isolated(mutation(workflow))));
test('native process checks preserve the local execution policy before archive building', () => {
  assert.ok(native.indexOf('Assert-ClearraTrustedExecutionSurface') < native.indexOf('$libraryDirectory = Resolve-ProductE2ENativeLibraryDir'));
  assert.match(native, /Sync-ClearraNativeCargoLinkState/u);
  assert.match(native, /--test process_e2e/u);
  assert.match(native, /cargo test --locked --offline -p clearra-cli --features native-c-core\s*`/u);
  assert.doesNotMatch(native, /--features [^\r\n]*wasm-cpu-runtime/u);
  assert.doesNotMatch(native, /Unblock-File|Set-AuthenticodeSignature|ExecutionPolicy|\bwsl\b/u);
  assert.ok(native.indexOf('Assert-ClearraTrustedExecutionSurface') < native.indexOf('& cargo fetch --locked'));
  assert.match(workflow, /"-FetchDependencies"/u);
  assert.match(workflow, /"--row-normalization","--fetch"/u);
});

test('MSVC archive and C tests share the Rust-compatible CRT without changing build profiles', () => {
  const policy = /if\(MSVC AND NOT DEFINED CMAKE_MSVC_RUNTIME_LIBRARY\)\s+set\(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL"\)\s+endif\(\)/u;
  const policyIndex = nativeCMake.search(policy);
  assert.ok(policyIndex > nativeCMake.indexOf('project('));
  assert.ok(policyIndex < nativeCMake.indexOf('include(cmake/library_target.cmake)'));
  assert.ok(policyIndex < nativeCMake.indexOf('include(cmake/test_targets.cmake)'));
  assert.doesNotMatch(nativeCMake, /NODEFAULTLIB|CMAKE_C_FLAGS_DEBUG|CMAKE_BUILD_TYPE|FORCE/u);
});

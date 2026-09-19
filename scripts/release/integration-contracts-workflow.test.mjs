import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const workflow = await readFile(new URL('../../.github/workflows/integration-contracts.yml', import.meta.url), 'utf8');
const native = await readFile(new URL('../tools/check-native-cli-contracts.ps1', import.meta.url), 'utf8');
const nativeCMake = await readFile(new URL('../../core-c/CMakeLists.txt', import.meta.url), 'utf8');
const pc4Contracts = await readFile(new URL('../tools/check-pc4-app-contracts.mjs', import.meta.url), 'utf8');
const cliManifest = await readFile(new URL('../../crates/clearra-cli/Cargo.toml', import.meta.url), 'utf8');
function isolated(source) {
  assert.match(source, /^name: Integration Contracts \(Non-publishing\)$/mu);
  assert.match(source, /^    branches: \["codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912"\]$/mu);
  assert.match(source, /if: github\.ref == 'refs\/heads\/codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912' && github\.ref_type == 'branch'/u);
  assert.match(source, /\[\[ "\$GITHUB_REF" == 'refs\/heads\/codex\/v0\.9\.0-stacked-on-v0\.8\.1-20260912' && "\$GITHUB_REF_TYPE" == 'branch' \]\]/u);
  assert.match(source, /\[\[ "\$\(git rev-parse HEAD\)" == "\$GITHUB_SHA" \]\]/u);
  assert.match(source, /^permissions:\r?\n  contents: read$/mu);
  assert.match(source, /^concurrency:\r?\n  group: integration-contracts-\$\{\{ github\.ref \}\}\r?\n  cancel-in-progress: true$/mu);
  assert.doesNotMatch(source, /^  group: .*github\.sha/mu);
  assert.doesNotMatch(source, /workflow_run:|workflow_call:|pull_request:|\bsecrets\.|\benvironment:|id-token:|:\s*write\b|continue-on-error/u);
  assert.doesNotMatch(source, /\brun:\s*(?:&\s+)?cargo\b/u, 'even cargo fetch requires its live build owner');
  assert.doesNotMatch(source, /\bgh\s|\bgcloud\s|\bssh\s|\bscp\s|git\s+(?:push|tag)|deploy-pages|\/dispatches|canonical-acceptance-evidence|CLEARRA_ACCEPTED_/u);
  const actions = [...source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)].map(m => m[1]);
  for (const action of actions) assert.ok(['actions/checkout@v4', 'actions/setup-node@v4', 'actions/upload-artifact@v4'].includes(action));
  assert.equal((source.match(/persist-credentials: false/gu) ?? []).length,
    actions.filter(a => a === 'actions/checkout@v4').length);
  for (const name of ['native-cli', 'pc4-contracts', 'surface-contracts']) {
    const job = source.split(`  ${name}:`)[1]?.split(/^  [a-z][a-z0-9-]*:/mu)[0];
    assert.ok(job);
    assert.match(job, /needs: source/u);
    assert.match(job, /timeout-minutes: (?:20|60)/u);
    assert.match(job, /invoke-clearra-build\.ps1 -Purpose experiment/u);
  }
  const nativeHttp2 = source.split('  native-http2-candidate:')[1]?.split(/^  [a-z][a-z0-9-]*:/mu)[0];
  assert.ok(nativeHttp2);
  assert.match(nativeHttp2, /needs: source/u);
  assert.match(nativeHttp2, /if: inputs\.pc4_native_http2 == true \|\| \(github\.event_name == 'push' && contains\(github\.event\.head_commit\.message, '\[pc4-http2-ab\]'\)\)/u);
  assert.match(nativeHttp2, /os: \[ubuntu-latest, windows-latest\]/u);
  assert.equal((nativeHttp2.match(/invoke-clearra-build\.ps1 -Purpose experiment/gu) ?? []).length, 2);
  assert.ok(nativeHttp2.includes('"--features","native-pc4-libcurl"'));
  assert.doesNotMatch(nativeHttp2, /upload-artifact|environment:|secrets\.|id-token/u);
  const nativeHttp2Live = source.split('  native-http2-live-ab:')[1]?.split(/^  [a-z][a-z0-9-]*:/mu)[0];
  assert.ok(nativeHttp2Live);
  assert.match(nativeHttp2Live, /needs: source/u);
  assert.match(nativeHttp2Live, /if: inputs\.pc4_native_http2_live_ab == true/u);
  assert.match(nativeHttp2Live, /runs-on: ubuntu-latest/u);
  assert.match(nativeHttp2Live, /CLEARRA_PC4_HTTP_LIVE_AB: "1"/u);
  assert.match(nativeHttp2Live, /native_pool_live_qualification_reuses_connection_for_graph_range/u);
  assert.match(nativeHttp2Live, /--ignored/u);
  assert.equal((nativeHttp2Live.match(/invoke-clearra-build\.ps1 -Purpose experiment/gu) ?? []).length, 2);
  assert.doesNotMatch(nativeHttp2Live, /upload-artifact|environment:|secrets\.|id-token/u);
  const pc4ContractsJob = source.split('  pc4-contracts:')[1]?.split(/^  [a-z][a-z0-9-]*:/mu)[0];
  assert.ok(pc4ContractsJob);
  assert.match(pc4ContractsJob, /inputs\.pc4_completion_proof/u);
  assert.match(pc4ContractsJob, /--completion-proof/u);
  assert.equal(
    (pc4ContractsJob.match(/invoke-clearra-build\.ps1 -Purpose experiment/gu) ?? []).length,
    1,
    'the explicit completion proof must reuse the existing managed PC4 build owner',
  );
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
test('persistent native PC4 transport stays opt-in and HTTP/2-capable', () => {
  assert.match(cliManifest, /^default = \["online-pc4-tablebase"\]$/mu);
  assert.match(cliManifest, /^native-pc4-libcurl = \["online-pc4-tablebase", "dep:curl"\]$/mu);
  assert.match(cliManifest, /^curl = \{ version = "0\.4\.50", optional = true, features = \["http2"\] \}$/mu);
  assert.doesNotMatch(cliManifest, /^default = .*native-pc4-libcurl/mu);
});
test('online PC4 discovery and real host transport contracts stay in non-publishing checks', () => {
  const source = workflow.split('  source:')[1].split('  native-cli:')[0];
  assert.ok(source.includes('scripts/release/pc4/discover-upstream-generation.test.mjs'));
  assert.ok(source.includes('scripts/release/pc4/qualify-upstream-generation.test.mjs'));
  assert.ok(source.includes('scripts/release/pc4/pc4-range-reader.test.mjs'));
  assert.ok(source.includes('scripts/release/pc4/pc4-download.test.mjs'));
  assert.ok(source.includes('scripts/tools/rust-test-evidence.test.mjs'));
  const surfaces = workflow.split('  surface-contracts:')[1].split('  preview-wasm:')[0];
  assert.ok(surfaces.includes('apps/clearra-web/test/onlinePc4Host.test.mjs'));
  assert.ok(surfaces.includes('apps/clearra-web/test/pc4HostTypes.test.mjs'));
  assert.ok(surfaces.includes('apps/clearra-web/test/pc4LocalStore.test.mjs'));
  assert.ok(surfaces.includes('apps/clearra-web/test/LocalSearchProfile.contract.ts'));
});
test('compact input union executes its exhaustive parity contracts in the managed test owner', () => {
  assert.match(pc4Contracts, /'pc4-compact-union',[^\n]*'-p', 'clearra-supply', '--lib', 'compact_pattern_union_', '--', '--nocapture'/u);
  assert.match(pc4Contracts, /!evidence\.hasExecutedTests\(\)/u);
  assert.match(pc4Contracts, /assertManagedBuildTransaction\(\)/u);
});
test('compact graph union parity runs once in its own nonzero-evidence group', () => {
  assert.match(pc4Contracts, /'pc4-row-app', \[\.\.\.common, 'pc4_', '--', '--skip', 'pc4_compact_graph_union_'\]/u);
  assert.match(pc4Contracts, /'pc4-compact-graph-union', \[\.\.\.common, 'pc4_compact_graph_union_', '--', '--nocapture'\]/u);
  assert.match(pc4Contracts, /!evidence\.hasExecutedTests\(\)/u);
});
test('ordinary regression checks do not repeat retained A/B measurements', () => {
  assert.match(pc4Contracts, /new Set\(\['pc4-suffix-dag-ab', 'pc4-shared-prefix-ab', 'pc4-compact-input-ab'\]\)/u);
  assert.match(pc4Contracts, /const selectedChecks = checks\.filter\(\(\[name\]\) => process\.argv\.includes\('--benchmarks'\) \|\| !benchmarkChecks\.has\(name\)\);/u);
  assert.match(pc4Contracts, /for \(const \[name, args\] of selectedChecks\)/u);
  assert.doesNotMatch(workflow, /--benchmarks/u);
});
test('the unresolved HF completion proof is explicit and reuses the PC4 compile generation', () => {
  assert.match(pc4Contracts, /process\.argv\.includes\('--completion-proof'\)/u);
  assert.match(pc4Contracts, /'pc4-hf-completion-proof'/u);
  assert.match(
    pc4Contracts,
    /classify_all_hf_omitted_pc4_targets_with_exact_completion_receipts/u,
  );
  assert.match(pc4Contracts, /'--ignored', '--nocapture', '--test-threads=1'/u);
  assert.match(
    pc4Contracts,
    /args\.some\(argument => argument\.startsWith\('--test-threads='\)\)/u,
    'the shared runner must preserve an explicit proof thread count instead of appending a duplicate flag'
  );
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
  assert.match(
    workflow,
    /\$arguments = @\('scripts\/tools\/check-pc4-app-contracts\.mjs', '--row-normalization', '--fetch'\)/u,
  );
});

test('MSVC archive and C tests share the Rust-compatible CRT without changing build profiles', () => {
  const policy = /if\(MSVC AND NOT DEFINED CMAKE_MSVC_RUNTIME_LIBRARY\)\s+set\(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL"\)\s+endif\(\)/u;
  const policyIndex = nativeCMake.search(policy);
  assert.ok(policyIndex > nativeCMake.indexOf('project('));
  assert.ok(policyIndex < nativeCMake.indexOf('include(cmake/library_target.cmake)'));
  assert.ok(policyIndex < nativeCMake.indexOf('include(cmake/test_targets.cmake)'));
  assert.doesNotMatch(nativeCMake, /NODEFAULTLIB|CMAKE_C_FLAGS_DEBUG|CMAKE_BUILD_TYPE|FORCE/u);
});

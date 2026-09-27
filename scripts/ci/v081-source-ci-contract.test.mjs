// Source contracts for the non-publishing v0.8.1 feedback workflow. These
// checks neither execute Cargo nor create a synthetic qualification receipt.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const workflow = readFileSync(new URL('../../.github/workflows/v081-selective-source-ci.yml', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const fixture = readFileSync(new URL('../../crates/clearra-cli/src/accelerator_asset_store_repair_tests.rs', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const signedMetadata = [
  'config/accelerator-activation-keyring.v1.json',
  'config/legal-board-product-catalog.v1.json',
  'config/conditioned-reachability-product-catalog.v1.json',
];

test('embedded signed metadata retains canonical LF bytes without parser normalization', () => {
  for (const path of signedMetadata) {
    const bytes = readFileSync(new URL(`../../${path}`, import.meta.url));
    assert.equal(bytes.includes(13), false, `${path}: CR changes catalog authority`);
    assert.equal(bytes.subarray(0, 3).equals(Buffer.from([0xef, 0xbb, 0xbf])), false,
      `${path}: BOM changes catalog authority`);
    assert.equal(bytes.at(-1), 10, `${path}: canonical final LF is required`);
    assert.notEqual(bytes.at(-2), 10, `${path}: extra final LF changes catalog authority`);
    assert.doesNotThrow(() => JSON.parse(bytes.toString('utf8')));
  }
});

test('Windows autocrlf checkout cannot alter embedded signed metadata', () => {
  const result = spawnSync('git', [
    '-c', 'core.autocrlf=true', '-c', 'core.eol=crlf',
    'check-attr', '-z', 'text', 'eol', '--', ...signedMetadata,
  ], { cwd: fileURLToPath(new URL('../..', import.meta.url)),
    encoding: 'utf8', timeout: 10_000, windowsHide: true });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0, result.stderr);
  const attributes = result.stdout.split('\0');
  assert.equal(attributes.pop(), '');
  const expected = signedMetadata.flatMap(path => [path, 'text', 'set', path, 'eol', 'lf']);
  assert.deepEqual(attributes, expected);
});

test('CI uses the existing narrow Rust fixture root and checks it before compilation', () => {
  const requiredLeaf = fixture.match(/assert_eq!\(normalized\.file_name\(\)\.unwrap\(\),\s*"([^"]+)"\)/u)?.[1];
  assert.equal(requiredLeaf, 'accelerator-store-tests');
  const expected = `CLEARRA_FOCUSED_TEST_OUTPUT_ROOT: \${{ github.workspace }}/_local/artifacts/${requiredLeaf}`;
  assert.ok(workflow.includes(expected));
  assert.ok(workflow.includes(`test "$CLEARRA_FOCUSED_TEST_OUTPUT_ROOT" = "$GITHUB_WORKSPACE/_local/artifacts/${requiredLeaf}"`));
  assert.ok(workflow.indexOf('Prepare the exact bounded asset-test root') < workflow.indexOf('Typecheck integrated native products'));
});

test('an asset-test failure remains failure without suppressing independent proof feedback', () => {
  assert.ok(!workflow.includes('continue-on-error:'));
  assert.match(workflow, /if cargo test --locked -p clearra-cli --lib "\$filter"[\s\S]*failed=\$\(\(failed \+ 1\)\)/u);
  assert.ok(workflow.includes('test "$failed" -eq 0'));
  const proofStep = workflow.slice(workflow.indexOf('- name: Test bounded generation and parser proofs'));
  assert.ok(proofStep.startsWith("- name: Test bounded generation and parser proofs, not full profile regeneration\n        if: ${{ !cancelled() && steps.native-check.outcome == 'success' }}\n"));
});

test('CI exercises the authoritative CLI ingress and real Desktop/Discord accelerator paths', () => {
  assert.ok(workflow.includes('cargo test --locked -p clearra-cli-command --lib exact_accelerator --'));
  assert.ok(workflow.includes('exact_accelerator_flags \\\n'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-gui-host --test exact_accelerator_surface_parity --no-default-features --'));
  assert.ok(workflow.includes('apps/clearra-discord-bot/test/exact-accelerator-command.test.mjs'));
});

test('real signed-pack smoke reuses the compiled Core tests without generating or requalifying assets', () => {
  const smoke = workflow.slice(workflow.indexOf('- name: Fetch existing immutable qualified packs'),
    workflow.indexOf('\n  native-products:'));
  assert.ok(smoke.includes('gh release download conditioned-data-v081-20260924-rc1 --repo daejunnom/Clearra'));
  for (const profile of ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']) {
    assert.ok(smoke.includes(`--pattern 'conditioned-${profile}.cllr'`));
  }
  assert.ok(!smoke.includes('conditioned-*.cllr'));
  assert.ok(smoke.includes('CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR: ${{ github.workspace }}/_local/artifacts/v081-peer-signed-smoke'));
  assert.ok(smoke.includes('v081_signed_conditioned_pack_owner_peer_solver_smoke --features parallel,local-search-ab,qualification-reference -- --ignored --test-threads=1'));
  assert.ok(!smoke.includes('cargo build'));
  assert.ok(!smoke.includes('qualifier'));
  assert.ok(!smoke.includes('qualification-receipt'));
});

test('real App parity reuses one product-policy test binary and only the qualified SRS+ pair', () => {
  const smoke = workflow.slice(workflow.indexOf('- name: Test real PC and Build reducer results'),
    workflow.indexOf('- name: Test explicit asset lifecycle'));
  const command = 'cargo test --locked -p clearra-app --test exact_accelerator_product_execution --no-default-features --features parallel --';
  assert.equal(smoke.split(command).length - 1, 2);
  assert.ok(smoke.includes(`${command} --test-threads=1`));
  assert.ok(smoke.includes(`${command} --ignored --test-threads=1`));
  assert.ok(smoke.includes('CLEARRA_SIGNED_APP_SMOKE_DIR: ${{ github.workspace }}/_local/artifacts/v081-peer-signed-smoke'));
  assert.ok(smoke.includes('gh release download legal-board-data-v081-20260924-rc1 --repo daejunnom/Clearra'));
  assert.ok(smoke.includes("--pattern 'legal-board-srs-plus-v2.cllb'"));
  assert.ok(smoke.includes('gh release download conditioned-data-v081-20260924-rc1 --repo daejunnom/Clearra'));
  assert.ok(smoke.includes("--pattern 'conditioned-srs-plus.cllr'"));
  assert.ok(!smoke.includes('--pattern \'*'));
  assert.ok(!smoke.includes('local-search-ab'));
  assert.ok(!smoke.includes('cargo build'));
  assert.ok(!smoke.includes('qualification-receipt'));
  assert.ok(!smoke.includes('continue-on-error:'));
});

test('real portfolio UI proof consumes a current-run fixture without delaying independent surfaces', () => {
  assert.ok(workflow.includes('RUST_MIN_STACK: "16777216"'));
  const producer = workflow.slice(workflow.indexOf('- name: Produce real minimum portfolios'),
    workflow.indexOf('\n  product-wire-ui:'));
  const consumer = workflow.slice(workflow.indexOf('\n  product-wire-ui:'), workflow.indexOf('\n  surfaces:'));
  const name = 'v081-real-product-wire-${{ github.sha }}-${{ github.run_id }}-${{ github.run_attempt }}';
  assert.ok(producer.includes('cargo test --locked -p clearra-wasm --test exact_accelerator_portfolio_wire --features webgpu-search -- --ignored --test-threads=1'));
  assert.ok(producer.includes(`name: ${name}`));
  assert.ok(producer.includes('if-no-files-found: error'));
  assert.ok(producer.includes('retention-days: 3'));
  assert.ok(consumer.includes(`name: ${name}`));
  assert.ok(consumer.includes('needs: wasm-abi'));
  assert.ok(consumer.includes("needs.wasm-abi.outputs.product_wire_ready == 'true'"));
  assert.ok(consumer.includes('test -s "$CLEARRA_REAL_PORTFOLIO_SMOKE_DIR/portfolio-wire-smoke.json"'));
  assert.ok(consumer.includes('node --test packages/clearra-ui/test/realPortfolioWire.test.mjs'));
  assert.ok(!consumer.includes('cargo '));
  assert.ok(!workflow.slice(workflow.indexOf('\n  surfaces:')).includes('needs:'));
});

test('real WASM realms use one ordinary build and unchanged packs independently of native jobs', () => {
  const job = workflow.slice(workflow.indexOf('\n  wasm-realms:'));
  assert.ok(job.startsWith('\n  wasm-realms:\n'));
  assert.ok(!job.includes('needs:'));
  assert.ok(!job.includes('continue-on-error:'));
  assert.ok(job.includes('CLEARRA_SOURCE_COMMIT: ${{ github.sha }}'));
  assert.ok(job.includes('CLEARRA_ENGINE_BUILD_ID: ${{ github.sha }}'));
  assert.equal(job.split('node scripts/tools/build-clearra-wasm.mjs --destination').length - 1, 1);
  assert.ok(!job.includes('--stage-profiling'));
  assert.ok(!job.includes('--benchmark-provenance'));
  assert.ok(job.includes('storage verify --path "$CLEARRA_REAL_ACCELERATOR_WASM_DIR"'));
  assert.ok(job.includes('storage verify --path "$CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR"'));
  assert.ok(job.includes('test -s "$CLEARRA_REAL_ACCELERATOR_WASM_DIR/clearra_wasm.manifest.json"'));
  assert.ok(job.includes('gh release download conditioned-data-v081-20260924-rc1 --repo daejunnom/Clearra'));
  for (const profile of ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']) {
    assert.ok(job.includes(`--pattern 'conditioned-${profile}.cllr'`));
  }
  assert.ok(job.includes('node --test apps/clearra-web/test/realAcceleratorRealms.test.mjs'));
  const testSource = readFileSync(new URL('../../apps/clearra-web/test/realAcceleratorRealms.test.mjs', import.meta.url), 'utf8');
  assert.ok(testSource.includes('WebAssembly.compile(wasm)'));
  assert.ok(testSource.includes('new Worker(new URL(import.meta.url)'));
  assert.ok(testSource.includes('clearraWasmBuildContractsEqual'));
  assert.ok(testSource.includes('clearra_wasm_accelerator_peer_answer'));
  assert.ok(testSource.includes('clearra_wasm_start_job'));
  assert.ok(testSource.includes('totalBatches > 0'));
});

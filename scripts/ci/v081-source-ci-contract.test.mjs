// Source contracts for the non-publishing v0.8.1 feedback workflow. These
// checks neither execute Cargo nor create a synthetic qualification receipt.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { prepareClearraArguments } from '../../apps/clearra-discord-bot/src/clearra/command.mjs';
import { realCliProductProjectionRequests, realCliProjectionProfiles, realSetupScoreDocument }
  from '../../apps/clearra-discord-bot/test/support/realCliProductProjectionRequests.mjs';

const workflow = readFileSync(new URL('../../.github/workflows/v081-selective-source-ci.yml', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const recoveryWorkflow = readFileSync(new URL('../../.github/workflows/recovery-build-regression.yml', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const fixture = readFileSync(new URL('../../crates/clearra-cli/src/accelerator_asset_store_repair_tests.rs', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const signedMetadata = [
  'config/accelerator-activation-keyring.v1.json',
  'config/legal-board-product-catalog.v1.json',
  'config/conditioned-reachability-product-catalog.v1.json',
];

test('both convergence branches run the non-publishing v0.8.1 and recovery gates', () => {
  const branches = ['codex/v081-main-convergence-20260928', 'codex/converge-v081-linear-20260928'];
  assert.ok(workflow.includes(`branches: ["codex/v081-selective-source-ci-20260927", "${branches[0]}", "${branches[1]}"]`));
  for (const job of ['core', 'native-products', 'wasm-abi', 'surfaces', 'wasm-realms']) {
    const start = workflow.indexOf(`\n  ${job}:`);
    assert.ok(start >= 0, `missing job ${job}`);
    const guard = workflow.slice(start, workflow.indexOf('\n    runs-on:', start));
    for (const branch of branches) {
      assert.ok(guard.includes(`refs/heads/${branch}`), `${job} must run on ${branch}`);
    }
  }
  for (const branch of branches) {
    assert.ok(recoveryWorkflow.includes(`      - ${branch}`));
  }
  assert.ok(recoveryWorkflow.includes('node scripts/tools/v081-accelerator-opfs-browser-acceptance.mjs'));
  assert.ok(!recoveryWorkflow.includes('deploy-pages'));
});

test('the production Web pool smoke consumes one real WASM build and unchanged signed packs', () => {
  const realmJob = workflow.slice(workflow.indexOf('\n  wasm-realms:'));
  assert.equal(realmJob.split('node scripts/tools/build-clearra-wasm.mjs').length - 1, 1);
  assert.ok(realmJob.includes('pnpm install --frozen-lockfile --ignore-scripts'));
  assert.ok(realmJob.includes('node scripts/tools/run-real-web-verifier-pool-smoke.mjs'));
  assert.ok(realmJob.indexOf('node scripts/tools/run-real-web-verifier-pool-smoke.mjs') >
    realmJob.indexOf('node --test apps/clearra-web/test/realAcceleratorRealms.test.mjs'));
  assert.ok(!realmJob.includes('--benchmark-provenance'));
  assert.ok(!realmJob.includes('--stage-profiling'));
  const consumer = readFileSync(new URL('../../apps/clearra-web/test/realVerifierPool.smoke.mjs', import.meta.url), 'utf8');
  assert.ok(consumer.includes("from '../src/workers/ClearraVerifierPool.ts'"));
  assert.ok(consumer.includes("from '../src/workers/DistributedWasmJobRunner.ts'"));
  assert.ok(consumer.includes("assert.equal(plan.mode, 'cpu-multi'"));
  assert.ok(consumer.includes('assert.equal(plan.workerCount, 3)'));
  assert.ok(consumer.includes('assert.ok(exchanges > 0'));
  assert.ok(consumer.includes('await assert.rejects(execution, /distributed.*(?:cancelled|disposed|terminated)/u)'));
  assert.ok(!consumer.includes("assert.equal(terminal.event, 'cancelled'"));
  const boot = readFileSync(new URL('../../apps/clearra-web/test/helpers/nodeVerifierRealm.mjs', import.meta.url), 'utf8');
  assert.ok(boot.includes("await import('../../src/workers/clearraVerifierWorker.ts')"));
  assert.ok(boot.includes("assert.equal(url.protocol, 'file:'"));
  const launcher = readFileSync(new URL('../../scripts/tools/run-real-web-verifier-pool-smoke.mjs', import.meta.url), 'utf8');
  assert.ok(launcher.indexOf('assert.ok(clearraWasmBuildContractsEqual') <
    launcher.indexOf('const owner = enterManagedBuildOrRelaunch'));
  assert.ok(launcher.includes("outExtension: { '.js': '.mjs' }"));
});

test('signed browser acceptance searches qualified profiles and fails open after pointer corruption', () => {
  const browser = readFileSync(new URL('../tools/v081-accelerator-signed-browser-acceptance.mjs', import.meta.url), 'utf8');
  assert.ok(browser.includes("['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']"));
  assert.ok(browser.includes('assert.equal(assets.length, profiles.length * 2)'));
  assert.ok(browser.includes('await profileSelect.selectOption(String(index))'));
  assert.ok(browser.includes('await secondProfile.selectOption(String(index))'));
  assert.ok(browser.includes('`--solution-probabilities --backend cpu --workers ${workers} --rule ${profile} --no-tablebase `'));
  assert.ok(browser.includes('results[profile].existing = {'));
  assert.ok(browser.includes("results[profile].setup = {"));
  assert.ok(browser.includes("input === 'setup-score'"));
  assert.ok(browser.includes("assert.deepEqual(ranking(setup.activated), ranking(setup.baseline)"));
  assert.ok(browser.includes("results['srs-plus'].build = {"));
  assert.ok(browser.includes("input === 'build-probability'"));
  assert.ok(browser.includes('assert.equal(report.solution_keys_complete, true'));
  assert.ok(browser.includes('assert.deepEqual(compact(build.activated), buildBaseline'));
  assert.ok(browser.includes("realCliProductProjectionRequests('srs-plus')"));
  assert.ok(browser.includes("['minimum', 'score-minimum', 'replay']"));
  assert.ok(browser.includes("results['srs-plus'].products[name] = {"));
  assert.ok(browser.includes('productSearchMeaning(pair.activated)'));
  assert.ok(!browser.includes('assert.deepEqual(compact(pair.activated), compact(pair.baseline)'));
  assert.ok(browser.includes('assert.deepEqual(pair.activated.result.response.product_result_payload,'));
  assert.ok(browser.includes("'srs-x': 289"));
  assert.ok(browser.includes('assert.deepEqual(activated, baseline'));
  assert.ok(browser.includes("await corrupt('exact-legal-board')"));
  assert.ok(browser.includes("await corrupt('board-conditioned-reachability')"));
  assert.ok(browser.includes("assert.equal(summary(corruption.legalAfter).get('legal_board_verified_negative_prunes'), '0'"));
  assert.ok(browser.includes("assert.equal(summary(corruption.relationAfter).get('conditioned_reachability_snapshot_active'), 'false'"));
  assert.ok(browser.includes("assert.equal(assetRequests.length, assets.length, 'a search must read OPFS"));
});

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
  assert.ok(workflow.includes('tie_snapshot::tests'));
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
  assert.ok(consumer.includes('playwright@1.56.1'));
  assert.ok(consumer.includes('node scripts/tools/v081-portfolio-browser-copy-acceptance.mjs'));
  assert.ok(!consumer.includes('cargo '));
  assert.ok(!workflow.slice(workflow.indexOf('\n  surfaces:')).includes('needs:'));
  const producerJob = workflow.slice(workflow.indexOf('\n  wasm-abi:'), workflow.indexOf('\n  product-wire-ui:'));
  assert.ok(producerJob.includes('CLEARRA_SOURCE_COMMIT: ${{ github.sha }}'));
  assert.ok(producerJob.includes('CLEARRA_ENGINE_BUILD_ID: ${{ github.sha }}'));
  assert.ok(consumer.includes('CLEARRA_REAL_PORTFOLIO_SOURCE_COMMIT: ${{ github.sha }}'));
});

test('actual multi-page portfolio copy is source-bound and cannot silently skip incomplete setup', () => {
  const producer = readFileSync(new URL('../../crates/clearra-wasm/tests/support/multi_member_portfolio.rs', import.meta.url), 'utf8');
  assert.ok(producer.includes('const EXPECTED_MEMBERS: usize = 246;'));
  assert.ok(producer.includes('clearra pc pinned-minimals {QUERY}'));
  assert.ok(producer.includes('--required-document {document} --expected-source-set-hash {source_hash}'));
  assert.ok(producer.includes('CoveragePortfolioPageStore::new(set.clone())'));
  assert.ok(producer.includes('serialize_coverage_portfolio_page(&store, 1, page_number)'));
  assert.ok(!producer.includes('fn fake'));
  const consumer = readFileSync(new URL('../../packages/clearra-ui/test/realPortfolioWire.test.mjs', import.meta.url), 'utf8');
  assert.ok(consumer.includes('clearra.v081.real-portfolio-wire-smoke.v2'));
  assert.ok(consumer.includes('fixture.multi_member_cases.length, 4'));
  assert.ok(consumer.includes('actual.pages.length, 246'));
  assert.ok(consumer.includes('!configuredRoot && !expectedSource'));
  assert.ok(consumer.includes('compiledIdentity?.source_commit, expectedSource'));
  assert.ok(consumer.includes('response.runtime_identity, compiledIdentity'));
  const browser = readFileSync(new URL('../tools/v081-portfolio-browser-copy-acceptance.mjs', import.meta.url), 'utf8');
  assert.ok(browser.includes('ProductResultPager.svelte'));
  assert.ok(browser.includes('CLEARRA_REAL_PORTFOLIO_SOURCE_COMMIT'));
  assert.ok(browser.includes('actual.pages.length, 246'));
  assert.ok(browser.includes("name: 'Next 100'"));
  assert.ok(browser.includes("name: 'Copy all'"));
  assert.ok(browser.includes("[['1', '2'], ['1', '2'], ['1', '3']]"));
  const environment = { ...process.env, CLEARRA_REAL_PORTFOLIO_SMOKE_DIR: '',
    CLEARRA_REAL_PORTFOLIO_SOURCE_COMMIT: 'f'.repeat(40) };
  delete environment.NODE_TEST_CONTEXT;
  const incomplete = spawnSync(process.execPath, ['--test',
    fileURLToPath(new URL('../../packages/clearra-ui/test/realPortfolioWire.test.mjs', import.meta.url))], {
    env: environment, encoding: 'utf8', timeout: 10_000, windowsHide: true,
  });
  assert.equal(incomplete.error, undefined);
  assert.equal(incomplete.status, 1, 'incomplete explicit source setup must fail, not skip');
  assert.match(incomplete.stdout, /# skipped 0/u);
});

test('Desktop panel browser smoke reuses pinned Chromium and exercises real Svelte command wiring', () => {
  const consumer = workflow.slice(workflow.indexOf('\n  product-wire-ui:'), workflow.indexOf('\n  surfaces:'));
  assert.ok(consumer.includes('node scripts/tools/v081-desktop-accelerator-panel-browser-acceptance.mjs'));
  assert.equal(consumer.split('playwright@1.56.1').length - 1, 1);
  assert.ok(!consumer.includes('cargo build'));
  const browser = readFileSync(new URL('../tools/v081-desktop-accelerator-panel-browser-acceptance.mjs', import.meta.url), 'utf8');
  assert.ok(browser.includes('AcceleratorAssetPanel.svelte'));
  for (const command of ['accelerator_asset_action', 'accelerator_asset_start_download',
    'accelerator_asset_progress', 'accelerator_asset_cancel']) {
    assert.ok(browser.includes(command), `${command}: actual Desktop panel boundary must be exercised`);
  }
  assert.ok(browser.includes('Only the native IPC replies are simulated'));
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
    assert.ok(job.includes(`--pattern 'legal-board-${profile}-v2.cllb'`));
  }
  assert.ok(job.includes('node --test apps/clearra-web/test/realAcceleratorRealms.test.mjs'));
  const testSource = readFileSync(new URL('../../apps/clearra-web/test/realAcceleratorRealms.test.mjs', import.meta.url), 'utf8');
  assert.ok(testSource.includes('WebAssembly.compile(wasm)'));
  assert.ok(testSource.includes('new Worker(new URL(import.meta.url)'));
  assert.ok(testSource.includes('clearraWasmBuildContractsEqual'));
  assert.ok(testSource.includes('clearra_wasm_accelerator_peer_answer'));
  assert.ok(testSource.includes('clearra_wasm_accelerator_export_negative_synopsis'));
  assert.ok(testSource.includes('clearra_wasm_accelerator_admit_negative_synopsis'));
  assert.ok(testSource.includes('clearra_wasm_start_job'));
  assert.ok(testSource.includes('totalBatches > 0'));
});

test('native compute smoke uses a real current-source CLI without image or release authority', () => {
  const job = workflow.slice(workflow.indexOf('\n  native-products:'), workflow.indexOf('\n  wasm-abi:'));
  assert.ok(job.includes('CLEARRA_SOURCE_COMMIT: ${{ github.sha }}'));
  assert.ok(job.includes('CLEARRA_ENGINE_BUILD_ID: ${{ github.sha }}'));
  assert.ok(job.includes('node-version: 22.23.2'));
  const smoke = job.slice(job.indexOf('- name: Build one ordinary CLI'));
  assert.equal(smoke.split('cargo build --locked -p clearra-cli --no-default-features --features wasm-cpu-runtime').length - 1, 1);
  assert.ok(smoke.includes('CLEARRA_REAL_COMPUTE_SOURCE_COMMIT: ${{ github.sha }}'));
  assert.ok(smoke.includes('CLEARRA_REAL_COMPUTE_MODE: provision'));
  assert.ok(smoke.includes('test "$CLEARRA_REAL_COMPUTE_ASSET_ROOT" = "$GITHUB_WORKSPACE/_local/artifacts/v081-compute-data-smoke"'));
  assert.ok(smoke.includes('node --test apps/clearra-discord-bot/test/realComputeAccelerators.test.mjs'));
  const readonly = smoke.slice(smoke.indexOf('- name: Recheck the same native data layer read-only'));
  assert.ok(readonly.includes('shell: bash'));
  assert.ok(readonly.includes('test "$CLEARRA_REAL_COMPUTE_ASSET_ROOT" = "$GITHUB_WORKSPACE/_local/artifacts/v081-compute-data-smoke"'));
  assert.ok(readonly.includes('readonly_root="$GITHUB_WORKSPACE/_local/artifacts/v081-compute-readonly-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"'));
  assert.ok(readonly.includes('storage verify --path "$readonly_root"'));
  assert.ok(readonly.includes('test ! -e "$readonly_root"'));
  assert.ok(readonly.includes('cmp -s "$CLEARRA_REAL_COMPUTE_CLI" "$readonly_root/clearra"'));
  assert.ok(readonly.includes('chmod -R a+rX,a-w "$readonly_root"'));
  assert.ok(readonly.includes('test -r /proof/provision-v081-accelerators.mjs'));
  assert.ok(readonly.includes('test -x /proof/clearra'));
  for (const directory of ['data', 'data/legal-board', 'data/conditioned-reachability'])
    assert.ok(readonly.includes(`test ! -w /proof/${directory}`));
  assert.ok(readonly.includes('cmp -s "$node_binary" "$readonly_root/node"'));
  assert.ok(readonly.includes('node scripts/ci/readonly-runtime-libraries.mjs "$readonly_root/dynamic-libraries.txt"'));
  assert.ok(readonly.includes('done < "$readonly_root/runtime-libraries.txt"'));
  assert.ok(!readonly.includes("awk '/=>"));
  assert.ok(workflow.includes('node --test scripts/ci/v081-source-ci-contract.test.mjs scripts/ci/readonly-runtime-libraries.test.mjs'));
  assert.ok(readonly.includes('docker run --rm --name "$readonly_container" --read-only --network none --user 65534:65534'));
  assert.ok(readonly.includes('if docker container inspect "$readonly_container" >/dev/null 2>&1; then exit 1; fi'));
  assert.ok(readonly.includes('trap \'docker container rm --force "$readonly_container"'));
  assert.ok(readonly.includes('--mount "type=bind,src=$readonly_root,dst=/proof,readonly"'));
  assert.ok(readonly.includes('--cap-drop ALL --security-opt no-new-privileges --memory 4g --pids-limit 64'));
  assert.ok(readonly.includes('verify 0.8.1 /proof/clearra /proof/data'));
  assert.ok(!readonly.includes('docker build'));
  assert.ok(!readonly.includes('docker push'));
  assert.ok(!readonly.includes('--force-unmanaged-output'));
  assert.ok(!readonly.includes('chmod a+x /tmp'));
  assert.ok(!readonly.includes('chmod -R a+rX,a-w "$GITHUB_WORKSPACE"'));
  assert.ok(!readonly.includes(' provision '));
  assert.ok(!smoke.includes('local-search-ab'));
  assert.ok(!smoke.slice(0, smoke.indexOf('- name: Recheck the same native data layer read-only')).includes('docker '));
  assert.ok(!smoke.includes('gcloud '));
  assert.ok(!smoke.includes('qualification-receipt'));
  const testSource = readFileSync(new URL('../../apps/clearra-discord-bot/test/realComputeAccelerators.test.mjs', import.meta.url), 'utf8');
  assert.ok(testSource.includes("process.env.CLEARRA_REAL_COMPUTE_MODE ?? 'verify'"));
  assert.ok(testSource.includes('skip: !executable && !assetRoot && !sourceCommit'));
  assert.ok(testSource.includes("prepareComputeAccelerators({ mode, version: '0.8.1', executable, root: assetRoot })"));
  assert.ok(testSource.includes("prepareComputeAccelerators({ mode: 'verify', version: '0.8.1', executable, root: assetRoot })"));
  assert.ok(testSource.includes('installedSnapshot(), before'));
  assert.ok(testSource.includes('value.runtime_identity?.source_commit, sourceCommit'));
  assert.ok(testSource.includes('policyPairs.slice(1)'));
  assert.ok(testSource.includes('Number(summary.legal_board_verified_negative_prunes) > 0'));
  assert.ok(!testSource.includes('fakeCli('));
  assert.ok(!testSource.includes('invoke:'));
  const incompleteEnvironment = { ...process.env,
    CLEARRA_REAL_COMPUTE_CLI: '', CLEARRA_REAL_COMPUTE_ASSET_ROOT: '',
    CLEARRA_REAL_COMPUTE_SOURCE_COMMIT: 'f'.repeat(40) };
  delete incompleteEnvironment.NODE_TEST_CONTEXT;
  const incomplete = spawnSync(process.execPath, ['--test',
    fileURLToPath(new URL('../../apps/clearra-discord-bot/test/realComputeAccelerators.test.mjs', import.meta.url))], {
    env: incompleteEnvironment, encoding: 'utf8', timeout: 10_000, windowsHide: true,
  });
  assert.equal(incomplete.error, undefined);
  assert.equal(incomplete.status, 1, 'an incomplete explicit setup must fail, not silently skip');
  assert.match(incomplete.stdout, /# skipped 0/u);
});

test('Desktop native proof reuses installed data before read-only admission and uses real jobs', () => {
  const job = workflow.slice(workflow.indexOf('\n  native-products:'), workflow.indexOf('\n  wasm-abi:'));
  const desktop = job.slice(job.indexOf('- name: Verify real Desktop native jobs'),
    job.indexOf('- name: Recheck the same native data layer read-only'));
  assert.ok(desktop.startsWith('- name: Verify real Desktop native jobs'));
  assert.ok(job.includes('id: compute-assets'));
  assert.ok(desktop.includes("steps.compute-assets.outcome == 'success'"));
  assert.ok(desktop.includes('CLEARRA_REAL_DESKTOP_SOURCE_COMMIT: ${{ github.sha }}'));
  assert.ok(desktop.includes('cargo test --locked -p clearra-cli --test desktop_signed_accelerator_execution --no-default-features --features wasm-cpu-runtime,clearra-gui-host/wasm-cpu-runtime -- --ignored --test-threads=1'));
  assert.ok(!desktop.includes('gh release download'));
  assert.ok(!desktop.includes('local-search-ab'));
  assert.ok(!desktop.includes('benchmark'));
  const source = readFileSync(new URL('../../crates/clearra-cli/tests/desktop_signed_accelerator_execution.rs', import.meta.url), 'utf8');
  for (const productionCall of ['activate_native_accelerators_for_request(&request)',
    'bridge.start_job(&wire)', 'bridge.get_job_events(job)', 'bridge.cancel_job(job)',
    'bridge.product_page_get("2", "1")', 'SystemNativeBuildProbabilityAdmissionProvider'])
    assert.ok(source.includes(productionCall), productionCall);
  assert.match(source, /assert_eq!\(\s*installed\(\),\s*before,/u);
  assert.ok(source.includes('only the selected qualified profile may remain resident'));
  assert.ok(source.includes('cancelled download must not use transport'));
  assert.ok(source.includes('assert_eq!(cancelled["event"], "cancelled"'));
  assert.ok(!source.includes('Fake'));
  assert.ok(!source.includes('with_core_executor'));
});

test('real Discord result proof uses the existing CLI and production runner with no fake executor', () => {
  const job = workflow.slice(workflow.indexOf('\n  native-products:'), workflow.indexOf('\n  wasm-abi:'));
  const proof = job.slice(job.indexOf('- name: Verify real CLI products'),
    job.indexOf('- name: Verify real Desktop native jobs'));
  assert.ok(proof.startsWith('- name: Verify real CLI products'));
  assert.ok(proof.includes("steps.compute-assets.outcome == 'success'"));
  assert.ok(proof.includes('CLEARRA_REAL_COMPUTE_SOURCE_COMMIT: ${{ github.sha }}'));
  assert.ok(proof.includes('CLEARRA_REAL_COMPUTE_CLI: ${{ github.workspace }}/build/cargo/default/debug/clearra'));
  assert.ok(proof.includes('CLEARRA_REAL_COMPUTE_ASSET_ROOT: ${{ github.workspace }}/_local/artifacts/v081-compute-data-smoke'));
  assert.ok(proof.includes('node --test apps/clearra-discord-bot/test/realCliProductProjection.test.mjs'));
  for (const forbidden of ['cargo ', 'pnpm ', 'gcloud ', 'gh release download', 'benchmark'])
    assert.ok(!proof.includes(forbidden));
  const source = readFileSync(new URL('../../apps/clearra-discord-bot/test/realCliProductProjection.test.mjs', import.meta.url), 'utf8');
  assert.ok(source.includes('new ClearraDirectExecutor({'));
  assert.ok(source.includes('await executor.execute(arguments_)'));
  assert.ok(source.includes('assertDiscordCanonicalOnlyResult(actual).stdout, actual.stdout'));
  assert.ok(!source.includes('runner:'));
  assert.ok(!source.includes('options.spawn'));
  const environment = { ...process.env, CLEARRA_REAL_COMPUTE_CLI: '',
    CLEARRA_REAL_COMPUTE_ASSET_ROOT: '', CLEARRA_REAL_COMPUTE_SOURCE_COMMIT: 'f'.repeat(40) };
  delete environment.NODE_TEST_CONTEXT;
  const incomplete = spawnSync(process.execPath, ['--test',
    fileURLToPath(new URL('../../apps/clearra-discord-bot/test/realCliProductProjection.test.mjs', import.meta.url))], {
    env: environment, encoding: 'utf8', timeout: 10_000, windowsHide: true,
  });
  assert.equal(incomplete.error, undefined);
  assert.equal(incomplete.status, 1);
  assert.match(incomplete.stdout, /# skipped 0/u);
});

test('every actual Discord product fixture obeys the existing closed command registry', () => {
  let count = 0;
  for (const profile of realCliProjectionProfiles) {
    const requests = realCliProductProjectionRequests(profile);
    assert.equal(requests.length, 17);
    for (const request of requests) {
      const prepared = prepareClearraArguments(request.arguments, { workers: 1,
        logicalProcessors: 1, outputFormat: 'json', includeSolutionData: true });
      assert.ok(prepared.includes('--include-solution-data'));
      assert.deepEqual(prepared.slice(-3), ['--format', 'json', '--include-solution-data']);
      count += 1;
    }
    const scoreRequests = requests.filter(request => request.name === 'score-minimum');
    assert.equal(scoreRequests.length, 4);
    for (const request of scoreRequests) {
      for (const forbidden of ['--backend', '--no-backend-fallback', '--count', '--objective', '--max-patterns'])
        assert.ok(!request.arguments.includes(forbidden), 'score product owns its execution controls');
    }
    const setupRequests = requests.filter(request => request.name === 'setup-score');
    assert.equal(setupRequests.length, 4);
    assert.deepEqual(setupRequests.map(request => request.policy),
      ['false:false', 'true:false', 'false:true', 'true:true']);
    for (const request of setupRequests) {
      assert.equal(request.arguments[request.arguments.indexOf('--document') + 1], realSetupScoreDocument);
      assert.ok(!request.arguments.includes('--backend'));
      assert.ok(!request.arguments.includes('--max-patterns'));
    }
    const build = requests.find(request => request.name === 'build-cover');
    assert.equal(build.policy, 'default');
    for (const flag of ['--legal-board', '--no-legal-board', '--conditioned-reachability',
      '--no-conditioned-reachability', '--no-tablebase']) {
      assert.ok(!build.arguments.includes(flag));
      assert.throws(() => prepareClearraArguments([...build.arguments, flag]), /does not expose/u);
    }
  }
  assert.equal(count, 85);
  assert.throws(() => realCliProductProjectionRequests('unknown-profile'), /unknown/u);
});

test('the shared Setup-score fixture is bound to an actual Rust decoder proof without JS dependencies', () => {
  const source = readFileSync(new URL('../../crates/clearra-app/tests/exact_accelerator_product_execution.rs', import.meta.url), 'utf8');
  assert.ok(source.includes(`const SETUP_SCORE_DOCUMENT: &str = "${realSetupScoreDocument}";`));
  assert.ok(source.includes('fn actual_setup_score_fixture_has_two_i_targets_and_one_duplicate_page()'));
  assert.ok(source.includes('clearra_ctk3::decode_ctk3_exact(SETUP_SCORE_DOCUMENT)'));
  assert.ok(source.includes('assert_eq!(masks, [0xf, 0x3c0, 0xf])'));
  assert.ok(workflow.indexOf('- name: Verify focused CI path and failure contracts') <
    workflow.indexOf('- name: Install locked UI dependencies without scripts'));
});

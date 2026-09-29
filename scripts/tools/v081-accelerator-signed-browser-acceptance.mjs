// Browser acceptance for the actual v0.8.1 UI -> download worker -> signed
// product WASM -> immutable asset -> OPFS path. Reuse the wasm-realms job's
// ordinary WASM and qualified Release bytes; never rebuild or re-download.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { readFile, realpath, stat } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { frontendAcceleratorAssets } from './clearra-frontend-paths.mjs';
import { createClearraWasmBuildContract, clearraWasmBuildContractsEqual }
  from './clearra-wasm-build-contract.mjs';
import { realCliProductProjectionRequests, realSetupScoreDocument }
  from '../../apps/clearra-discord-bot/test/support/realCliProductProjectionRequests.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const profiles = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const buildResultSurfaces = {
  'all-solutions': 'section.solutions-section',
  'complete-replay-paths': 'section.product-pager.path-family',
  'minimum-solutions': 'section.product-pager',
  'field-average-score': 'section.solutions-section',
  'fixed-queue-maximum-score': 'section.product-pager.score-family',
  'highest-score-minimum-set': 'section.product-pager'
};
const pcProductInputs = Object.fromEntries(profiles.map(profile => {
  const requests = realCliProductProjectionRequests(profile);
  const products = Object.fromEntries(['minimum', 'score-minimum', 'replay'].map(name => {
    const baseline = requests.find(request => request.name === name && request.policy === 'false:false');
    const activated = requests.find(request => request.name === name && request.policy === 'true:true');
    assert.ok(baseline && activated && baseline.kind === activated.kind,
      `${profile}/${name}: the shared real CLI fixture must supply both accelerator policies`);
    return [name, { kind: baseline.kind, baseline: baseline.arguments, activated: activated.arguments }];
  }));
  return [profile, products];
}));
const preprocessor = vitePreprocess();
const fixturePath = resolve(root, 'apps/clearra-web/test/accelerator-signed-browser-fixture.svelte');
const fixture = `<script>
  import { tick } from 'svelte';
  import Panel from '../src/lib/AcceleratorDownloadPanel.svelte';
  import BuildProbabilityResult from '../../../packages/clearra-ui/src/lib/workspace/BuildProbabilityResult.svelte';
  import { projectWorkspaceSearchReport } from '../../../packages/clearra-ui/src/lib/workspace/workspaceSearchReport';
  let buildView = null;
  let buildMode = 'all-solutions';
  let loadNextProductPage = null;
  let loadProductMemberPage = null;
  let releaseProductPages = null;
  let loadSolutionPage = null;
  window.__clearBuildResult = async () => {
    buildView = null;
    await tick();
  };
  window.__showBuildResult = ({ response, searchReport, mode, nextPage, memberPage, releasePages, solutionPage }) => {
    loadNextProductPage = nextPage;
    loadProductMemberPage = memberPage;
    releaseProductPages = releasePages;
    loadSolutionPage = solutionPage;
    buildMode = mode;
    buildView = {
      kind: 'web', status: 'completed', terminationReason: null, jobId: null,
      progressLabel: '', progressDone: 0, progressTotal: 0,
      forwardPatternDone: 0, forwardPatternTotal: 0, progressTelemetry: null,
      publicFailures: [], developerDiagnostics: [], response,
      searchReport: projectWorkspaceSearchReport(
        searchReport,
        response.resource_report?.execution_availability,
        response.resource_report?.result_completeness
      ),
      webgpuReport: null, backendReport: response.backend_report ?? null,
      resourceReport: response.resource_report ?? null,
      renderCapability: response.capability_report?.render_capability ?? null,
      developerError: null
    };
  };
</script>
<Panel language="en" />
{#if buildView}
  <div data-testid="build-result" data-result-mode={buildMode}>
    <BuildProbabilityResult view={buildView} language="en" resultMode={buildMode}
      aggregation="buildability" height={4} existingMask={0n} targetMask={15n}
      {loadNextProductPage} {loadProductMemberPage} {releaseProductPages} {loadSolutionPage} />
  </div>
{/if}`;

async function component(source, path) {
  const processed = await preprocess(source, preprocessor, { filename: path });
  return {
    contents: compile(processed.code, { filename: path, generate: 'client', css: 'injected' }).js.code,
    loader: 'js', resolveDir: dirname(path),
  };
}

const ui = await build({
  absWorkingDir: root,
  stdin: {
    contents: `import { mount } from 'svelte';
      import Fixture from 'clearra-accelerator-browser-fixture';
      mount(Fixture, { target: document.getElementById('app') });`,
    resolveDir: root, sourcefile: 'accelerator-signed-browser-acceptance.js',
  },
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte', 'browser', 'module', 'main'],
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'compiled-accelerator-panel', setup(builder) {
    builder.onResolve({ filter: /^clearra-accelerator-browser-fixture$/ },
      () => ({ path: fixturePath, namespace: 'accelerator-fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'accelerator-fixture' }, () => component(fixture, fixturePath));
    builder.onResolve({ filter: /^\$app\/paths$/ },
      () => ({ path: 'paths', namespace: 'accelerator-sveltekit' }));
    builder.onLoad({ filter: /.*/, namespace: 'accelerator-sveltekit' },
      () => ({ contents: "export const base = '';", loader: 'js' }));
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path }) => component(await readFile(path, 'utf8'), path));
  } }],
});
const worker = await build({
  absWorkingDir: root,
  entryPoints: [resolve(root, 'apps/clearra-web/src/workers/acceleratorDownloadWorker.ts')],
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['browser', 'module', 'main'],
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
});
const productWorker = await build({
  absWorkingDir: root,
  entryPoints: [resolve(root, 'apps/clearra-web/src/workers/clearraWorker.ts')],
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['browser', 'module', 'main'],
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
  define: { 'import.meta.env.MODE': '"production"' },
});
const verifierWorker = await build({
  absWorkingDir: root,
  entryPoints: [resolve(root, 'apps/clearra-web/src/workers/clearraVerifierWorker.ts')],
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['browser', 'module', 'main'],
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
});
const hostSnapshot = await build({
  absWorkingDir: root,
  entryPoints: [resolve(root, 'packages/clearra-ui/src/lib/wasm/hostCapabilitySnapshot.ts')],
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['browser', 'module', 'main'],
});
assert.equal(ui.outputFiles.length, 1);
assert.equal(worker.outputFiles.length, 1);
assert.equal(productWorker.outputFiles.length, 1);
assert.equal(verifierWorker.outputFiles.length, 1);
assert.equal(hostSnapshot.outputFiles.length, 1);
assert.match(ui.outputFiles[0].text, /\.\.\/workers\/acceleratorDownloadWorker\.ts/u,
  'the actual panel must construct its production download worker');
assert.match(productWorker.outputFiles[0].text, /clearraVerifierWorker\.ts/u,
  'the actual product worker must construct its production verifier worker');
// A source-only bundle check is useful locally when the signed CI inputs are
// absent. It makes no browser, WASM, or asset-admission claim.
if (process.argv.includes('--bundle-only')) {
  console.log('v0.8.1 signed browser UI, product, and verifier worker bundles: source-only pass');
} else {
  await browserAcceptance();
}

async function sha256(path) {
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(path)) hash.update(bytes);
  return hash.digest('hex');
}

async function browserAcceptance() {
  for (const name of ['CLEARRA_BROWSER_TOOLS_ROOT', 'CLEARRA_REAL_ACCELERATOR_WASM_DIR',
    'CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR', 'CLEARRA_SOURCE_COMMIT', 'CLEARRA_ENGINE_BUILD_ID']) {
    assert.ok(process.env[name], `${name} is required`);
  }
  const wasmRoot = await realpath(process.env.CLEARRA_REAL_ACCELERATOR_WASM_DIR);
  const assetRoot = await realpath(process.env.CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR);
  assert.equal(wasmRoot, join(await realpath(root), '_local/artifacts/v081-browser-peer-smoke/wasm'));
  assert.equal(assetRoot, join(await realpath(root), '_local/artifacts/v081-peer-signed-smoke'));
  const manifest = JSON.parse(await readFile(join(wasmRoot, 'clearra_wasm.manifest.json'), 'utf8'));
  assert.ok(clearraWasmBuildContractsEqual(manifest.build, await createClearraWasmBuildContract(root)),
    'the ordinary product WASM must match the exact source and runtime identity');

  const files = new Map();
  files.set('/wasm/clearra_wasm.manifest.json', {
    path: join(wasmRoot, 'clearra_wasm.manifest.json'), type: 'application/json',
  });
  for (const [entry, type] of [[manifest.bindings, 'text/javascript'],
    [manifest.wasm, 'application/wasm']]) {
    assert.match(entry.path, /^clearra_wasm(?:_bg)?\.[a-z0-9.]+$/u);
    const path = join(wasmRoot, entry.path);
    assert.equal((await stat(path)).size, entry.bytes);
    assert.equal(await sha256(path), entry.sha256);
    files.set(`/wasm/${entry.path}`, { path, type });
  }
  const assets = await frontendAcceleratorAssets(root);
  assert.equal(assets.length, profiles.length * 2);
  for (const profile of profiles) {
    assert.deepEqual(assets.filter(asset => asset.profile === profile).map(asset => asset.product).sort(),
      ['board-conditioned-reachability', 'exact-legal-board'],
      `${profile}: both independent qualified products must be present`);
  }
  for (const asset of assets) {
    const path = join(assetRoot, basename(new URL(asset.url).pathname));
    assert.equal((await stat(path)).size, asset.bytes);
    assert.equal(await sha256(path), asset.digest);
    files.set(asset.pathname, { path, type: 'application/octet-stream' });
  }

  const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
  const { chromium } = require('playwright');
  const assetRequests = [];
  const verifierRequests = [];
  const server = createServer((request, response) => {
    const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
    response.setHeader('Cache-Control', 'no-store');
    if (pathname === '/') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/ui.js"></script>');
      return;
    }
    const bundled = pathname === '/ui.js' ? ui.outputFiles[0].contents
      : pathname === '/capabilities.js' ? hostSnapshot.outputFiles[0].contents
      : pathname === '/workers/acceleratorDownloadWorker.ts' ? worker.outputFiles[0].contents
      : pathname === '/workers/clearraWorker.ts' ? productWorker.outputFiles[0].contents
      : pathname === '/workers/clearraVerifierWorker.ts' ? verifierWorker.outputFiles[0].contents : null;
    if (bundled) {
      if (pathname === '/workers/clearraVerifierWorker.ts') verifierRequests.push(pathname);
      response.writeHead(200, { 'Content-Type': 'text/javascript; charset=utf-8' });
      response.end(bundled);
      return;
    }
    const file = files.get(pathname);
    if (!file) { response.writeHead(404).end(); return; }
    if (pathname.startsWith('/accel/')) assetRequests.push(pathname);
    const stream = createReadStream(file.path);
    stream.on('error', error => response.destroy(error));
    response.writeHead(200, { 'Content-Type': file.type });
    stream.pipe(response);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  let browser;
  try {
    browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] });
    const context = await browser.newContext();
    const page = await context.newPage();
    page.setDefaultTimeout(90_000);
    const errors = [];
    page.on('pageerror', error => errors.push(String(error.stack || error)));
    await page.goto(`http://127.0.0.1:${address.port}/`);
    await page.locator('details.accelerator-download summary').click();
    const download = page.getByRole('button', { name: 'Download displayed asset' });
    const current = page.getByRole('status').filter({ hasText: 'Verified asset saved' });
    const empty = page.getByRole('status').filter({ hasText: 'No saved asset' });
    const productSelect = page.locator('details.accelerator-download select').first();
    const profileSelect = page.locator('details.accelerator-download select').nth(1);
    await download.waitFor();
    assert.deepEqual(assetRequests, [], 'status and capacity must not download an asset');
    for (const [index, profile] of profiles.entries()) {
      if (index > 0) await profileSelect.selectOption(String(index));
      for (const kind of ['0', '1']) {
        await productSelect.selectOption(kind);
        await empty.waitFor();
        await download.waitFor();
        assert.equal(assetRequests.filter(pathname => assets.some(asset =>
          asset.profile === profile && asset.pathname === pathname)).length, Number(kind),
        `${profile}: status or selection must not download an asset`);
        await download.click();
        await current.waitFor();
      }
      assert.deepEqual(assetRequests.filter(pathname => assets.some(asset =>
        asset.profile === profile && asset.pathname === pathname)).sort(),
      assets.filter(asset => asset.profile === profile).map(asset => asset.pathname).sort(),
      `${profile}: one explicit request per signed product`);
    }

    const second = await context.newPage();
    second.setDefaultTimeout(90_000);
    second.on('pageerror', error => errors.push(String(error.stack || error)));
    await second.goto(`http://127.0.0.1:${address.port}/`);
    await second.locator('details.accelerator-download summary').click();
    const secondProduct = second.locator('details.accelerator-download select').first();
    const secondProfile = second.locator('details.accelerator-download select').nth(1);
    for (const [index] of profiles.entries()) {
      if (index > 0) await secondProfile.selectOption(String(index));
      for (const kind of ['0', '1']) {
        await secondProduct.selectOption(kind);
        await second.getByRole('status').filter({ hasText: 'Verified asset saved' }).waitFor();
      }
    }
    assert.equal(assetRequests.length, assets.length, 'cross-tab read must not re-download');
    const execution = await page.evaluate(async ({ profiles, setupScoreDocument, pcProductInputs, buildResultSurfaces }) => {
      const { createHostCapabilitySnapshot, resolveWorkerAuthority } = await import('/capabilities.js');
      const logicalProcessors = navigator.hardwareConcurrency;
      if (!Number.isSafeInteger(logicalProcessors) || logicalProcessors < 2) {
        throw new Error('browser_verifier_pool_requires_two_logical_processors');
      }
      const snapshot = createHostCapabilitySnapshot({
        snapshotId: 'signed-browser-functional-pool', source: 'browser-main',
        reportedLogicalProcessors: logicalProcessors,
        reportedDeviceMemoryGiB: navigator.deviceMemory,
        webGpuAvailable: false, crossOriginIsolated: self.crossOriginIsolated,
      });
      const workers = Math.min(3, logicalProcessors);
      const workerAuthority = resolveWorkerAuthority(snapshot, workers);
      async function runOnWorker(rootWorker, profile, legal, relation, input) {
        let timeout;
        let onMessage;
        let onError;
        try {
          const terminal = new Promise((resolve, reject) => {
            timeout = setTimeout(() => reject(new Error('browser_product_worker_timeout')), 120_000);
            onError = error => reject(new Error(`browser_product_worker_error: ${error.message}`));
            onMessage = ({ data }) => {
              if (data.event === 'final_response') resolve(data);
              if (data.event === 'failed' || data.event === 'cancelled' || data.event === 'terminated') {
                reject(new Error(`browser_product_worker_${data.event}: ${JSON.stringify(data.diagnostics ?? data)}`));
              }
            };
            rootWorker.addEventListener('error', onError);
            rootWorker.addEventListener('message', onMessage);
          });
          const acceleratorFlags = (legal ? '--legal-board ' : '--no-legal-board ') +
            (relation ? '--conditioned-reachability' : '--no-conditioned-reachability');
          let commandText;
          if (Array.isArray(input)) {
            commandText = `clearra ${input.join(' ')} --workers ${workers}`;
          } else if (input === 'setup-score') {
            commandText = `clearra setup score --document-format ctk3 --document ${setupScoreDocument} ` +
              '--setup-queue I --solution-queue OOOI --clear 2 --no-hold ' +
              `--score-profile tetrio --initial-b2b 0 --workers ${workers} --rule ${profile} ${acceleratorFlags}`;
          } else if (input === 'build-minimum') {
            commandText = 'clearra build cover --base-mask 0 --target-mask 0xf --height 4 ' +
              '--queue I --no-hold --queue-knowledge oracle --objective min-cover ' +
              `--backend cpu --no-backend-fallback --workers ${workers} --rule ${profile} ${acceleratorFlags}`;
          } else if (input.startsWith('build-probability:')) {
            const resultMode = input.slice('build-probability:'.length);
            const scoreOptions = [
              'field-average-score', 'fixed-queue-maximum-score', 'highest-score-minimum-set'
            ].includes(resultMode) ? '--score-profile tetrio --initial-b2b 0 ' : '';
            commandText = 'clearra build-probability --base-mask 0 --target-mask 0xf --height 4 ' +
              '--queue I --no-hold --include-mirror --aggregate buildability ' +
              `--result-mode ${resultMode} --solution-probabilities ${scoreOptions}` +
              `--backend cpu --workers ${workers} --rule ${profile} ${acceleratorFlags}`;
          } else {
            commandText = `clearra pc --lines 4 --height 4 ` +
              (input === 'existing'
                ? '--board-mask 0x3c0f03c0f --pieces 6 --patterns P7 --hold empty '
                : '--board-mask 0 --pieces 10 --queue IIOOOIIOOO --no-hold ') +
              '--objective unique --count unique ' +
              `--solution-probabilities --backend cpu --workers ${workers} --rule ${profile} --no-tablebase ` +
              acceleratorFlags;
          }
          rootWorker.postMessage({ type: 'run_command_text', commandText,
            prewarmWorkerCount: workers, tablebaseRequested: false,
            hostCapabilitySnapshot: snapshot, workerAuthority,
            warmupPolicy: { backend: 'cpu', cpuWarmup: false, gpuWarmup: false } });
          const result = await terminal;
          return { result };
        } finally {
          clearTimeout(timeout);
          if (onError) rootWorker.removeEventListener('error', onError);
          if (onMessage) rootWorker.removeEventListener('message', onMessage);
        }
      }
      async function run(profile, legal, relation, input, renderMode = null) {
        const rootWorker = new Worker('/workers/clearraWorker.ts', { type: 'module' });
        try {
          const output = await runOnWorker(rootWorker, profile, legal, relation, input);
          if (renderMode !== null) {
            await window.__clearBuildResult();
            let pageRequestId = 0;
            let completedProductPages = 0;
            const loadProductPage = (action, alternativeIndex, memberPageNumber, signal, maximumWorkSteps) =>
              new Promise((resolve, reject) => {
                const requestId = ++pageRequestId;
                const finish = () => {
                  clearTimeout(timeout);
                  rootWorker.removeEventListener('message', onMessage);
                  rootWorker.removeEventListener('error', onError);
                  signal?.removeEventListener('abort', onAbort);
                };
                const onMessage = ({ data }) => {
                  if (data.request_id !== requestId) return;
                  if (data.type === 'product_page') {
                    if (data.payload?.state === 'page') completedProductPages += 1;
                    finish();
                    resolve(data.payload);
                  } else if (data.type === 'product_page_failed') {
                    finish();
                    reject(new Error(data.message));
                  }
                };
                const onError = error => {
                  finish();
                  reject(new Error(`browser_product_page_worker_error: ${error.message}`));
                };
                const onAbort = () => {
                  finish();
                  reject(new DOMException('Product page request aborted', 'AbortError'));
                };
                const timeout = setTimeout(() => {
                  finish();
                  reject(new Error('browser_product_page_timeout'));
                }, 30_000);
                rootWorker.addEventListener('message', onMessage);
                rootWorker.addEventListener('error', onError);
                signal?.addEventListener('abort', onAbort, { once: true });
                if (signal?.aborted) {
                  onAbort();
                  return;
                }
                rootWorker.postMessage({ type: 'load_product_page', requestId, action,
                  alternativeIndex, memberPageNumber, maximumWorkSteps });
              });
            const loadSolutionPage = (offset, limit, signal) =>
              new Promise((resolve, reject) => {
                const requestId = ++pageRequestId;
                const finish = () => {
                  clearTimeout(timeout);
                  rootWorker.removeEventListener('message', onMessage);
                  rootWorker.removeEventListener('error', onError);
                  signal?.removeEventListener('abort', onAbort);
                };
                const onMessage = ({ data }) => {
                  if (data.request_id !== requestId) return;
                  if (data.type === 'solution_page') {
                    finish();
                    resolve({ keys: data.keys, total: data.total });
                  } else if (data.type === 'solution_page_failed') {
                    finish();
                    reject(new Error(data.message));
                  }
                };
                const onError = error => {
                  finish();
                  reject(new Error(`browser_solution_page_worker_error: ${error.message}`));
                };
                const onAbort = () => {
                  finish();
                  reject(new DOMException('Solution page request aborted', 'AbortError'));
                };
                const timeout = setTimeout(() => {
                  finish();
                  reject(new Error('browser_solution_page_timeout'));
                }, 30_000);
                rootWorker.addEventListener('message', onMessage);
                rootWorker.addEventListener('error', onError);
                signal?.addEventListener('abort', onAbort, { once: true });
                if (signal?.aborted) {
                  onAbort();
                  return;
                }
                rootWorker.postMessage({ type: 'load_solution_page', requestId, offset, limit });
              });
            window.__showBuildResult({
              response: output.result.response,
              searchReport: output.result.search_report ?? null,
              mode: renderMode,
              nextPage: (signal) => loadProductPage('next', undefined, undefined, signal),
              memberPage: (alternativeIndex, memberPageNumber, signal, maximumWorkSteps) =>
                loadProductPage('get', alternativeIndex, memberPageNumber, signal, maximumWorkSteps),
              releasePages: () => rootWorker.postMessage({ type: 'release_product_pages' }),
              solutionPage: loadSolutionPage
            });
            await new Promise((resolve, reject) => {
              const surfaceSelector = `[data-testid="build-result"][data-result-mode="${renderMode}"] ${buildResultSurfaces[renderMode]}`;
              const visibleResultSelector = renderMode === 'complete-replay-paths'
                ? '.path-representative' : 'li[data-solution-key]';
              const app = document.getElementById('app');
              const finish = () => {
                clearTimeout(timeout);
                observer.disconnect();
              };
              const check = () => {
                if (!document.querySelector(`${surfaceSelector} ${visibleResultSelector}`)) return;
                finish();
                resolve();
              };
              const observer = new MutationObserver(check);
              const timeout = setTimeout(() => {
                finish();
                const surface = document.querySelector(surfaceSelector);
                reject(new Error(`${renderMode}: production Build result did not show ${visibleResultSelector}; ` +
                  `surface_present=${Boolean(surface)}; response_solutions=${output.result.search_report?.unique_solution_count ?? 'typed'}; ` +
                  `resource_state=${output.result.response.resource_report?.execution_availability?.state ?? 'missing'}; ` +
                  `completeness=${output.result.response.resource_report?.result_completeness ?? 'missing'}; ` +
                  `surface_text=${JSON.stringify(surface?.textContent?.trim().slice(0, 200) ?? '')}`));
              }, 30_000);
              observer.observe(app, { childList: true, subtree: true, attributes: true });
              check();
            });
            const productContent = output.result.response.product_result_payload?.content;
            const lazyBuildPage = productContent?.payload_kind === 'build-coverage-portfolio-v2'
              ? productContent.payload.page_source_available
              : productContent?.payload_kind === 'build-v2' &&
                ['portfolio', 'score-portfolio'].includes(productContent.payload.kind)
                ? productContent.payload.page_source_available : false;
            if (lazyBuildPage && completedProductPages < 1) {
              throw new Error(`${renderMode}: production renderer did not consume a real worker product page`);
            }
          }
          return output;
        } finally {
          rootWorker.postMessage({ type: 'dispose_runtime' });
          await new Promise(resolve => setTimeout(resolve, 100));
          rootWorker.terminate();
        }
      }
      const results = {};
      for (const profile of profiles) {
        results[profile] = {
          eligible: {
            baseline: await run(profile, false, false, 'eligible'),
            activated: await run(profile, true, true, 'eligible')
          }
        };
        results[profile].existing = {
          baseline: await run(profile, false, false, 'existing'),
          activated: await run(profile, true, true, 'existing')
        };
        results[profile].setup = {
          baseline: await run(profile, false, false, 'setup-score'),
          activated: await run(profile, true, true, 'setup-score')
        };
        results[profile].products = {};
        for (const [name, input] of Object.entries(pcProductInputs[profile])) {
          results[profile].products[name] = {
            baseline: await run(profile, false, false, input.baseline),
            activated: await run(profile, true, true, input.activated)
          };
        }
      }
      results['srs-plus'].build = {};
      for (const mode of ['all-solutions', 'complete-replay-paths', 'minimum-solutions',
        'field-average-score', 'fixed-queue-maximum-score', 'highest-score-minimum-set']) {
        const input = mode === 'minimum-solutions' ? 'build-minimum' : `build-probability:${mode}`;
        results['srs-plus'].build[mode] = {
          baseline: await run('srs-plus', false, false, input),
          activated: await run('srs-plus', true, true, input, mode)
        };
      }
      // Exercise invalidation on one warm owner, not merely a fresh worker.
      // A corrupt local pointer must revoke already-admitted negative proof
      // and relation authority before the next exact search begins.
      const rootWorker = new Worker('/workers/clearraWorker.ts', { type: 'module' });
      const corruption = {};
      try {
        async function corrupt(product) {
          const origin = await navigator.storage.getDirectory();
          const root = await origin.getDirectoryHandle('clearra-exact-accelerators-v1');
          const directory = await (await root.getDirectoryHandle(product)).getDirectoryHandle('srs-plus');
          const pointer = await (await directory.getFileHandle('active.json')).createWritable();
          await pointer.write('null');
          await pointer.close();
        }
        corruption.legalBefore = await runOnWorker(rootWorker, 'srs-plus', true, false, 'eligible');
        await corrupt('exact-legal-board');
        corruption.legalAfter = await runOnWorker(rootWorker, 'srs-plus', true, false, 'eligible');
        corruption.relationBefore = await runOnWorker(rootWorker, 'srs-plus', false, true, 'eligible');
        await corrupt('board-conditioned-reachability');
        corruption.relationAfter = await runOnWorker(rootWorker, 'srs-plus', false, true, 'eligible');
      } finally {
        rootWorker.postMessage({ type: 'dispose_runtime' });
        await new Promise(resolve => setTimeout(resolve, 100));
        rootWorker.terminate();
      }
      return { results, workers, corruption };
    }, { profiles, setupScoreDocument: realSetupScoreDocument, pcProductInputs, buildResultSurfaces })
      .catch(error => {
        throw new Error(`signed browser product execution failed; page_errors=${JSON.stringify(errors.slice(-3))}`,
          { cause: error });
      });
    const compact = ({ result }) => {
      assert.equal(result.event, 'final_response');
      assert.equal(result.response.status, 'success');
      assert.deepEqual(result.response.runtime_identity, manifest.build.runtime_identity);
      const report = result.search_report;
      assert.ok(report.unique_solution_count > 0, 'the functional query must find actual solutions');
      assert.equal(report.normalized_solution_keys.length, report.unique_solution_count);
      return {
        keys: report.normalized_solution_keys,
        hash: report.normalized_solution_set_hash,
        covered: report.covered_pattern_count,
        total: report.total_possible_pattern_count,
        probability: report.coverage_probability,
        probabilities: report.solution_probabilities,
      };
    };
    const existingCounts = { srs: 245, 'srs-plus': 246, 'srs-x': 289,
      'jstris-180': 246, 'no-kick': 175 };
    for (const profile of profiles) {
      for (const scope of ['eligible', 'existing']) {
        const pair = execution.results[profile][scope];
        const name = `${profile}/${scope}`;
        const activated = compact(pair.activated);
        const baseline = compact(pair.baseline);
        assert.equal(activated.keys.length, scope === 'existing' ? existingCounts[profile] : 159,
          `${name}: retain the independently measured exact WASM fixture count`);
        assert.deepEqual(activated, baseline,
          `${name}: installed accelerators must preserve the complete browser result`);
        assert.equal(pair.activated.result.search_report.cpu_parallel_execution, true,
          `${name}: the browser product must execute its distributed CPU path`);
        assert.equal(pair.activated.result.search_report.workers_used, execution.workers,
          `${name}: the browser product must report the requested worker count without silent reduction`);
        const baselineFields = new Map(pair.baseline.result.search_report.summary_fields);
        const activatedFields = new Map(pair.activated.result.search_report.summary_fields);
        assert.equal(baselineFields.get('conditioned_reachability_snapshot_active'), 'false');
        assert.equal(activatedFields.get('conditioned_reachability_requested'), 'true');
        assert.equal(activatedFields.get('conditioned_reachability_snapshot_active'), 'true',
          `${name}: the browser root must activate the matching OPFS relation generation`);
        assert.match(activatedFields.get('legal_board_verified_negative_prunes') ?? '', /^\d+$/u,
          `${name}: retain the legal-board pruning counter, including zero outside its scope`);
      }
      const setup = execution.results[profile].setup;
      const ranking = sample => {
        assert.equal(sample.result.event, 'final_response');
        assert.equal(sample.result.response.status, 'success');
        assert.deepEqual(sample.result.response.runtime_identity, manifest.build.runtime_identity);
        const payload = sample.result.response.product_result_payload;
        assert.equal(payload?.contract, 'setup.score');
        assert.equal(payload?.result_kind, 'setup-score-ranking.v1');
        assert.equal(payload?.content.payload_kind, 'setup-score-ranking');
        const result = payload.content.payload;
        assert.equal(result.complete, true);
        assert.equal(result.rule_profile, profile);
        assert.equal(result.source_page_count, '3');
        assert.equal(result.candidate_count, '2');
        assert.equal(result.setup_pattern_count, '1');
        assert.deepEqual(result.candidates.map(candidate => candidate.rank), ['1', '2']);
        assert.deepEqual(result.candidates.map(candidate => candidate.completed_board_mask),
          ['0x000000000000000f', '0x00000000000003c0']);
        assert.ok(Number.isFinite(Number(result.average_priority_score)) &&
          Number(result.average_priority_score) > 0);
        return result;
      };
      assert.deepEqual(ranking(setup.activated), ranking(setup.baseline),
        `${profile}/setup: accelerator policy must preserve complete ranked Setup-score results`);
    }
    const build = execution.results['srs-plus'].build['all-solutions'];
    const buildBaseline = compact(build.baseline);
    assert.equal(buildBaseline.keys.length, 2,
      'the Web Build probability fixture must retain both mirror-distinct I placements');
    for (const [name, sample] of Object.entries(build)) {
      const report = sample.result.search_report;
      assert.equal(report.count_complete, true, `${name}: Build solution count must be complete`);
      assert.equal(report.solution_keys_complete, true, `${name}: Build solution keys must be complete`);
      assert.equal(report.probability_complete, true, `${name}: Build probability must be complete`);
      assert.equal(report.resource_truncated, false, `${name}: Build result must not be truncated`);
      assert.equal(report.covered_pattern_count, 1, `${name}: the fixed queue must be covered`);
      assert.equal(report.materialized_pattern_count, 1, `${name}: retain the complete queue universe`);
    }
    assert.deepEqual(compact(build.activated), buildBaseline,
      'installed accelerators must preserve complete Web Build probability results');
    // Lazy products may deliberately leave the generic solution family
    // unmaterialized; compare its actual meaning separately from the payload.
    const productSearchMeaning = ({ result }) => {
      const report = result.search_report;
      return {
        solutionFound: report.solution_found,
        countComplete: report.count_complete,
        solutionCountCalculated: report.solution_count_calculated,
        solutionSetMaterialized: report.solution_set_materialized,
        solutionPageAvailable: report.solution_page_available,
        uniqueSolutionCount: report.unique_solution_count,
        normalizedSolutionKeys: report.normalized_solution_keys,
        normalizedSolutionSetHash: report.normalized_solution_set_hash,
        coveredPatternCount: report.covered_pattern_count,
        totalPossiblePatternCount: report.total_possible_pattern_count,
        coverageProbability: report.coverage_probability,
        probabilityComplete: report.probability_complete,
      };
    };
    const buildProducts = {
      'complete-replay-paths': ['build.complete-replay-paths', 'build-path-family.v1'],
      'minimum-solutions': ['build.cover', 'build-coverage-portfolio.v2'],
      'field-average-score': ['build.field-average-score', 'build-field-average-score.v1'],
      'fixed-queue-maximum-score': ['build.fixed-queue-maximum-score', 'build-fixed-score-witness.v1'],
      'highest-score-minimum-set': ['build.highest-score-minimum-set', 'build-probability-score-minimum.v1']
    };
    for (const [mode, [contract, resultKind]] of Object.entries(buildProducts)) {
      const pair = execution.results['srs-plus'].build[mode];
      for (const sample of Object.values(pair)) {
        assert.equal(sample.result.event, 'final_response', `${mode}: complete execution`);
        assert.equal(sample.result.response.status, 'success', `${mode}: successful execution`);
        assert.deepEqual(sample.result.response.runtime_identity, manifest.build.runtime_identity);
        const payload = sample.result.response.product_result_payload;
        assert.equal(payload?.contract, contract);
        assert.equal(payload?.result_kind, resultKind);
        if (mode === 'minimum-solutions') {
          // Typed Build cover owns its result without a legacy SearchReport.
          assert.equal(payload.content.payload.completeness.exact_minimum_proven, true);
          assert.equal(payload.content.payload.page_source_available, true);
        } else {
          assert.equal(sample.result.search_report.count_complete, true, `${mode}: complete source count`);
          assert.equal(sample.result.search_report.resource_truncated, false, `${mode}: no truncation`);
        }
      }
      if (mode !== 'minimum-solutions') {
        assert.deepEqual(productSearchMeaning(pair.activated), productSearchMeaning(pair.baseline),
          `${mode}: signed assets must preserve the Build search meaning`);
      }
      assert.deepEqual(pair.activated.result.response.product_result_payload,
        pair.baseline.result.response.product_result_payload,
        `${mode}: signed assets must preserve the complete Build product result`);
    }
    for (const profile of profiles) {
      for (const [name, input] of Object.entries(pcProductInputs[profile])) {
        const pair = execution.results[profile].products[name];
        for (const sample of Object.values(pair)) {
          assert.equal(sample.result.event, 'final_response');
          assert.equal(sample.result.response.status, 'success');
          assert.deepEqual(sample.result.response.runtime_identity, manifest.build.runtime_identity);
          const payload = sample.result.response.product_result_payload;
          assert.equal(payload?.result_kind, input.kind);
          assert.equal(payload?.contract, {
            minimum: 'pc.minimals', 'score-minimum': 'pc.score-minimals', replay: 'pc.path'
          }[name]);
          assert.equal(payload?.content.payload_kind,
            name === 'replay' ? 'pc-path-family' : 'coverage-portfolio');
          assert.equal(sample.result.search_report.count_complete, true);
          assert.equal(sample.result.search_report.resource_truncated, false);
          if (name === 'replay') {
            assert.equal(payload.content.payload.complete, true);
            if (profile === 'srs-plus') assert.ok(Number(payload.content.payload.witness_count) > 0);
          } else {
            assert.equal(payload.content.payload.page_handle_available, true);
            if (profile === 'srs-plus') assert.ok(payload.content.payload.members.length > 0);
          }
        }
        assert.deepEqual(productSearchMeaning(pair.activated), productSearchMeaning(pair.baseline),
          `${profile}/${name}: installed accelerators must preserve complete search results`);
        assert.deepEqual(pair.activated.result.response.product_result_payload,
          pair.baseline.result.response.product_result_payload,
          `${profile}/${name}: installed accelerators must preserve canonical product pages`);
      }
    }
    const corruption = execution.corruption;
    const expected = compact(execution.results['srs-plus'].eligible.baseline);
    for (const [name, sample] of Object.entries(corruption)) {
      assert.deepEqual(compact(sample), expected,
        `${name}: corrupt OPFS pointers must preserve the complete exact result`);
    }
    const summary = sample => new Map(sample.result.search_report.summary_fields);
    const legalPrunesBefore = Number(summary(corruption.legalBefore).get('legal_board_verified_negative_prunes'));
    assert.ok(legalPrunesBefore > 0,
      `the warm legal-board owner must have exercised a real negative prune; observed=${legalPrunesBefore}`);
    assert.equal(summary(corruption.legalAfter).get('legal_board_verified_negative_prunes'), '0',
      'a corrupt legal-board pointer must revoke the warm negative proof');
    assert.equal(summary(corruption.relationBefore).get('conditioned_reachability_snapshot_active'), 'true');
    assert.equal(summary(corruption.relationAfter).get('conditioned_reachability_snapshot_active'), 'false',
      'a corrupt relation pointer must revoke the warm relation snapshot');
    assert.ok(verifierRequests.length >= 1, 'the actual verifier worker must be loaded');
    assert.equal(assetRequests.length, assets.length, 'a search must read OPFS, not re-download signed assets');
    assert.deepEqual(errors, []);
    // A 0.5 GiB device cannot transfer the larger legal-board bundle in one
    // search-worker message. The saved asset must be presented as inactive,
    // not as an accelerator that silently ran or as a failed exact search.
    await context.addInitScript(() => {
      Object.defineProperty(navigator, 'deviceMemory', { configurable: true, value: 0.5 });
    });
    const lowMemoryPage = await context.newPage();
    try {
      await lowMemoryPage.goto(`http://127.0.0.1:${address.port}/`);
      await lowMemoryPage.locator('details.accelerator-download summary').click();
      await lowMemoryPage.locator('details.accelerator-download select').nth(1).selectOption('2');
      await lowMemoryPage.getByRole('status').filter({ hasText: 'Verified asset saved' }).waitFor();
      const unavailable = lowMemoryPage.getByRole('status').filter({
        hasText: 'This asset exceeds this device’s search-worker transfer limit'
      });
      await unavailable.waitFor();
      assert.match(await unavailable.textContent(), /16\.0 MiB/u,
        'low-memory SRS-X must disclose the actual worker limit before download');
      assert.equal(assetRequests.length, assets.length,
        'a low-memory status check must not download an unusable asset');
      const lowMemoryResult = await lowMemoryPage.evaluate(async () => {
        const { createHostCapabilitySnapshot, resolveWorkerAuthority } = await import('/capabilities.js');
        const snapshot = createHostCapabilitySnapshot({
          snapshotId: 'signed-browser-low-memory-exact-fallback', source: 'browser-main',
          reportedLogicalProcessors: navigator.hardwareConcurrency,
          reportedDeviceMemoryGiB: navigator.deviceMemory,
          webGpuAvailable: false, crossOriginIsolated: self.crossOriginIsolated
        });
        if (snapshot.wasmTransferByteCap !== 16 * 1024 * 1024) {
          throw new Error('low-memory browser snapshot did not keep the 16 MiB transfer cap');
        }
        const worker = new Worker('/workers/clearraWorker.ts', { type: 'module' });
        try {
          return await new Promise((resolve, reject) => {
            const timeout = setTimeout(() => reject(new Error('low_memory_exact_fallback_timeout')), 120_000);
            worker.addEventListener('error', error => {
              clearTimeout(timeout);
              reject(new Error(`low_memory_worker_error: ${error.message}`));
            });
            worker.addEventListener('message', ({ data }) => {
              if (data.event === 'final_response') {
                clearTimeout(timeout);
                resolve(data);
              } else if (['failed', 'cancelled', 'terminated'].includes(data.event)) {
                clearTimeout(timeout);
                reject(new Error(`low_memory_worker_${data.event}: ${JSON.stringify(data.diagnostics ?? data)}`));
              }
            });
            worker.postMessage({
              type: 'run_command_text',
              commandText: 'clearra pc --lines 4 --height 4 --board-mask 0 --pieces 10 ' +
                '--queue IIOOOIIOOO --no-hold --objective unique --count unique ' +
                '--solution-probabilities --backend cpu --workers 1 --rule srs-x ' +
                '--no-tablebase --legal-board --no-conditioned-reachability',
              prewarmWorkerCount: 1, tablebaseRequested: false,
              hostCapabilitySnapshot: snapshot,
              workerAuthority: resolveWorkerAuthority(snapshot, 1),
              warmupPolicy: { backend: 'cpu', cpuWarmup: false, gpuWarmup: false }
            });
          });
        } finally {
          worker.postMessage({ type: 'dispose_runtime' });
          await new Promise(resolve => setTimeout(resolve, 100));
          worker.terminate();
        }
      });
      assert.deepEqual(compact({ result: lowMemoryResult }),
        compact(execution.results['srs-x'].eligible.baseline),
        'an installed oversized SRS-X asset must preserve the exact browser result');
      assert.equal(new Map(lowMemoryResult.search_report.summary_fields)
        .get('legal_board_verified_negative_prunes'), '0',
      'the oversized asset must not silently claim a negative proof');
      assert.equal(assetRequests.length, assets.length,
        'low-memory exact fallback must read local state without new network asset reads');
    } finally {
      await lowMemoryPage.close();
    }
    await context.close();
    console.log('v0.8.1 signed browser UI and product pool: five profiles, OPFS, cross-tab read, actual verifier workers, warm corrupt-pointer fail-open and complete PC/Setup-score/minimum/score-minimum/replay parity, plus SRS+ Build result-mode and production renderer parity passed');
  } finally {
    await browser?.close();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
}

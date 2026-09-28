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

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const profiles = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const preprocessor = vitePreprocess();
const fixturePath = resolve(root, 'apps/clearra-web/test/accelerator-signed-browser-fixture.svelte');
const fixture = `<script>
  import Panel from '../src/lib/AcceleratorDownloadPanel.svelte';
</script>
<Panel language="en" />`;

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
    const execution = await page.evaluate(async profiles => {
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
      async function run(profile, legal, relation, input) {
        const rootWorker = new Worker('/workers/clearraWorker.ts', { type: 'module' });
        let timeout;
        try {
          const terminal = new Promise((resolve, reject) => {
            timeout = setTimeout(() => reject(new Error('browser_product_worker_timeout')), 120_000);
            rootWorker.addEventListener('error', error => reject(new Error(`browser_product_worker_error: ${error.message}`)));
            rootWorker.addEventListener('message', ({ data }) => {
              if (data.event === 'final_response') resolve(data);
              if (data.event === 'failed' || data.event === 'cancelled' || data.event === 'terminated') {
                reject(new Error(`browser_product_worker_${data.event}: ${JSON.stringify(data.diagnostics ?? data)}`));
              }
            });
          });
          const inputArgs = input === 'existing'
            ? '--board-mask 0x3c0f03c0f --pieces 6 --patterns P7 --hold empty'
            : '--board-mask 0 --pieces 10 --queue IIOOOIIOOO --no-hold';
          const commandText = `clearra pc --lines 4 --height 4 ${inputArgs} ` +
            '--objective unique --count unique ' +
            `--solution-probabilities --backend cpu --workers ${workers} --rule ${profile} --no-tablebase ` +
            (legal ? '--legal-board ' : '--no-legal-board ') +
            (relation ? '--conditioned-reachability' : '--no-conditioned-reachability');
          rootWorker.postMessage({ type: 'run_command_text', commandText,
            prewarmWorkerCount: workers, tablebaseRequested: false,
            hostCapabilitySnapshot: snapshot, workerAuthority,
            warmupPolicy: { backend: 'cpu', cpuWarmup: false, gpuWarmup: false } });
          const result = await terminal;
          return { result };
        } finally {
          clearTimeout(timeout);
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
        if (profile === 'srs') {
          results[profile].existing = {
            baseline: await run(profile, false, false, 'existing'),
            activated: await run(profile, true, true, 'existing')
          };
        }
      }
      return { results, workers };
    }, profiles);
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
    for (const profile of profiles) {
      for (const [scope, pair] of Object.entries(execution.results[profile])) {
        const name = `${profile}/${scope}`;
        const activated = compact(pair.activated);
        const baseline = compact(pair.baseline);
        if (profile === 'srs') {
          assert.equal(activated.keys.length, scope === 'existing' ? 245 : 159,
            `${name}: retain the previously qualified browser fixture count`);
        }
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
    }
    assert.ok(verifierRequests.length >= 1, 'the actual verifier worker must be loaded');
    assert.equal(assetRequests.length, assets.length, 'a search must read OPFS, not re-download signed assets');
    assert.deepEqual(errors, []);
    await context.close();
    console.log('v0.8.1 signed browser UI and product pool: five profiles, OPFS, cross-tab read, actual verifier workers and complete existing/eligible result parity passed');
  } finally {
    await browser?.close();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
}

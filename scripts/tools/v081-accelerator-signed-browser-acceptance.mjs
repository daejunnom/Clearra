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
assert.equal(ui.outputFiles.length, 1);
assert.equal(worker.outputFiles.length, 1);
assert.match(ui.outputFiles[0].text, /\.\.\/workers\/acceleratorDownloadWorker\.ts/u,
  'the actual panel must construct its production download worker');
// A source-only bundle check is useful locally when the signed CI inputs are
// absent. It makes no browser, WASM, or asset-admission claim.
if (process.argv.includes('--bundle-only')) {
  console.log('v0.8.1 signed browser UI and worker bundles: source-only pass');
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
  const assets = (await frontendAcceleratorAssets(root)).filter(asset => asset.profile === 'srs');
  assert.deepEqual(assets.map(asset => asset.product).sort(),
    ['board-conditioned-reachability', 'exact-legal-board']);
  for (const asset of assets) {
    const path = join(assetRoot, basename(new URL(asset.url).pathname));
    assert.equal((await stat(path)).size, asset.bytes);
    assert.equal(await sha256(path), asset.digest);
    files.set(asset.pathname, { path, type: 'application/octet-stream' });
  }

  const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
  const { chromium } = require('playwright');
  const assetRequests = [];
  const server = createServer((request, response) => {
    const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
    response.setHeader('Cache-Control', 'no-store');
    if (pathname === '/') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/ui.js"></script>');
      return;
    }
    const bundled = pathname === '/ui.js' ? ui.outputFiles[0].contents
      : pathname === '/workers/acceleratorDownloadWorker.ts' ? worker.outputFiles[0].contents : null;
    if (bundled) {
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
    await download.waitFor();
    assert.deepEqual(assetRequests, [], 'status and capacity must not download an asset');
    await download.click();
    await current.waitFor();
    await page.locator('details.accelerator-download select').first().selectOption('1');
    await download.waitFor();
    assert.deepEqual(assetRequests, [assets.find(asset => asset.product === 'exact-legal-board').pathname]);
    await download.click();
    await current.waitFor();
    assert.deepEqual(assetRequests.sort(), assets.map(asset => asset.pathname).sort(),
      'one explicit request per signed product');

    const second = await context.newPage();
    second.setDefaultTimeout(90_000);
    second.on('pageerror', error => errors.push(String(error.stack || error)));
    await second.goto(`http://127.0.0.1:${address.port}/`);
    await second.locator('details.accelerator-download summary').click();
    await second.getByRole('status').filter({ hasText: 'Verified asset saved' }).waitFor();
    await second.locator('details.accelerator-download select').first().selectOption('1');
    await second.getByRole('status').filter({ hasText: 'Verified asset saved' }).waitFor();
    assert.equal(assetRequests.length, 2, 'cross-tab read must not re-download');
    assert.deepEqual(errors, []);
    await context.close();
    console.log('v0.8.1 signed browser UI: explicit download, actual WASM admission, OPFS and cross-tab read passed');
  } finally {
    await browser?.close();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
}

// Exercise the production Desktop Svelte panel and its exact Tauri command
// envelopes in Chromium. Only the native IPC replies are simulated; this is
// not a Tauri window, signed-asset admission, or release readback proof.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const fixturePath = resolve(root, 'apps/clearra-desktop/test/acceleratorPanelBrowser.fixture.svelte');
const fixture = `<script>
  import Panel from '../src/lib/AcceleratorAssetPanel.svelte';
</script>
<Panel language="en" />`;
const preprocessor = vitePreprocess();

async function component(source, path) {
  const processed = await preprocess(source, preprocessor, { filename: path });
  return {
    contents: compile(processed.code, { filename: path, generate: 'client', css: 'injected' }).js.code,
    loader: 'js', resolveDir: dirname(path),
  };
}

const bundle = await build({
  absWorkingDir: root,
  stdin: {
    contents: `import { mount } from 'svelte';
      import Fixture from 'clearra-desktop-accelerator-fixture';
      mount(Fixture, { target: document.getElementById('app') });`,
    resolveDir: root, sourcefile: 'desktop-accelerator-panel-acceptance.js',
  },
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte', 'browser', 'module', 'main'],
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'compiled-desktop-accelerator-panel', setup(builder) {
    builder.onResolve({ filter: /^clearra-desktop-accelerator-fixture$/ },
      () => ({ path: fixturePath, namespace: 'desktop-fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'desktop-fixture' }, () => component(fixture, fixturePath));
    builder.onResolve({ filter: /^@tauri-apps\/api\/core$/ },
      () => ({ path: 'invoke', namespace: 'tauri-ipc-test-boundary' }));
    builder.onLoad({ filter: /.*/, namespace: 'tauri-ipc-test-boundary' },
      () => ({ contents: 'export const invoke = (command, args) => globalThis.__tauriInvoke(command, args);', loader: 'js' }));
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path }) => component(await readFile(path, 'utf8'), path));
  } }],
});
assert.equal(bundle.outputFiles.length, 1);
assert.match(bundle.outputFiles[0].text, /accelerator_asset_start_download/u);
assert.match(bundle.outputFiles[0].text, /accelerator_asset_progress/u);
assert.match(bundle.outputFiles[0].text, /accelerator_asset_cancel/u);

if (process.argv.includes('--bundle-only')) {
  console.log('v0.8.1 Desktop accelerator panel: source-only bundle pass');
} else {
  await browserAcceptance();
}

async function browserAcceptance() {
  assert.ok(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'pinned browser tools are required');
  const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
  const { chromium } = require('playwright');
  const server = createServer((request, response) => {
    response.setHeader('Cache-Control', 'no-store');
    if (request.url === '/') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/ui.js"></script>');
    } else if (request.url === '/ui.js') {
      response.writeHead(200, { 'Content-Type': 'text/javascript; charset=utf-8' });
      response.end(bundle.outputFiles[0].contents);
    } else {
      response.writeHead(404).end();
    }
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
    const page = await browser.newPage();
    page.setDefaultTimeout(15_000);
    const errors = [];
    page.on('pageerror', error => errors.push(String(error.stack || error)));
    await page.addInitScript(() => {
      const calls = [];
      const installed = new Set();
      const candidateOnly = new Map([
        ['board-conditioned-reachability:no-kick', {
          installed: false, local_candidate_bundle_bytes: 0, local_candidate_catalog_bytes: 0
        }],
        ['exact-legal-board:srs-plus', { installed: false, candidate_catalog_bytes: 0 }]
      ]);
      const operations = new Map();
      let nextId = 0;
      window.__desktopAcceleratorCalls = calls;
      window.__tauriInvoke = async (command, args) => {
        calls.push({ command, ...args });
        const key = `${args.product}:${args.profile}`;
        if (command === 'accelerator_asset_action') {
          if (args.action === 'check') return JSON.stringify({ qualified: true, compressed_bytes: 2 * 1024 * 1024 });
          if (args.action === 'status') return JSON.stringify(candidateOnly.get(key) ?? (installed.has(key)
            ? { installed: true, qualified: true, installed_payload_bytes: 2 * 1024 * 1024 }
            : { installed: false }));
          if (args.action === 'remove') { installed.delete(key); candidateOnly.delete(key); return '{}'; }
        }
        if (command === 'accelerator_asset_start_download') {
          const id = ++nextId;
          operations.set(id, { key, polls: 0, cancelled: false,
            done: false,
            dropTerminalReplies: args.product === 'exact-legal-board' && args.profile === 'srs',
            completeAfter: args.profile === 'srs-x' ? 1000 : 2 });
          return id;
        }
        if (command === 'accelerator_asset_cancel') {
          const operation = operations.get(args.operationId);
          if (!operation) throw new Error('unknown download operation');
          if (operation.done) throw new Error('download operation is not active');
          operation.cancelled = true;
          return;
        }
        if (command === 'accelerator_asset_progress') {
          const operation = operations.get(args.operationId);
          if (!operation) throw new Error('unknown download operation');
          operation.polls += 1;
          if (operation.cancelled) return JSON.stringify({ done: true,
            transferred_bytes: 0, total_bytes: 2 * 1024 * 1024,
            error: 'accelerator: download cancelled' });
          if (operation.dropTerminalReplies && operation.polls <= 3) {
            // Native work completed, but each of the first three IPC replies
            // was lost. A terminal receipt must remain available for retry.
            operation.done = true;
            installed.add(operation.key);
            throw new Error('simulated lost progress reply');
          }
          if (operation.done) return JSON.stringify({ done: true,
            transferred_bytes: 2 * 1024 * 1024,
            total_bytes: 2 * 1024 * 1024, result: '{}' });
          if (operation.polls >= operation.completeAfter) {
            operation.done = true;
            installed.add(operation.key);
            return JSON.stringify({ done: true, transferred_bytes: 2 * 1024 * 1024,
              total_bytes: 2 * 1024 * 1024, result: '{}' });
          }
          return JSON.stringify({ done: false, transferred_bytes: 0,
            total_bytes: 2 * 1024 * 1024 });
        }
        throw new Error(`unexpected Tauri command: ${command}`);
      };
    });
    await page.goto(`http://127.0.0.1:${address.port}/`);
    await page.locator('details.accelerator-download summary').click();
    await page.getByRole('status').filter({ hasText: 'No installed asset' }).waitFor();
    await page.waitForFunction(() => window.__desktopAcceleratorCalls
      .filter(call => call.command === 'accelerator_asset_action').length >= 2);
    const readCalls = () => page.evaluate(() => window.__desktopAcceleratorCalls);
    assert.deepEqual((await readCalls()).map(call => call.action), ['check', 'status'],
      'opening the panel must inspect without a download');

    const product = page.locator('details.accelerator-download select').first();
    const profile = page.locator('details.accelerator-download select').nth(1);
    await profile.selectOption('jstris-180');
    await product.selectOption('board-conditioned-reachability');
    await page.getByRole('button', { name: 'Download displayed asset' }).click();
    await page.getByRole('status').filter({ hasText: 'Verified asset installed' }).waitFor();
    const installedCalls = await readCalls();
    assert.deepEqual(installedCalls.filter(call => call.command === 'accelerator_asset_start_download'),
      [{ command: 'accelerator_asset_start_download',
        product: 'board-conditioned-reachability', profile: 'jstris-180' }]);
    assert.ok(installedCalls.filter(call => call.command === 'accelerator_asset_progress').length >= 2);
    await page.getByRole('button', { name: 'Delete saved asset' }).click();
    await page.getByRole('status').filter({ hasText: 'No installed asset' }).waitFor();
    assert.deepEqual((await readCalls()).filter(call => call.action === 'remove'),
      [{ command: 'accelerator_asset_action', action: 'remove',
        product: 'board-conditioned-reachability', profile: 'jstris-180' }]);

    await profile.selectOption('srs-x');
    await page.getByRole('button', { name: 'Download displayed asset' }).click();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await page.getByRole('status').filter({ hasText: 'Download cancelled' }).waitFor();
    assert.deepEqual((await readCalls()).filter(call => call.command === 'accelerator_asset_cancel'),
      [{ command: 'accelerator_asset_cancel', operationId: 2 }]);
    await page.getByRole('status').filter({ hasText: 'No installed asset' }).waitFor();
    await profile.selectOption('no-kick');
    await page.getByRole('status').filter({ hasText: 'An unqualified local candidate is stored' }).waitFor();
    await page.getByRole('button', { name: 'Delete saved asset' }).click();
    await page.getByRole('status').filter({ hasText: 'No installed asset' }).waitFor();
    await product.selectOption('exact-legal-board');
    await profile.selectOption('srs-plus');
    await page.getByRole('status').filter({ hasText: 'An unqualified local candidate is stored' }).waitFor();
    await page.getByRole('button', { name: 'Delete saved asset' }).click();
    await page.getByRole('status').filter({ hasText: 'No installed asset' }).waitFor();
    assert.deepEqual((await readCalls()).filter(call => call.action === 'remove').slice(-2), [
      { command: 'accelerator_asset_action', action: 'remove',
        product: 'board-conditioned-reachability', profile: 'no-kick' },
      { command: 'accelerator_asset_action', action: 'remove',
        product: 'exact-legal-board', profile: 'srs-plus' }
    ], 'zero-byte local candidates and catalog-only files must remain removable');

    await profile.selectOption('srs');
    await page.getByRole('button', { name: 'Download displayed asset' }).click();
    await page.getByRole('status').filter({ hasText: 'Download status could not be confirmed' }).waitFor();
    assert.equal(await page.getByRole('button', { name: 'Download displayed asset' }).isDisabled(), true);
    assert.equal(await page.getByRole('button', { name: 'Check status and size' }).isDisabled(), true);
    assert.equal(await product.isDisabled(), true);
    assert.equal(await profile.isDisabled(), true);
    assert.equal((await readCalls()).filter(call => call.command === 'accelerator_asset_start_download').length, 3,
      'lost progress replies must not admit a second download');
    await page.getByRole('button', { name: 'Retry operation status' }).click();
    await page.getByRole('status').filter({ hasText: 'Verified asset installed' }).waitFor();
    assert.ok((await readCalls()).filter(call => call.command === 'accelerator_asset_progress'
      && call.operationId === 3).length >= 4, 'retry must read the retained terminal receipt');
    assert.deepEqual(errors, []);
    await page.close();
    console.log('v0.8.1 Desktop Svelte panel: explicit selection, IPC envelope, progress, remove and cancel passed');
  } finally {
    await browser?.close();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
}

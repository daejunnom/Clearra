// Real-browser storage proof for the v0.8.1 signed-asset staging boundary.
// A tiny local qualified plan tests OPFS and Web Locks; it does not qualify
// an asset, exercise WASM, or make a release/readback claim.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
assert.ok(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'pinned browser tools are required');
const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
const { chromium } = require('playwright');
const bundle = await build({
  entryPoints: [resolve(root, 'apps/clearra-web/src/workers/acceleratorLocalStore.ts')],
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'browser',
  target: 'es2022',
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
});
assert.equal(bundle.outputFiles.length, 1);

const server = createServer((request, response) => {
  if (request.url === '/store.js') {
    response.writeHead(200, { 'content-type': 'text/javascript; charset=utf-8', 'cache-control': 'no-store' });
    response.end(bundle.outputFiles[0].contents);
  } else if (request.url === '/') {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
    response.end('<!doctype html><title>Clearra accelerator OPFS acceptance</title>');
  } else {
    response.writeHead(404);
    response.end();
  }
});
await new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', resolve);
});
const address = server.address();
assert.ok(address && typeof address !== 'string');
async function bounded(promise, label) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`browser_${label}_timeout`)), 30_000);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
let browser;
try {
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] });
  const context = await browser.newContext();
  const first = await context.newPage();
  const second = await context.newPage();
  const url = `http://127.0.0.1:${address.port}/`;
  await Promise.all([first.goto(url), second.goto(url)]);

  const bytes = [1, 2, 3, 4];
  const plan = {
    product: 'exact-legal-board',
    profile: 'srs',
    state: 'qualified',
    payload_bytes: bytes.length,
    payload_identity: createHash('sha256').update(Buffer.from(bytes)).digest('hex'),
    generation: 'a'.repeat(64),
    catalog_identity: 'b'.repeat(64),
    active_session_shared_bytes: 1024,
    url: 'https://example.invalid/asset',
  };
  const availability = await first.evaluate(() => ({
    opfs: typeof navigator.storage?.getDirectory === 'function',
    locks: typeof navigator.locks?.request === 'function',
  }));
  assert.deepEqual(availability, { opfs: true, locks: true });

  const installed = await bounded(first.evaluate(async ({ plan, bytes }) => {
    const store = await import('/store.js');
    await store.storeQualifiedAccelerator(plan, new Uint8Array(bytes));
    return store.acceleratorLocalStatus(plan.product, plan.profile, plan);
  }, { plan, bytes }), 'install');
  assert.equal(installed.current, true);
  const crossTab = await bounded(second.evaluate(async plan => {
    const store = await import('/store.js');
    const status = await store.acceleratorLocalStatus(plan.product, plan.profile, plan);
    const payload = await store.readQualifiedAccelerator(plan);
    return { current: status?.current, bytes: payload ? [...new Uint8Array(payload)] : null };
  }, plan), 'cross_tab_read');
  assert.deepEqual(crossTab, { current: true, bytes });

  const lockName = 'clearra-exact-accelerators-v1:exact-legal-board:srs';
  await bounded(first.evaluate(async name => {
    let acquired;
    const ready = new Promise(resolve => { acquired = resolve; });
    let release;
    const blocked = new Promise(resolve => { release = resolve; });
    window.__releaseAcceleratorTestLock = release;
    window.__acceleratorTestLock = navigator.locks.request(name, { mode: 'exclusive' }, async lock => {
      if (!lock) throw new Error('test_lock_unavailable');
      acquired();
      await blocked;
    });
    await ready;
  }, lockName), 'lock_acquisition');
  const conflict = await bounded(second.evaluate(async plan => {
    const store = await import('/store.js');
    try {
      await store.acceleratorLocalStatus(plan.product, plan.profile, plan);
      return 'unexpected-success';
    } catch (error) {
      return error.message;
    }
  }, plan), 'lock_conflict');
  assert.equal(conflict, 'accelerator_store_busy');
  await bounded(first.evaluate(async () => {
    window.__releaseAcceleratorTestLock();
    await window.__acceleratorTestLock;
  }), 'lock_release');

  const repair = await bounded(second.evaluate(async ({ plan, bytes }) => {
    const store = await import('/store.js');
    const origin = await navigator.storage.getDirectory();
    const profile = await (await (await origin.getDirectoryHandle('clearra-exact-accelerators-v1'))
      .getDirectoryHandle(plan.product)).getDirectoryHandle(plan.profile);
    const pointer = await (await profile.getFileHandle('active.json')).createWritable();
    await pointer.write('null');
    await pointer.close();
    let error = null;
    try { await store.acceleratorLocalStatus(plan.product, plan.profile, plan); }
    catch (caught) { error = caught.message; }
    await store.removeLocalAccelerator(plan.product, plan.profile);
    const empty = await store.acceleratorLocalStatus(plan.product, plan.profile, plan);
    await store.storeQualifiedAccelerator(plan, new Uint8Array(bytes));
    const restored = await store.acceleratorLocalStatus(plan.product, plan.profile, plan);
    return { error, empty, restored: restored?.current };
  }, { plan, bytes }), 'pointer_repair');
  assert.deepEqual(repair, { error: 'accelerator_store_pointer_invalid', empty: null, restored: true });
  await context.close();
  process.stdout.write('v0.8.1 real OPFS: install, cross-tab read, lock conflict, corrupt-pointer remove and reinstall passed\n');
} finally {
  await browser?.close();
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
}

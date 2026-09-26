// Real compiled Svelte + real WASM. Only GPU hardware discovery is simulated.
// This is an acceptance test: every assertion and timeout is a failing exit.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, stat, writeFile } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
const { chromium } = require('playwright');
const root = resolve(process.argv[2] || 'apps/clearra-web/build');
const reportRoot = resolve(process.env.RUNNER_TEMP || 'build', 'clearra-surface-acceptance');
await mkdir(reportRoot, { recursive: true });
const base = process.env.CLEARRA_WEB_BASE_PATH || '';
assert.match(base, /^(?:\/[A-Za-z0-9._-]+)?$/u);
const mime = { '.js': 'text/javascript', '.json': 'application/json', '.css': 'text/css', '.wasm': 'application/wasm', '.html': 'text/html', '.svg': 'image/svg+xml' };
const server = createServer(async (req, res) => {
  try {
    const path = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    if (!path.startsWith(base + '/') && path !== base) { res.writeHead(404).end(); return; }
    let file = resolve(root, '.' + path.slice(base.length));
    if (file !== root && !file.startsWith(root + sep)) { res.writeHead(403).end(); return; }
    if (!(await stat(file).catch(() => null))?.isFile()) file = resolve(root, 'index.html');
    res.writeHead(200, { 'Content-Type': mime[extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
    res.end(await readFile(file));
  } catch { res.writeHead(500).end(); }
});
await new Promise((ok, fail) => { server.once('error', fail); server.listen(4194, '127.0.0.1', ok); });
const browser = await chromium.launch({ headless: true });
const results = [];
function gpuShim(mode) {
  const body = mode === 'null' ? 'Promise.resolve(null)' : mode === 'reject' ? "Promise.reject(new Error('simulated adapter refusal'))" : 'new Promise(() => {})';
  return `Object.defineProperty(navigator, 'gpu', { configurable: true, value: { requestAdapter: () => ${body} } });`;
}
async function context(mode) {
  const ctx = await browser.newContext({ locale: 'en-US', viewport: { width: 1440, height: 1100 } });
  await ctx.addInitScript(gpuShim(mode));
  // Preserve real worker construction, URL/base-path, lifecycle and all messages.
  await ctx.route(/\/_app\/immutable\/workers\/[^/]+\.js(?:\?.*)?$/, async route => {
    const response = await route.fetch();
    await route.fulfill({ response, body: gpuShim(mode) + '\n' + await response.text() });
  });
  return ctx;
}
async function keys(page) {
  return page.locator('.solution-gallery > li[data-solution-key]').evaluateAll(nodes => nodes.map(n => n.dataset.solutionKey));
}
// Arm before clicking: an old Completed result must not satisfy a new run.
// Observe the real header lifecycle, including short jobs between polling ticks.
async function completeRun(page, action, label) {
  await page.evaluate(() => {
    window.__clearraAcceptanceRun?.observer.disconnect();
    const status = document.querySelector('.header-status > span');
    if (!status) throw new Error('workspace status is missing');
    const state = { started: false, status: null, observer: null };
    const observer = new MutationObserver(records => {
      const active = status.classList.contains('running');
      state.started ||= active || records.some(record => record.type === 'attributes' &&
        record.attributeName === 'class' && (record.oldValue || '').split(/\s+/u).includes('running'));
      const text = status.textContent.trim();
      if (state.started && !active && ['Completed', 'Failed', 'Cancelled', 'Terminated'].includes(text)) {
        state.status = text;
        observer.disconnect();
      }
    });
    state.observer = observer;
    window.__clearraAcceptanceRun = state;
    observer.observe(status, { attributes: true, attributeOldValue: true,
      childList: true, subtree: true, characterData: true });
  });
  try {
    await action();
    await page.waitForFunction(() => window.__clearraAcceptanceRun.status !== null, null, { timeout: 60000 });
    const status = await page.evaluate(() => window.__clearraAcceptanceRun.status);
    assert.equal(status, 'Completed', `${label}: execution did not complete successfully`);
  } finally {
    await page.evaluate(() => window.__clearraAcceptanceRun?.observer.disconnect());
  }
}
async function completedKeys(page, count, label) {
  // Computation and lazy result-page loading have separate completion points.
  // Wait for the real page loader, never for a particular expected key count.
  await page.locator('.pager-loading').waitFor({ state: 'hidden', timeout: 60000 });
  const actual = await keys(page);
  assert.equal(actual.length, count, `${label}: unexpected complete solution set: ${JSON.stringify(actual)}`);
  assert.equal(new Set(actual).size, count, `${label}: duplicate solution keys`);
  return actual;
}
const LEFT_I = 'ctk1|initial=0000000000000000|placements=I:000000000000000f';
const RIGHT_I = 'ctk1|initial=0000000000000000|placements=I:00000000000003c0';
const CENTER_I = 'ctk1|initial=0000000000000000|placements=I:0000000000000078';

async function paint(page, index, mask, height) {
  const board = page.locator('.board-tool .board').nth(index);
  for (let y = 0; y < height; y++) for (let x = 0; x < 10; x++) {
    if ((mask & (1n << BigInt(y * 10 + x))) !== 0n) await board.locator('button').nth((height - 1 - y) * 10 + x).click();
  }
}
try {
  const manifest = JSON.parse(await readFile(resolve(root, 'wasm/clearra_wasm.manifest.json'), 'utf8'));
  assert.equal(manifest.build.runtime_identity.source_commit, process.env.CLEARRA_SOURCE_COMMIT);
  let reference;
  for (const mode of ['null', 'reject', 'timeout']) {
    const ctx = await context(mode);
    const page = await ctx.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    try {
      await page.goto(`http://127.0.0.1:4194${base}/?tool=pc`);
      await page.locator('.product-tabs').waitFor();
      const nav = await page.locator('.product-tabs a').evaluateAll(nodes => nodes.map(n => n.getAttribute('href')));
      assert.ok(nav.length > 7, 'obsolete seven-item navigation is still active');
      const index = nav.indexOf('?tool=recovery');
      assert.equal(nav[index - 1], '?tool=build-probability');
      assert.equal(nav[index + 1], '?tool=damage');
      assert.ok(nav.includes('?tool=build'));
      await page.locator('.fumen-import input:not([type="file"])').fill('ctk3_w0kCQBhwwAEHHABh4Q');
      await page.locator('.fumen-import button').last().click();
      await page.locator('.workspace-queue-input').fill('STOILJZ');
      await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), `${mode}: PC`);
      const actual = (await completedKeys(page, 18, `${mode}: PC`)).sort();
      assert.equal(new Set(actual).size, 18);
      if (reference) assert.deepEqual(actual, reference); else reference = actual;
      assert.equal(await page.locator('.mandatory-choice input').count(), 18);
      // A new run through the same controller must also finish (no leaked lease).
      await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), `${mode}: repeated PC`);
      assert.deepEqual((await completedKeys(page, 18, `${mode}: repeated PC`)).sort(), reference);
      if (mode === 'null') {
        const selected = [(await keys(page))[0], (await keys(page))[17]];
        await page.locator('.mandatory-choice input').first().check();
        await page.locator('.mandatory-choice input').last().check();
        await completeRun(page, () => page.locator('.mandatory-summary button').first().click(), 'pinned PC');
        assert.deepEqual((await completedKeys(page, 2, 'pinned PC')).sort(), selected.sort(), 'mandatory solutions were not retained');
        await page.locator('.workspace-queue-input').fill('I');
        await page.waitForFunction(() => !document.querySelector('.mandatory-summary'));
        // Navigate using the visible menu, then execute a known two-stage case.
        await page.locator('.product-tabs a[href="?tool=recovery"]').click();
        await page.locator('.recovery-controls').waitFor();
        await page.locator('.dimension-field input').fill('4');
        await page.getByLabel('Known queue across both stages', { exact: true }).fill('IO');
        await page.getByLabel('Stage-one supply tokens', { exact: true }).fill('1');
        await page.getByLabel('Required placements', { exact: true }).fill('2');
        await page.getByLabel('Selected early placement role (1-based)', { exact: true }).fill('2');
        await page.locator('.recovery-controls input[type="checkbox"]').nth(1).uncheck();
        await paint(page, 0, 0x3f0n, 4);
        await paint(page, 1, 0xc030n, 4);
        await paint(page, 2, 0x300c000n, 4);
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), 'boundary recovery');
        await page.locator('.recovery-result .outcome').waitFor({ timeout: 60000 });
        assert.equal(await page.locator('.recovery-result .outcome').innerText(), 'Normal PC connection');
        assert.equal(await page.locator('.recovery-result ol li').count(), 2);
        await page.locator('.product-tabs a[href="?tool=build-probability"]').click();
        await page.locator('.workspace-queue-input').waitFor();
        await page.locator('.dimension-field input').fill('4');
        await page.locator('.workspace-queue-input').fill('I');
        const hold = page.getByRole('checkbox', { name: 'Hold', exact: true });
        await hold.uncheck();
        assert.equal(await hold.isChecked(), false, 'build request must disable hold through the real control');
        await paint(page, 0, 0xfn, 4);
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), 'mirrored Build source');
        // A symmetric empty base includes both the drawn target and its mirror.
        // These are two tilings covering ONE queue, not two probability events.
        const buildKeys = (await completedKeys(page, 2, 'mirrored Build source')).sort();
        assert.deepEqual(buildKeys, [LEFT_I, RIGHT_I].sort());
        // Select the mirrored drawing, not just the first canonical solution.
        await page.locator(`.solution-gallery > li[data-solution-key="${RIGHT_I}"] .mandatory-choice input`).check();
        await completeRun(page, () => page.locator('.mandatory-summary button').first().click(), 'mirrored mandatory Build');
        assert.deepEqual(await completedKeys(page, 1, 'mirrored mandatory Build'), [RIGHT_I]);
        // Request both pins from the complete source: ordinary minimum size is
        // one, but mandatory inclusion must retain both selected drawings.
        await page.getByRole('combobox', { name: 'Result aggregation', exact: true }).selectOption('all-solutions');
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), 'Build source replay');
        assert.deepEqual((await completedKeys(page, 2, 'Build source replay')).sort(), buildKeys);
        await page.locator(`.solution-gallery > li[data-solution-key="${LEFT_I}"] .mandatory-choice input`).check();
        await completeRun(page, () => page.locator('.mandatory-summary button').first().click(), 'two mandatory Build drawings');
        assert.deepEqual((await completedKeys(page, 2, 'two mandatory Build drawings')).sort(), buildKeys);
        // Changing the target invalidates the old pins. A centered I equals its
        // mirror and must be deduplicated to exactly one source solution.
        await page.getByRole('combobox', { name: 'Result aggregation', exact: true }).selectOption('all-solutions');
        await paint(page, 0, 0xfn ^ 0x78n, 4);
        await page.waitForFunction(() => !document.querySelector('.mandatory-summary'));
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), 'symmetric Build source');
        assert.deepEqual(await completedKeys(page, 1, 'symmetric Build source'), [CENTER_I]);
        results.push({ case: 'Build mirror and mandatory source', original: LEFT_I, mirrored: RIGHT_I,
          sourceCount: 2, mirroredPinCount: 1, twoPinsCount: 2, symmetricCount: 1 });

      }
      assert.deepEqual(errors, []);
      results.push({ mode, result: 'passed', solutionCount: 18 });
    } catch (error) {
      await page.screenshot({ path: resolve(reportRoot, `failed-${mode}.png`), fullPage: true });
      const text = await page.locator('body').innerText().catch(() => 'unavailable');
      await writeFile(resolve(reportRoot, `failed-${mode}.txt`), `${error.stack}\n${errors.join('\n')}\n${text}`).catch(() => {});
      throw error;
    } finally { await ctx.close(); }
  }
  const report = { source: process.env.CLEARRA_SOURCE_COMMIT, results };
  await writeFile(resolve(reportRoot, 'results.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
} finally {
  await browser.close();
  await new Promise(resolve => server.close(resolve));
}

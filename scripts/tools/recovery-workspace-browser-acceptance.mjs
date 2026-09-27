// Real BoundaryRecoveryWorkspace, including its actual height/event owner.
// This prebuild test never executes a solver and does not fake a successful result.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
assert.ok(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'pinned browser tools are required');
const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
const { chromium } = require('playwright');
const reportRoot = resolve(process.env.RUNNER_TEMP || resolve(root, 'build'), 'clearra-recovery-workspace-acceptance');
await mkdir(reportRoot, { recursive: true });
const preprocessor = vitePreprocess();
const fixture = `<script>
  import { setContext } from 'svelte';
  import Workspace from './packages/clearra-ui/src/lib/workspace/BoundaryRecoveryWorkspace.svelte';
  import { WORKSPACE_MODE_VISIBILITY_CONTEXT } from './packages/clearra-ui/src/lib/workspace/workspaceNavigation.ts';
  setContext(WORKSPACE_MODE_VISIBILITY_CONTEXT, ['pc', 'setup', 'build-probability', 'recovery', 'damage', 'spin-finder', 'ctk', 'player']);
</script>
<Workspace workerFactory={null} />`;
async function component(source, path) {
  const processed = await preprocess(source, preprocessor, { filename: path });
  return { contents: compile(processed.code, { filename: path, generate: 'client', css: 'injected' }).js.code,
    loader: 'js', resolveDir: dirname(path) };
}
const bundle = await build({
  absWorkingDir: root,
  stdin: { contents: `import { mount } from 'svelte';
    import Fixture from 'clearra-workspace-fixture'; mount(Fixture, { target: document.getElementById('app') });`,
    resolveDir: root, sourcefile: 'recovery-workspace-acceptance.js' },
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte', 'browser', 'module', 'main'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'compiled-workspace', setup(builder) {
    builder.onResolve({ filter: /^clearra-workspace-fixture$/ }, () => ({ path: 'fixture', namespace: 'workspace-fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'workspace-fixture' },
      () => component(fixture, resolve(root, 'recovery-workspace-fixture.svelte')));
    // No route is exercised by this prebuild test. An accidental navigation
    // must fail rather than pretending to be a SvelteKit transition.
    builder.onResolve({ filter: /^\$app\/navigation$/ }, () => ({ path: 'navigation', namespace: 'unavailable-router' }));
    builder.onLoad({ filter: /.*/, namespace: 'unavailable-router' }, () => ({
      contents: "export function goto() { throw new Error('Navigation is outside this input-owner test'); }", loader: 'js'
    }));
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path }) => component(await readFile(path, 'utf8'), path));
  } }],
});
assert.equal(bundle.outputFiles.length, 1);
const server = createServer((req, res) => {
  const path = new URL(req.url, 'http://127.0.0.1').pathname;
  res.setHeader('Cache-Control', 'no-store');
  if (path === '/workspace.js') res.writeHead(200, { 'Content-Type': 'text/javascript' }).end(bundle.outputFiles[0].contents);
  else if (path === '/') res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }).end(
    '<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><div id="app"></div><script type="module" src="/workspace.js"></script>');
  else res.writeHead(404).end();
});
const specs = [
  { language: 'en', width: 1440, touch: false },
  { language: 'en', width: 320, touch: true },
  { language: 'ko', width: 390, touch: true },
  { language: 'ja', width: 390, touch: true },
];
const results = [];
let browser;
try {
  await new Promise((ok, fail) => { server.once('error', fail); server.listen(0, '127.0.0.1', ok); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ headless: true, executablePath: process.env.CLEARRA_BROWSER_EXECUTABLE || undefined });
  for (const spec of specs) {
    const context = await browser.newContext({ viewport: { width: spec.width, height: 1100 },
      hasTouch: spec.touch, isMobile: spec.touch, locale: spec.language });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on('pageerror', error => errors.push(error.stack || String(error)));
    const flush = () => page.evaluate(() => new Promise(requestAnimationFrame));
    try {
      if (process.env.CLEARRA_BROWSER_DOCUMENT_MODE === 'memory') {
        // Offline local inspection: mount the same compiled component without
        // a network server; no solver or response is mocked here.
        await page.setContent('<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><div id="app"></div>');
        await page.addScriptTag({ type: 'module', content: bundle.outputFiles[0].text });
      } else {
        await page.goto(origin);
      }
      await page.locator('.language-control button').filter({ hasText: spec.language.toUpperCase() }).click();
      await flush();
      const height = page.locator('.dimension-field input');
      const board = page.locator('.recovery-field-editor .board');
      const gridSize = () => board.locator('button').count();
      const pressed = () => board.locator('button[aria-pressed="true"]').count();
      assert.equal(await gridSize(), 80);
      // This is the event handler missed by the Fields+Controls-only fixture.
      await height.fill('4'); await flush();
      assert.deepEqual(errors, [], 'changing height must not perform bigint operations on an absent mask');
      assert.equal(await height.inputValue(), '4');
      assert.equal(await gridSize(), 40);
      const supplies = page.locator('.recovery-supplies .workspace-queue-input');
      assert.equal(await supplies.count(), 2, 'each Build owns one ordinary shared supply input');
      await supplies.nth(0).fill('P7'); await supplies.nth(1).fill('P7');
      const left = await supplies.nth(0).boundingBox();
      const right = await supplies.nth(1).boundingBox();
      assert.ok(left && right && Math.abs(left.y - right.y) < 2 && left.x + left.width <= right.x,
        'both supply inputs remain side by side, even on narrow viewports');
      assert.equal(await page.locator('input[type="number"]').count(), 1, 'only outer field height remains numeric');
      assert.equal(await page.locator('.placement-constraints,.recovery-required-pieces,.board-stats').count(), 0);
      const fields = page.locator('.field-palette button');
      assert.equal(await fields.count(), 3);
      // A result O enables 0..1 early placement choices without parsing P7 in JS.
      await fields.nth(2).click();
      for (const offset of [30,31,20,21]) await board.locator('button').nth(offset).click();
      const early = page.locator('.workspace-controls select').first();
      assert.equal(await early.inputValue(), 'auto');
      for (const value of ['1', '0', 'auto']) {
        await early.selectOption(value); await flush();
        assert.equal(await early.inputValue(), value);
        assert.equal(await page.locator('.board-tool .board').count(), 1, 'a quota must never create another board editor');
        assert.deepEqual(errors, []);
      }
      for (const offset of [30,31,20,21]) await board.locator('button').nth(offset).click();
      const switches = page.locator('.workspace-controls input[type="checkbox"]');
      assert.equal(await switches.count(), 3, 'hold, different-piece repayment and one global B2B toggle');
      for (const toggle of await switches.all()) {
        const initial = await toggle.isChecked();
        await toggle.setChecked(!initial); await toggle.setChecked(initial);
      }
      // Paint each independent snapshot, then exercise real dimension changes.
      for (let field = 0; field < 3; field++) {
        await fields.nth(field).click();
        await board.locator('button').nth(30 + field).click();
        assert.equal(await pressed(), 1);
      }
      await height.fill(''); await flush();
      assert.equal(await gridSize(), 40, 'an empty draft must not truncate snapshots');
      assert.equal(await pressed(), 1);
      assert.equal(await page.locator('.run-actions button.run').isDisabled(), true);
      await height.fill('6'); await flush();
      assert.equal(await gridSize(), 60);
      for (let field = 0; field < 3; field++) {
        await fields.nth(field).click(); await flush();
        assert.equal(await pressed(), 1, 'resizing must preserve each low-row snapshot');
      }
      // Public Build inputs never expose the legacy role editor, even after
      // toggling different-piece repayment or changing the display height.
      await page.locator('.workspace-controls input[type="checkbox"]').nth(1).check();
      await height.fill('4'); await flush();
      assert.equal(await page.locator('.board-tool .board').count(), 1);
      assert.equal(await gridSize(), 40);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
      assert.equal(overflow, false, 'public recovery input must not horizontally overflow');
      assert.deepEqual(errors, []);
      // Render the user's supplied geometry in the actual public input owner.
      // Only input editing is exercised; this test never clicks Run search.
      await height.fill('8'); await flush();
      const masks = [0xc0383f3fc7n, 0x3ff3fc7c0c038n, 0x30483f07f3f8fn];
      for (let field = 0; field < 3; field++) {
        await fields.nth(field).click();
        const clear = page.locator('.board-actions button').last();
        if (await clear.isEnabled()) await clear.click();
        for (let y = 0; y < 8; y++) for (let x = 0; x < 10; x++) {
          if (masks[field] & (1n << BigInt(y * 10 + x))) await board.locator('button').nth((7 - y) * 10 + x).click();
        }
      }
      await fields.nth(1).click();
      assert.equal(await page.locator('.workspace-validation').count(), 0);
      assert.equal(await page.locator('.board-stats').count(), 0);
      assert.deepEqual(errors, []);
      await page.screenshot({ path: resolve(reportRoot, `${spec.language}-${spec.width}.png`), fullPage: true });
      results.push({ ...spec, status: 'passed', heightTransition: [8, 4, 6, 4, 8], quotas: ['auto', 1, 0], supplies: ['P7','P7'], removedLegacyControls: true });
    } catch (error) {
      await writeFile(resolve(reportRoot, `failed-${spec.language}-${spec.width}.txt`),
        [String(error.stack || error), ...errors, await page.locator('body').innerText()].join('\n'));
      await page.screenshot({ path: resolve(reportRoot, `failed-${spec.language}-${spec.width}.png`), fullPage: true });
      throw error;
    } finally { await context.close(); }
  }
  await writeFile(resolve(reportRoot, 'result.json'), JSON.stringify({ source: process.env.CLEARRA_SOURCE_COMMIT, browser: await browser.version(), solverExecuted: false, documentMode: process.env.CLEARRA_BROWSER_DOCUMENT_MODE || 'http', cases: results }, null, 2) + '\n');
  console.log('recovery_workspace_acceptance=passed cases=' + results.length);
} finally { await browser?.close(); await new Promise(ok => server.close(ok)); }

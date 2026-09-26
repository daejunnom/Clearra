// Real compiled result controls; no generated screenshot is used as evidence.
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
assert.ok(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'pinned browser tooling is required');
const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
const { chromium } = require('playwright');
const reportRoot = resolve(process.env.RUNNER_TEMP || resolve(root, 'build'), 'clearra-solution-toolbar-acceptance');
await mkdir(reportRoot, { recursive: true });
const fixture = `<script>
  import SolutionToolbar from './packages/clearra-ui/src/lib/workspace/SolutionToolbar.svelte';
  import MandatorySelectionSummary from './packages/clearra-ui/src/lib/workspace/MandatorySelectionSummary.svelte';
  import SolutionCopyFormatControl from './packages/clearra-ui/src/lib/workspace/SolutionCopyFormatControl.svelte';
  import SolutionGallery from './packages/clearra-ui/src/lib/workspace/SolutionGallery.svelte';
  const language = new URLSearchParams(location.search).get('language') || 'en';
  const keys = ['ctk1|initial=0000000000000000|placements=I:000000000000000f',
    'ctk1|initial=0000000000000000|placements=I:00000000000003c0'];
  let selected = [];
  let runs = 0;
  let submitted = [];
  let busy = false;
  let ordinalBase = '0';
  let copyFormat = 'ctk';
  function toggle(key) { selected = selected.includes(key) ? selected.filter(value => value !== key) : [...selected, key]; }
</script>
<main style="padding:12px;max-width:1800px;margin:auto;box-sizing:border-box">
  <SolutionToolbar {language}>
    <svelte:fragment slot="actions">
      <MandatorySelectionSummary count={selected.length} {language} disabled={busy}
        on:run={() => { runs++; submitted = [...selected]; }} on:clear={() => selected=[]} />
    </svelte:fragment>
    <svelte:fragment slot="copy"><SolutionCopyFormatControl {language} solutionKeys={keys} bind:value={copyFormat} /></svelte:fragment>
  </SolutionToolbar>
  <div style="max-width:900px">
    <SolutionGallery {language} solutionKeys={keys} targetLines={4} allowMandatorySelection={true}
      mandatorySolutionKeys={selected} solutionOrdinalBase={ordinalBase} {copyFormat} on:toggleMandatory={event => toggle(event.detail)} />
  </div>
  <button data-testid="busy" on:click={() => busy=!busy}>Busy fixture</button>
  <button data-testid="ordinal" on:click={() => ordinalBase='999999999999999999999999999999'}>Ordinal fixture</button>
  <output hidden data-testid="state">{JSON.stringify({ selected, runs, submitted })}</output>
</main>`;
const preprocessor = vitePreprocess();
async function component(source, filename) {
  const processed = await preprocess(source, preprocessor, { filename });
  return { contents: compile(processed.code, { filename, generate: 'client', css: 'injected' }).js.code,
    loader: 'js', resolveDir: dirname(filename) };
}
const compiled = await build({ absWorkingDir: root,
  stdin: { contents: "import { mount } from 'svelte'; import Fixture from 'toolbar-fixture'; mount(Fixture,{target:document.getElementById('app')});",
    resolveDir: root, sourcefile: 'toolbar-entry.js' },
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte', 'browser', 'module', 'main'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'real-svelte-results', setup(builder) {
    builder.onResolve({ filter: /^toolbar-fixture$/ }, () => ({ path: 'fixture', namespace: 'toolbar-fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'toolbar-fixture' }, () => component(fixture, resolve(root, 'toolbar-fixture.svelte')));
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path }) => component(await readFile(path, 'utf8'), path));
  } }],
});
assert.equal(compiled.outputFiles.length, 1);
const server = createServer((request, response) => {
  const path = new URL(request.url, 'http://127.0.0.1:4194').pathname;
  response.setHeader('Cache-Control', 'no-store');
  if (path === '/toolbar.js') response.writeHead(200, { 'Content-Type': 'text/javascript' }).end(compiled.outputFiles[0].contents);
  else if (path === '/') response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }).end(
    '<!doctype html><meta name="viewport" content="width=device-width, initial-scale=1"><style>body{margin:0;font-family:Arial,sans-serif}button,input{font:inherit}*{box-sizing:border-box}</style><div id="app"></div><script type="module" src="/toolbar.js"></script>');
  else response.writeHead(404).end();
});
const cases = [
  { language: 'en', width: 1920, touch: false, title: 'Solution 1' },
  { language: 'ko', width: 1920, touch: false, title: '해법 1' },
  { language: 'en', width: 320, touch: true, title: 'Solution 1' },
  { language: 'ko', width: 390, touch: true, title: '해법 1' },
];
async function state(page) { return JSON.parse(await page.getByTestId('state').textContent()); }
async function flush(page) { await page.evaluate(() => new Promise(requestAnimationFrame)); }
async function bounds(page) {
  for (const card of await page.locator('.solution-gallery > li').all()) {
    const b = await card.boundingBox();
    const title = card.locator('.mandatory-choice strong');
    const input = card.locator('.mandatory-choice input');
    const t = await title.boundingBox();
    const c = await input.boundingBox();
    assert.ok(b && t && c && c.x >= b.x && c.x + c.width <= t.x && t.x + t.width <= b.x + b.width,
      'checkbox must precede the title inside the card');
    assert.equal(await title.evaluate(el => getComputedStyle(el).whiteSpace), 'nowrap');
  }
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
}
let browser;
const results = [];
try {
  await new Promise((ok, fail) => { server.once('error', fail); server.listen(4194, '127.0.0.1', ok); });
  browser = await chromium.launch({ headless: true });
  for (const spec of cases) {
    const context = await browser.newContext({ viewport: { width: spec.width, height: 1100 }, hasTouch: spec.touch, isMobile: spec.touch });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    try {
      await page.goto(`http://127.0.0.1:4194/?language=${spec.language}`);
      const first = page.locator('.mandatory-choice').first();
      await first.waitFor();
      assert.equal(await first.innerText(), spec.title, 'only the solution title is visible beside the checkbox');
      await first.locator('input').check();
      await flush(page);
      assert.equal((await state(page)).selected.length, 1);
      const summary = page.locator('.solution-toolbar .mandatory-summary');
      await summary.waitFor();
      const order = await page.locator('.solution-toolbar').evaluate(el => {
        const h = el.querySelector('h3'), action = el.querySelector('.mandatory-summary'), copy = el.querySelector('.copy-format');
        return Boolean(h.compareDocumentPosition(action) & Node.DOCUMENT_POSITION_FOLLOWING) &&
          Boolean(action.compareDocumentPosition(copy) & Node.DOCUMENT_POSITION_FOLLOWING);
      });
      assert.ok(order, 'the mandatory action must be between title and copy in DOM order');
      if (!spec.touch) {
        const boxes = await Promise.all(['.solution-toolbar h3', '.mandatory-summary', '.copy-format'].map(selector => page.locator(selector).boundingBox()));
        assert.ok(boxes[0].x + boxes[0].width <= boxes[1].x && boxes[1].x + boxes[1].width <= boxes[2].x,
          'wide result toolbar must order title, mandatory action and copy horizontally');
      }
      await summary.locator('button').first().click();
      await flush(page);
      const submitted = await state(page);
      assert.equal(submitted.runs, 1);
      assert.deepEqual(submitted.submitted, submitted.selected);
      await page.getByTestId('busy').click();
      assert.equal(await summary.locator('button').first().isDisabled(), true);
      assert.equal(await summary.locator('button').last().isDisabled(), true);
      await page.getByTestId('busy').click();
      await bounds(page);
      await page.screenshot({ path: resolve(reportRoot, `${spec.language}-${spec.width}.png`), fullPage: true });
      await page.getByTestId('ordinal').click();
      await flush(page);
      await bounds(page);
      await summary.locator('button').last().click();
      await flush(page);
      assert.equal((await state(page)).selected.length, 0);
      assert.equal(await page.locator('.mandatory-summary').count(), 0);
      assert.deepEqual(errors, []);
      results.push({ ...spec, status: 'passed' });
    } catch (error) {
      await page.screenshot({ path: resolve(reportRoot, `failed-${spec.language}-${spec.width}.png`), fullPage: true });
      throw error;
    } finally { await context.close(); }
  }
  await writeFile(resolve(reportRoot, 'result.json'), JSON.stringify({ source: process.env.CLEARRA_SOURCE_COMMIT, cases: results }, null, 2) + '\n');
  console.log(`solution_toolbar_acceptance=passed cases=${results.length}`);
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }

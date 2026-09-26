// Compile the real Svelte controls, without loading Vite configuration or WASM.
// Native pointer, touch and keyboard events must reach the actual change handler.
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
const reportRoot = resolve(process.env.RUNNER_TEMP || resolve(root, 'build'), 'clearra-control-acceptance');
await mkdir(reportRoot, { recursive: true });
const fixture = `<script>
  import BuildProbabilityControls from './packages/clearra-ui/src/lib/workspace/BuildProbabilityControls.svelte';
  import { createDefaultBuildProbabilityRequest } from './packages/clearra-ui/src/lib/workspace/buildProbabilityModel.ts';
  let request = createDefaultBuildProbabilityRequest();
  let changes = 0;
  const language = new URLSearchParams(location.search).get('language') || 'en';
  const workerAuthority = { snapshotId: 'control-regression', workersRequested: 1,
    workersEffective: 1, reason: 'explicit-request' };
</script>
<header style="position:sticky;top:0;z-index:10;background:white;height:52px">Clearra controls</header>
<main style="padding:16px;max-width:480px">
  <BuildProbabilityControls {request} {language} {workerAuthority}
    on:change={event => { request = event.detail; changes += 1; }} />
  <output data-testid="request">{JSON.stringify({hold:request.holdEnabled,b2b:request.preserveB2B,
    probabilities:request.solutionProbabilities,changes})}</output>
</main>`;
const preprocessor = vitePreprocess();
async function compileComponent(source, filename) {
  const processed = await preprocess(source, preprocessor, { filename });
  return { contents: compile(processed.code, { filename, generate: 'client', css: 'injected' }).js.code,
    loader: 'js', resolveDir: dirname(filename) };
}
const compiled = await build({
  absWorkingDir: root,
  stdin: { contents: "import { mount } from 'svelte'; import Fixture from 'clearra-control-fixture'; mount(Fixture,{target:document.getElementById('app')});",
    resolveDir: root, sourcefile: 'control-acceptance-entry.js' },
  bundle: true, write: false, format: 'iife', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte', 'browser', 'module', 'main'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'real-svelte-controls', setup(builder) {
    builder.onResolve({ filter: /^clearra-control-fixture$/ }, () => ({ path: 'fixture', namespace: 'control-fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'control-fixture' }, () => compileComponent(fixture, resolve(root, 'control-fixture.svelte')));
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path }) => compileComponent(await readFile(path, 'utf8'), path));
  } }],
});
assert.equal(compiled.outputFiles.length, 1);
const server = createServer((request, response) => {
  const path = new URL(request.url, 'http://127.0.0.1:4194').pathname;
  response.setHeader('Cache-Control', 'no-store');
  if (path === '/controls.js') {
    response.writeHead(200, { 'Content-Type': 'text/javascript' }).end(compiled.outputFiles[0].contents);
  } else if (path === '/') {
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }).end(
      '<!doctype html><meta name="viewport" content="width=device-width, initial-scale=1"><div id="app"></div><script src="/controls.js"></script>');
  } else response.writeHead(404).end();
});
const cases = [
  { language: 'en', width: 1440, touch: false, hold: 'Hold', b2b: 'Preserve B2B', aggregation: 'Build engine aggregation', result: 'Result aggregation' },
  { language: 'en', width: 320, touch: true, hold: 'Hold', b2b: 'Preserve B2B', aggregation: 'Build engine aggregation', result: 'Result aggregation' },
  { language: 'ko', width: 390, touch: true, hold: '홀드', b2b: 'B2B 보존', aggregation: '구축 엔진 집계', result: '결과 집계' },
];
async function state(page) { return JSON.parse(await page.getByTestId('request').textContent()); }
async function flush(page) { await page.evaluate(() => new Promise(requestAnimationFrame)); }
async function point(page, locator, touch) {
  await locator.scrollIntoViewIfNeeded();
  const box = await locator.boundingBox();
  assert.ok(box, 'visible label part must have a hit area');
  const x = box.x + box.width / 2, y = box.y + box.height / 2;
  if (touch) await page.touchscreen.tap(x, y); else await page.mouse.click(x, y);
  await flush(page);
}
const results = [];
let browser;
try {
  await new Promise((ok, fail) => { server.once('error', fail); server.listen(4194, '127.0.0.1', ok); });
  browser = await chromium.launch({ headless: true });
  for (const spec of cases) {
    const context = await browser.newContext({ viewport: { width: spec.width, height: 1100 }, hasTouch: spec.touch, isMobile: spec.touch });
    const page = await context.newPage();
    page.setDefaultTimeout(5000);
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    try {
      await page.goto(`http://127.0.0.1:4194/?language=${spec.language}`);
      const hold = page.getByRole('checkbox', { name: spec.hold, exact: true });
      await hold.waitFor();
      assert.equal(await hold.isChecked(), true);
      // This is the original failure: no force click or synthetic DOM event.
      await hold.uncheck();
      await flush(page);
      assert.equal((await state(page)).hold, false);
      assert.equal((await state(page)).changes, 1);
      const label = hold.locator('..');
      const graphic = label.locator('.workspace-switch');
      await point(page, graphic, spec.touch);
      assert.equal((await state(page)).hold, true);
      assert.equal((await state(page)).changes, 2, 'one native event per graphic click');
      await point(page, label.locator('span').last(), spec.touch);
      assert.equal((await state(page)).hold, false);
      assert.equal((await state(page)).changes, 3, 'one native event per text click');
      await hold.focus();
      await hold.press('Space');
      await flush(page);
      assert.equal((await state(page)).hold, true);
      assert.equal((await state(page)).changes, 4);
      assert.equal(await graphic.evaluate(node => getComputedStyle(node).outlineStyle), 'solid');
      const inputBox = await hold.boundingBox(), labelBox = await label.boundingBox();
      assert.ok(inputBox && labelBox);
      assert.ok(Math.abs(inputBox.width - labelBox.width) < 1 && Math.abs(inputBox.height - labelBox.height) < 1);
      if (spec.touch) assert.ok(inputBox.width >= 44 && inputBox.height >= 44, 'coarse-pointer hit area');
      // Exercise actual disabled properties, including a disabled ancestor fieldset.
      await page.getByLabel(spec.aggregation, { exact: false }).selectOption('tiling');
      const b2b = page.getByRole('checkbox', { name: spec.b2b, exact: true });
      assert.equal(await b2b.isDisabled(), true);
      const beforeDisabled = await state(page);
      await point(page, b2b.locator('..').locator('.workspace-switch'), spec.touch);
      assert.deepEqual(await state(page), beforeDisabled);
      await page.getByLabel(spec.aggregation, { exact: false }).selectOption('buildability');
      await page.getByLabel(spec.result, { exact: false }).selectOption('minimum-solutions');
      const probabilities = page.locator('.solution-probabilities-control input');
      assert.equal(await probabilities.getAttribute('disabled'), null, 'disabled by fieldset, not the input attribute');
      assert.equal(await probabilities.isDisabled(), true);
      const beforeFieldset = await state(page);
      await point(page, probabilities.locator('..').locator('.workspace-switch'), spec.touch);
      assert.deepEqual(await state(page), beforeFieldset);
      assert.deepEqual(errors, []);
      results.push({ language: spec.language, width: spec.width, touch: spec.touch, result: 'passed' });
      await page.screenshot({ path: resolve(reportRoot, `${spec.language}-${spec.width}.png`), fullPage: true });
    } catch (error) {
      await page.screenshot({ path: resolve(reportRoot, `failed-${spec.language}-${spec.width}.png`), fullPage: true }).catch(() => {});
      await writeFile(resolve(reportRoot, `failed-${spec.language}-${spec.width}.txt`), `${error.stack}\n${errors.join('\n')}`).catch(() => {});
      throw error;
    } finally { await context.close(); }
  }
  const report = { source: process.env.CLEARRA_SOURCE_COMMIT || null, fixture: 'compiled BuildProbabilityControls', wasm: false, results };
  await writeFile(resolve(reportRoot, 'results.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
} finally {
  await browser?.close();
  if (server.listening) await new Promise((ok, fail) => server.close(error => error ? fail(error) : ok()));
}

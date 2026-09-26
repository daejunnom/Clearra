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
const reportRoot = resolve(process.env.RUNNER_TEMP || resolve(root, 'build'), 'clearra-recovery-editor-acceptance');
await mkdir(reportRoot, { recursive: true });
const fixture = `<script>
  import BoundaryRecoveryFields from './packages/clearra-ui/src/lib/workspace/BoundaryRecoveryFields.svelte';
  import BoundaryRecoveryControls from './packages/clearra-ui/src/lib/workspace/BoundaryRecoveryControls.svelte';
  import { createBoundaryRecoveryRequest, boundaryRecoveryArguments } from './packages/clearra-ui/src/lib/workspace/boundaryRecoveryModel.ts';
  import { workspaceMessage } from './packages/clearra-ui/src/lib/workspace/workspaceI18n.ts';
  let request = { ...createBoundaryRecoveryRequest(), height: 4, queue: 'IO' };
  let selectedRolePosition = 1;
  const language = new URLSearchParams(location.search).get('language') || 'en';
</script>
<main style="padding:12px;max-width:860px;margin:auto">
  <fieldset style="border:0;padding:0;margin:0;min-width:0">
    <BoundaryRecoveryFields {request} {language} on:change={event => request=event.detail} />
    <BoundaryRecoveryControls {request} {language} {selectedRolePosition} on:role={event => selectedRolePosition=event.detail}
      on:change={event => request=event.detail} />
  </fieldset>
  <output hidden data-testid="request">{JSON.stringify({ initial: request.initialBoardMask.toString(),
    first: request.stageOneBoardMask.toString(), final: request.targetBoardMask.toString(),
    hold: request.holdEnabled, roles: request.placementRoleMasks.length, maxEarly: request.maxEarlyPlacements,
    args: boundaryRecoveryArguments(request), undo: workspaceMessage(language,'undo'), redo: workspaceMessage(language,'redo') })}</output>
</main>`;
const preprocessor = vitePreprocess();
async function compileComponent(source, filename) {
  const processed = await preprocess(source, preprocessor, { filename });
  return { contents: compile(processed.code, { filename, generate: 'client', css: 'injected' }).js.code,
    loader: 'js', resolveDir: dirname(filename) };
}
const compiled = await build({
  absWorkingDir: root,
  stdin: { contents: "import { mount } from 'svelte'; import Fixture from 'clearra-recovery-fixture'; mount(Fixture,{target:document.getElementById('app')});",
    resolveDir: root, sourcefile: 'control-acceptance-entry.js' },
  bundle: true, write: false, format: 'iife', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte', 'browser', 'module', 'main'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'real-svelte-controls', setup(builder) {
    builder.onResolve({ filter: /^clearra-recovery-fixture$/ }, () => ({ path: 'fixture', namespace: 'recovery-fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'recovery-fixture' }, () => compileComponent(fixture, resolve(root, 'recovery-fixture.svelte')));
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
  { language: 'en', width: 1440, touch: false, fields: ['Existing field', 'First-stage field', 'Final field'], hold: 'Hold', exact: 'Require an exact placement for every role', normal: 'Normal only' },
  { language: 'en', width: 320, touch: true, fields: ['Existing field', 'First-stage field', 'Final field'], hold: 'Hold', exact: 'Require an exact placement for every role', normal: 'Normal only' },
  { language: 'ko', width: 390, touch: true, fields: ['기존 필드', '1단계 필드', '최종 필드'], hold: '홀드', exact: '각 배치 역할의 정확한 위치 지정', normal: '정상 연결만' },
];
async function state(page) { return JSON.parse(await page.getByTestId('request').textContent()); }
async function flush(page) { await page.evaluate(() => new Promise(requestAnimationFrame)); }
async function click(page, locator, touch) { if (touch) await locator.tap(); else await locator.click(); await flush(page); }
const results = [];
let browser;
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
      const select = async (index) => click(page, page.getByRole('button', { name: spec.fields[index], exact: true }), spec.touch);
      const cell = (index, x, y) => page.getByRole('button', { name: `${spec.fields[index]} ${x}, ${y}`, exact: true });
      assert.equal(await page.locator('.recovery-field-editor .board').count(), 1);
      const colors = [];
      for (let i = 0; i < 3; i++) {
        await select(i);
        await click(page, cell(i, i + 1, 1), spec.touch);
        colors.push(await cell(i, i + 1, 1).evaluate(el => getComputedStyle(el).backgroundColor));
      }
      assert.deepEqual(colors, ['rgb(96, 96, 96)', 'rgb(160, 160, 160)', 'rgb(222, 222, 222)']);
      assert.deepEqual(((s) => [s.initial,s.first,s.final])(await state(page)), ['1','2','4']);
      // A reference square is not selected, and editing its coordinate cannot
      // remove the same coordinate from another point in time.
      await select(1);
      assert.equal(await cell(1,1,1).getAttribute('aria-pressed'), 'false');
      assert.match(await cell(1,1,1).getAttribute('class'), /reference/);
      await click(page, cell(1,1,1), spec.touch);
      assert.deepEqual(((s) => [s.initial,s.first,s.final])(await state(page)), ['1','3','4']);
      const labels = await state(page);
      await click(page, page.getByRole('button', { name: labels.undo, exact: true }), spec.touch);
      assert.equal((await state(page)).first, '2');
      await click(page, page.getByRole('button', { name: labels.redo, exact: true }), spec.touch);
      assert.equal((await state(page)).first, '3');
      await click(page, cell(1,1,1), spec.touch);
      assert.equal((await state(page)).initial, '1');
      await page.getByRole('checkbox', { name: spec.hold, exact: true }).uncheck();
      assert.equal((await state(page)).hold, false);
      await page.getByRole('checkbox', { name: spec.exact, exact: true }).check();
      assert.equal((await state(page)).roles, 2);
      await click(page, page.getByRole('button', { name: spec.normal, exact: true }), spec.touch);
      assert.equal((await state(page)).maxEarly, 0);
      const args = (await state(page)).args;
      assert.equal(args[args.indexOf('--initial-board-mask')+1], '0x0000000000000001');
      assert.equal(args[args.indexOf('--stage-one-board-mask')+1], '0x0000000000000002');
      assert.equal(args[args.indexOf('--target-board-mask')+1], '0x0000000000000004');
      assert.ok(args.includes('--no-hold'));
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth);
      assert.equal(overflow, false, 'narrow recovery view must not scroll horizontally');
      if (spec.touch) {
        for (const label of spec.fields) {
          const box = await page.getByRole('button',{name:label,exact:true}).boundingBox();
          assert.ok(box && box.height >= 44);
        }
      }
      assert.deepEqual(errors, []);
      await page.screenshot({ path: resolve(reportRoot, `${spec.language}-${spec.width}.png`), fullPage: true });
      results.push({ ...spec, colors, status: 'passed' });
    } catch(error) {
      await page.screenshot({ path: resolve(reportRoot, `failed-${spec.language}-${spec.width}.png`), fullPage: true });
      throw error;
    } finally { await context.close(); }
  }
  await writeFile(resolve(reportRoot, 'result.json'), JSON.stringify({ source:process.env.CLEARRA_SOURCE_COMMIT, cases:results }, null, 2)+'\n');
  console.log('recovery_editor_acceptance=passed cases='+results.length);
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }

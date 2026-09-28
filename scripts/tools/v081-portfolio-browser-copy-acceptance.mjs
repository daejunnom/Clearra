// Consume the exact Rust-produced portfolio fixture through the production
// pager and clipboard button in Chromium. This is a UI/clipboard proof, not a
// replacement for a live Pages or Tauri window readback.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { readFile, realpath } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const preprocessor = vitePreprocess();
async function component(path) {
  const processed = await preprocess(await readFile(path, 'utf8'), preprocessor, { filename: path });
  return {
    contents: compile(processed.code, { filename: path, generate: 'client', css: 'injected' }).js.code,
    loader: 'js', resolveDir: dirname(path)
  };
}
const ui = await build({
  absWorkingDir: root,
  stdin: {
    contents: `import { mount } from 'svelte';
      import ProductResultPager from './packages/clearra-ui/src/lib/workspace/ProductResultPager.svelte';
      import { decodeCtk3 } from './packages/clearra-ui/src/lib/workspace/ctk3Codec.ts';
      const response = await fetch('/fixture.json');
      if (!response.ok) throw new Error('portfolio fixture unavailable');
      const value = await response.json();
      const pages = value.member_pages;
      window.__portfolioPageLoads = [];
      window.__portfolioDecode = decodeCtk3;
      window.__portfolioNative = value.artifact.formats.find(item => item.format === 'ctk3').document;
      mount(ProductResultPager, { target: document.getElementById('app'), props: {
        payload: value.final_response.product_result_payload,
        language: 'en', targetLines: 4,
        loadMemberPage: async (alternativeIndex, memberPageNumber) => {
          window.__portfolioPageLoads.push([alternativeIndex, memberPageNumber]);
          return pages[Number(memberPageNumber) - 1];
        }
      } });`,
    resolveDir: root, sourcefile: 'v081-portfolio-browser-copy-acceptance.js'
  },
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte', 'browser', 'module', 'main'],
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
  define: { 'process.env.NODE_ENV': '"production"', 'import.meta.env.MODE': '"production"' },
  plugins: [{ name: 'compiled-product-pager', setup(builder) {
    builder.onResolve({ filter: /^\$app\/paths$/ },
      () => ({ path: 'paths', namespace: 'portfolio-sveltekit' }));
    builder.onLoad({ filter: /.*/, namespace: 'portfolio-sveltekit' },
      () => ({ contents: "export const base = '';", loader: 'js' }));
    builder.onLoad({ filter: /\.svelte$/ }, ({ path }) => component(path));
  } }]
});
assert.equal(ui.outputFiles.length, 1);
assert.match(ui.outputFiles[0].text, /portfolio export ended before the declared member count/u,
  'the production pager must retain its whole-set export source');

if (process.argv.includes('--bundle-only')) {
  console.log('v0.8.1 product pager browser bundle: source-only pass');
} else {
  await browserAcceptance();
}

async function browserAcceptance() {
  const configuredRoot = process.env.CLEARRA_REAL_PORTFOLIO_SMOKE_DIR;
  const expectedSource = process.env.CLEARRA_REAL_PORTFOLIO_SOURCE_COMMIT;
  const browserTools = process.env.CLEARRA_BROWSER_TOOLS_ROOT;
  assert.ok(configuredRoot && browserTools, 'exact fixture and browser tool roots are required');
  assert.match(expectedSource ?? '', /^[a-f0-9]{40}$/u, 'exact source commit is required');
  const repository = await realpath(root);
  const fixtureRoot = await realpath(configuredRoot);
  assert.equal(fixtureRoot, join(repository, '_local', 'artifacts', 'v081-product-page-smoke'));
  const fixture = JSON.parse(await readFile(join(fixtureRoot, 'portfolio-wire-smoke.json'), 'utf8'));
  assert.equal(fixture.schema_id, 'clearra.v081.real-portfolio-wire-smoke.v2');
  assert.deepEqual(fixture.runtime_identity, {
    ...fixture.runtime_identity,
    source_commit: expectedSource,
    engine_build_id: expectedSource
  });
  assert.equal(fixture.multi_member_cases.length, 4);
  const value = fixture.multi_member_cases[0];
  assert.equal(value.member_pages.length, 3);
  assert.equal(value.candidate_keys.length, 246);
  assert.equal(value.member_pages[1].page.members.length, 100);
  assert.ok(value.artifact.formats.some(item => item.format === 'ctk3'));
  const fixtureBytes = Buffer.from(JSON.stringify(value));

  const require = createRequire(resolve(browserTools, 'package.json'));
  const { chromium } = require('playwright');
  const server = createServer((request, response) => {
    const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
    response.setHeader('Cache-Control', 'no-store');
    if (pathname === '/') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/ui.js"></script>');
    } else if (pathname === '/ui.js') {
      response.writeHead(200, { 'Content-Type': 'text/javascript; charset=utf-8' });
      response.end(ui.outputFiles[0].contents);
    } else if (pathname === '/fixture.json') {
      response.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8' });
      response.end(fixtureBytes);
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
    const context = await browser.newContext();
    const origin = `http://127.0.0.1:${address.port}`;
    await context.grantPermissions(['clipboard-read', 'clipboard-write'], { origin });
    const page = await context.newPage();
    page.setDefaultTimeout(90_000);
    const errors = [];
    page.on('pageerror', error => errors.push(String(error.stack || error)));
    await page.goto(origin);
    const gallery = page.locator('ol.solution-gallery > li');
    await page.waitForFunction(() => document.querySelectorAll('ol.solution-gallery > li').length === 100);
    assert.equal(await gallery.count(), 100);
    await page.getByRole('button', { name: 'Next 100' }).click();
    await page.waitForFunction((expected) =>
      document.querySelector('ol.solution-gallery > li')?.getAttribute('data-solution-key') === expected,
      value.candidate_keys[100]);
    assert.equal(await gallery.count(), 100, 'member page 2 renders only 100 solutions');
    await page.getByRole('button', { name: 'Copy all' }).click();
    await page.waitForFunction(async () => {
      try { return (await navigator.clipboard.readText()).startsWith('ctk3'); }
      catch { return false; }
    });
    const copied = await page.evaluate(async () => {
      const text = await navigator.clipboard.readText();
      const actual = window.__portfolioDecode(text);
      const expected = window.__portfolioDecode(window.__portfolioNative);
      const meaning = document => ({
        width: document.width,
        pages: document.pages.map(item => ({ height: item.height, cells: item.cells }))
      });
      return { text, actual: meaning(actual), expected: meaning(expected),
        pageLoads: window.__portfolioPageLoads };
    });
    assert.equal(copied.actual.pages.length, 246,
      'copy must include the selected set, not only the visible 100 members');
    assert.deepEqual(copied.actual, copied.expected,
      'browser clipboard must retain the real native fields and canonical order');
    assert.deepEqual(copied.pageLoads, [['1', '2'], ['1', '2'], ['1', '3']],
      'visible paging and whole-set export have separate, exact page reads');
    assert.deepEqual(errors, []);
    await context.close();
    console.log('v0.8.1 real portfolio browser clipboard: rendered=100 copied=246 member_pages=3 passed');
  } finally {
    await browser?.close();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
}

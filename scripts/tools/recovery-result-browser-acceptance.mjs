// Actual shared result components with an explicitly labeled UI fixture.
// The supplied witness was reconstructed independently; no Rust execution is claimed.
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
const reportRoot = resolve(process.env.RUNNER_TEMP || resolve(root, 'build'), 'clearra-recovery-result-acceptance');
await mkdir(reportRoot, { recursive: true });
const preprocessor = vitePreprocess();
const fixture = `<script>
  import Result from './packages/clearra-ui/src/lib/workspace/RecoveryBuildResult.svelte';
  import payload from './tests/fixtures/recovery-build/browser-example.json';
  let language = 'en';
  const view = { kind:'web',status:'completed',terminationReason:null,jobId:1,
    progressLabel:'',progressDone:1,progressTotal:1,forwardPatternDone:0,forwardPatternTotal:0,
    progressTelemetry:null,publicFailures:[],developerDiagnostics:[],searchReport:null,
    webgpuReport:null,backendReport:null,resourceReport:null,renderCapability:null,developerError:null,
    response:{product_result_payload:{contract:'recovery-build.v2',result_kind:'recovery-build',
      content:{payload_kind:'recovery-build',payload}}} };
</script>
<p data-fixture-notice>UI fixture from an independently reconstructed path; not a Rust engine execution.</p>
<div data-fixture-language>{#each ['en','ko','ja'] as value}<button on:click={() => language=value}>{value}</button>{/each}</div>
<Result {view} {language} elapsedMs={0} />`;
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
  { language:'en',width:1440,touch:false },
  { language:'en',width:320,touch:true },
  { language:'ko',width:390,touch:true },
  { language:'ja',width:390,touch:true },
];
const results=[];
let browser;
try {
  await new Promise((ok,fail)=>{server.once('error',fail);server.listen(0,'127.0.0.1',ok);});
  const origin=`http://127.0.0.1:${server.address().port}`;
  browser=await chromium.launch({headless:true,executablePath:process.env.CLEARRA_BROWSER_EXECUTABLE||undefined});
  for(const spec of specs) {
    const context=await browser.newContext({viewport:{width:spec.width,height:1000},hasTouch:spec.touch,isMobile:spec.touch});
    const page=await context.newPage();page.setDefaultTimeout(10000);
    const errors=[];page.on('pageerror',error=>errors.push(error.stack||String(error)));
    try {
      if(process.env.CLEARRA_BROWSER_DOCUMENT_MODE==='memory') {
        await page.setContent('<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><div id="app"></div>');
        await page.addScriptTag({type:'module',content:bundle.outputFiles[0].text});
      } else await page.goto(origin);
      await page.locator('[data-fixture-language] button').filter({hasText:spec.language}).click();
      const result=page.locator('.result-workspace');await result.waitFor();
      const img=page.locator('.pc-path-replay-gif img');await img.waitFor();
      await img.evaluate(node=>node.decode());
      assert.equal(await page.locator('.invalid-evidence,.invalid-replay').count(),0);
      assert.equal(await page.locator('.result-heading').count(),1);
      assert.equal(await page.locator('.solution-toolbar .copy-format').count(),1);
      assert.equal(await page.locator('.recovery-path-gallery>li').count(),1);
      assert.equal(await page.locator('.representative-note').count(),1,'do not claim one representative means all solutions');
      assert.equal(await page.locator('.recovery-path-gallery details,.recovery-path-gallery code,.frame-count').count(),0);
      assert.equal(await page.locator('.solution-toolbar input[type="checkbox"]').count(),1);
      const [download]=await Promise.all([page.waitForEvent('download'),page.locator('.download-action button').click()]);
      assert.equal(await download.failure(),null);
      const destination=resolve(reportRoot,`${spec.language}-${spec.width}.ctk3`);await download.saveAs(destination);
      assert.match(await readFile(destination,'utf8'),/^ctk3_/);
      await page.getByRole('button',{name:'Fumen',exact:true}).click();
      assert.equal(await page.locator('.download-action').count(),0);
      assert.equal(await page.getByRole('button',{name:'Fumen',exact:true}).getAttribute('aria-pressed'),'true');
      await page.getByRole('button',{name:'CTK3',exact:true}).click();
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1),false,'no horizontal overflow');
      assert.deepEqual(errors,[]);
      await page.screenshot({path:resolve(reportRoot,`${spec.language}-${spec.width}.png`),fullPage:true});
      results.push({...spec,status:'passed',frames:14,download:'ctk3',sharedResult:true});
    } catch(error) {
      await writeFile(resolve(reportRoot,`failed-${spec.language}-${spec.width}.txt`),[String(error.stack||error),...errors,await page.locator('body').innerText()].join('\n'));
      await page.screenshot({path:resolve(reportRoot,`failed-${spec.language}-${spec.width}.png`),fullPage:true});throw error;
    } finally{await context.close();}
  }
  await writeFile(resolve(reportRoot,'result.json'),JSON.stringify({source:process.env.CLEARRA_SOURCE_COMMIT,browser:await browser.version(),
    documentMode:process.env.CLEARRA_BROWSER_DOCUMENT_MODE||'http',solverExecuted:false,fixture:'independent-source-derived-srs-path',cases:results},null,2)+'\n');
  console.log('recovery_result_acceptance=passed cases='+results.length);
} finally{await browser?.close();await new Promise(ok=>server.close(ok));}

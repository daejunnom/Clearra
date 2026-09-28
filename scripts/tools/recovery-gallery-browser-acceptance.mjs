// UI-only 203-record fixture to exercise paging, pin ownership, exports and URL lifetime.
// Repeated witness geometry is intentional test data, not a solver catalog or minimum proof.
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
const reportRoot = resolve(process.env.RUNNER_TEMP || resolve(root, 'build'), 'clearra-recovery-gallery-acceptance');
await mkdir(reportRoot, { recursive: true });
const preprocessor = vitePreprocess();
const fixture = `<script>
  import Result from './packages/clearra-ui/src/lib/workspace/RecoveryBuildResult.svelte';
  import payload from './tests/fixtures/recovery-build/browser-example.json';
  import { decodeCtk3 } from './packages/clearra-ui/src/lib/workspace/ctk3Codec';
  let language = 'en';
  let pngRender = true;
  let showProbabilities = false;
  let locked = false;
  let content = {...payload,solutions_complete:true,
    solutions:Array.from({length:203},(_,i)=>({key:'gallery-'+String(i).padStart(3,'0'),covered_count:'1',probability:'1',example:payload.examples[0]}))};
  $: view = { kind:'web',status:'completed',terminationReason:null,jobId:1,
    progressLabel:'',progressDone:1,progressTotal:1,forwardPatternDone:0,forwardPatternTotal:0,
    progressTelemetry:null,publicFailures:[],developerDiagnostics:[],searchReport:null,
    webgpuReport:null,backendReport:null,resourceReport:null,renderCapability:null,developerError:null,
    response:{product_result_payload:{contract:'recovery-build.v2',result_kind:'recovery-build',
      content:{payload_kind:'recovery-build',payload:content}}} };
  window.minimumSelections=[];
  window.decodeGalleryExport=(text)=>decodeCtk3(text).pages.length;
  function provenFixture() {content={...content,minimum_proven:true,selected_solution_keys:['gallery-000','gallery-202'],required_solution_keys:['gallery-000','gallery-202']};}
</script>
<p data-fixture-notice>UI-only 203 rows; this is not solver or minimum proof evidence.</p>
<button data-fixture-png on:click={() => pngRender=!pngRender}>PNG</button>
<button data-fixture-probability on:click={() => showProbabilities=!showProbabilities}>Probability</button>
<button data-fixture-lock on:click={() => locked=!locked}>Input changed</button>
<button data-fixture-proven on:click={provenFixture}>Load proven-minimum UI fixture</button>
<Result {view} {language} elapsedMs={0} {pngRender} {showProbabilities} minimumDisabled={locked}
 on:minimum={event=>window.minimumSelections=[...window.minimumSelections,event.detail]}/>`;
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
const specs = [{ language:'en',width:1440,touch:false }];
const results=[];
let browser;
try {
  await new Promise((ok,fail)=>{server.once('error',fail);server.listen(4194,'127.0.0.1',ok);});
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
      const gallery=page.locator('.recovery-path-gallery');
      await gallery.locator('img').first().waitFor();
      assert.equal(await gallery.locator(':scope > li').count(),100);
      assert.equal(await gallery.locator('details,code,.frame-count').count(),0);
      assert.equal(await gallery.locator('.solution-probability').count(),0);
      await page.locator('[data-fixture-probability]').click();
      await page.waitForFunction(()=>document.querySelectorAll('.solution-probability').length===100);
      assert.ok((await gallery.locator('.solution-probability').allTextContents()).every(text=>text.endsWith('100%')));
      await gallery.locator('.mandatory-choice input').first().check();
      await page.getByRole('button',{name:'Next page',exact:true}).click();
      assert.equal(await gallery.locator(':scope > li').count(),100);
      await page.getByRole('button',{name:'Next page',exact:true}).click();
      assert.equal(await gallery.locator(':scope > li').count(),3);
      assert.deepEqual(await gallery.locator(':scope > li').evaluateAll(nodes=>nodes.map(n=>n.dataset.solutionKey)),['gallery-200','gallery-201','gallery-202']);
      await gallery.locator('.mandatory-choice input').last().check();
      await page.locator('.mandatory-summary button').first().click();
      let selected=await page.evaluate(()=>window.minimumSelections);
      assert.equal(selected.length,1);
      assert.deepEqual(selected[0].keys,['gallery-000','gallery-202']);
      assert.match(selected[0].sourceIdentity,/^[a-f0-9]{64}$/);
      await page.getByRole('button',{name:'Previous page',exact:true}).click();
      await page.getByRole('button',{name:'Previous page',exact:true}).click();
      assert.equal(await gallery.locator('.mandatory-choice input').first().isChecked(),true);
      await page.locator('[data-fixture-lock]').click();
      assert.equal(await page.locator('.mandatory-summary button').first().isDisabled(),true);
      assert.equal(await gallery.locator('.mandatory-choice input').first().isDisabled(),true);
      await page.locator('[data-fixture-lock]').click();
      await page.locator('.solution-toolbar').getByRole('button',{name:'Minimum solutions',exact:true}).click();
      selected=await page.evaluate(()=>window.minimumSelections);
      assert.equal(selected.length,2);assert.deepEqual(selected[1].keys,[]);
      async function exportedPages(name) {
        const [download]=await Promise.all([page.waitForEvent('download'),page.locator('.download-action button').click()]);
        assert.equal(await download.failure(),null);
        const destination=resolve(reportRoot,name+'.ctk3');await download.saveAs(destination);
        const text=await readFile(destination,'utf8');
        return await page.evaluate(text=>window.decodeGalleryExport(text),text);
      }
      assert.equal(await exportedPages('all-pages'),406,'copy includes every catalog row, not the rendered 100');
      await page.getByRole('checkbox',{name:'Result only',exact:true}).check();
      assert.equal(await exportedPages('all-results-only'),203);
      await page.locator('[data-fixture-proven]').click();
      await page.waitForFunction(()=>document.querySelectorAll('.recovery-path-gallery>li').length===2);
      assert.equal(await page.locator('.recovery-pagination').count(),0);
      assert.equal(await exportedPages('minimum-results-only'),2);
      await page.getByRole('checkbox',{name:'Result only',exact:true}).uncheck();
      assert.equal(await exportedPages('minimum-two-checkpoints'),4);
      await page.locator('[data-fixture-png]').click();
      await gallery.locator('.pc-path-replay-gif img').first().waitFor();
      for(const image of await gallery.locator('img').all()) {
        await image.evaluate(node=>node.decode());
        assert.equal(await image.evaluate(async node=>(await fetch(node.src)).headers.get('content-type')),'image/gif');
      }
      assert.deepEqual(errors,[]);
      await page.screenshot({path:resolve(reportRoot,'paged-minimum-gallery.png'),fullPage:true});
      results.push({...spec,status:'passed',catalogRows:203,pages:[100,100,3],copyPages:[406,203,2,4],pinnedKeys:selected[0].keys,solverExecuted:false});
    } catch(error) {
      await writeFile(resolve(reportRoot,`failed-${spec.language}-${spec.width}.txt`),[String(error.stack||error),...errors,await page.locator('body').innerText()].join('\n'));
      await page.screenshot({path:resolve(reportRoot,`failed-${spec.language}-${spec.width}.png`),fullPage:true});throw error;
    } finally{await context.close();}
  }
  await writeFile(resolve(reportRoot,'result.json'),JSON.stringify({source:process.env.CLEARRA_SOURCE_COMMIT,browser:await browser.version(),
    documentMode:process.env.CLEARRA_BROWSER_DOCUMENT_MODE||'http',solverExecuted:false,fixture:'independent-source-derived-srs-path',cases:results},null,2)+'\n');
  console.log('recovery_gallery_acceptance=passed cases='+results.length);
} finally{await browser?.close();await new Promise(ok=>server.close(ok));}

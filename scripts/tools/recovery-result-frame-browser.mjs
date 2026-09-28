// Actual current UI + the unchanged released worker/WASM. No response mocking,
// field translation in the test, or altered solver rules. Baseline uses the
// original released UI to demonstrate the regression with the same drawing.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const output = resolve(root, '_local/artifacts/test/recovery-result-frame');
const accepted = resolve(root, '_local/artifacts/test/recovery-zero/accepted');
assert.ok(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'pinned browser tools required');
const { chromium, expect } = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'))('playwright/test');
const identity = JSON.parse(await readFile(resolve(accepted, 'clearra-build-identity.json'), 'utf8'));
assert.equal(identity.sourceCommit, '1868d31da7730cc9d3018aecd18e442c6940a5d7');
for (const file of identity.files) {
  assert.ok(!file.path.startsWith('/') && !file.path.split('/').includes('..'));
  const content = await readFile(resolve(accepted, file.path));
  assert.equal(content.length, file.size);
  assert.equal(createHash('sha256').update(content).digest('hex'), file.sha256);
}
const workers = identity.files.filter(file => /^_app\/immutable\/workers\/clearraWorker-[^/]+\.js$/.test(file.path));
assert.equal(workers.length, 1);
const fixture = JSON.parse(await readFile(resolve(root, 'tests/fixtures/recovery-build/user-20260928.json'), 'utf8'));
assert.equal(fixture.result_mask, '0xc120fc1fcfe3c000000000000');
assert.equal(fixture.result_frame, 'shared');
const preprocessor = vitePreprocess();
const harness = `<script>
import { setContext } from 'svelte';
import Workspace from './packages/clearra-ui/src/lib/workspace/BoundaryRecoveryWorkspace.svelte';
import { WORKSPACE_MODE_VISIBILITY_CONTEXT } from './packages/clearra-ui/src/lib/workspace/workspaceNavigation.ts';
setContext(WORKSPACE_MODE_VISIBILITY_CONTEXT, ['pc','setup','build-probability','recovery','damage','spin-finder','ctk','player']);
const workerFactory = () => new Worker(${JSON.stringify('/Clearra/' + workers[0].path)}, { type: 'module' });
</script><Workspace {workerFactory}/>`;
async function component(source, filename) {
  const processed = await preprocess(source, preprocessor, { filename });
  const result = compile(processed.code, { filename, generate: 'client', css: 'injected' });
  assert.deepEqual(result.warnings, [], filename);
  return { contents: result.js.code, loader: 'js', resolveDir: dirname(filename) };
}
const bundle = await build({ absWorkingDir: root,
  stdin: { contents: "import {mount} from 'svelte'; import Harness from 'result-frame-harness'; mount(Harness,{target:document.getElementById('app')});", resolveDir: root },
  bundle: true, write: false, platform: 'browser', format: 'esm', target: 'es2022',
  conditions: ['browser'], mainFields: ['svelte','browser','module','main'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'actual-recovery-workspace', setup(builder) {
    builder.onResolve({ filter: /^result-frame-harness$/ }, () => ({ path: 'fixture', namespace: 'harness' }));
    builder.onLoad({ filter: /.*/, namespace: 'harness' }, () => component(harness, resolve(root, 'recovery-result-frame-harness.svelte')));
    builder.onResolve({ filter: /^\$app\/navigation$/ }, () => ({ path: 'unused', namespace: 'router' }));
    builder.onLoad({ filter: /.*/, namespace: 'router' }, () => ({ contents: "export function goto(){throw new Error('Unexpected navigation in recovery regression');}", loader: 'js' }));
    builder.onLoad({ filter: /\.svelte$/ }, async ({path}) => component(await readFile(path,'utf8'),path));
  }}] });
assert.equal(bundle.outputFiles.length, 1);
await mkdir(output, {recursive:true});
const server = createServer(async (req,res) => {
  try {
    const path = decodeURIComponent(new URL(req.url,'http://127.0.0.1').pathname);
    if (path.split('/').includes('..')) return res.writeHead(400).end();
    if (path === '/candidate.js') return res.writeHead(200,{'Content-Type':'text/javascript'}).end(bundle.outputFiles[0].contents);
    if (path === '/') return res.writeHead(200,{'Content-Type':'text/html; charset=utf-8'}).end('<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><div id="app"></div><script type="module" src="/candidate.js"></script>');
    if (!path.startsWith('/Clearra/')) return res.writeHead(404).end();
    const relative = path.slice('/Clearra/'.length) || 'index.html';
    const data = await readFile(resolve(accepted,relative));
    const mime={'.html':'text/html; charset=utf-8','.js':'text/javascript','.json':'application/json','.wasm':'application/wasm','.css':'text/css'};
    res.writeHead(200,{'Content-Type':mime[extname(relative)]||'application/octet-stream','Cache-Control':'no-store'}).end(data);
  } catch {res.writeHead(404).end();}
});
await new Promise((ok,fail)=>{server.once('error',fail);server.listen(4194,'127.0.0.1',ok);});
const normalized='0x30483f07f3f8f';
const cases = process.env.CLEARRA_RECOVERY_FULL_FIXTURE === '1'
  ? [{id:'original-full-P7-P7',processors:4,first:fixture.first_supply,second:fixture.second_supply,exchange:fixture.allow_piece_exchange,full:true}]
  : [{id:'released-original-zero',processors:1,legacy:true,first:'ITOLSZJ',second:'P7',exchange:true,zero:true},
     ...[1,4].flatMap(processors=>[false,true].map(exchange=>({id:`original-fixed-P7-${exchange}-${processors}`,processors,first:'ITOLSZJ',second:'P7',exchange}))),
     {id:'explicit-after-middle',processors:4,first:'ITOLSZJ',second:'P7',exchange:true,after:true},
     {id:'no-clear-I-O',processors:1,first:'I',second:'O',exchange:false,small:true}];
const reports=[];
let browser;
try {
  browser=await chromium.launch({headless:true});
  for(const spec of cases) {
    const context=await browser.newContext({viewport:{width:1440,height:1100},locale:'en-US'});
    await context.addInitScript(n=>{
      Object.defineProperty(navigator,'hardwareConcurrency',{get:()=>n});
      window.recoveryCommands=[];window.recoveryFinals=[];
      const Original=window.Worker;
      window.Worker=class extends Original {
        constructor(...args){super(...args);this.addEventListener('message',event=>{if(event.data?.event==='final_response')window.recoveryFinals.push(event.data);});}
        postMessage(...args){if(args[0]?.type==='run_command_text')window.recoveryCommands.push(args[0]);return super.postMessage(...args);}
      };
    },spec.processors);
    const page=await context.newPage();page.setDefaultTimeout(20000);
    const errors=[];page.on('pageerror',e=>errors.push(String(e)));
    const report={id:spec.id,ui_source:spec.legacy?identity.sourceCommit:process.env.GITHUB_SHA,
      engine_source:identity.sourceCommit,first:spec.first,second:spec.second,exchange:spec.exchange,
      full_original_fixture:!!spec.full,processors:spec.processors,status:'running'};
    try {
      await page.goto(spec.legacy?'http://127.0.0.1:4194/Clearra/?tool=recovery':'http://127.0.0.1:4194/');
      await page.locator('.field-palette button').first().waitFor();
      const height=spec.small?8:fixture.height;
      await page.locator('.dimension-field input').fill(String(height));
      const fields=page.locator('.field-palette button');
      // Select the legacy frame explicitly BEFORE entering a legacy drawing.
      // No test-side conversion of the original shared-frame drawing is allowed.
      if(spec.after){await fields.nth(2).click();await page.getByRole('combobox',{name:'Result coordinates',exact:true}).selectOption('after-middle');}
      const masks=spec.small?['0','0xf','0xc030']:[fixture.start_mask,fixture.middle_mask,spec.after?normalized:fixture.result_mask];
      for(let f=0;f<3;f++) {
        await fields.nth(f).click();
        const cells=page.locator('.recovery-field-editor .board button');
        await expect(cells).toHaveCount(height*10);
        for(let y=0;y<height;y++)for(let x=0;x<10;x++)if(BigInt(masks[f])&(1n<<BigInt(y*10+x)))await cells.nth((height-1-y)*10+x).click();
      }
      const drawn=()=>page.locator('.recovery-field-editor .board button').evaluateAll((cells,h)=>{
        let mask=0n;cells.forEach((cell,i)=>{if(cell.classList.contains('existing'))mask|=1n<<BigInt((h-1-Math.floor(i/10))*10+i%10);});return mask.toString();
      },height);
      assert.equal(await drawn(),BigInt(masks[2]).toString());
      if(!spec.legacy&&!spec.after&&!spec.small){
        await expect(page.locator('.recovery-field-editor .board button.reference')).toHaveCount(50);
        const frame=page.getByRole('combobox',{name:'Result coordinates',exact:true});
        await expect(frame).toHaveValue('shared');
        await frame.selectOption('after-middle');await expect.poll(drawn).toBe(BigInt(normalized).toString());
        await expect(page.locator('.recovery-field-editor .board button.reference')).toHaveCount(0);
        await frame.selectOption('shared');await expect.poll(drawn).toBe(BigInt(fixture.result_mask).toString());
      }
      const supplies=page.locator('.recovery-supplies .workspace-queue-input');
      await supplies.nth(0).fill(spec.first);await supplies.nth(1).fill(spec.second);
      await page.getByRole('checkbox',{name:'Hold',exact:true}).check();
      await page.getByRole('checkbox',{name:'Allow different-piece repayment',exact:true}).setChecked(spec.exchange);
      await page.getByRole('checkbox',{name:'Preserve B2B',exact:true}).setChecked(!spec.small);
      await expect(page.locator('.workspace-controls select').first()).toHaveValue('auto');
      await expect(page.locator('.run-actions button.run')).toBeEnabled();
      await page.locator('.run-actions button.run').click();
      await page.locator('.recovery-metrics').waitFor({timeout:spec.full?720000:60000});
      const finals=await page.evaluate(()=>window.recoveryFinals);
      assert.equal(finals.length,1);
      const response=finals[0].response;
      assert.equal(response.status,'success');
      const payload=response.product_result_payload.content.payload;
      assert.equal(payload.complete,true);
      assert.equal(payload.pattern_count,spec.full?fixture.expected_pair_count:spec.small?'1':'5040');
      assert.equal(payload.evaluated_pattern_count,payload.pattern_count);
      assert.equal(BigInt(payload.normal_count)+BigInt(payload.recovery_count)+BigInt(payload.no_path_count),BigInt(payload.pattern_count));
      if(spec.zero){assert.equal(payload.normal_count,'0');assert.equal(payload.recovery_count,'0');}
      else {
        assert.ok(BigInt(payload.normal_count)+BigInt(payload.recovery_count)>0n,'known successful drawing must not be lost');
        if(!spec.full)assert.equal(payload.no_path_count,'0');
        assert.ok(payload.examples.length>0);
        assert.equal(await page.locator('.invalid-evidence').count(),0);
      }
      const commands=await page.evaluate(()=>window.recoveryCommands);assert.equal(commands.length,1);
      const command=commands[0].commandText;
      const actual=/--result-mask\s+(\S+)/.exec(command)?.[1];
      const expected=spec.zero?fixture.result_mask:spec.small?'0xc030':normalized;
      assert.equal(BigInt(actual),BigInt(expected));
      assert.equal(BigInt(payload.result_target_mask),BigInt(expected));
      assert.equal(await drawn(),BigInt(masks[2]).toString(),'search must not move the editor draft');
      assert.deepEqual(errors,[]);
      Object.assign(report,{status:'passed',command,payload,metrics:await page.locator('.recovery-metrics').innerText()});
      console.log(JSON.stringify({id:spec.id,status:report.status,normal:payload.normal_count,recovery:payload.recovery_count,no_path:payload.no_path_count}));
    } catch(error) {
      Object.assign(report,{status:'failed',error:String(error),errors,body:await page.locator('body').innerText(),commands:await page.evaluate(()=>window.recoveryCommands)});
      throw error;
    } finally {
      await page.screenshot({path:resolve(output,spec.id+'.png'),fullPage:true}).catch(()=>{});
      reports.push(report);await writeFile(resolve(output,'results.json'),JSON.stringify(reports,null,2)+'\n');
      await context.close();
    }
  }
} finally {await browser?.close();await new Promise(ok=>server.close(ok));}
assert.ok(reports.every(r=>r.status==='passed'));

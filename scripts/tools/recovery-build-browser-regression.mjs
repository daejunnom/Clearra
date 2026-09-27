// Real compiled components and real pointer events; no solver result is mocked.
// The full P7/P7 search is a separate performance fixture, not claimed here.
import assert from 'node:assert/strict';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createServer} from 'node:http';
import {createRequire} from 'node:module';
import {build} from 'esbuild';
import {compile,preprocess} from 'svelte/compiler';
import {vitePreprocess} from '@sveltejs/vite-plugin-svelte';
const root=process.cwd(),out=resolve(root,'build/recovery-regression');
const require=createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT,'package.json'));
const {chromium}=require('playwright');
const fixture=JSON.parse(await readFile(resolve(root,'tests/fixtures/recovery-build/user-20260928.json'),'utf8'));
await mkdir(out,{recursive:true});
const harness=`<script>
import Fields from '${root}/packages/clearra-ui/src/lib/workspace/RecoveryBuildFields.svelte';
import {createRecoveryBuildRequest} from '${root}/packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';
let request={...createRecoveryBuildRequest(),height:${fixture.height},startMask:BigInt('${fixture.start_mask}'),middleMask:BigInt('${fixture.middle_mask}'),resultMask:BigInt('${fixture.result_mask}')};
$: window.recoverySnapshot={start:request.startMask.toString(),middle:request.middleMask.toString(),result:request.resultMask.toString()};
</script><Fields {request} language="ko" on:change={event=>request=event.detail}/>`;
await build({stdin:{contents:"import {mount} from 'svelte'; import Harness from 'recovery-harness'; mount(Harness,{target:document.body});",resolveDir:root},
 bundle:true,format:'iife',platform:'browser',conditions:['browser'],mainFields:['svelte','browser','module','main'],
 outfile:resolve(out,'fields.js'),plugins:[{name:'svelte-regression',setup(build){
  build.onResolve({filter:/^recovery-harness$/},()=>({path:'recovery-harness.svelte',namespace:'harness'}));
  const load=async(args)=>{const filename=args.namespace==='harness'?resolve(out,args.path):args.path;
   const source=args.namespace==='harness'?harness:await readFile(filename,'utf8');
   const processed=await preprocess(source,vitePreprocess(),{filename});
   return {contents:compile(processed.code,{filename,generate:'client',css:'injected'}).js.code,loader:'js',resolveDir:root};};
  build.onLoad({filter:/.*/,namespace:'harness'},load);build.onLoad({filter:/\.svelte$/},load);
 }}]});
const server=createServer(async(req,res)=>{
 try {if(req.url==='/fields.js'){res.setHeader('Content-Type','text/javascript');res.end(await readFile(resolve(out,'fields.js')));}
 else res.end('<!doctype html><meta charset="utf-8"><style>body{max-width:660px;margin:16px auto;font-family:sans-serif}</style><script src="/fields.js"></script>');}
 catch{res.writeHead(500).end();}
});
await new Promise((ok,fail)=>{server.once('error',fail);server.listen(4194,'127.0.0.1',ok);});
let browser;
try{
 browser=await chromium.launch({headless:true});const page=await browser.newPage({viewport:{width:900,height:1250}});
 const errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.goto('http://127.0.0.1:4194');await page.locator('.board button').last().waitFor();
 const snapshot=()=>page.evaluate(()=>window.recoverySnapshot);
 const expected=await snapshot();assert.deepEqual(expected,{start:BigInt(fixture.start_mask).toString(),middle:BigInt(fixture.middle_mask).toString(),result:BigInt(fixture.result_mask).toString()});
 for(const label of ['중간','결과','시작','결과']){
  await page.getByRole('button',{name:label,exact:true}).click();
  await page.waitForFunction(()=>Boolean(window.recoverySnapshot));
  assert.deepEqual(await snapshot(),expected);
  if(label==='결과'){
   assert.equal(await page.locator('.board button.existing').count(),28);
   assert.equal(await page.locator('.board button.reference').count(),50);
  }
 }
 const context=page.locator('.recovery-field-editor input[type="checkbox"]');
 await context.uncheck();assert.equal(await page.locator('.board button.reference').count(),0);
 await context.check();assert.equal(await page.locator('.board button.reference').count(),50);
 assert.deepEqual(await snapshot(),expected);
 // Editing a reference cell affects only the currently selected Result mask.
 await page.locator('.board button').nth(99).click();
 assert.deepEqual(await snapshot(),{...expected,result:(BigInt(expected.result)|(1n<<9n)).toString()});
 await page.locator('.board button').nth(99).click();assert.deepEqual(await snapshot(),expected);
 await page.screenshot({path:resolve(out,'result-preserves-start-middle.png'),fullPage:true});
 assert.deepEqual(errors,[]);
 await writeFile(resolve(out,'browser-result.json'),JSON.stringify({status:'passed',fixture:fixture.id,initial_cells:22,middle_cells:28,result_cells:28,reference_cells_on_result:50,search_executed:false},null,2));
 console.log('recovery field browser regression: passed (real Svelte/pointer events; no full population search)');
}finally{await browser?.close();await new Promise(ok=>server.close(ok));}

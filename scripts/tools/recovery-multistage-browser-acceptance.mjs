// Real compiled field/control components and pointer edits, without solver mocks.
import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
const root=process.cwd(), out=resolve(root,'_local/artifacts/test/multistage-browser');
const require=createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT,'package.json'));
const {chromium,expect}=require('playwright/test');
await mkdir(out,{recursive:true});
const harness=`<script>
import Fields from '${root}/packages/clearra-ui/src/lib/workspace/RecoveryBuildFields.svelte';
import Controls from '${root}/packages/clearra-ui/src/lib/workspace/RecoveryBuildControls.svelte';
import {createRecoveryBuildRequest,recoveryBuildArguments} from '${root}/packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';
import {recoveryStageEntries} from '${root}/packages/clearra-ui/src/lib/workspace/recoveryStages.ts';
let request={...createRecoveryBuildRequest(),height:12};
$: window.snapshot={...request,stages:recoveryStageEntries(request)};
$: window.command=()=>recoveryBuildArguments(request);
</script><Fields {request} language="ko" on:change={e=>request=e.detail}/><Controls {request} language="ko" on:change={e=>request=e.detail}/>`;
await build({stdin:{contents:"import{mount}from'svelte';import H from'harness';mount(H,{target:document.body});",resolveDir:root},bundle:true,format:'esm',platform:'browser',conditions:['browser'],mainFields:['svelte','browser','module','main'],outfile:resolve(out,'view.js'),plugins:[{name:'compiled-components',setup(b){
 b.onResolve({filter:/^harness$/},()=>({path:'harness.svelte',namespace:'harness'}));
 const load=async a=>{const f=a.namespace==='harness'?resolve(out,a.path):a.path; const input=a.namespace==='harness'?harness:await readFile(f,'utf8');const p=await preprocess(input,vitePreprocess(),{filename:f});return{contents:compile(p.code,{filename:f,generate:'client',css:'injected'}).js.code,loader:'js',resolveDir:dirname(f)};};
 b.onLoad({filter:/.*/,namespace:'harness'},load);b.onLoad({filter:/\.svelte$/},load);
}}]});
const server=createServer(async(req,res)=>{try{res.setHeader('Content-Type',req.url==='/view.js'?'text/javascript':'text/html; charset=utf-8');res.end(req.url==='/view.js'?await readFile(resolve(out,'view.js')):'<!doctype html><meta charset="utf-8"><style>body{max-width:800px;margin:16px auto;font-family:sans-serif}*{box-sizing:border-box}</style><body><script type="module" src="/view.js"></script></body>');}catch{res.writeHead(500).end();}});
await new Promise((ok,fail)=>{server.once('error',fail);server.listen(4194,'127.0.0.1',ok);});
let browser,page; const errors=[],checks=[];
try{
 browser=await chromium.launch({headless:true});page=await browser.newPage({viewport:{width:960,height:1300}});page.on('pageerror',e=>errors.push(e.message));
 await page.goto('http://127.0.0.1:4194');await page.locator('.board button').last().waitFor();
 const snap=()=>page.evaluate(()=>JSON.parse(JSON.stringify(window.snapshot,(_,v)=>typeof v==='bigint'?v.toString():v)));
 const plus=page.getByRole('button',{name:'중간 필드 추가',exact:true}), minus=page.getByRole('button',{name:'단계 삭제 (중간 필드 4개 이상)',exact:true});
 const select=(text)=>page.getByRole('button',{name:text,exact:true}).click();
 const draw=async(x,y)=>page.locator('.board button').nth((11-y)*10+x).click();
 const supply=()=>page.locator('.recovery-supplies input');
 await expect(minus).toBeDisabled();await expect(plus).toBeEnabled();
 assert.deepEqual((await page.locator('.board-actions button').all()).length>3,true);
 assert.equal(await plus.evaluate(e=>e.previousElementSibling?.getAttribute('aria-label')),'실행 취소');
 await select('중간');await draw(0,0);await supply().nth(0).fill('I');
 await select('결과');await draw(8,8);await supply().nth(1).fill('Z');
 for(let count=2;count<=4;count++){await plus.click();await expect(page.locator('.middle-palette button')).toHaveCount(count);await expect(supply()).toHaveCount(count+1);if(count<4)await expect(minus).toBeDisabled();}
 await expect(minus).toBeEnabled();
 const full=await snap();await minus.click();await expect(page.locator('.middle-palette button')).toHaveCount(3);
 const reduced=await snap();assert.equal(reduced.resultMask,full.resultMask);assert.equal(reduced.secondSupply,'Z');assert.equal(reduced.extraMiddles[0].id,full.extraMiddles[1].id);await expect(minus).toBeDisabled();checks.push('empty middle first, result/supply retained, literal four-middle gate');
 await select('중간 2');await draw(1,2);await supply().nth(1).fill('J');
 await select('중간 3');await draw(2,4);await supply().nth(2).fill('L');
 await plus.click();await draw(3,6);await supply().nth(3).fill('T');
 await select('시작');
 const images=await page.locator('.board button.reference').evaluateAll(es=>es.map(e=>({title:e.title,color:e.style.backgroundColor,image:getComputedStyle(e).backgroundImage})));
 const middles=images.filter(e=>e.title.startsWith('중간'));
 assert.equal(middles.length,4);assert.equal(new Set(middles.map(e=>e.color)).size,1);assert.equal(new Set(middles.map(e=>e.image)).size,2);checks.push('all middle stages share gray; inactive sequential hatches alternate');
 await select('중간');await select('중간 2');await draw(0,0);
 assert.equal((await snap()).middleMask,'0');checks.push('editing a reference cell transfers ownership');
 await select('중간 1');await draw(0,0);
 const before=await snap();await minus.click();const after=await snap();
 assert.equal(after.resultMask,before.extraMiddles.at(-1).mask);assert.equal(after.secondSupply,'T');assert.equal(after.extraMiddles.length,2);checks.push('occupied removal promotes last middle and its supply');
 const args=await page.evaluate(()=>window.command());assert.equal(args.filter(x=>x==='--stage-target').length,4);assert.equal(args.filter(x=>x==='--stage-supply').length,4);checks.push('four targets and four supplies preserved by command adapter');
 await page.screenshot({path:resolve(out,'three-middle-desktop.png'),fullPage:true});
 await page.setViewportSize({width:360,height:1200});
 const boxes=await supply().evaluateAll(es=>es.map(e=>({x:e.getBoundingClientRect().x,y:e.getBoundingClientRect().y,w:e.getBoundingClientRect().width})));
 assert.equal(new Set(boxes.map(b=>b.y)).size,4);assert.ok(boxes.every(b=>b.w>=190 && b.x+b.w<=361));checks.push('360px viewport wraps all supplies without truncation');
 await page.screenshot({path:resolve(out,'three-middle-mobile.png'),fullPage:true});
 assert.deepEqual(errors,[]);await writeFile(resolve(out,'result.json'),JSON.stringify({status:'passed',checks,snapshot:await snap(),args,boxes,search_executed:false},null,2));
 console.log(JSON.stringify({status:'passed',checks}));
}catch(error){await writeFile(resolve(out,'failure.json'),JSON.stringify({error:String(error),errors},null,2));await page?.screenshot({path:resolve(out,'failure.png'),fullPage:true});throw error;}
finally{await browser?.close();await new Promise(ok=>server.close(ok));}

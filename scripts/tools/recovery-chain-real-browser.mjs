// Full compiled app, real workers and the new WASM; only reported CPU count is varied.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile,writeFile,mkdir,stat} from 'node:fs/promises';
import {resolve,extname,sep} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {availableParallelism} from 'node:os';
const site=resolve('apps/clearra-web/build'),out=resolve('_local/artifacts/test/chain-e2e');
const source=process.env.CLEARRA_SOURCE_COMMIT;
assert.match(source,/^[a-f0-9]{40}$/u);
const {chromium,expect}=await import(pathToFileURL(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT,'node_modules/playwright/test.mjs')));
const codec=await import(pathToFileURL(resolve('packages/ctk3/dist/index.js')));
await mkdir(out,{recursive:true});
const manifest=JSON.parse(await readFile(resolve(site,'wasm/clearra_wasm.manifest.json'),'utf8'));
assert.equal(manifest.build.runtime_identity.source_commit,source);
for(const entry of [manifest.wasm,manifest.bindings]){
 const bytes=await readFile(resolve(site,'wasm',entry.path));
 assert.equal(bytes.length,entry.bytes);assert.equal(createHash('sha256').update(bytes).digest('hex'),entry.sha256);
}
const server=createServer(async(req,res)=>{try{
 const pathname=decodeURIComponent(new URL(req.url,'http://127.0.0.1').pathname);
 let path=resolve(site,'.'+pathname);
 if(path!==site&&!path.startsWith(site+sep)){res.writeHead(403).end();return;}
 if(!(await stat(path).catch(()=>null))?.isFile()){
   if(extname(pathname)){res.writeHead(404).end();return;}path=resolve(site,'index.html');
 }
 res.writeHead(200,{'content-type':{'.html':'text/html','.js':'text/javascript','.wasm':'application/wasm','.css':'text/css','.json':'application/json','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'}[extname(path)]??'application/octet-stream','cache-control':'no-store'});res.end(await readFile(path));
}catch{res.writeHead(500).end();}});
await new Promise((ok,no)=>{server.once('error',no);server.listen(4194,'127.0.0.1',ok);});
const browser=await chromium.launch({headless:true});
const summary={source,actualLogical:availableParallelism(),chromium:browser.version(),status:'running',cases:[]};
const save=(name,value)=>writeFile(resolve(out,name),JSON.stringify(value,null,2)+'\n');
let page;
try{
 for(const advertised of [1,12]){
  const ctx=await browser.newContext({locale:'en-US',viewport:{width:1440,height:1100},permissions:['clipboard-read','clipboard-write']});
  await ctx.addInitScript(logical=>{
   window.observed={finals:[],commands:[]};Object.defineProperty(navigator,'hardwareConcurrency',{configurable:true,get:()=>logical});
   const Original=window.Worker;window.Worker=class extends Original{
    constructor(...a){super(...a);this.addEventListener('message',e=>{if(['final_response','failed','terminated','cancelled'].includes(e.data?.event))window.observed.finals.push(e.data);});}
    postMessage(...a){if(a[0]?.type==='run_command_text')window.observed.commands.push(a[0].commandText);return super.postMessage(...a);}
   };
  },advertised);
  page=await ctx.newPage();const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  async function open(){await page.goto('http://127.0.0.1:4194/?tool=recovery');await page.locator('.recovery-field-editor').waitFor();await page.locator('.dimension-field input').fill('12');await expect(page.locator('.recovery-field-editor .board button')).toHaveCount(120);}
  const outer=name=>page.locator('.field-palette').getByRole('button',{name,exact:true});
  const add=()=>page.getByRole('button',{name:'Add middle field',exact:true}).click();
  const remove=()=>page.getByRole('button',{name:'Remove stage (at least 4 middle fields)',exact:true});
  async function paint(xs){const buttons=page.locator('.recovery-field-editor .board button');for(const index of xs){const x=index%10,y=Math.floor(index/10);await buttons.nth((11-y)*10+x).click();}}
  async function run(label,expected,action){
   const n=await page.evaluate(()=>window.observed.finals.length),start=performance.now();
   await (action??(()=>page.getByRole('button',{name:'Run search',exact:true}).click()))();
   await page.waitForFunction(n=>window.observed.finals.length>n,n,{timeout:90000});
   const event=await page.evaluate(()=>window.observed.finals.at(-1));await save(`${label}-${advertised}-response.json`,event);
   assert.equal(event.event,'final_response');assert.equal(event.response.status,'success');
   const p=event.response.product_result_payload.content.payload;
   assert.ok(p.complete&&p.solutions_complete);assert.equal(p.pattern_count,p.evaluated_pattern_count);
   assert.deepEqual([p.pattern_count,p.normal_count,p.recovery_count,p.no_path_count],expected);
   await expect(page.locator('.header-status > span')).toHaveText('Completed');
   assert.equal(await page.locator('.invalid-evidence,.invalid-replay').count(),0);
   const visible=p.minimum_proven?p.selected_solution_keys.length:p.solutions.length;
   await expect(page.locator('.recovery-path-gallery > li')).toHaveCount(visible);
   await page.waitForFunction(()=>[...document.querySelectorAll('.recovery-path-gallery>li')].every(c=>{const i=c.querySelector('img');return i?.complete&&i.naturalWidth>0;}));
   summary.cases.push({label,advertised,elapsedMs:Math.round(performance.now()-start),solutions:p.solutions.length,minimum:p.minimum_proven?p.selected_solution_keys.length:null,counts:expected,command:await page.evaluate(()=>window.observed.commands.at(-1))});
   return p;
  }
  async function copied(label,resultOnly){
   await page.getByRole('checkbox',{name:'Result only',exact:true}).setChecked(resultOnly);
   await page.evaluate(()=>navigator.clipboard.writeText(''));
   await page.locator('.solution-toolbar .copy-all').click();
   let text='';for(let i=0;i<100;i++){text=await page.evaluate(()=>navigator.clipboard.readText());if(/^ctk3[@b_]/u.test(text))break;await page.waitForTimeout(50);}
   assert.match(text,/^ctk3[@b_]/u);await writeFile(resolve(out,`${label}-${advertised}.ctk`),text);return codec.decodeCtk3Exact(text).pages;
  }
  await open();await outer('Middle').click();await paint([0,1,10,11]);
  await add();await paint([20,21,30,31]);await add();await paint([40,41,50,51]);
  await expect(page.locator('.middle-palette button')).toHaveCount(3);await expect(remove()).toBeDisabled();
  await outer('Result').click();await paint([60,61,70,71]);
  await add();await expect(remove()).toBeEnabled();await remove().click();
  await expect(page.locator('.middle-palette button')).toHaveCount(3);await expect(remove()).toBeDisabled();
  const supplies=page.locator('.recovery-supplies .workspace-queue-input');await expect(supplies).toHaveCount(4);
  for(const input of await supplies.all())await input.fill('[IO]');
  await page.getByRole('combobox',{name:'Maximum early placements',exact:true}).selectOption('0');
  assert.equal(await page.getByRole('combobox',{name:'Result coordinates',exact:true}).count(),0);
  assert.equal(await page.getByRole('checkbox',{name:'Show field context',exact:true}).count(),0);
  const all=await run('four-stage',['16','1','0','15']);assert.equal(all.solutions.length,2);
  const frames=await copied('four-stage-all',false);assert.equal(frames.length,8);
  for(let i=0;i<frames.length;i++)assert.equal(frames[i].cells.filter(x=>x==='O').length,4*(i%4+1));
  const final=await copied('four-stage-final',true);assert.equal(final.length,2);
  for(let i=0;i<2;i++)assert.deepEqual(final[i].cells,frames[i*4+3].cells);
  await page.getByRole('checkbox',{name:'Result only',exact:true}).uncheck();
  await page.getByRole('checkbox',{name:'PNG render',exact:true}).check();
  for(const pin of await page.locator('.mandatory-choice input').all())await pin.check();
  const pinned=await run('four-stage-pinned',['16','1','0','15'],()=>page.locator('.mandatory-summary button').first().click());
  assert.ok(pinned.minimum_proven);assert.equal(pinned.required_solution_keys.length,2);assert.equal(pinned.selected_solution_keys.length,2);
  const minimum=await run('four-stage-minimum',['16','1','0','15'],()=>page.locator('.solution-toolbar').getByRole('button',{name:'Minimum solutions',exact:true}).click());
  assert.ok(minimum.minimum_proven);assert.equal(minimum.selected_solution_keys.length,1);
  assert.equal((await copied('four-stage-minimum',false)).length,4);
  await page.screenshot({path:resolve(out,`four-stage-${advertised}.png`),fullPage:true});
  if(advertised===12){await page.setViewportSize({width:360,height:900});const boxes=await supplies.evaluateAll(ns=>ns.map(n=>({left:n.getBoundingClientRect().left,right:n.getBoundingClientRect().right,top:n.getBoundingClientRect().top})));assert.equal(boxes.length,4);assert.ok(boxes.every(b=>b.left>=0&&b.right<=361));assert.ok(new Set(boxes.map(b=>Math.round(b.top))).size>1);await page.screenshot({path:resolve(out,'four-stage-mobile.png'),fullPage:true});await page.setViewportSize({width:1440,height:1100});}
  await open();await outer('Middle').click();await paint([0,1,11,21]);await outer('Result').click();await paint([4,5,14,15]);
  await page.locator('.recovery-supplies .workspace-queue-input').nth(0).fill('[JL]');await page.locator('.recovery-supplies .workspace-queue-input').nth(1).fill('O');
  await page.getByRole('checkbox',{name:'Hold',exact:true}).uncheck();await page.getByRole('combobox',{name:'Maximum early placements',exact:true}).selectOption('0');
  const reflected=await run('initial-mirror',['2','2','0','0']);assert.equal(new Set(reflected.solutions.map(s=>s.example.middle_target_mask)).size,2);
  await open();await outer('Middle').click();await paint([0,1,2,3]);await add();await paint([4,5,6,7]);await outer('Result').click();await paint([8,9,18,19]);
  const carry=page.locator('.recovery-supplies .workspace-queue-input');for(const [i,s] of ['O','I','I'].entries())await carry.nth(i).fill(s);
  await page.getByRole('checkbox',{name:'Allow different-piece repayment',exact:true}).check();await page.getByRole('combobox',{name:'Maximum early placements',exact:true}).selectOption('0');
  const held=await run('long-hold',['1','1','0','0']);for(const s of held.solutions)assert.deepEqual(s.example.steps.map(s=>s.source_index),['1','2','0']);
  const carryFrames=await copied('long-hold',false);assert.equal(carryFrames.length,held.solutions.length*3);
  for(let i=0;i<held.solutions.length;i++){const first=carryFrames[i*3].cells.filter(x=>x);assert.deepEqual(first,['O','O','O','O']);}
  assert.deepEqual(errors,[]);await ctx.close();page=null;
 }
 summary.status='passed';
}catch(error){summary.status='failed';summary.error=String(error.stack??error);if(page){await page.screenshot({path:resolve(out,'failure.png'),fullPage:true}).catch(()=>{});await writeFile(resolve(out,'failure.txt'),await page.locator('body').innerText().catch(()=>''));}process.exitCode=1;
}finally{await save('summary.json',summary);await browser.close();await new Promise(ok=>server.close(ok));console.log(JSON.stringify(summary));}

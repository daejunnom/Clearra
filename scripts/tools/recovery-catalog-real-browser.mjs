/** Actual UI -> real workers -> accepted WASM. No mocked solver responses.
 * navigator=12 exercises the eleven-slot policy; actual CI capacity is recorded
 * separately and these functional acceptance times are not twelve-core benchmarks.
 */
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile,writeFile,mkdir,stat} from 'node:fs/promises';
import {resolve,extname,sep} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {availableParallelism,cpus,totalmem} from 'node:os';
const root=process.cwd(),site=resolve(process.env.CLEARRA_ACCEPTED_SITE),out=resolve(root,'_local/artifacts/test/recovery-catalog-browser');
const source='b781f6b6ef1c9ce27e0430f87663a92508e1cb7d';
const {chromium}=await import(pathToFileURL(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT,'node_modules/playwright/index.mjs')));
const codec=await import(pathToFileURL(resolve(root,'packages/ctk3/dist/index.js')));
await mkdir(out,{recursive:true});
const manifestBytes=await readFile(resolve(site,'wasm/clearra_wasm.manifest.json'));
assert.equal(JSON.parse(manifestBytes).build.runtime_identity.source_commit,source);
const meta={source,acceptedRunId:'36515160162',actualCpu:availableParallelism(),osLogical:cpus().length,totalmem:totalmem(),advertisedLogical:12,status:'running',cases:[]};
meta.manifestSha256=createHash('sha256').update(manifestBytes).digest('hex');
for(const name of ['cpu.max','memory.max'])meta[name]=await readFile('/sys/fs/cgroup/'+name,'utf8').catch(()=>null);
const server=createServer(async(req,res)=>{
  try{
    const pathname=decodeURIComponent(new URL(req.url,'http://127.0.0.1').pathname);
    if(pathname!='/Clearra'&&!pathname.startsWith('/Clearra/')){res.writeHead(404).end();return;}
    let path=resolve(site,pathname.slice('/Clearra'.length).replace(/^\//u,''));
    if(path!==site&&!path.startsWith(site+sep)){res.writeHead(403).end();return;}
    if(!(await stat(path).catch(()=>null))?.isFile()){
      if(extname(pathname)){res.writeHead(404).end();return;}path=resolve(site,'index.html');
    }
    const bytes=await readFile(path),mime={'.html':'text/html','.js':'text/javascript','.wasm':'application/wasm','.css':'text/css','.json':'application/json','.png':'image/png','.gif':'image/gif','.svg':'image/svg+xml','.woff2':'font/woff2'}[extname(path)]??'application/octet-stream';
    res.writeHead(200,{'content-type':mime,'content-length':bytes.length,'cache-control':'no-store'});res.end(bytes);
  }catch{res.writeHead(500).end();}
});
await new Promise((yes,no)=>{server.once('error',no);server.listen(4194,'127.0.0.1',yes);});
const browser=await chromium.launch({headless:true});meta.chromium=browser.version();
const context=await browser.newContext({locale:'en-US',viewport:{width:1440,height:1100},permissions:['clipboard-read','clipboard-write']});
await context.addInitScript(()=>{
  const s={actual:navigator.hardwareConcurrency,advertised:12,commands:[],finals:[],progress:[]};
  window.__recoveryObserved=s;
  Object.defineProperty(navigator,'hardwareConcurrency',{configurable:true,get:()=>12});
  const Original=window.Worker;
  window.Worker=class extends Original{
    constructor(...args){super(...args);this.addEventListener('message',e=>{
      const d=e.data;if(!d||typeof d!=='object')return;
      if(['final_response','failed','cancelled','terminated'].includes(d.event))s.finals.push(d);
      if(d.event==='progress'){s.progress.push(d.progress);if(s.progress.length>96)s.progress.shift();}
    });}
    postMessage(...args){if(args[0]?.type==='run_command_text'){
      const {commandText,prewarmWorkerCount,workersRequested,workersEffective}=args[0];s.commands.push({commandText,prewarmWorkerCount,workersRequested,workersEffective});
    }return super.postMessage(...args);}
  };
});
const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(String(e)));
const save=async(name,value)=>writeFile(resolve(out,name),JSON.stringify(value,null,2)+'\n');
const counts=p=>['normal_count','recovery_count','no_path_count'].map(k=>Number(p[k]));
function validate(p,expected,early){
  assert.equal(p.complete,true);assert.equal(p.solutions_complete,true);
  assert.equal(p.pattern_count,p.evaluated_pattern_count);assert.equal(p.pattern_count,'25401600');
  assert.equal(counts(p).reduce((a,b)=>a+b,0),Number(p.pattern_count));
  assert.ok(Math.abs(['normal_probability','recovery_probability','no_path_probability'].reduce((s,k)=>s+Number(p[k]),0)-1)<1e-10);
  assert.equal(new Set(p.solutions.map(s=>s.key)).size,p.solutions.length);
  if(expected)assert.deepEqual([...counts(p),p.solutions.length],expected);
  for(const s of p.solutions){
    assert.ok(Number(s.covered_count)>0&&Number(s.covered_count)<=Number(p.pattern_count));
    assert.ok(Number(s.probability)>0&&Number(s.probability)<=1);
    const e=s.example,q=e.first_queue+e.second_queue,used=new Set();let before=true,n=0;
    for(const step of e.steps){const i=Number(step.source_index);assert.ok(!used.has(i));used.add(i);assert.equal(q[i],step.piece);
      if(before&&step.result_target)n++;before&&=!step.middle_complete;}
    assert.equal(n,Number(e.actual_early));assert.ok(n<=Number(e.effective_max_early));if(early!==undefined)assert.ok(n<=early);
  }
}
async function paint(name,mask){
  await page.locator('.field-palette button').filter({hasText:name}).click();
  const cells=page.locator('.recovery-field-editor .board button');assert.equal(await cells.count(),100);
  for(let y=0;y<10;y++)for(let x=0;x<10;x++)if(mask&(1n<<BigInt(y*10+x)))await cells.nth((9-y)*10+x).click();
}
async function run(label,action,expected,early){
  const before=await page.evaluate(()=>window.__recoveryObserved.finals.length),start=performance.now();
  await action();await page.waitForFunction(n=>window.__recoveryObserved.finals.length>n,before,{timeout:600000});
  const event=await page.evaluate(()=>window.__recoveryObserved.finals.at(-1));
  await save(label+'-response.json',event);assert.equal(event.event,'final_response');assert.equal(event.response.status,'success');
  const envelope=event.response.product_result_payload;assert.equal(envelope.contract,'recovery-build.v2');const p=envelope.content.payload;
  validate(p,expected,early);
  await page.waitForFunction(()=>document.querySelector('.header-status > span')?.textContent.trim()==='Completed',null,{timeout:30000});
  assert.equal(await page.locator('.invalid-evidence,.invalid-replay').count(),0);
  const visible=p.minimum_proven?p.selected_solution_keys.length:p.solutions.length;
  assert.equal(await page.locator('.recovery-path-gallery > li').count(),Math.min(visible,100));
  await page.waitForFunction(()=>{const cards=[...document.querySelectorAll('.recovery-path-gallery>li')];return cards.length>0&&cards.every(c=>{const i=c.querySelector('img');return i&&i.complete&&i.naturalWidth>0;});},null,{timeout:60000});
  const command=await page.evaluate(()=>window.__recoveryObserved.commands.at(-1));assert.equal(command.prewarmWorkerCount,11);assert.ok(command.commandText.includes('--workers 11'));
  const report={label,elapsedToRenderMs:Math.round(performance.now()-start),solutionCount:p.solutions.length,minimumCount:p.minimum_proven?p.selected_solution_keys.length:null,counts:counts(p),command,progressTail:await page.evaluate(()=>window.__recoveryObserved.progress)};
  await save(label+'.json',report);meta.cases.push(report);console.log(JSON.stringify({label,ms:report.elapsedToRenderMs,solutions:report.solutionCount,minimum:report.minimumCount}));
  await page.screenshot({path:resolve(out,label+'.png'),fullPage:true});return p;
}
async function exports(p,label){
  async function copy(resultOnly){
    await page.getByRole('checkbox',{name:'Result only',exact:true}).setChecked(resultOnly);
    await page.evaluate(()=>navigator.clipboard.writeText(''));
    await page.locator('.solution-toolbar .copy-all').click();
    let text='';for(let i=0;i<100;i++){text=await page.evaluate(()=>navigator.clipboard.readText());if(/^ctk3[@b_]/u.test(text))break;await page.waitForTimeout(100);}
    assert.match(text,/^ctk3[@b_]/u);
    await writeFile(resolve(out,`${label}-${resultOnly?'one':'two'}-frames.ctk`),text+'\n');
    return codec.decodeCtk3Exact(text);
  }
  const two=await copy(false),one=await copy(true),selected=p.minimum_proven?new Set(p.selected_solution_keys):null;
  const solutions=p.solutions.filter(s=>!selected||selected.has(s.key));assert.equal(two.pages.length,solutions.length*2);assert.equal(one.pages.length,solutions.length);
  const colorCounts=p=>p.cells.reduce((s,c)=>{if(c)s[c]=(s[c]??0)+1;return s;},{});
  let gray=0;for(let m=BigInt(p.start_board_mask);m;m&=m-1n)gray++;
  for(let i=0;i<solutions.length;i++){
    assert.deepEqual(colorCounts(two.pages[2*i]),{...Object.fromEntries([...'IJLOSTZ'].map(p=>[p,4])),G:gray});
    assert.deepEqual(colorCounts(one.pages[i]),{...Object.fromEntries([...'IJLOSTZ'].map(p=>[p,8])),G:gray});
    assert.deepEqual(two.pages[2*i+1].cells,one.pages[i].cells);
  }
  await save(label+'-exports.json',{twoFrames:two.pages.length,resultOnly:one.pages.length,firstOwnedCells:28,allOwnedCells:56,gray});
  await page.getByRole('checkbox',{name:'Result only',exact:true}).uncheck();
}
try{
  await page.goto('http://127.0.0.1:4194/Clearra/?tool=recovery');await page.locator('.recovery-field-editor').waitFor();
  await page.locator('.dimension-field input').fill('10');
  await paint('Start',0xc0383f3fc7n);await paint('Middle',0x3ff3fc7c0c038n);await paint('Result',0x30483f07f3f8fn<<50n);
  assert.equal(await page.getByRole('combobox',{name:'Result coordinates',exact:true}).count(),0);
  assert.equal(await page.getByRole('checkbox',{name:'Show field context',exact:true}).count(),0);
  const supply=page.locator('.recovery-supplies .workspace-queue-input');await supply.nth(0).fill('P7');await supply.nth(1).fill('P7');
  for(const name of ['Hold','Preserve B2B','Allow different-piece repayment'])await page.getByRole('checkbox',{name,exact:true}).check();
  assert.equal(await page.getByRole('checkbox',{name:'PNG render',exact:true}).isChecked(),false);
  const early=page.getByRole('combobox',{name:'Maximum early placements',exact:true});
  const execute=()=>page.getByRole('button',{name:'Run search',exact:true}).click();
  const expected={'0':[20972208,0,4429392,22],'1':[20972208,696340,3733052,26],auto:[20972208,4359606,69786,53]};
  let all;
  for(const limit of ['0','1','auto']){await early.selectOption(limit);all=await run('full-'+limit,execute,expected[limit],limit==='auto'?undefined:Number(limit));}
  const allKeys=new Set(all.solutions.map(s=>s.key));
  const pins=page.locator('.mandatory-choice input');await pins.first().check();await pins.last().check();
  const keys=await page.locator('.recovery-path-gallery>li').evaluateAll(ns=>ns.filter(n=>n.querySelector('.mandatory-choice input').checked).map(n=>n.dataset.solutionKey));
  await page.getByRole('checkbox',{name:'PNG render',exact:true}).check();await page.getByRole('checkbox',{name:'Per-solution build probability',exact:true}).check();
  assert.equal(await page.locator('.mandatory-choice input:checked').count(),2);
  const pinned=await run('full-auto-pinned',()=>page.locator('.mandatory-summary button').first().click());
  assert.equal(pinned.minimum_proven,true);assert.ok(keys.every(k=>pinned.selected_solution_keys.includes(k)));
  assert.ok(pinned.selected_solution_keys.every(k=>allKeys.has(k)));assert.deepEqual(counts(pinned),counts(all));
  assert.equal(await page.locator('.solution-probability').count(),pinned.selected_solution_keys.length);
  const minimum=await run('full-auto-minimum',()=>page.locator('.solution-toolbar').getByRole('button',{name:'Minimum solutions',exact:true}).click());
  assert.equal(minimum.minimum_proven,true);assert.ok(minimum.selected_solution_keys.length<=pinned.selected_solution_keys.length);assert.deepEqual(counts(minimum),counts(all));
  await exports(minimum,'full-auto-minimum');
  await page.getByRole('checkbox',{name:'Allow different-piece repayment',exact:true}).uncheck();await early.selectOption('0');
  const same=await run('full-0-no-exchange',execute,undefined,0);assert.equal(same.allow_piece_exchange,false);assert.equal(same.recovery_count,'0');
  assert.equal(same.required_solution_keys.length,0);assert.equal(await page.locator('.mandatory-choice input:checked').count(),0);await exports(same,'full-0-no-exchange');
  assert.deepEqual(errors,[]);meta.status='passed';
}catch(error){
  meta.status='failed';meta.error=String(error.stack??error);await page.screenshot({path:resolve(out,'failure.png'),fullPage:true}).catch(()=>{});
  await writeFile(resolve(out,'failure.txt'),await page.locator('body').innerText().catch(()=>''));process.exitCode=1;
}finally{
  meta.browserErrors=errors;meta.observed=await page.evaluate(()=>({actual:window.__recoveryObserved?.actual,advertised:window.__recoveryObserved?.advertised,commands:window.__recoveryObserved?.commands})).catch(()=>null);
  await save('summary.json',meta);await context.close();await browser.close();await new Promise(yes=>server.close(yes));
  console.log('recovery_actual_browser='+meta.status);if(meta.error)console.error(meta.error);
}

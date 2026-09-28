// Full original input, current built UI and current built WASM. This is a
// finite product regression, not release authority or a sampled probability.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { cpus, totalmem } from 'node:os';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const site = resolve(root, 'apps/clearra-web/build');
const output = resolve(root, '_local/artifacts/test/recovery-full-coverage');
const source = execFileSync('git', ['rev-parse', 'HEAD'], {cwd:root, encoding:'utf8'}).trim();
assert.match(source,/^[0-9a-f]{40}$/);
assert.equal(source, process.env.GITHUB_SHA, 'test and built source must be the same commit');
assert.ok(process.env.CLEARRA_BROWSER_TOOLS_ROOT);
const { chromium, expect } = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT,'package.json'))('playwright/test');
const manifest = JSON.parse(await readFile(resolve(site,'wasm/clearra_wasm.manifest.json'),'utf8'));
assert.equal(manifest.build.runtime_identity.source_commit, source);
assert.equal(manifest.build.runtime_identity.engine_build_id, source);
for (const item of [manifest.bindings, manifest.wasm]) {
  assert.ok(!item.path.includes('/') && !item.path.includes('\\') && !item.path.includes('..'));
  const bytes=await readFile(resolve(site,'wasm',item.path));
  assert.equal(bytes.length,item.bytes);
  assert.equal(createHash('sha256').update(bytes).digest('hex'),item.sha256);
}
const fixture = JSON.parse(await readFile(resolve(root,'tests/fixtures/recovery-build/user-20260928.json'),'utf8'));
assert.equal(fixture.result_mask,'0xc120fc1fcfe3c000000000000');
assert.equal(fixture.result_frame,'shared');
assert.equal(fixture.first_supply,'P7');assert.equal(fixture.second_supply,'P7');
assert.equal(fixture.expected_pair_count,'25401600');
await mkdir(output,{recursive:true});
const server=createServer(async(req,res)=>{
  try {
    const path=decodeURIComponent(new URL(req.url,'http://127.0.0.1').pathname);
    if(!path.startsWith('/Clearra/')||path.split('/').includes('..')) return res.writeHead(404).end();
    const file=resolve(site,path.slice('/Clearra/'.length)||'index.html');
    if(!file.startsWith(site+sep)) return res.writeHead(400).end();
    const body=await readFile(file);
    const mime={'.html':'text/html; charset=utf-8','.js':'text/javascript','.json':'application/json','.wasm':'application/wasm','.css':'text/css'};
    res.writeHead(200,{'Content-Type':mime[extname(file)]||'application/octet-stream','Cache-Control':'no-store'}).end(body);
  }catch{res.writeHead(404).end();}
});
await new Promise((ok,fail)=>{server.once('error',fail);server.listen(4194,'127.0.0.1',ok);});
const report={source,fixture:fixture.id,status:'running',full_original_fixture:true,
  actual_logical_processors:cpus().length,actual_memory_bytes:totalmem(),reported_logical_processors:4,
  solver_timeout_ms:720000,wasm:manifest.wasm,commands:[]};
let browser,context,page;
const errors=[];
let started;
try{
  browser=await chromium.launch({headless:true});
  report.browser=browser.version();
  context=await browser.newContext({viewport:{width:1440,height:1100},locale:'en-US'});
  await context.addInitScript(()=>{
    Object.defineProperty(navigator,'hardwareConcurrency',{get:()=>4});
    window.recoveryCommands=[];window.recoveryFinals=[];
    const Original=window.Worker;
    window.Worker=class extends Original {
      constructor(...args){super(...args);this.addEventListener('message',event=>{if(event.data?.event==='final_response')window.recoveryFinals.push(event.data);});}
      postMessage(...args){if(args[0]?.type==='run_command_text')window.recoveryCommands.push(args[0]);return super.postMessage(...args);}
    };
  });
  page=await context.newPage();page.setDefaultTimeout(20000);
  page.on('pageerror',e=>errors.push(String(e)));
  await page.goto('http://127.0.0.1:4194/Clearra/?tool=recovery');
  await page.locator('.field-palette button').first().waitFor();
  await page.locator('.dimension-field input').fill(String(fixture.height));
  const fields=page.locator('.field-palette button');
  const masks=[fixture.start_mask,fixture.middle_mask,fixture.result_mask];
  for(let f=0;f<3;f++){
    await fields.nth(f).click();
    const cells=page.locator('.recovery-field-editor .board button');
    await expect(cells).toHaveCount(10*fixture.height);
    for(let y=0;y<fixture.height;y++)for(let x=0;x<10;x++){
      if(BigInt(masks[f])&(1n<<BigInt(y*10+x))) await cells.nth((fixture.height-1-y)*10+x).click();
    }
  }
  const drawn=()=>page.locator('.recovery-field-editor .board button').evaluateAll((cells,h)=>{
    let mask=0n;cells.forEach((c,i)=>{if(c.classList.contains('existing'))mask|=1n<<BigInt((h-1-Math.floor(i/10))*10+i%10);});return mask.toString();
  },fixture.height);
  assert.equal(await drawn(),BigInt(fixture.result_mask).toString());
  await expect(page.getByRole('combobox',{name:'Result coordinates',exact:true})).toHaveValue('shared');
  const supplies=page.locator('.recovery-supplies .workspace-queue-input');
  await supplies.nth(0).fill(fixture.first_supply);await supplies.nth(1).fill(fixture.second_supply);
  await page.getByRole('checkbox',{name:'Hold',exact:true}).setChecked(fixture.hold_enabled);
  await page.getByRole('checkbox',{name:'Allow different-piece repayment',exact:true}).setChecked(fixture.allow_piece_exchange);
  await page.getByRole('checkbox',{name:'Preserve B2B',exact:true}).setChecked(fixture.preserve_b2b);
  await expect(page.getByRole('combobox',{name:'Maximum early placements',exact:true})).toHaveValue(fixture.max_early);
  await expect(page.locator('.run-actions button.run')).toBeEnabled();
  started=performance.now();
  await page.locator('.run-actions button.run').click();
  await page.locator('.recovery-metrics').waitFor({timeout:report.solver_timeout_ms});
  report.elapsed_ms=performance.now()-started;
  const finals=await page.evaluate(()=>window.recoveryFinals);
  assert.equal(finals.length,1);
  const response=finals[0].response;
  assert.equal(response.status,'success');
  const payload=response.product_result_payload.content.payload;
  assert.equal(payload.complete,true);
  assert.equal(payload.pattern_count,fixture.expected_pair_count);
  assert.equal(payload.evaluated_pattern_count,payload.pattern_count);
  assert.equal(BigInt(payload.normal_count)+BigInt(payload.recovery_count)+BigInt(payload.no_path_count),BigInt(payload.pattern_count));
  assert.ok(BigInt(payload.normal_count)>0n,'known normal paths must be covered');
  assert.ok(BigInt(payload.recovery_count)>0n,'known recovery paths must be covered');
  assert.ok(payload.examples.length>0);
  assert.equal(await page.locator('.invalid-evidence').count(),0);
  report.commands=await page.evaluate(()=>window.recoveryCommands);
  assert.equal(report.commands.length,1);
  const command=report.commands[0].commandText;
  for(const [name,value] of [['first-supply','P7'],['second-supply','P7'],['max-early','auto'],['rule','srs-plus'],['spin-profile','all-spin-plus'],['workers','3']]){
    assert.equal(new RegExp(`--${name}\\s+(\\S+)`).exec(command)?.[1],value);
  }
  assert.match(command,/--allow-piece-exchange\b/);assert.match(command,/--hold\b/);assert.match(command,/--preserve-b2b\b/);
  assert.equal(BigInt(/--result-mask\s+(\S+)/.exec(command)?.[1]),0x30483f07f3f8fn);
  assert.equal(BigInt(payload.result_target_mask),0x30483f07f3f8fn);
  assert.equal(await drawn(),BigInt(fixture.result_mask).toString());
  assert.deepEqual(errors,[]);
  Object.assign(report,{status:'passed',payload,metrics:await page.locator('.recovery-metrics').innerText()});
  console.log(JSON.stringify({source,status:report.status,elapsed_ms:report.elapsed_ms,normal:payload.normal_count,recovery:payload.recovery_count,no_path:payload.no_path_count,total:payload.pattern_count}));
}catch(error){
  Object.assign(report,{status:'failed',error:String(error),elapsed_ms:started?performance.now()-started:null,errors,
    body:page?await page.locator('body').innerText().catch(()=>''):'',
    commands:page?await page.evaluate(()=>window.recoveryCommands).catch(()=>[]):[]});
  throw error;
}finally{
  await page?.screenshot({path:resolve(output,'original-full-P7-P7.png'),fullPage:true}).catch(()=>{});
  await writeFile(resolve(output,'result.json'),JSON.stringify(report,null,2)+'\n');
  await context?.close();await browser?.close();await new Promise(ok=>server.close(ok));
}

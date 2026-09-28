import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const compiled=await build({stdin:{contents:`export * from './packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';
export * from './packages/clearra-ui/src/lib/workspace/recoveryBuildPresentation.ts';
export { buildPcPathReplayFrames } from './packages/clearra-ui/src/lib/workspace/pcPathReplayPresentation.ts';
export { buildWorkspaceProgressModel } from './packages/clearra-ui/src/lib/workspace/workspaceProgressModel.ts';
export { encodeSolutionPages } from './packages/clearra-ui/src/lib/workspace/solutionExport.ts';`,resolveDir:root},bundle:true,write:false,platform:'node',format:'esm',target:'node22'});
const api=await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].text+'\n//# sourceURL=recovery-build-test-bundle.mjs').toString('base64')}`);
const fixture=JSON.parse(await readFile(new URL('../../../tests/fixtures/recovery-build/browser-example.json',import.meta.url),'utf8'));

test('two independent supply inputs preserve canonical expressions and explicit exchange policy',()=>{
 const input={...api.createRecoveryBuildRequest(),firstSupply:'[IJ]!',secondSupply:'P7',middleMask:15n,resultMask:0xc030n};
 assert.deepEqual(api.validateRecoveryBuildRequest(input),[]);
 for(const allow of [false,true]){
  const args=api.recoveryBuildArguments({...input,allowPieceExchange:allow});
  assert.deepEqual(args.slice(0,3),['clearra','recovery','build']);
  assert.equal(args[args.indexOf('--first-supply')+1],'[IJ]!');
  assert.equal(args[args.indexOf('--second-supply')+1],'P7');
  assert.ok(args.includes(allow?'--allow-piece-exchange':'--no-piece-exchange'));
  for(const gone of ['--queue','--stage-one-count','--placements','--role-mask','--queue-pattern','--max-states','--max-pattern-evaluations'])assert.ok(!args.includes(gone));
 }
});
test('automatic early options derive from distinct target cells, not P7 text length',()=>{
 const input={...api.createRecoveryBuildRequest(),firstSupply:'P7',secondSupply:'P7',resultMask:BigInt(fixture.result_target_mask)};
 assert.deepEqual(api.recoveryEarlyChoices(input),[0,1,2,3,4,5,6,7]);
 for(const n of ['auto',0,2,7])assert.equal(api.recoveryBuildArguments({...input,maxEarly:n})[16],String(n));
});
test('blank height drafts never erase independent gray fields',()=>{
 const input={...api.createRecoveryBuildRequest(),startMask:1n<<70n,middleMask:2n,resultMask:4n};
 for(const invalid of [NaN,0,25,-1,1.5])assert.equal(api.resizeRecoveryBuild(input,invalid),input);
 const grown=api.resizeRecoveryBuild(input,12);assert.equal(grown.startMask,input.startMask);
 const smaller=api.resizeRecoveryBuild(input,4);assert.equal(smaller.startMask,0n);assert.equal(smaller.middleMask,2n);assert.equal(smaller.resultMask,4n);
});
test('legacy after-middle image target uses the emptied board',()=>{
 const input={...api.createRecoveryBuildRequest(),startMask:BigInt(fixture.start_board_mask),middleMask:BigInt(fixture.middle_target_mask),resultMask:BigInt(fixture.result_target_mask),resultFrame:'after-middle',firstSupply:'P7',secondSupply:'P7'};
 assert.equal(api.countRecoveryCells(input.startMask),22);assert.equal(api.countRecoveryCells(input.middleMask),28);assert.equal(api.countRecoveryCells(input.resultMask),28);
 assert.equal(api.recoveryMiddleBase(input),0n);assert.deepEqual(api.validateRecoveryBuildRequest(input),[]);
});
test('the independently constructed 14-lock image witness reuses the existing replay renderer',()=>{
 assert.equal(api.validateRecoveryBuildPayload(fixture),true);
 const witness=api.recoveryBuildWitness(fixture,fixture.examples[0]);
 const frames=api.buildPcPathReplayFrames(witness,8,api.recoveryBuildTerminalMask(fixture.examples[0]));
 assert.equal(frames.filter(f=>f.phase==='lock').length,14);
 assert.equal(frames.filter(f=>f.phase==='after-clear').length,4);
 assert.equal(frames.at(-1).cells.filter(c=>c!==null).length,28);
});
test('different kinds repay only with explicit opt-in; physical supply identity cannot be forged',()=>{
 let changed=structuredClone(fixture);changed.allow_piece_exchange=false;assert.equal(api.validateRecoveryBuildPayload(changed),false);
 changed=structuredClone(fixture);changed.examples[0].steps[1].source_index='0';assert.equal(api.validateRecoveryBuildPayload(changed),false);
 changed=structuredClone(fixture);changed.examples[0].steps[6].piece='I';assert.equal(api.validateRecoveryBuildPayload(changed),false);
 changed=structuredClone(fixture);changed.examples[0].exchange_balance=[0,0,0,0,0,0,0];assert.equal(api.validateRecoveryBuildPayload(changed),false);
});
test('missing clears, wrong target ownership and fake exhaustive counts are rejected',()=>{
 for(const mutate of [p=>p.examples[0].steps[1].cleared_rows=0,p=>p.examples[0].steps[6].result_target=false,
  p=>p.examples[0].steps[11].board_after_mask='0x0',p=>p.normal_count='1',p=>p.all_paths_enumerated=true,p=>p.complete=false]){
  const p=structuredClone(fixture);mutate(p);assert.equal(api.validateRecoveryBuildPayload(p),false);
 }
});
test('copy exports two cumulative checkpoints, or all 14 placements on one result page',()=>{
 const pages=api.recoveryBuildExportPages(fixture);assert.equal(pages.length,2);
 assert.equal(pages[0].initialMask,BigInt(fixture.start_board_mask));
 assert.equal(pages[0].placements.length,7);assert.equal(pages[1].placements.length,14);
 assert.deepEqual(pages[1].placements.slice(0,7),pages[0].placements);
 const result=api.recoveryBuildExportPages(fixture,true);assert.deepEqual(result,[pages[1]]);
 for(const page of pages) {
  let occupied=page.initialMask;
  for(const placement of page.placements){assert.equal(placement.mask&occupied,0n);occupied|=placement.mask;}
 }
 assert.match(api.encodeSolutionPages(pages,'ctk'),/^ctk3_/);
 assert.match(api.encodeSolutionPages(pages,'fumen'),/v115@/);
});

test('early count includes every result placement before middle completion, regardless of hold source',()=>{
 assert.equal(api.validateRecoveryBuildPayload(fixture),true);
 const early=structuredClone(fixture);early.early_limit='1';early.examples[0].effective_max_early='1';early.examples[0].actual_early='1';
 assert.equal(api.validateRecoveryBuildPayload(early),false);
});



test('paired Build progress reports actual pair work rather than phantom geometry phases',()=>{
 const common={profile:'recovery-build',status:'running',progressLabel:'recovery-build',progressDone:256,progressTotal:5040,telemetry:null};
 const value=api.buildWorkspaceProgressModel(common);
 assert.deepEqual(value.stages.map(s=>s.id),['prepare','verify','finalize']);
 assert.deepEqual(value.stages.map(s=>s.status),['complete','running','pending']);
 assert.equal(value.stages[1].done,'256');assert.equal(value.stages[1].total,'5040');
 assert.ok(api.buildWorkspaceProgressModel({...common,status:'completed'}).stages.every(s=>s.status==='complete'));
});

test('explicit 256-bit recovery replay preserves high cells while default PC masks stay strict',()=>{
 const p=structuredClone(fixture);p.height=12;p.first_supply='O';p.second_supply='O';p.allow_piece_exchange=false;
 const start=Array.from({length:8},(_,y)=>3n<<BigInt(10*y)).reduce((a,b)=>a|b,0n), middle=0xc03n<<80n, result=0xc030n;
 const h=n=>'0x'+n.toString(16);
 p.start_board_mask=h(start);p.middle_target_mask=h(middle);p.result_target_mask=h(result);
 p.normal_count='1';p.recovery_count='0';p.normal_probability='1';p.recovery_probability='0';
 const step=(index,before,lock,second)=>({source_index:String(index),piece:'O',result_target:second,rotation:0,x:second?4:0,y:second?0:8,
  hold_decision:'none',board_before_mask:h(before),placement_mask:h(lock),board_after_mask:h(before|lock),
  cleared_rows:0,cleared_lines:0,recognized_spin:false,b2b_active:true,middle_complete:true});
 p.examples=[{first_pattern:'0',second_pattern:'0',first_queue:'O',second_queue:'O',status:'normal',
  effective_max_early:'1',actual_early:'0',exchange_balance:[0,0,0,0,0,0,0],terminal_board_mask:h(start|middle|result),
  steps:[step(0,start,middle,false),step(1,start|middle,result,true)]}];
 assert.ok(api.validateRecoveryBuildPayload(p));
 const witness=api.recoveryBuildWitness(p,p.examples[0]);
 assert.equal(api.buildPcPathReplayFrames(witness,12,api.recoveryBuildTerminalMask(p.examples[0])).at(-1).cells.filter(x=>x!==null).length,24);
 assert.throws(()=>api.buildPcPathReplayFrames({...witness,maskHexDigits:16},12,api.recoveryBuildTerminalMask(p.examples[0])));
});

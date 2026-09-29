import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const compiled=await build({stdin:{contents:`export * from './packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';
export * from './packages/clearra-ui/src/lib/workspace/recoveryFieldReferences.ts';`,resolveDir:root},bundle:true,write:false,platform:'node',format:'esm',target:'node22'});
const api=await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].text).toString('base64')}`);
const fixture=JSON.parse(await readFile(new URL('../../../tests/fixtures/recovery-build/user-20260928.json',import.meta.url),'utf8'));
const request={...api.createRecoveryBuildRequest(),height:fixture.height,startMask:BigInt(fixture.start_mask),middleMask:BigInt(fixture.middle_mask),resultMask:BigInt(fixture.result_mask),
 firstSupply:fixture.first_supply,secondSupply:fixture.second_supply,maxEarly:fixture.max_early,holdEnabled:fixture.hold_enabled,preserveB2B:fixture.preserve_b2b,
 allowPieceExchange:fixture.allow_piece_exchange,rule:fixture.rule,spinProfile:fixture.spin_profile};
test('user screenshot result selection preserves original start and middle, including cleared rows',()=>{
 assert.equal(api.recoveryMiddleBase(request),0n);
 assert.deepEqual(api.validateRecoveryBuildRequest(request),[]);
 const before={...request};
 for(const field of ['startMask','middleMask','resultMask','middleMask','resultMask','startMask']){
  const refs=api.recoveryFieldReferences(request,field,true);
  assert.ok(refs.every(ref=>ref.field!==field));
  assert.equal(refs.length,2);
  if(field!=='resultMask') assert.equal(refs.find(r=>r.field==='resultMask').mask,request.resultMask);
  if(field==='resultMask'){
   assert.deepEqual(refs.map(ref=>[ref.field,ref.mask]),[['startMask',request.startMask],['middleMask',request.middleMask]]);
   assert.equal(api.countRecoveryCells(refs[0].mask),22);assert.equal(api.countRecoveryCells(refs[1].mask),28);
  }
  assert.deepEqual(request,before);
 }
 assert.equal(api.recoveryFieldReferences(request,'resultMask',false).length,2,'legacy false cannot hide context');
 const restored=api.recoveryFieldReferences(request,'resultMask',true);assert.equal(restored.length,2);
});
test('literal screenshot coordinates and complete product are not shifted or sampled',()=>{
 function mask(rows){let out=0n;[...rows].reverse().forEach((row,y)=>[...row].forEach((c,x)=>{if(c==='1')out|=1n<<BigInt(10*y+x);}));return out;}
 assert.equal(mask(fixture.rows_top_down.start),request.startMask);assert.equal(mask(fixture.rows_top_down.middle),request.middleMask);
 assert.equal(mask(fixture.rows_top_down.result),request.resultMask);assert.equal(request.resultMask&((1n<<50n)-1n),0n);
 assert.equal(fixture.expected_pair_count,String(5040*5040));
 const args=api.recoveryBuildArguments(request,11);
 for(const [key,value] of [['--first-supply','P7'],['--second-supply','P7'],['--max-early','auto'],['--workers','11'],['--rule','srs-plus'],['--spin-profile','all-spin-plus']])assert.equal(args[args.indexOf(key)+1],value);
 assert.ok(args.includes('--hold'));assert.ok(args.includes('--preserve-b2b'));assert.ok(args.includes('--allow-piece-exchange'));
 assert.deepEqual(api.recoveryBuildDesktopRequest(request,'ko',11).arguments,args);
 for(const count of [0,NaN,Infinity,65536,1.5])assert.throws(()=>api.recoveryBuildArguments(request,count));
});
test('modified recovery components compile without warnings',async()=>{
 for(const name of ['RecoveryBuildFields.svelte','RecoveryBuildControls.svelte','BoundaryRecoveryWorkspace.svelte']){
  const url=new URL('../src/lib/workspace/'+name,import.meta.url),filename=fileURLToPath(url);
  const processed=await preprocess(await readFile(url,'utf8'),vitePreprocess(),{filename});
  assert.deepEqual(compile(processed.code,{filename,generate:'client'}).warnings,[],name);
 }
});

test('all-processor opt-in reaches both browser and desktop command paths',()=>{
 const full={...request,useAllLogicalProcessors:true};
 const args=api.recoveryBuildArguments(full,12);
 assert.ok(args.includes('--use-all-cpu-threads'));
 assert.equal(args[args.indexOf('--workers')+1],'12');
 assert.deepEqual(api.recoveryBuildDesktopRequest(full,'ko',12).arguments,args);
 assert.ok(!api.recoveryBuildArguments(request,11).includes('--use-all-cpu-threads'));
});


test('painting and imports transfer cell ownership instead of overlapping layers',()=>{
 let current=request;
 for(const field of ['startMask','middleMask','resultMask','startMask']) {
  const before={...current};
  current=api.overwriteRecoveryField(current,field,current[field]|(1n<<9n));
  assert.ok(current[field]&(1n<<9n));
  for(const other of ['startMask','middleMask','resultMask']) if(other!==field) assert.equal(current[other]&(1n<<9n),0n);
  assert.equal(current.startMask&current.middleMask,0n);
  assert.equal(current.resultMask&(current.startMask|current.middleMask),0n);
  assert.deepEqual(current.firstSupply,before.firstSupply);
 }
 const erased=api.overwriteRecoveryField(current,'startMask',current.startMask&~(1n<<9n));
 assert.equal((erased.startMask|erased.middleMask|erased.resultMask)&(1n<<9n),0n,'erase does not resurrect overwritten cells');
 assert.throws(()=>api.overwriteRecoveryField(current,'resultMask',1n<<240n,24));
});

import assert from 'node:assert/strict';
import test from 'node:test';
import {fileURLToPath} from 'node:url';
import {build} from 'esbuild';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const result=await build({stdin:{contents:`export * from './packages/clearra-ui/src/lib/workspace/recoveryStageModel.ts';
export * from './packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';
export * from './packages/clearra-ui/src/lib/workspace/recoveryFieldReferences.ts';`,resolveDir:root},bundle:true,write:false,format:'esm',platform:'node',target:'node22'});
const api=await import('data:text/javascript;base64,'+Buffer.from(result.outputFiles[0].text).toString('base64'));
const base=()=>({...api.createRecoveryBuildRequest(),height:12,startMask:0n,middleMask:0xfn,resultMask:0xfn<<80n,firstSupply:'I',secondSupply:'T'});
function chain(count=4){
 let q=base();for(let n=1;n<count;n++)q=api.appendRecoveryMiddle(q).request;
 return api.withRecoveryMiddleStages(q,api.recoveryMiddleStages(q).map((s,i)=>({...s,mask:0xfn<<BigInt(i*10),supply:['I','J','L','O','S','T','Z'][i%7]})));
}
const selection=(q,index,field='middleMask')=>({field,middleId:api.recoveryMiddleStages(q)[index].id});

test('plus inserts an empty middle before Result and preserves destination-owned supplies',()=>{
 const q=base();let current=q;
 for(let count=2;count<=5;count++){
  const prior=current,before=structuredClone(current), added=api.appendRecoveryMiddle(current);current=added.request;
  const stages=api.recoveryMiddleStages(current);
  assert.equal(stages.length,count);assert.deepEqual(stages.slice(0,-1),api.recoveryMiddleStages(before));
  assert.equal(stages.at(-1).mask,0n);assert.equal(stages.at(-1).supply,'');
  assert.deepEqual(added.selection,{field:'middleMask',middleId:stages.at(-1).id});
  assert.equal(current.resultMask,q.resultMask);assert.equal(current.secondSupply,q.secondSupply);
  assert.deepEqual(prior,before,'each prior stage list remains unchanged');
 }
 assert.equal(q.middleStages,undefined,'input object is immutable');
});
test('minus literally requires at least four middle fields, not four total fields',()=>{
 for(let count=1;count<=6;count++){
  const q=chain(count);assert.equal(api.recoveryCanRemoveMiddle(q),count>=4);
  if(count<4)assert.throws(()=>api.removeRecoveryMiddle(q,selection(q,0)),/four middle/);
  else assert.equal(api.recoveryMiddleStages(api.removeRecoveryMiddle(q,selection(q,0)).request).length,count-1);
 }
});
test('every empty middle position is removed with its own supply, never the final field',()=>{
 for(let empty=0;empty<4;empty++){
  let q=chain();const stages=api.recoveryMiddleStages(q);stages[empty].mask=0n;
  q=api.withRecoveryMiddleStages(q,stages);const before=structuredClone(q);
  const next=api.removeRecoveryMiddle(q,selection(q,empty));
  assert.deepEqual(api.recoveryMiddleStages(next.request),stages.filter((_,i)=>i!==empty));
  assert.equal(next.request.resultMask,q.resultMask);assert.equal(next.request.secondSupply,q.secondSupply);
  assert.ok(api.recoveryMiddleStages(next.request).some(s=>s.id===next.selection.middleId));
  assert.equal(next.request.firstSupply,stages[empty===0?1:0].supply);
  assert.deepEqual(q,before);
 }
});
test('selected empty middle wins; otherwise the rightmost empty middle is deterministic',()=>{
 let q=chain(5),stages=api.recoveryMiddleStages(q);stages[1].mask=0n;stages[3].mask=0n;q=api.withRecoveryMiddleStages(q,stages);
 const a=api.removeRecoveryMiddle(q,selection(q,1)),b=api.removeRecoveryMiddle(q,selection(q,0,'startMask'));
 assert.ok(!api.recoveryMiddleStages(a.request).some(s=>s.id===stages[1].id));
 assert.ok(!api.recoveryMiddleStages(b.request).some(s=>s.id===stages[3].id));
});
test('without an empty middle, the last middle AND its supply become Result',()=>{
 const q=chain(),before=structuredClone(q),stages=api.recoveryMiddleStages(q);
 const next=api.removeRecoveryMiddle(q,selection(q,3));
 assert.equal(next.request.resultMask,stages[3].mask);assert.equal(next.request.secondSupply,stages[3].supply);
 assert.deepEqual(api.recoveryMiddleStages(next.request),stages.slice(0,-1));
 assert.equal(next.selection.field,'resultMask');assert.deepEqual(q,before);
});
test('promoting the last middle preserves an active Start or Result outer tab',()=>{
 for(const field of ['startMask','resultMask']){
  const q=chain(),next=api.removeRecoveryMiddle(q,selection(q,3,field));
  assert.equal(next.selection.field,field);
  assert.ok(api.recoveryMiddleStages(next.request).some(s=>s.id===next.selection.middleId));
 }
});
test('structural undo snapshots restore fields, destination supplies and IDs without reverting display settings',()=>{
 const q=chain(),saved=api.captureRecoveryEditor(q);
 const shrunk=api.removeRecoveryMiddle(q,selection(q,3)).request;
 const restored=api.restoreRecoveryEditor({...shrunk,pngRender:true,preserveB2B:true},saved);
 assert.deepEqual(api.captureRecoveryEditor(restored),saved);
 assert.equal(restored.pngRender,true);assert.equal(restored.preserveB2B,true);
 saved.middleStages[0].mask=0n;
 assert.equal(q.middleMask,0xfn);assert.equal(restored.middleMask,0xfn,'snapshots do not share mutable stage arrays');
});
test('painting transfers ownership across every layer; erase does not revive an old owner',()=>{
 let q=chain(5);const bit=1n<<100n;
 for(const selected of ['startMask',...api.recoveryMiddleStages(q).map(s=>s.id),'resultMask']){
  const field=selected.startsWith('middle-')?'middleMask':selected;
  const stages=api.recoveryMiddleStages(q),old=field==='middleMask'?stages.find(s=>s.id===selected).mask:q[field];
  q=api.overwriteRecoveryField(q,field,old|bit,12,selected);
  const masks=[q.startMask,...api.recoveryMiddleStages(q).map(s=>s.mask),q.resultMask];
  assert.equal(masks.filter(mask=>(mask&bit)!==0n).length,1);
  for(let i=0;i<masks.length;i++)for(let j=i+1;j<masks.length;j++)assert.equal(masks[i]&masks[j],0n);
 }
 q=api.overwriteRecoveryField(q,'resultMask',q.resultMask&~bit);
 assert.equal([q.startMask,...api.recoveryMiddleStages(q).map(s=>s.mask),q.resultMask].reduce((u,m)=>u|m,0n)&bit,0n);
});
test('inactive middle colors match and hatch parity uses original adjacency, even across a selected gap',()=>{
 const q=chain(5),stages=api.recoveryMiddleStages(q);
 for(let selected=0;selected<5;selected++){
  const refs=api.recoveryFieldReferences(q,'middleMask',true,stages[selected].id).filter(r=>r.field==='middleMask');
  assert.equal(refs.length,4);assert.ok(refs.every(r=>r.tone==='medium'));
  for(const ref of refs)assert.equal(ref.hatch,ref.middleIndex%2?'backward':'forward');
 }
 const refs=api.recoveryFieldReferences(q,'middleMask',true,stages[1].id).filter(r=>r.field==='middleMask');
 assert.equal(refs[0].hatch,refs[1].hatch,'non-neighbouring stages 1 and 3 may look alike');
});
test('stage source edits bind to stable destination IDs and cannot accidentally modify a neighbouring input',()=>{
 let q=chain(),stages=api.recoveryMiddleStages(q);
 q=api.setRecoveryStageSupply(q,stages[2].id,'P7P1');
 assert.deepEqual(api.recoveryMiddleStages(q).map(s=>s.supply),['I','J','P7P1','O']);
 q=api.setRecoveryStageSupply(q,stages[0].id,'[IJ]!');assert.equal(q.firstSupply,'[IJ]!');
 q=api.setRecoveryStageSupply(q,'result','P7');assert.equal(q.secondSupply,'P7');
 assert.throws(()=>api.setRecoveryStageSupply(q,'missing','I'));
});
test('nominal stage frames remove only earlier completed logical rows, never future rows or intentional gaps',()=>{
 let q=base();q.startMask=0x3f0n;q.resultMask=0xc030n<<30n;
 q=api.withRecoveryMiddleStages(q,[{id:'middle-0',mask:0xfn,supply:'I'},
  {id:'middle-1',mask:0xfn<<10n,supply:'I'},
  {id:'middle-2',mask:(0x3f0n<<10n)|(3n<<20n),supply:'IO'}]);
 const frames=api.recoveryStageFrames(q);
 assert.deepEqual(frames.map(f=>f.removedLogicalRows),[[],[0],[0],[0,1]]);
 assert.equal(frames[1].targetAfterPreviousClears,0xfn);
 assert.equal(frames.at(-1).targetAfterPreviousClears,0xc030n<<10n,'one empty physical row remains');
 let union=q.startMask;
 for(const f of frames){
  let mapped=0n;
  for(let y=0;y<q.height;y++)for(let x=0;x<10;x++)if(f.logicalTarget&(1n<<BigInt(y*10+x))){
   const removed=Array.from({length:y},(_,r)=>r).filter(r=>((union>>BigInt(r*10))&1023n)===1023n).length;
   mapped|=1n<<BigInt((y-removed)*10+x);
  }
  assert.equal(f.targetAfterPreviousClears,mapped);union|=f.logicalTarget;
 }
});
test('legacy after-clear Result converts once on expansion, and shared field edits remain unchanged',()=>{
 const q={...base(),startMask:0x3f0n,middleMask:0xfn,resultMask:0xc030n,resultFrame:'after-middle'};
 const added=api.appendRecoveryMiddle(q).request;
 assert.equal(added.resultMask,0x300c000n);assert.equal(added.resultFrame,'shared');
 assert.equal(added.firstSupply,q.firstSupply);assert.equal(q.resultMask,0xc030n);
});
test('multi-stage commands carry every target and source without falling through to the paired solver',()=>{
 const q=chain(),baseKey=api.recoveryBuildInputKey(base());
 assert.deepEqual(api.validateRecoveryBuildRequest(q),[]);
 const args=api.recoveryBuildArguments(q,11), masks=args.flatMap((v,i)=>v==='--stage-mask'?[BigInt(args[i+1])]:[]);
 const supplies=args.flatMap((v,i)=>v==='--stage-supply'?[args[i+1]]:[]);
 assert.deepEqual(masks,[...q.middleStages.map(s=>s.mask),q.resultMask]);
 assert.deepEqual(supplies,[...q.middleStages.map(s=>s.supply),q.secondSupply]);
 assert.ok(!args.includes('--middle-mask')&&!args.includes('--first-supply'));
 assert.deepEqual(api.recoveryBuildDesktopRequest(q,'ko',11).arguments,args);
 const incomplete=api.appendRecoveryMiddle(q).request;
 assert.ok(api.validateRecoveryBuildRequest(incomplete).includes('supply'));
 assert.ok(api.validateRecoveryBuildRequest(incomplete).includes('target-area'));
 assert.notEqual(api.recoveryBuildInputKey(q),baseKey);
 assert.notEqual(api.recoveryBuildInputKey(q),api.recoveryBuildInputKey(api.setRecoveryStageSupply(q,q.middleStages[1].id,'P7')));
 assert.equal(api.recoveryBuildInputKey(q),api.recoveryBuildInputKey({...q,pngRender:true,minimumSolutions:true}));
});
test('invalid stage lists and identities are rejected rather than truncated or merged',()=>{
 const q=chain();
 for(const bad of [{...q,middleStages:[]},{...q,firstSupply:'wrong'},
  {...q,middleStages:q.middleStages.map(()=>q.middleStages[0])}])assert.throws(()=>api.recoveryMiddleStages(bad));
 let large=base();for(let i=1;i<api.MAX_RECOVERY_MIDDLES;i++)large=api.appendRecoveryMiddle(large).request;
 assert.equal(api.recoveryMiddleStages(large).length,59);assert.throws(()=>api.appendRecoveryMiddle(large));
});

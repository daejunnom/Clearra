import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const result=await build({stdin:{contents:`export * from './packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';export * from './packages/clearra-ui/src/lib/workspace/recoveryStages.ts';export * from './packages/clearra-ui/src/lib/workspace/recoveryFieldReferences.ts';`,resolveDir:root},bundle:true,write:false,platform:'node',format:'esm'});
const api=await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);
const base=()=>({...api.createRecoveryBuildRequest(),height:12,middleMask:0xc03n,firstSupply:'O',resultMask:0xc03000000000000000n,secondSupply:'O'});
test('middle removal remains enabled until only the required middle remains',()=>{
 let q=base();
 assert.equal(api.canRemoveRecoveryMiddle(q),false);
 assert.equal(api.removeRecoveryMiddle(q),q);
 for(let count=1;count<5;count++){
  q=api.addRecoveryMiddle(q);
  assert.equal(q.resultMask,base().resultMask);
  assert.equal(q.secondSupply,base().secondSupply);
 }
 for(let count=5;count>1;count--){
  assert.equal(api.recoveryMiddleCount(q),count);
  assert.equal(api.canRemoveRecoveryMiddle(q),true);
  const next=api.removeRecoveryMiddle(q);
  assert.equal(api.recoveryMiddleCount(next),count-1);
  assert.equal(next.resultMask,q.resultMask);
  assert.equal(next.secondSupply,q.secondSupply);
  assert.deepEqual(next.extraMiddles,q.extraMiddles.slice(1),'earliest empty middle removed');
  q=next;
 }
 assert.equal(api.recoveryMiddleCount(q),1);
 assert.equal(api.canRemoveRecoveryMiddle(q),false);
 assert.equal(api.removeRecoveryMiddle(q),q);
 assert.equal(q.middleMask,base().middleMask);
 assert.equal(q.firstSupply,base().firstSupply);
});
test('nonempty removal promotes last middle WITH its supply and discards only old result',()=>{
 let q=base();for(let i=1;i<4;i++)q=api.addRecoveryMiddle(q);
 for(const [index,stage]of api.recoveryStageEntries(q).slice(0,-1).entries()) {q=api.overwriteRecoveryField(q,stage.field,0xc03n<<BigInt(index*20));q=api.updateRecoverySupply(q,stage.field,`O${index}`);}
 for(let count=4;count>1;count--){
  const last=api.recoveryStageEntries(q).at(-2), next=api.removeRecoveryMiddle(q);
  assert.equal(next.resultMask,last.mask);
  assert.equal(next.secondSupply,last.supply);
  assert.equal(next.firstSupply,'O0');
  assert.equal(api.recoveryMiddleCount(next),count-1);
  q=next;
 }
 assert.equal(api.canRemoveRecoveryMiddle(q),false);
 assert.equal(api.removeRecoveryMiddle(q),q);
});
test('shared layers overwrite, source args retain stage identity, adjacent hatch alternates',()=>{
 let q=base();for(let i=0;i<2;i++)q=api.addRecoveryMiddle(q);
 for(const [index,stage]of api.recoveryStageEntries(q).entries()){q=api.overwriteRecoveryField(q,stage.field,0xc03n<<BigInt(index*20));q=api.updateRecoverySupply(q,stage.field,'O');}
 assert.deepEqual(api.validateRecoveryBuildRequest(q),[]);
 const args=api.recoveryBuildArguments(q,11);assert.equal(args.filter(x=>x==='--stage-target').length,4);assert.equal(args.filter(x=>x==='--stage-supply').length,4);assert.ok(!args.includes('--middle-mask'));
 const refs=api.recoveryFieldReferences(q,'startMask');assert.equal(refs[0].tone,refs[1].tone);assert.notEqual(refs[0].hatch,refs[1].hatch);assert.equal(refs[0].hatch,refs[2].hatch);
 const key=api.recoveryBuildInputKey(q);q=api.updateRecoverySupply(q,refs[1].field,'I');assert.notEqual(api.recoveryBuildInputKey(q),key);
 q=api.overwriteRecoveryField(q,refs[1].field,1n);q=api.overwriteRecoveryField(q,'resultMask',1n);
 assert.equal(api.recoveryFieldMask(q,refs[1].field),0n);assert.equal(q.resultMask,1n);
});
test('empty first middle is removed before any occupied stage or final',()=>{
 let q=base();for(let i=0;i<3;i++)q=api.addRecoveryMiddle(q);
 q={...q,middleMask:0n,extraMiddles:q.extraMiddles.map((s,i)=>({...s,mask:0xc03n<<BigInt(i*20),supply:`s${i}`}))};
 const out=api.removeRecoveryMiddle(q);assert.equal(out.firstSupply,'s0');assert.equal(out.middleMask,q.extraMiddles[0].mask);assert.equal(out.resultMask,q.resultMask);
 assert.equal(api.recoveryStageEntries(out).length,4);
});
test('two-middle removal promotes an occupied first extra into the required middle',()=>{
 const added=api.addRecoveryMiddle(base());
 const q={...added,middleMask:0n,firstSupply:'obsolete',extraMiddles:[{...added.extraMiddles[0],mask:0xc0300000n,supply:'IJ'}]};
 const out=api.removeRecoveryMiddle(q);
 assert.equal(api.recoveryMiddleCount(out),1);
 assert.equal(out.middleMask,q.extraMiddles[0].mask);
 assert.equal(out.firstSupply,'IJ');
 assert.equal(out.resultMask,q.resultMask);
 assert.equal(out.secondSupply,q.secondSupply);
 assert.equal(api.canRemoveRecoveryMiddle(out),false);
});

import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const result=await build({stdin:{contents:`export * from './packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';export * from './packages/clearra-ui/src/lib/workspace/recoveryStages.ts';export * from './packages/clearra-ui/src/lib/workspace/recoveryFieldReferences.ts';`,resolveDir:root},bundle:true,write:false,platform:'node',format:'esm'});
const api=await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);
const base=()=>({...api.createRecoveryBuildRequest(),height:12,middleMask:0xc03n,firstSupply:'O',resultMask:0xc03000000000000000n,secondSupply:'O'});
test('middle addition retains result; literal four-middle remove gate and empty priority',()=>{
 let q=base();
 for(let count=1;count<=3;count++){assert.equal(api.canRemoveRecoveryMiddle(q),false);assert.equal(api.removeRecoveryMiddle(q),q);q=api.addRecoveryMiddle(q);assert.equal(q.resultMask,base().resultMask);}
 assert.equal(api.recoveryMiddleCount(q),4);assert.equal(api.canRemoveRecoveryMiddle(q),true);
 const next=api.removeRecoveryMiddle(q);assert.equal(api.recoveryMiddleCount(next),3);assert.equal(next.resultMask,q.resultMask);assert.equal(next.secondSupply,q.secondSupply);
 assert.equal(next.extraMiddles[0].id,q.extraMiddles[1].id,'earliest empty middle removed');
});
test('nonempty removal promotes last middle WITH its supply and discards only old result',()=>{
 let q=base();for(let i=1;i<4;i++)q=api.addRecoveryMiddle(q);
 for(const [index,stage]of api.recoveryStageEntries(q).slice(0,-1).entries()) {q=api.overwriteRecoveryField(q,stage.field,0xc03n<<BigInt(index*20));q=api.updateRecoverySupply(q,stage.field,`O${index}`);}
 const last=api.recoveryStageEntries(q).at(-2), next=api.removeRecoveryMiddle(q);
 assert.equal(next.resultMask,last.mask);assert.equal(next.secondSupply,last.supply);assert.equal(next.firstSupply,'O0');assert.equal(api.recoveryMiddleCount(next),3);
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

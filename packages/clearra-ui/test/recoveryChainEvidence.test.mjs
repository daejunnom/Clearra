import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const bundled=await build({stdin:{contents:`export * from './packages/clearra-ui/src/lib/workspace/recoveryChainEvidence.ts';`,resolveDir:root},bundle:true,write:false,format:'esm',platform:'node',target:'node22'});
const api=await import('data:text/javascript;base64,'+Buffer.from(bundled.outputFiles[0].text).toString('base64'));
const I=15n,O1=0xc030n,O2=0x300c0n;
const pop=n=>n.toString(2).replaceAll('0','').length;
function make(targets,queues,placements,{initial=0n,earlyLimit=0,exchange=false,hold=true,status='normal'}={}){
 const height=8,flat=queues.join(''),bounds=[0];for(const q of queues)bounds.push(bounds.at(-1)+q.length);
 let board=initial,used=targets.map(()=>0n),deleted=new Set(),activeB2b=true;
 const early=targets.slice(1).map(()=>0),balance=targets.map(()=>Array(7).fill(0));
 const frontier=()=>{const i=used.findIndex((m,j)=>m!==targets[j]);return i<0?targets.length:i;};
 const steps=placements.map(({target,source,mask,decision='none'})=>{
  const origin=bounds.slice(0,-1).findIndex((a,i)=>a<=source&&source<bounds[i+1]);
  const map=Array.from({length:height},(_,i)=>i).filter(y=>!deleted.has(y));
  let logical=0n;for(let y=0;y<height;y++){const row=(mask>>BigInt(y*10))&1023n;if(row)logical|=row<<BigInt(map[y]*10);}
  for(let boundary=frontier();boundary<target;boundary++)early[boundary]++;
  used[target]|=logical;
  const piece=flat[source],p='IJLOSTZ'.indexOf(piece);balance[origin][p]++;balance[target][p]--;
  const before=board,locked=board|mask;let rows=0,after=0n,out=0;
  for(let y=0;y<height;y++){
   const row=(locked>>BigInt(y*10))&1023n;
   if(row===1023n){rows+=2**y;deleted.add(map[y]);}else {after|=row<<BigInt(out*10);out++;}
  }
  const lines=pop(BigInt(rows));if(lines>0)activeB2b=lines===4||after===0n;
  board=after;
  return {targetStage:target,sourceStage:origin,sourceIndex:source,piece,holdDecision:decision,
   boardBefore:before,placement:mask,boardAfter:after,logicalPlacement:logical,clearedRows:rows,clearedLines:lines,
   recognizedSpin:false,b2bActive:activeB2b,completedStages:frontier()};
 });
 return {height,initial,requestedTargets:targets,targets,queues,earlyLimit,holdEnabled:hold,allowPieceExchange:exchange,
  preserveB2B:false,initialB2B:true,status,earlyByBoundary:early,exchangeByStage:balance,steps,terminalBoard:board};
}
const ordinary=()=>make([I,O1,O2],['I','O','O'],[{target:0,source:0,mask:I},{target:1,source:1,mask:O1},{target:2,source:2,mask:O2}]);
const carried=()=>make([I,O1,O2],['O','I','O'],[{target:0,source:1,mask:I,decision:'store'},
 {target:1,source:2,mask:O1},{target:2,source:0,mask:O2,decision:'release-held-at-terminal'}],{exchange:true});
const future=()=>make([I<<20n,I<<10n,I],['I','I','I'],[{target:2,source:0,mask:I},{target:1,source:1,mask:I<<10n},{target:0,source:2,mask:I<<20n}],{earlyLimit:2,status:'recovery',hold:false});

test('three target stages validate a continuous board and chronological queue',()=>{
 const e=ordinary();assert.ok(api.validateRecoveryChainEvidence(e));
 assert.deepEqual(api.recoveryChainSourcePages(e).map(p=>p.placements.length),[1,2,3]);
 assert.equal(api.recoveryChainSourcePages(e,true).length,1);
});
test('a token held across two boundaries belongs to the first source checkpoint, not the last',()=>{
 const e=carried();assert.ok(api.validateRecoveryChainEvidence(e));
 const pages=api.recoveryChainSourcePages(e);
 assert.deepEqual(pages.map(p=>p.placements.length),[1,2,3]);
 assert.deepEqual(pages[0].placements,[pages[2].placements[2]]);
 assert.deepEqual(pages[1].placements,[pages[2].placements[0],pages[2].placements[2]]);
 assert.equal(api.validateRecoveryChainEvidence({...e,allowPieceExchange:false}),false);
 assert.equal(api.validateRecoveryChainEvidence({...e,holdEnabled:false}),false);
});
test('a future placement is charged at each still-open boundary regardless of its source',()=>{
 const e=future();assert.deepEqual(e.earlyByBoundary,[2,1]);assert.ok(api.validateRecoveryChainEvidence(e));
 for(const altered of [{...e,earlyLimit:1},{...e,earlyByBoundary:[1,1]},{...e,status:'normal'}])assert.equal(api.validateRecoveryChainEvidence(altered),false);
 assert.ok(api.validateRecoveryChainEvidence({...e,earlyLimit:null}));
});
test('mid-chain clears preserve original logical ownership for later stages and source exports',()=>{
 const e=make([I,0xc03n<<10n,0x300cn<<10n],['I','O','O'],[
  {target:0,source:0,mask:I},{target:1,source:1,mask:0xc03n},{target:2,source:2,mask:0x300cn}],{initial:0x3f0n});
 assert.ok(api.validateRecoveryChainEvidence(e));assert.equal(e.steps[0].boardAfter,0n);
 assert.equal(e.steps[1].logicalPlacement,e.targets[1]);
 const pages=api.recoveryChainSourcePages(e);assert.deepEqual(pages.map(p=>p.placements.length),[1,2,3]);
 const bad=structuredClone(e);bad.steps[1].logicalPlacement>>=10n;assert.equal(api.validateRecoveryChainEvidence(bad),false);
});
test('four target stages yield four provenance checkpoints while result-only yields one',()=>{
 const masks=[I,I<<10n,I<<20n,I<<30n];const e=make(masks,['I','I','I','I'],masks.map((mask,i)=>({target:i,source:i,mask})));
 assert.ok(api.validateRecoveryChainEvidence(e));
 assert.deepEqual(api.recoveryChainSourcePages(e).map(p=>p.placements.length),[1,2,3,4]);
 assert.deepEqual(api.recoveryChainSourcePages(e,true),[api.recoveryChainSourcePages(e).at(-1)]);
});
test('source stage/index/type, clear history, and quota tampering are rejected',()=>{
 const mutations=[
  e=>e.steps[1].sourceIndex=0,e=>e.steps[1].sourceStage=2,e=>e.steps[1].piece='J',
  e=>e.steps[1].holdDecision='swap',e=>e.steps[1].clearedRows=1,
  e=>e.steps[1].boardBefore=0n,e=>e.steps[1].completedStages=3,
  e=>e.steps[1].logicalPlacement=e.targets[0],e=>e.steps[1].targetStage=2,
  e=>e.exchangeByStage[1][3]=1,e=>e.steps.pop(),e=>e.terminalBoard=0n,
 ];
 for(const mutate of mutations){const e=ordinary();mutate(e);assert.equal(api.validateRecoveryChainEvidence(e),false);assert.throws(()=>api.recoveryChainSourcePages(e));}
});
test('source checkpoints are not synthesized by resetting held pieces at boundaries',()=>{
 const e=carried();e.steps[2].holdDecision='none';assert.equal(api.validateRecoveryChainEvidence(e),false);
});
test('initial reflection transforms the entire suffix, while asymmetric starts do not permit it',()=>{
 const mirror=m=>{let out=0n;for(let y=0;y<8;y++)for(let x=0;x<10;x++)if(m&(1n<<BigInt(y*10+x)))out|=1n<<BigInt(y*10+9-x);return out;};
 const targets=[0x1007n,0x300c00n,0xf0n],reflected=targets.map(mirror);
 assert.ok(api.recoveryChainOrientationAllowed(0n,targets,reflected,8));
 assert.equal(api.recoveryChainOrientationAllowed(8n,targets,reflected,8),false);
 assert.equal(api.recoveryChainOrientationAllowed(0n,targets,[reflected[0],targets[1],reflected[2]],8),false);
});
test('a symmetric target does not discard a still-possible whole-suffix reflection parity',()=>{
 const targets=[0x78n,0x1007n,0x300c00n];const mirror=m=>{let r=0n;for(let y=0;y<8;y++)for(let x=0;x<10;x++)if(m&(1n<<BigInt(10*y+x)))r|=1n<<BigInt(10*y+9-x);return r;};
 assert.ok(api.recoveryChainOrientationAllowed(0n,targets,targets.map(mirror),8));
});
test('midpoint full-row symmetry may independently open the remaining suffix',()=>{
 const original=[I,0xc03n<<10n,0x300cn<<10n];
 // Reflect 10-bit rows explicitly; do not infer a piece relabeling.
 const mirror=m=>{let r=0n;for(let y=0;y<8;y++)for(let x=0;x<10;x++)if(m&(1n<<BigInt(10*y+x)))r|=1n<<BigInt(10*y+9-x);return r;};
 const actual=[I,...original.slice(1).map(mirror)];
 assert.ok(api.recoveryChainOrientationAllowed(0x3f0n,original,actual,8));
});

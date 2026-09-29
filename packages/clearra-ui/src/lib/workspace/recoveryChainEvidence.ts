/** Independent multi-stage replay/accounting validation for the forthcoming
 * host adapter. This is NOT a solver, a coverage certificate or a kick oracle. */
import { buildPcPathReplayFrames, pcPathWitnessExportPage, type PathReplayGeometryWitness } from './pcPathReplayPresentation';
import type { SolutionExportPage } from './solutionExport';
import { compactRecoveryBoard, countRecoveryCells } from './recoveryBuildModel';

export type RecoveryChainEvidenceStep = {
  targetStage:number; sourceStage:number; sourceIndex:number; piece:string;
  holdDecision:'none'|'store'|'swap'|'release-held-at-terminal';
  boardBefore:bigint; placement:bigint; boardAfter:bigint; logicalPlacement:bigint;
  clearedRows:number; clearedLines:number; recognizedSpin:boolean; b2bActive:boolean;
  completedStages:number;
};
export type RecoveryChainEvidence = {
  height:number; initial:bigint; requestedTargets:bigint[]; targets:bigint[];
  queues:string[]; earlyLimit:number|null; holdEnabled:boolean; allowPieceExchange:boolean;
  preserveB2B:boolean; initialB2B:boolean; status:'normal'|'recovery';
  earlyByBoundary:number[]; exchangeByStage:number[][]; steps:RecoveryChainEvidenceStep[];
  terminalBoard:bigint;
};
const integer=(n:unknown):n is number=>typeof n==='number' && Number.isSafeInteger(n);
const ensure=(ok:unknown):void=>{if(!ok)throw new Error('invalid recovery chain evidence');};
const hex=(n:bigint):string=>`0x${n.toString(16).padStart(64,'0')}`;
function mirror(mask:bigint,height:number):bigint {
  let out=0n;
  for(let y=0;y<height;y++)for(let x=0;x<10;x++)if(mask&(1n<<BigInt(y*10+x)))out|=1n<<BigInt(y*10+9-x);
  return out;
}
/** Track the TWO possible suffix orientations rather than enumerating 2^N
 * histories. A symmetric target must not arbitrarily discard one parity. */
export function recoveryChainOrientationAllowed(initial:bigint,requested:readonly bigint[],actual:readonly bigint[],height:number):boolean {
  if(!integer(height)||height<1||height>24||requested.length!==actual.length||requested.length<2||requested.length>60)return false;
  let prefix=initial;
  let parities=new Set([false]);
  for(let i=0;i<requested.length;i++){
    const base=compactRecoveryBoard(prefix,height);
    if(base===mirror(base,height))parities=new Set([...parities,...[...parities].map(p=>!p)]);
    parities=new Set([...parities].filter(p=>actual[i]===(p?mirror(requested[i],height):requested[i])));
    if(parities.size===0)return false;
    prefix|=actual[i];
  }
  return true;
}
function replay(e:RecoveryChainEvidence):PathReplayGeometryWitness {
  return {maskHexDigits:64,candidate_id:'recovery-chain-witness',pattern_id:e.queues.join('/'),normalized_trace_key:JSON.stringify(e.steps,(_,v)=>typeof v==='bigint'?hex(v):v),
    steps:e.steps.map((s,i)=>({step_index:String(i),active_piece:s.piece,placement_mask:hex(s.placement),
      board_before_mask:hex(s.boardBefore),board_after_placement_mask:hex(s.boardBefore|s.placement),
      board_after_line_clear_mask:hex(s.boardAfter),cleared_row_mask:hex(BigInt(s.clearedRows)),cleared_lines:String(s.clearedLines)}))};
}
export function validateRecoveryChainEvidence(value:unknown):value is RecoveryChainEvidence {
 try {
  const e=value as RecoveryChainEvidence;
  ensure(e && integer(e.height)&&e.height>=1&&e.height<=24);
  const bound=1n<<BigInt(e.height*10),validMask=(m:unknown)=>typeof m==='bigint'&&m>=0n&&m<bound;
  ensure(validMask(e.initial)&&validMask(e.terminalBoard));
  ensure(Array.isArray(e.targets)&&e.targets.length>=2&&e.targets.length<=60&&Array.isArray(e.requestedTargets)&&e.requestedTargets.length===e.targets.length);
  const n=e.targets.length,demands=e.targets.map(t=>countRecoveryCells(t)/4);
  for(const targets of [e.requestedTargets,e.targets]){
   let union=e.initial;
   for(const target of targets){ensure(validMask(target)&&target>0n&&countRecoveryCells(target)%4===0&&!(union&target));union|=target;}
  }
  ensure(recoveryChainOrientationAllowed(e.initial,e.requestedTargets,e.targets,e.height));
  ensure(Array.isArray(e.queues)&&e.queues.length===n&&e.queues.every(q=>typeof q==='string'&&/^[IJLOSTZ]+$/u.test(q)));
  const input=e.queues.join('');ensure(input.length<=256);
  ensure([e.holdEnabled,e.allowPieceExchange,e.preserveB2B,e.initialB2B].every(v=>typeof v==='boolean'));
  ensure(e.earlyLimit===null||(integer(e.earlyLimit)&&e.earlyLimit>=0));
  ensure(['normal','recovery'].includes(e.status));
  ensure(Array.isArray(e.earlyByBoundary)&&e.earlyByBoundary.length===n-1&&e.earlyByBoundary.every(v=>integer(v)&&v>=0));
  ensure(Array.isArray(e.exchangeByStage)&&e.exchangeByStage.length===n&&e.exchangeByStage.every(row=>Array.isArray(row)&&row.length===7&&row.every(integer)));
  ensure(Array.isArray(e.steps)&&e.steps.length===demands.reduce((a,b)=>a+b,0));
  const starts=[0];for(const queue of e.queues)starts.push(starts.at(-1)!+queue.length);
  const used=Array<bigint>(n).fill(0n),sourceUse=Array<number>(n).fill(0),early=Array<number>(n-1).fill(0),exchange=Array.from({length:n},()=>Array<number>(7).fill(0));
  const sourceIds=new Set<number>(),deleted=new Set<number>();
  for(let y=0;y<e.height;y++)if(((e.initial>>BigInt(y*10))&1023n)===1023n)deleted.add(y);
  let board=compactRecoveryBoard(e.initial,e.height),b2b=e.initialB2B;
  let active:number|null=0,held:number|null=null,cursor=1;
  const frontier=()=>{const i=used.findIndex((m,j)=>m!==e.targets[j]);return i===-1?n:i;};
  for(const s of e.steps){
   ensure(s && integer(s.targetStage)&&s.targetStage>=0&&s.targetStage<n&&integer(s.sourceIndex)&&s.sourceIndex>=0&&s.sourceIndex<input.length);
   ensure(typeof s.piece==='string'&&/^[IJLOSTZ]$/u.test(s.piece)&&input[s.sourceIndex]===s.piece&&!sourceIds.has(s.sourceIndex));
   const source=starts.slice(0,-1).findIndex((start,i)=>start<=s.sourceIndex&&s.sourceIndex<starts[i+1]);ensure(source===s.sourceStage);
   if(s.holdDecision==='none')ensure(active===s.sourceIndex);
   else if(s.holdDecision==='store'){ensure(e.holdEnabled&&active!==null&&held===null&&cursor===s.sourceIndex);held=active;cursor++;}
   else if(s.holdDecision==='swap'){ensure(e.holdEnabled&&active!==null&&held===s.sourceIndex);held=active;}
   else if(s.holdDecision==='release-held-at-terminal'){ensure(e.holdEnabled&&active===null&&held===s.sourceIndex);held=null;}
   else ensure(false);
   active=cursor<input.length?cursor++:null;
   sourceIds.add(s.sourceIndex);sourceUse[source]++;ensure(sourceUse[source]<=demands[source]);
   ensure([s.boardBefore,s.placement,s.boardAfter,s.logicalPlacement].every(validMask)&&s.boardBefore===board&&!(board&s.placement)&&countRecoveryCells(s.placement)===4);
   const map=Array.from({length:e.height},(_,y)=>y).filter(y=>!deleted.has(y));let lifted=0n;
   for(let y=0;y<e.height;y++){
    const bits=(s.placement>>BigInt(y*10))&1023n;
    if(bits!==0n){ensure(y<map.length);lifted|=bits<<BigInt(map[y]*10);}
   }
   ensure(lifted===s.logicalPlacement&&(lifted&e.targets[s.targetStage])===lifted&&!(used[s.targetStage]&lifted));
   const before=frontier();ensure(s.targetStage>=before);
   for(let boundary=before;boundary<s.targetStage;boundary++){
    early[boundary]++;const limit=e.earlyLimit===null?demands.slice(boundary+1).reduce((a,b)=>a+b,0):e.earlyLimit;
    ensure(early[boundary]<=limit);
   }
   used[s.targetStage]|=lifted;
   const p='IJLOSTZ'.indexOf(s.piece);exchange[source][p]++;exchange[s.targetStage][p]--;
   let rows=0;for(let y=0;y<e.height;y++)if((((board|s.placement)>>BigInt(y*10))&1023n)===1023n)rows+=2**y;
   ensure(integer(s.clearedRows)&&s.clearedRows===rows&&s.clearedLines===countRecoveryCells(BigInt(rows)));
   board=compactRecoveryBoard(board|s.placement,e.height);ensure(board===s.boardAfter);
   for(let y=0;y<e.height;y++)if(rows&2**y){ensure(y<map.length);deleted.add(map[y]);}
   ensure([s.recognizedSpin,s.b2bActive].every(v=>typeof v==='boolean'));
   if(s.clearedLines>0)b2b=s.clearedLines===4||board===0n||s.recognizedSpin;
   ensure(b2b===s.b2bActive&&(!e.preserveB2B||s.clearedLines===0||b2b));
   ensure(s.completedStages===frontier());
  }
  ensure(used.every((m,i)=>m===e.targets[i])&&sourceUse.every((v,i)=>v===demands[i]));
  ensure(early.every((v,i)=>v===e.earlyByBoundary[i])&&(e.status==='normal'?early.every(v=>v===0):early.some(v=>v>0)));
  ensure(exchange.every((row,i)=>row.every((v,p)=>v===e.exchangeByStage[i][p])));
  ensure(e.allowPieceExchange||exchange.every(row=>row.every(v=>v===0)));
  ensure(board===e.terminalBoard&&board===compactRecoveryBoard(e.targets.reduce((a,b)=>a|b,e.initial),e.height));
  buildPcPathReplayFrames(replay(e),e.height,hex(e.terminalBoard));
  return true;
 }catch{return false;}
}
/** N source checkpoints, not the first N locks in chronological order. */
export function recoveryChainSourcePages(e:RecoveryChainEvidence,resultOnly=false):SolutionExportPage[]{
 if(!validateRecoveryChainEvidence(e))throw new Error('invalid recovery chain evidence');
 const final=pcPathWitnessExportPage(replay(e),e.height,hex(e.terminalBoard));
 if(!final||final.placements.length!==e.steps.length)throw new Error('incomplete chain export');
 if(resultOnly)return [final];
 let boundary=0;
 return e.queues.map((queue,stage)=>{
  boundary+=queue.length;
  if(stage===e.queues.length-1)return final;
  const placements=final.placements.filter((_,i)=>e.steps[i].sourceIndex<boundary);
  const expected=e.targets.slice(0,stage+1).reduce((sum,t)=>sum+countRecoveryCells(t)/4,0);
  if(placements.length!==expected)throw new Error('missing chain source checkpoint');
  return {...final,placements};
 });
}

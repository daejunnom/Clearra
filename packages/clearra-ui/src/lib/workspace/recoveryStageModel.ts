/** Ordered editor stages, separate from the search algorithm. Masks here share
 * the original logical frame; supplies belong to destinations, not tab slots.
 * This draft is not silently lowered to the current two-target engine. */
import type { RecoveryBuildRequest } from './recoveryBuildModel';
import { changeRecoveryResultFrame, projectRecoveryRows, recoveryResultFrame } from './recoveryResultFrame';

export type RecoveryMiddleStage = { id: string; mask: bigint; supply: string };
export type RecoveryEditorSelection = { field: 'startMask' | 'middleMask' | 'resultMask'; middleId: string };
// A common 24 x 10 input can contain at most sixty disjoint four-cell targets,
// including the final target. Blank drafts may not grow beyond that capacity.
export const MAX_RECOVERY_MIDDLES = 59;

export function recoveryMiddleStages(request: RecoveryBuildRequest): RecoveryMiddleStage[] {
  const stages = request.middleStages;
  if (stages === undefined) return [{ id: 'middle-0', mask: request.middleMask, supply: request.firstSupply }];
  if (!Array.isArray(stages) || stages.length < 1 || stages.length > MAX_RECOVERY_MIDDLES ||
      stages.some(s => !s || typeof s.id !== 'string' || !/^middle-[0-9]+$/u.test(s.id) ||
        typeof s.mask !== 'bigint' || typeof s.supply !== 'string') ||
      new Set(stages.map(s=>s.id)).size !== stages.length ||
      stages[0].mask !== request.middleMask || stages[0].supply !== request.firstSupply) {
    throw new RangeError('inconsistent recovery stage draft');
  }
  return stages.map(s=>({...s}));
}
export function withRecoveryMiddleStages(request: RecoveryBuildRequest, stages: readonly RecoveryMiddleStage[]): RecoveryBuildRequest {
  if (!stages.length) throw new RangeError('at least one middle field is required');
  const next = { ...request, middleMask:stages[0].mask, firstSupply:stages[0].supply, middleStages:stages.map(s=>({...s})) };
  recoveryMiddleStages(next);
  return next;
}
export function recoveryCanRemoveMiddle(request: RecoveryBuildRequest): boolean {
  // Literal user policy: count MIDDLE fields, excluding Start and Result.
  return recoveryMiddleStages(request).length >= 4;
}
export function appendRecoveryMiddle(request: RecoveryBuildRequest): {request:RecoveryBuildRequest; selection:RecoveryEditorSelection} {
  const current = recoveryMiddleStages(request);
  if (current.length >= MAX_RECOVERY_MIDDLES) throw new RangeError('common-frame target capacity exceeded');
  const shared = current.length === 1 ? changeRecoveryResultFrame(request,'shared') : request;
  if (recoveryResultFrame(shared) !== 'shared') throw new RangeError('multiple middle fields require a common frame');
  const used = new Set(current.map(s=>s.id));
  let id = 0;
  while (used.has(`middle-${id}`)) id++;
  const stage = {id:`middle-${id}`,mask:0n,supply:''};
  const next = withRecoveryMiddleStages(shared,[...current,stage]);
  return {request:next,selection:{field:'middleMask',middleId:stage.id}};
}
export function removeRecoveryMiddle(request: RecoveryBuildRequest, selection:RecoveryEditorSelection): {request:RecoveryBuildRequest; selection:RecoveryEditorSelection} {
  const current = recoveryMiddleStages(request);
  if (current.length < 4) throw new RangeError('removal requires at least four middle fields');
  const selected = current.findIndex(s=>s.id===selection.middleId);
  let rightmost = -1;
  for (let i=current.length-1;i>=0;i--) if (current[i].mask===0n) { rightmost=i;break; }
  const empty = selected >= 0 && current[selected].mask === 0n ? selected : rightmost;
  if (empty >= 0) {
    const removed = current[empty];
    const rest = current.filter((_,i)=>i!==empty);
    const nextSelection = selection.middleId === removed.id
      ? {...selection,middleId:rest[Math.min(empty,rest.length-1)].id} : selection;
    return {request:withRecoveryMiddleStages(request,rest),selection:nextSelection};
  }
  // The final field and ITS incoming supply are discarded as one transaction.
  // The last middle keeps its mask and its own supply when becoming Result.
  const promoted = current.at(-1)!;
  const next = withRecoveryMiddleStages({...request,resultMask:promoted.mask,secondSupply:promoted.supply},current.slice(0,-1));
  const nextSelection = selection.middleId === promoted.id
    ? {field:selection.field==='middleMask'?'resultMask' as const:selection.field,middleId:current[current.length-2].id} : selection;
  return {request:next,selection:nextSelection};
}
export function setRecoveryStageSupply(request:RecoveryBuildRequest, destination:string, value:string):RecoveryBuildRequest {
  if (destination === 'result') return {...request,secondSupply:value};
  const stages = recoveryMiddleStages(request);
  if (!stages.some(s=>s.id===destination)) throw new RangeError('unknown recovery supply destination');
  return withRecoveryMiddleStages(request,stages.map(s=>s.id===destination?{...s,supply:value}:s));
}

export type RecoveryEditorSnapshot = Pick<RecoveryBuildRequest,
  'startMask'|'middleMask'|'resultMask'|'height'|'firstSupply'|'secondSupply'|'middleStages'|'resultFrame'>;
export function captureRecoveryEditor(request:RecoveryBuildRequest):RecoveryEditorSnapshot {
  return {startMask:request.startMask,middleMask:request.middleMask,resultMask:request.resultMask,height:request.height,
    firstSupply:request.firstSupply,secondSupply:request.secondSupply,resultFrame:request.resultFrame,
    middleStages:request.middleStages?.map(s=>({...s}))};
}
export function recoveryEditorKey(request:RecoveryEditorSnapshot):string {
  return JSON.stringify(request,(_,value)=>typeof value==='bigint'?`0x${value.toString(16)}`:value);
}
export function restoreRecoveryEditor(request:RecoveryBuildRequest, snapshot:RecoveryEditorSnapshot):RecoveryBuildRequest {
  return {...request,...captureRecoveryEditor({...request,...snapshot})};
}

export type RecoveryStageFrame = {
  id:string; index:number; supply:string; logicalTarget:bigint;
  nominalBase:bigint; targetAfterPreviousClears:bigint; removedLogicalRows:number[];
};
/** Nominal frame compiler for the next engine contract, NOT a product solver.
 * Early placements/hold/B2B can change actual boundary states; those must be
 * carried by the exact search and must not be inferred from these previews. */
export function recoveryStageFrames(request:RecoveryBuildRequest):RecoveryStageFrame[] {
  if (!Number.isInteger(request.height) || request.height<1 || request.height>24) throw new RangeError('invalid recovery height');
  const middles = recoveryMiddleStages(request);
  const shared = middles.length===1 ? changeRecoveryResultFrame(request,'shared') : request;
  if (recoveryResultFrame(shared)!=='shared') throw new RangeError('multiple middle fields require a common frame');
  const bound=1n<<BigInt(shared.height*10);
  let union=shared.startMask;
  if (union<0n || union>=bound) throw new RangeError('invalid start mask');
  return [...middles,{id:'result',mask:shared.resultMask,supply:shared.secondSupply}].map((s,index)=>{
    if (s.mask<0n || s.mask>=bound || (union&s.mask)!==0n) throw new RangeError('invalid or overlapping stage mask');
    const removedLogicalRows=Array.from({length:shared.height},(_,y)=>y).filter(y=>((union>>BigInt(y*10))&1023n)===1023n);
    const frame={id:s.id,index,supply:s.supply,logicalTarget:s.mask,
      nominalBase:projectRecoveryRows(union,union,shared.height),
      targetAfterPreviousClears:projectRecoveryRows(s.mask,union,shared.height),removedLogicalRows};
    union|=s.mask;
    return frame;
  });
}

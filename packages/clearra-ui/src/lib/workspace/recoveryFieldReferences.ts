import type { RecoveryBuildRequest } from './recoveryBuildModel';
import { recoveryMiddleStages, withRecoveryMiddleStages } from './recoveryStageModel';

export type RecoveryField = 'startMask' | 'middleMask' | 'resultMask';
export type RecoveryFieldReference = {
  field: RecoveryField; mask: bigint; tone: 'dark' | 'medium' | 'light';
  label: 'start' | 'middle' | 'result'; middleId?:string; middleIndex?:number;
  hatch?: 'forward' | 'backward';
};

/** Hatch parity follows the complete stage order, NOT the filtered list of
 * inactive fields. Thus non-neighbouring middle fields may share a direction. */
export function recoveryFieldReferences(
  request:RecoveryBuildRequest, selected:RecoveryField, _legacyVisible=true, selectedMiddleId?:string
):RecoveryFieldReference[] {
  const middles=recoveryMiddleStages(request);
  const id=selectedMiddleId??middles[0].id;
  const references:RecoveryFieldReference[]=[
    {field:'startMask',mask:request.startMask,tone:'dark',label:'start'},
    ...middles.map((s,index):RecoveryFieldReference=>({field:'middleMask',mask:s.mask,tone:'medium',label:'middle',
      middleId:s.id,middleIndex:index,hatch:index%2===0?'forward':'backward'})),
    {field:'resultMask',mask:request.resultMask,tone:'light',label:'result'}];
  return references.filter(r=>r.field!==selected || (selected==='middleMask' && r.middleId!==id));
}

/** Transfer a painted cell to exactly one owner across ALL stages. */
export function overwriteRecoveryField(
  request:RecoveryBuildRequest, field:RecoveryField, mask:bigint, height=request.height, middleId?:string
):RecoveryBuildRequest {
  if (!Number.isInteger(height)||height<1||height>24||mask<0n||(mask>>BigInt(height*10))!==0n) throw new RangeError('invalid recovery field edit');
  const middles=recoveryMiddleStages(request), id=middleId??middles[0].id;
  if (field==='middleMask' && !middles.some(s=>s.id===id)) throw new RangeError('unknown middle field');
  const next={...request,height:Math.max(height,request.height),resultFrame:'shared' as const,
    startMask:field==='startMask'?mask:request.startMask&~mask,
    resultMask:field==='resultMask'?mask:request.resultMask&~mask};
  return withRecoveryMiddleStages(next,middles.map(s=>({...s,mask:field==='middleMask'&&s.id===id?mask:s.mask&~mask})));
}

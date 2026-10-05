import type { RecoveryBuildRequest } from './recoveryBuildModel';
import { recoveryStageEntries, type RecoveryField } from './recoveryStages';
export type { RecoveryField } from './recoveryStages';
export type RecoveryFieldReference = {
  field: RecoveryField; mask: bigint; tone: 'dark' | 'medium' | 'light';
  label: 'start' | 'middle' | 'result'; stageNumber?: number; hatch?: 'forward' | 'backward';
};
export function recoveryFieldReferences(request: RecoveryBuildRequest, selected: RecoveryField, _legacyVisible = true): RecoveryFieldReference[] {
  const stages = recoveryStageEntries(request);
  const refs: RecoveryFieldReference[] = [{field:'startMask',mask:request.startMask,tone:'dark',label:'start'},
    ...stages.map((stage,index): RecoveryFieldReference => ({...stage,
      tone:index === stages.length-1 ? 'light' : 'medium',
      label:index === stages.length-1 ? 'result' : 'middle',
      stageNumber:index === stages.length-1 ? undefined : index+1,
      // Chronological parity, not visible adjacency: skipped stages may match.
      hatch:(index+1)%2 === 0 ? 'forward' : 'backward'}))];
  return refs.filter(reference=>reference.field !== selected);
}
export function overwriteRecoveryField(request: RecoveryBuildRequest, field: RecoveryField, mask: bigint, height = request.height): RecoveryBuildRequest {
  if (!Number.isInteger(height) || height < 1 || height > 24 || mask < 0n || (mask >> BigInt(height * 10)) !== 0n) throw new RangeError('invalid recovery field edit');
  if (field !== 'startMask' && !recoveryStageEntries(request).some(stage=>stage.field===field)) throw new RangeError('unknown recovery stage');
  const next = { ...request, height: Math.max(height, request.height), resultFrame: 'shared' as const };
  for (const layer of ['startMask','middleMask','resultMask'] as const) next[layer] = layer === field ? mask : request[layer] & ~mask;
  next.extraMiddles = request.extraMiddles?.map(stage=>({...stage,mask:field===`middle:${stage.id}` ? mask : stage.mask & ~mask}));
  return next;
}

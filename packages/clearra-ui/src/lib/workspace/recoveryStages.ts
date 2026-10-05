import type { RecoveryBuildRequest } from './recoveryBuildModel';

export type RecoveryMiddleStage = { id: number; mask: bigint; supply: string };
export type RecoveryField = 'startMask' | 'middleMask' | 'resultMask' | `middle:${number}`;
export type RecoveryStageEntry = { field: RecoveryField; mask: bigint; supply: string };

/** Stage identity stays separate from drawing coordinates and supply provenance. */
export function recoveryStageEntries(request: RecoveryBuildRequest): RecoveryStageEntry[] {
  return [
    { field: 'middleMask', mask: request.middleMask, supply: request.firstSupply },
    ...(request.extraMiddles ?? []).map(stage => ({ field: `middle:${stage.id}` as const, mask: stage.mask, supply: stage.supply })),
    { field: 'resultMask', mask: request.resultMask, supply: request.secondSupply }
  ];
}
export function recoveryMiddleCount(request: RecoveryBuildRequest): number { return 1 + (request.extraMiddles?.length ?? 0); }
export function recoveryAllMiddleMask(request: RecoveryBuildRequest): bigint {
  return (request.extraMiddles ?? []).reduce((mask, stage) => mask | stage.mask, request.middleMask);
}
export function recoveryFieldMask(request: RecoveryBuildRequest, field: RecoveryField): bigint {
  if (field === 'startMask') return request.startMask;
  const stage = recoveryStageEntries(request).find(stage => stage.field === field);
  if (!stage) throw new RangeError('unknown recovery stage');
  return stage.mask;
}
export function updateRecoverySupply(request: RecoveryBuildRequest, field: RecoveryField, supply: string): RecoveryBuildRequest {
  if (field === 'middleMask') return { ...request, firstSupply: supply };
  if (field === 'resultMask') return { ...request, secondSupply: supply };
  if (!recoveryStageEntries(request).some(stage => stage.field === field)) throw new RangeError('unknown recovery supply');
  return { ...request, extraMiddles: (request.extraMiddles ?? []).map(stage => `middle:${stage.id}` === field ? { ...stage, supply } : stage) };
}
export function addRecoveryMiddle(request: RecoveryBuildRequest): RecoveryBuildRequest {
  // At most 60 nonempty tetromino target regions fit in the 24-row field.
  if (recoveryMiddleCount(request) >= 59) return request;
  const id = Math.max(request.nextMiddleId ?? 1, 1 + Math.max(0, ...(request.extraMiddles ?? []).map(stage => stage.id)));
  return { ...request, resultFrame: 'shared', nextMiddleId: id + 1,
    extraMiddles: [...(request.extraMiddles ?? []), { id, mask: 0n, supply: '' }] };
}
/** Literal product rule: enable removal only with FOUR middle fields, not four total fields. */
export function canRemoveRecoveryMiddle(request: RecoveryBuildRequest): boolean { return recoveryMiddleCount(request) >= 4; }
export function removeRecoveryMiddle(request: RecoveryBuildRequest): RecoveryBuildRequest {
  if (!canRemoveRecoveryMiddle(request)) return request;
  const stages = recoveryStageEntries(request).slice(0, -1);
  const empty = stages.findIndex(stage => stage.mask === 0n);
  if (empty >= 0) {
    stages.splice(empty, 1);
    return { ...request, middleMask: stages[0].mask, firstSupply: stages[0].supply,
      extraMiddles: stages.slice(1).map(stage => ({ id: Number(stage.field.slice(7)), mask: stage.mask, supply: stage.supply })) };
  }
  // No empty middle: remove the old result, retaining the last middle's field AND supply.
  const last = stages.pop()!;
  return { ...request, resultMask: last.mask, secondSupply: last.supply,
    extraMiddles: (request.extraMiddles ?? []).slice(0, -1), resultFrame: 'shared' };
}

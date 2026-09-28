import type { RecoveryBuildRequest } from './recoveryBuildModel';

export type RecoveryField = 'startMask' | 'middleMask' | 'resultMask';
export type RecoveryFieldReference = {
  field: RecoveryField;
  mask: bigint;
  tone: 'dark' | 'medium' | 'light';
  label: 'start' | 'middle' | 'result';
};

/** All three layers use the original input frame. Palette selection changes
 * only the editing owner; it cannot hide or project any other layer. */
export function recoveryFieldReferences(
  request: RecoveryBuildRequest, selected: RecoveryField, _legacyVisible = true
): RecoveryFieldReference[] {
  return [
    { field: 'startMask', mask: request.startMask, tone: 'dark', label: 'start' },
    { field: 'middleMask', mask: request.middleMask, tone: 'medium', label: 'middle' },
    { field: 'resultMask', mask: request.resultMask, tone: 'light', label: 'result' }
  ].filter(reference => reference.field !== selected) as RecoveryFieldReference[];
}

/** Assign occupied cells to exactly one layer, as in the ordinary Build editor.
 * Erasing a cell never paints another layer. Imports use the same ownership rule. */
export function overwriteRecoveryField(
  request: RecoveryBuildRequest, field: RecoveryField, mask: bigint, height = request.height
): RecoveryBuildRequest {
  if (!Number.isInteger(height) || height < 1 || height > 24 || mask < 0n || (mask >> BigInt(height * 10)) !== 0n) {
    throw new RangeError('invalid recovery field edit');
  }
  const next = { ...request, height: Math.max(height, request.height), resultFrame: 'shared' as const };
  for (const layer of ['startMask','middleMask','resultMask'] as const) {
    next[layer] = layer === field ? mask : request[layer] & ~mask;
  }
  return next;
}

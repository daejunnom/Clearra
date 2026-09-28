import type { RecoveryBuildRequest } from './recoveryBuildModel';
import { projectRecoveryRows, recoveryResultFrame } from './recoveryResultFrame';

export type RecoveryField = 'startMask' | 'middleMask' | 'resultMask';
export type RecoveryFieldReference = {
  field: 'startMask' | 'middleMask';
  mask: bigint;
  tone: 'dark' | 'medium';
  label: 'start' | 'middle';
};

/** Original input snapshots, not a simulated after-clear board. Never mutate
 * or compact draft coordinates merely because the selected editor changes. */
export function recoveryFieldReferences(
  request: RecoveryBuildRequest,
  selected: RecoveryField,
  visible: boolean
): RecoveryFieldReference[] {
  if (!visible) return [];
  const references: RecoveryFieldReference[] = [
    { field: 'startMask', mask: request.startMask, tone: 'dark', label: 'start' },
    { field: 'middleMask', mask: request.middleMask, tone: 'medium', label: 'middle' }
  ];
  const visibleReferences = references.filter((reference) => reference.field !== selected);
  if (selected !== 'resultMask' || recoveryResultFrame(request) === 'shared') return visibleReferences;
  const completed = request.startMask | request.middleMask;
  return visibleReferences.map(reference => ({ ...reference,
    mask: projectRecoveryRows(reference.mask, completed, request.height) }));
}

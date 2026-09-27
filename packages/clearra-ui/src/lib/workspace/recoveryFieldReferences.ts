import type { RecoveryBuildRequest } from './recoveryBuildModel';

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
  return references.filter((reference) => reference.field !== selected);
}

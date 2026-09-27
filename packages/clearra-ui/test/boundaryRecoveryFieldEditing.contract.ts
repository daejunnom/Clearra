import assert from 'node:assert/strict';
import { createBoundaryRecoveryRequest, type BoundaryRecoveryRequest } from '../src/lib/workspace/boundaryRecoveryModel.ts';
import { resizeBoundaryRecoveryFields } from '../src/lib/workspace/boundaryRecoveryFieldEditing.ts';

const original: BoundaryRecoveryRequest = { ...createBoundaryRecoveryRequest(), queue: 'IOT' };
for (const maxEarlyPlacements of ['auto', 0, 1, 2] as const) {
  const edited: BoundaryRecoveryRequest = resizeBoundaryRecoveryFields({ ...original, maxEarlyPlacements }, 4);
  assert.equal(edited.maxEarlyPlacements, maxEarlyPlacements);
  assert.equal(edited.height, 4);
  assert.equal(edited.borrowPlacementMask, undefined);
  assert.equal(edited.borrowRolePosition, undefined);
}

import type { BoundaryRecoveryRequest } from './boundaryRecoveryModel.ts';
import { trimForwardBoardMask } from './forwardSearchModel.ts';

/** UI field bounds; an incomplete number draft must not erase any geometry. */
export function isRecoveryEditorHeight(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 1 && value <= 24;
}

/** Resize independent snapshots without inventing a selected legacy placement. */
export function resizeBoundaryRecoveryFields(
  request: BoundaryRecoveryRequest,
  height: number
): BoundaryRecoveryRequest {
  if (!isRecoveryEditorHeight(height)) return request;
  const next: BoundaryRecoveryRequest = {
    ...request,
    height,
    initialBoardMask: trimForwardBoardMask(request.initialBoardMask, height),
    stageOneBoardMask: trimForwardBoardMask(request.stageOneBoardMask, height),
    targetBoardMask: trimForwardBoardMask(request.targetBoardMask, height),
    placementRoleMasks: request.placementRoleMasks.map(mask => trimForwardBoardMask(mask, height))
  };
  if (request.borrowPlacementMask !== undefined) {
    next.borrowPlacementMask = trimForwardBoardMask(request.borrowPlacementMask, height);
  }
  return next;
}

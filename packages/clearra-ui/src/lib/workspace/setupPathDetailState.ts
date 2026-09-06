import type { SetupPathDetailState } from './setupFinderModel.ts';
import { workspacePublicFailure } from './workspacePublicFailure.ts';

export function cancelSetupPathDetail(
  pathDetails: Record<string, SetupPathDetailState>,
  activeKey: string | null
): Record<string, SetupPathDetailState> {
  if (!activeKey) return pathDetails;
  return {
    ...pathDetails,
    [activeKey]: {
      status: 'failed',
      paths: [],
      complete: false,
      publicFailures: [workspacePublicFailure('request-cancelled')],
      developerFailure: null
    }
  };
}

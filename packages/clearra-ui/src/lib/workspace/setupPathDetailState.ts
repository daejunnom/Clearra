import type { SetupPathDetailState } from './setupFinderModel';

export function cancelSetupPathDetail(
  pathDetails: Record<string, SetupPathDetailState>,
  activeKey: string | null,
  cancelledMessage: string
): Record<string, SetupPathDetailState> {
  if (!activeKey) return pathDetails;
  return {
    ...pathDetails,
    [activeKey]: {
      status: 'failed',
      paths: [],
      complete: false,
      error: cancelledMessage
    }
  };
}

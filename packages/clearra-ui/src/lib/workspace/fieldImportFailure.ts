import type { WorkspaceMessageKey } from './workspaceI18n';

export function fieldImportFailureMessageKey(
  error: unknown,
  fallback: WorkspaceMessageKey = 'fieldImportInvalid'
): WorkspaceMessageKey {
  const code = error instanceof Error ? error.message : String(error);
  if (code === 'fumen-bundle-input-too-large') return 'fumenBundleInputTooLarge';
  if (code === 'fumen-bundle-page-limit') return 'fumenBundlePageLimit';
  if (code === 'fumen-worker-unavailable' || code === 'fumen-worker-failed') return 'fumenWorkerUnavailable';
  if (code === 'fumen-input-too-large') return 'fumenInputTooLarge';
  if (code === 'fumen-page-limit') return 'fumenPageLimit';
  return fallback;
}

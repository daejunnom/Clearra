import type { WorkspaceMessageKey } from './workspaceI18n';

export function fieldImportFailureMessageKey(
  error: unknown,
  fallback: WorkspaceMessageKey = 'fieldImportInvalid'
): WorkspaceMessageKey {
  const code = error instanceof Error ? error.message : String(error);
  if (code === 'fumen-input-too-large') return 'fumenInputTooLarge';
  if (code === 'fumen-page-limit') return 'fumenPageLimit';
  return fallback;
}

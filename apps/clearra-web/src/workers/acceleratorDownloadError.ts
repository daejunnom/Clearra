// The worker's abort signal is not proof that a storage rollback succeeded.
// Keep these two commit-boundary failures distinct from a normal cancellation.
export function acceleratorDownloadErrorCode(error: unknown, aborted: boolean): string {
  const code = error instanceof Error ? error.message : 'accelerator_download_failed';
  if (code === 'accelerator_store_rollback_failed' ||
      code === 'accelerator_store_commit_uncertain') return code;
  return aborted ? 'accelerator_download_cancelled' : code;
}

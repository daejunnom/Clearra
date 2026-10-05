// SRP rationale: classify recoverable local-storage failures without owning a catalog,
// download, worker or persistent store. Unexpected failures stay visible.
export function repairableLocalAssetError(error: unknown) {
  if (error instanceof SyntaxError) return true;
  if (error instanceof DOMException) {
    return error.name === 'NotFoundError' || error.name === 'NotReadableError';
  }
  return error instanceof Error && [
    'accelerator_store_pointer_invalid',
    'accelerator_store_size_mismatch',
    'accelerator_store_digest_mismatch'
  ].includes(error.message);
}

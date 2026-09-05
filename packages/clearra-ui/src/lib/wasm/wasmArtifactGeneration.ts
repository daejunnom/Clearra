const SHA256_HEX = /^[0-9a-f]{64}$/;

export type ClearraWasmArtifactGeneration = Readonly<{
  sourceSha256: string;
  bindingsSha256: string;
  wasmSha256: string;
}>;

let currentGeneration = 'clearra-wasm-artifact:initial';

/**
 * Records a verified development artifact generation announced by the web
 * host. Existing workers deliberately keep their generation so completed
 * result paging remains usable; the next foreground run replaces a stale
 * worker before posting the command.
 */
export function announceWasmArtifactGeneration(
  generation: ClearraWasmArtifactGeneration
): boolean {
  if (
    !SHA256_HEX.test(generation.sourceSha256) ||
    !SHA256_HEX.test(generation.bindingsSha256) ||
    !SHA256_HEX.test(generation.wasmSha256)
  ) {
    return false;
  }
  const next = generationIdentity(generation);
  if (next === currentGeneration) return false;
  currentGeneration = next;
  return true;
}

export function currentWasmArtifactGeneration(): string {
  return currentGeneration;
}

/**
 * Treats an absent generation as stale. This is intentionally fail-safe for
 * workers transferred across owners: only a worker whose owner recorded the
 * exact current generation may be reused for a new command.
 */
export function isCurrentWasmArtifactGeneration(
  generation: string | null | undefined
): boolean {
  return typeof generation === 'string' && generation === currentGeneration;
}

function generationIdentity(generation: ClearraWasmArtifactGeneration): string {
  return [
    'clearra-wasm-artifact:v1',
    generation.sourceSha256,
    generation.bindingsSha256,
    generation.wasmSha256
  ].join(':');
}

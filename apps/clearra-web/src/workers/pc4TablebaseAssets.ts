// SRP: one cancellable, bounded lifetime for a discovered online generation.
// The prior bundled pruning data is not loaded by the public HF control.
import { qualifyPc4UpstreamGeneration } from '../../../../scripts/release/pc4/qualify-upstream-generation.mjs';
import type { Pc4HostGeneration } from '../../../../scripts/release/pc4/qualify-upstream-generation.mjs';

export type Pc4TablebaseAssetBundle = { generation: Pc4HostGeneration; byteLength: number };
let cached: Pc4TablebaseAssetBundle | null = null;
let active: Promise<Pc4TablebaseAssetBundle> | null = null;
let controller: AbortController | null = null;
let epoch = 0;
let checkedAt = 0;
export function prewarmPc4TablebaseAssets(): Promise<Pc4TablebaseAssetBundle> {
  if (cached && Date.now() - checkedAt < 5 * 60_000) return Promise.resolve(cached);
  if (active) return active;
  const token = ++epoch;
  controller = new AbortController();
  active = qualifyPc4UpstreamGeneration({ signal: controller.signal }).then((generation) => {
    if (token !== epoch) throw new DOMException('PC4 preparation cancelled', 'AbortError');
    cached = { generation, byteLength: generation.transferred_bytes };
    checkedAt = Date.now();
    return cached;
  }).finally(() => { if (token === epoch) { active = null; controller = null; } });
  return active;
}
export function getPc4OnlineGeneration(): Pc4HostGeneration | null { return cached?.generation ?? null; }
export function pc4TablebaseArtifactSha256(): string {
  const slot = cached?.generation.profiles.find(profile => profile.status === 'ready');
  return slot?.artifacts?.graph.content_identity.replace(/^sha256:/, '') ?? '';
}
export function releasePc4TablebaseAssets() {
  epoch++; controller?.abort(); controller = null; active = null; cached = null; checkedAt = 0;
}

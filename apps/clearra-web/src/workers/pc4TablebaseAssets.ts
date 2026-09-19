// SRP: one cancellable, bounded lifetime for a discovered online generation.
// The prior bundled pruning data is not loaded by the public HF control.
import {
  createPc4RangeReader,
  qualifyPc4UpstreamGeneration
} from '../../../../scripts/release/pc4/qualify-upstream-generation.mjs';
import type { Pc4HostGeneration } from '../../../../scripts/release/pc4/qualify-upstream-generation.mjs';
import { localPc4Status } from './pc4LocalStore';

export type Pc4TablebaseAssetBundle = { generation: Pc4HostGeneration; byteLength: number };
const GENERATION_CACHE_MS = 5 * 60_000;
const TRANSPORT_TOUCH_FLOOR_MS = 30_000;
let cached: Pc4TablebaseAssetBundle | null = null;
let active: Promise<Pc4TablebaseAssetBundle> | null = null;
let controller: AbortController | null = null;
let epoch = 0;
let checkedAt = 0;
let cachedProvider: 'local' | 'online' | null = null;
let transportTouchedAt = 0;
export function prewarmPc4TablebaseAssets(): Promise<Pc4TablebaseAssetBundle> {
  if (active) return active;
  const now = Date.now();
  if (cached && now - checkedAt < GENERATION_CACHE_MS) {
    if (cachedProvider !== 'online' || now - transportTouchedAt < TRANSPORT_TOUCH_FLOOR_MS) {
      return Promise.resolve(cached);
    }
    // Generation authority is already cached, but a browser may have evicted
    // the idle HTTPS connection while TB was disabled. Revalidate one exact,
    // bounded byte envelope on re-enable so DNS/TCP/TLS/ALPN starts before the
    // search. This is transport readiness only and mints no new qualification.
    const bundle = cached;
    const token = ++epoch;
    controller = new AbortController();
    active = touchPc4OnlineTransport(bundle.generation, controller.signal).then(() => {
      if (token !== epoch) throw new DOMException('PC4 preparation cancelled', 'AbortError');
      transportTouchedAt = Date.now();
      return bundle;
    }).finally(() => { if (token === epoch) { active = null; controller = null; } });
    return active;
  }
  const token = ++epoch;
  controller = new AbortController();
  const signal = controller.signal;
  active = localPc4Status().then(async local => {
    cachedProvider = local ? 'local' : 'online';
    return local ? local.generation : qualifyPc4UpstreamGeneration({ signal });
  }).then((generation) => {
    if (token !== epoch) throw new DOMException('PC4 preparation cancelled', 'AbortError');
    cached = { generation, byteLength: generation.transferred_bytes };
    checkedAt = Date.now();
    transportTouchedAt = cachedProvider === 'online' ? checkedAt : 0;
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
  cachedProvider = null; transportTouchedAt = 0;
}

async function touchPc4OnlineTransport(generation: Pc4HostGeneration, signal: AbortSignal) {
  const profile = generation.profiles.find(slot => slot.status === 'ready' && slot.artifacts);
  const artifact = profile?.artifacts?.fields;
  if (!artifact || artifact.byte_length < 1) throw new Error('pc4_online_generation_unavailable');
  const reader = createPc4RangeReader(generation, {
    signal,
    maxBytes: 1,
    maxRequests: 1,
    cacheBytes: 0,
    windowBytes: 0,
    maxConcurrent: 1,
    directPaths: [artifact.path]
  });
  try {
    const bytes = await reader.read(artifact, 0, 1);
    if (bytes.byteLength !== 1) throw new Error('pc4_online_transport_prewarm_invalid');
  } finally {
    reader.dispose();
  }
}

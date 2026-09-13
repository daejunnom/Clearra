// SRP: origin-private streamed files and cross-tab lifetime. No HTTP, graph
// traversal or fallback. One active generation + one unpublished transaction.
import { pc4DownloadPlan } from '../../../../scripts/release/pc4/pc4-download.mjs';
import { createPc4LocalReader } from '../../../../scripts/release/pc4/pc4-local-reader.mjs';
import { openPc4LocalFileSource } from './pc4LocalFileSource';
import type { Pc4Artifact, Pc4HostGeneration } from '../../../../scripts/release/pc4/qualify-upstream-generation.mjs';

const ROOT = 'clearra-pc4-v1';
const PROFILE = 'jstris-180';
const PROFILES = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const LOCK = 'clearra-pc4-local-files-v1';
type Active = { schema: 'clearra.pc4.local-files.v1'; directory: string; generation: Pc4HostGeneration };
const missing = (e: unknown) => e instanceof DOMException && e.name === 'NotFoundError';

function fail(code: string): never { throw Object.assign(new Error(code), { code }); }
export function pc4LocalStorageSupported() {
  return !!globalThis.navigator?.storage?.getDirectory && !!globalThis.navigator?.locks?.request;
}
async function lease(mode: 'shared' | 'exclusive', profile = PROFILE): Promise<() => Promise<void>> {
  if (!PROFILES.includes(profile)) fail('pc4_download_profile_unavailable');
  if (!pc4LocalStorageSupported()) fail('pc4_download_storage_unavailable');
  return new Promise((resolve, reject) => {
    const finished = navigator.locks.request(`${LOCK}:${profile}`, { mode, ifAvailable: true }, async lock => {
      if (!lock) { reject(Object.assign(new Error('pc4_download_storage_busy'), { code: 'pc4_download_storage_busy' })); return; }
      await new Promise<void>(release => resolve(async () => { release(); await finished; }));
    });
    void finished.catch(reject);
  });
}
async function root(create = false, profile = PROFILE) {
  if (!PROFILES.includes(profile)) fail('pc4_download_profile_unavailable');
  const origin = await navigator.storage.getDirectory();
  const tablebase = await origin.getDirectoryHandle(ROOT, { create });
  return tablebase.getDirectoryHandle(profile, { create });
}
async function activeFrom(directory: FileSystemDirectoryHandle, profile = PROFILE): Promise<Active | null> {
  try {
    const file = await (await directory.getFileHandle('active.json')).getFile();
    if (file.size > 131_072) fail('pc4_download_local_manifest_invalid');
    const active: Active = JSON.parse(await file.text());
    if (active.schema !== 'clearra.pc4.local-files.v1' || !/^gen-[0-9a-f-]{36}$/.test(active.directory)) {
      fail('pc4_download_local_manifest_invalid');
    }
    pc4DownloadPlan(active.generation, profile);
    return active;
  } catch (error) { if (missing(error)) return null; throw error; }
}
async function cleanGenerations(directory: FileSystemDirectoryHandle, keep?: string) {
  // Exact managed child namespace only, never the OPFS origin or other data.
  const iterable = directory as FileSystemDirectoryHandle & { keys(): AsyncIterableIterator<string> };
  for await (const name of iterable.keys()) {
    if (name !== keep && /^gen-[0-9a-f-]{36}$/.test(name)) await directory.removeEntry(name, { recursive: true });
  }
}
export async function localPc4Status(profile = PROFILE): Promise<{ generation: Pc4HostGeneration; storedBytes: number } | null> {
  if (!pc4LocalStorageSupported()) return null;
  const release = await lease('shared', profile);
  try {
    const directory = await root(false, profile);
    const active = await activeFrom(directory, profile);
    if (!active) return null;
    const plan = pc4DownloadPlan(active.generation, profile);
    const files = await directory.getDirectoryHandle(active.directory);
    for (const artifact of plan.files) {
      if ((await (await files.getFileHandle(artifact.path)).getFile()).size !== artifact.byte_length) {
        fail('pc4_download_local_size_mismatch');
      }
    }
    return { generation: active.generation, storedBytes: plan.totalBytes };
  } catch (error) { if (missing(error)) return null; throw error; }
  finally { await release(); }
}
export const pc4LocalDownloadStore = {
  async begin(plan: ReturnType<typeof pc4DownloadPlan>) {
    const release = await lease('exclusive', plan.profile);
    let directory: FileSystemDirectoryHandle | undefined;
    const name = `gen-${crypto.randomUUID()}`;
    let committed = false;
    try {
      directory = await root(true, plan.profile);
      const active = await activeFrom(directory, plan.profile);
      await cleanGenerations(directory, active?.directory);
      const estimate = await navigator.storage.estimate();
      if (estimate.quota !== undefined && estimate.usage !== undefined &&
          estimate.quota - estimate.usage < plan.totalBytes + 1_048_576) fail('pc4_download_insufficient_space');
      const staging = await directory.getDirectoryHandle(name, { create: true });
      return {
        async open(file: Pc4Artifact) {
          if (!plan.files.some(f => f.path === file.path && f.content_identity === file.content_identity && f.byte_length === file.byte_length)) {
            fail('pc4_download_artifact_invalid');
          }
          const stream = await (await staging.getFileHandle(file.path, { create: true })).createWritable();
          return {
            write: (bytes: Uint8Array) => stream.write(new Uint8Array(bytes)),
            close: () => stream.close(), abort: () => stream.abort()
          };
        },
        async commit(generation: Pc4HostGeneration) {
          const output = await (await directory!.getFileHandle('active.json', { create: true })).createWritable();
          try {
            await output.write(JSON.stringify({ schema: 'clearra.pc4.local-files.v1', directory: name, generation }));
            await output.close();
          } catch (error) { await output.abort().catch(() => {}); throw error; }
          // createWritable publishes only on close. A failed download/update
          // never replaces the prior generation's pointer or files.
          committed = true;
          try {
            await cleanGenerations(directory!, name);
            return { cleanupPending: false };
          } catch {
            // Publication already succeeded. Never report that the previous
            // generation was retained after switching the active pointer.
            return { cleanupPending: true };
          } finally { await release(); }
        },
        async abort() {
          try { if (!committed) await directory!.removeEntry(name, { recursive: true }); }
          finally { await release(); }
        }
      };
    } catch (error) { await release(); throw error; }
  }
};
export async function removeLocalPc4(profile = PROFILE) {
  const release = await lease('exclusive', profile);
  try {
    const directory = await root(false, profile);
    // Validate the marker before removing an active generation. Unknown names
    // are deliberately left alone. No other profiles/origins are touched.
    await activeFrom(directory, profile);
    try { await directory.removeEntry('active.json'); } catch (error) { if (!missing(error)) throw error; }
    await cleanGenerations(directory);
  } catch (error) { if (!missing(error)) throw error; }
  finally { await release(); }
}
export async function openLocalPc4Reader(generation: Pc4HostGeneration, signal?: AbortSignal) {
  if (!pc4LocalStorageSupported()) return null;
  const release = await lease('shared');
  let transferred = false;
  const sources = new Map<string, Awaited<ReturnType<typeof openPc4LocalFileSource>>>();
  const closeSources = async () => {
    const closed = await Promise.allSettled([...sources.values()].map(source => source.close()));
    const failure = closed.find(result => result.status === 'rejected');
    if (failure?.status === 'rejected') throw failure.reason;
  };
  try {
    const directory = await root(), active = await activeFrom(directory);
    if (!active || active.generation.revision !== generation.revision || active.generation.repository !== generation.repository) return null;
    const plan = pc4DownloadPlan(active.generation, PROFILE);
    const expected = pc4DownloadPlan(generation, PROFILE);
    if (plan.files.some((file, i) => file.path !== expected.files[i].path ||
        file.byte_length !== expected.files[i].byte_length || file.content_identity !== expected.files[i].content_identity)) {
      fail('pc4_download_local_manifest_invalid');
    }
    const files = await directory.getDirectoryHandle(active.directory);
    for (const artifact of plan.files) {
      const handle = await files.getFileHandle(artifact.path);
      sources.set(artifact.path, await openPc4LocalFileSource(handle, artifact.byte_length, signal));
    }
    const reader = createPc4LocalReader(plan.files, (artifact, offset, length) =>
      sources.get(artifact.path)!.read(offset, length), { signal, directPaths: [plan.files[2].path] });
    // Resolve only after Web Locks has released ownership, not merely after
    // signalling its callback. A following update/delete must not see our own
    // already-finished read as a busy search.
    let disposing: Promise<void> | undefined;
    const dispose = () => {
      if (!disposing) {
        reader.dispose(); signal?.removeEventListener('abort', onAbort);
        disposing = (async () => { try { await closeSources(); } finally { await release(); } })();
      }
      return disposing;
    };
    const onAbort = () => { void dispose().catch(() => {}); };
    signal?.addEventListener('abort', onAbort, { once: true });
    if (signal?.aborted) { await dispose(); fail('pc4_online_cancelled'); }
    transferred = true;
    return {
      provider: reader.provider, requests: 0, bytes: 0,
      fileAccess: [...new Set([...sources.values()].map(source => source.backend))].join('+'),
      get reads() { return reader.reads; }, get localBytes() { return reader.localBytes; },
      get fileReads() { return reader.fileReads; }, get cacheHits() { return reader.cacheHits; },
      get joinedRequests() { return reader.joinedRequests; }, get retainedBytes() { return reader.retainedBytes; },
      read: reader.read, readMany: reader.readMany, dispose
    };
  } catch (error) { if (missing(error)) return null; throw error; }
  finally { if (!transferred) { try { await closeSources(); } finally { await release(); } } }
}

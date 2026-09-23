// Origin-private accelerator storage. The qualified, source-embedded WASM
// catalog owns authority; this module owns only an atomic local pointer and
// cross-tab lifetime. Legal-board, relation pack and PC4 use distinct roots.
import type { AcceleratorCatalogPlan } from './clearraWasmRuntime';

const ROOT = 'clearra-exact-accelerators-v1';
const LOCK = 'clearra-exact-accelerators-v1';
const PROFILES = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const PRODUCTS = ['exact-legal-board', 'board-conditioned-reachability'];
const MAX_BYTES = 64 * 1024 * 1024;
type Active = {
  schema: 'clearra.exact-accelerator.local.v1';
  generation: string;
  catalog_identity: string;
  payload_identity: string;
  payload_bytes: number;
  file: string;
};
const missing = (error: unknown) => error instanceof DOMException && error.name === 'NotFoundError';
const identity = (value: unknown) => typeof value === 'string' && /^[0-9a-f]{64}$/u.test(value);

function key(product: string, profile: string) {
  if (!PRODUCTS.includes(product) || !PROFILES.includes(profile)) throw new Error('accelerator_store_selection_invalid');
  return `${LOCK}:${product}:${profile}`;
}

export function acceleratorLocalStorageSupported() {
  return !!globalThis.navigator?.storage?.getDirectory && !!globalThis.navigator?.locks?.request;
}

async function lease(product: string, profile: string, mode: 'shared' | 'exclusive'): Promise<() => Promise<void>> {
  const name = key(product, profile);
  if (!acceleratorLocalStorageSupported()) throw new Error('accelerator_store_unavailable');
  return new Promise((resolve, reject) => {
    const finished = navigator.locks.request(name, { mode, ifAvailable: true }, async lock => {
      if (!lock) { reject(new Error('accelerator_store_busy')); return; }
      await new Promise<void>(release => resolve(async () => { release(); await finished; }));
    });
    void finished.catch(reject);
  });
}

async function directory(product: string, profile: string, create: boolean) {
  key(product, profile);
  const origin = await navigator.storage.getDirectory();
  const root = await origin.getDirectoryHandle(ROOT, { create });
  const productRoot = await root.getDirectoryHandle(product, { create });
  return productRoot.getDirectoryHandle(profile, { create });
}

async function activeFrom(root: FileSystemDirectoryHandle): Promise<Active | null> {
  try {
    const file = await (await root.getFileHandle('active.json')).getFile();
    if (file.size > 2048) throw new Error('accelerator_store_pointer_invalid');
    const active: Active = JSON.parse(await file.text());
    if (active.schema !== 'clearra.exact-accelerator.local.v1' ||
        !identity(active.generation) || !identity(active.catalog_identity) ||
        !identity(active.payload_identity) || !Number.isSafeInteger(active.payload_bytes) ||
        active.payload_bytes < 1 || active.payload_bytes > MAX_BYTES ||
        typeof active.file !== 'string' ||
        !new RegExp(`^gen-${active.generation}-[0-9a-f-]{36}\\.bin$`, 'u').test(active.file)) {
      throw new Error('accelerator_store_pointer_invalid');
    }
    return active;
  } catch (error) { if (missing(error)) return null; throw error; }
}

async function cleanOld(root: FileSystemDirectoryHandle, keep?: string) {
  const iterable = root as FileSystemDirectoryHandle & { keys(): AsyncIterableIterator<string> };
  for await (const name of iterable.keys()) {
    if (name !== keep && /^gen-[0-9a-f]{64}-[0-9a-f-]{36}\.bin$/u.test(name)) await root.removeEntry(name);
  }
}

export async function acceleratorLocalStatus(product: string, profile: string, plan?: AcceleratorCatalogPlan) {
  const release = await lease(product, profile, 'shared');
  let active: Active | null = null;
  try {
    const root = await directory(product, profile, false);
    active = await activeFrom(root);
    if (!active) return null;
    const file = await (await root.getFileHandle(active.file)).getFile();
    if (file.size !== active.payload_bytes) throw new Error('accelerator_store_size_mismatch');
    const actualDigest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', await file.arrayBuffer()))]
      .map(value => value.toString(16).padStart(2, '0')).join('');
    if (actualDigest !== active.payload_identity) throw new Error('accelerator_store_digest_mismatch');
    return { ...active, current: plan?.state === 'qualified' &&
      plan.generation === active.generation && plan.catalog_identity === active.catalog_identity &&
      plan.payload_identity === active.payload_identity && plan.payload_bytes === active.payload_bytes };
  } catch (error) {
    // A missing profile/pointer is empty storage. A missing payload behind an
    // existing pointer is corruption and must remain visible to the repair UI.
    if (missing(error) && !active) return null;
    throw error;
  }
  finally { await release(); }
}

function requireActiveDownload(signal?: AbortSignal) {
  if (signal?.aborted) throw new Error('accelerator_download_cancelled');
}

export async function storeQualifiedAccelerator(
  plan: AcceleratorCatalogPlan,
  bytes: Uint8Array<ArrayBuffer>,
  signal?: AbortSignal
) {
  if (plan.state !== 'qualified' || !plan.generation || !plan.payload_identity ||
      !identity(plan.catalog_identity) || !identity(plan.generation) || !identity(plan.payload_identity) ||
      plan.payload_bytes !== bytes.byteLength || bytes.byteLength > MAX_BYTES ||
      (plan.product === 'board-conditioned-reachability' && bytes.byteLength > 16 * 1024 * 1024)) {
    throw new Error('accelerator_store_plan_invalid');
  }
  requireActiveDownload(signal);
  // The persistent store is an authority boundary even when its current
  // caller already verified the transfer. Reject accidental future callers
  // that try to publish unverified bytes under a signed catalog identity.
  const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
    .map(value => value.toString(16).padStart(2, '0')).join('');
  if (digest !== plan.payload_identity) throw new Error('accelerator_store_digest_mismatch');
  requireActiveDownload(signal);
  const release = await lease(plan.product, plan.profile, 'exclusive');
  try {
    requireActiveDownload(signal);
    const root = await directory(plan.product, plan.profile, true);
    const estimate = await navigator.storage.estimate();
    if (estimate.quota !== undefined && estimate.usage !== undefined &&
        estimate.quota - estimate.usage < bytes.byteLength + 1_048_576) throw new Error('accelerator_store_insufficient_space');
    const fileName = `gen-${plan.generation}-${crypto.randomUUID()}.bin`;
    const handle = await root.getFileHandle(fileName, { create: true });
    let output: FileSystemWritableFileStream | null = null;
    const abortStagedWrite = () => { void output?.abort().catch(() => {}); };
    signal?.addEventListener('abort', abortStagedWrite, { once: true });
    try {
      requireActiveDownload(signal);
      output = await handle.createWritable();
      requireActiveDownload(signal);
      await output.write(new Uint8Array(bytes));
      requireActiveDownload(signal);
      await output.close();
      requireActiveDownload(signal);
    } catch (error) {
      await output?.abort().catch(() => {});
      // The active pointer has not changed yet, so this newly owned staging
      // name is safe to reclaim even if the write failed after allocation.
      await root.removeEntry(fileName).catch(() => {});
      if (signal?.aborted) throw new Error('accelerator_download_cancelled');
      throw error;
    } finally {
      signal?.removeEventListener('abort', abortStagedWrite);
    }
    // The pointer write is the commit boundary. Cancellation before it must
    // leave the previously active generation untouched.
    if (signal?.aborted) {
      await root.removeEntry(fileName).catch(() => {});
      throw new Error('accelerator_download_cancelled');
    }
    const pointer: Active = {
      schema: 'clearra.exact-accelerator.local.v1', generation: plan.generation,
      catalog_identity: plan.catalog_identity, payload_identity: plan.payload_identity,
      payload_bytes: bytes.byteLength, file: fileName
    };
    let markerExisted = true;
    let marker: FileSystemWritableFileStream;
    try {
      try { await root.getFileHandle('active.json'); }
      catch (error) {
        if (!missing(error)) throw error;
        markerExisted = false;
      }
      marker = await (await root.getFileHandle('active.json', { create: true })).createWritable();
    } catch (error) {
      await root.removeEntry(fileName).catch(() => {});
      if (!markerExisted) await root.removeEntry('active.json').catch(() => {});
      throw error;
    }
    try {
      requireActiveDownload(signal);
      await marker.write(JSON.stringify(pointer));
      requireActiveDownload(signal);
      await marker.close();
    }
    catch (error) {
      await marker.abort().catch(() => {});
      // close() can fail after publishing. Keep the new bytes if the pointer
      // did commit; otherwise reclaim only this generation's staging file.
      const current = await activeFrom(root).catch(() => null);
      if (current?.file !== fileName) {
        await root.removeEntry(fileName).catch(() => {});
        if (!markerExisted) await root.removeEntry('active.json').catch(() => {});
      }
      if (signal?.aborted) throw new Error('accelerator_download_cancelled');
      throw error;
    }
    try { await cleanOld(root, fileName); return { cleanupPending: false }; }
    catch { return { cleanupPending: true }; }
  } finally { await release(); }
}

export async function readQualifiedAccelerator(plan: AcceleratorCatalogPlan): Promise<ArrayBuffer | null> {
  const release = await lease(plan.product, plan.profile, 'shared');
  try {
    const root = await directory(plan.product, plan.profile, false);
    const active = await activeFrom(root);
    if (!active || plan.state !== 'qualified' || active.generation !== plan.generation ||
        active.catalog_identity !== plan.catalog_identity || active.payload_identity !== plan.payload_identity ||
        active.payload_bytes !== plan.payload_bytes) return null;
    const file = await (await root.getFileHandle(active.file)).getFile();
    if (file.size !== active.payload_bytes) throw new Error('accelerator_store_size_mismatch');
    return await file.arrayBuffer();
  } catch (error) { if (missing(error)) return null; throw error; }
  finally { await release(); }
}

// An already-qualified WASM owner keeps immutable parsed bytes. On a later
// request it only needs to know whether OPFS still points to that same signed
// generation; re-reading and copying up to 64 MiB would defeat a warm asset.
export async function currentQualifiedAcceleratorIdentity(plan: AcceleratorCatalogPlan): Promise<string | null> {
  const release = await lease(plan.product, plan.profile, 'shared');
  try {
    const root = await directory(plan.product, plan.profile, false);
    const active = await activeFrom(root);
    if (!active || plan.state !== 'qualified' || active.generation !== plan.generation ||
        active.catalog_identity !== plan.catalog_identity || active.payload_identity !== plan.payload_identity ||
        active.payload_bytes !== plan.payload_bytes) return null;
    const file = await (await root.getFileHandle(active.file)).getFile();
    if (file.size !== active.payload_bytes) return null;
    return `${active.catalog_identity}:${active.generation}:${active.payload_identity}`;
  } catch (error) { if (missing(error)) return null; throw error; }
  finally { await release(); }
}

export async function removeLocalAccelerator(product: string, profile: string) {
  const release = await lease(product, profile, 'exclusive');
  try {
    const root = await directory(product, profile, false);
    // Deletion is the recovery path for a corrupt pointer. Its contents are
    // never trusted to choose a file to remove; only names in our namespace
    // are deleted below.
    try { await root.removeEntry('active.json'); } catch (error) { if (!missing(error)) throw error; }
    await cleanOld(root);
  } catch (error) { if (!missing(error)) throw error; }
  finally { await release(); }
}

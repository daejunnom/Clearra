// Explicit browser lifecycle for the two independent signed accelerator
// products. The product WASM validates catalog authority and the exact binary
// before an OPFS pointer is published. No request is made to a remote asset
// until the user presses Download.
import { loadClearraWasmModule, type AcceleratorCatalogPlan } from './clearraWasmRuntime';
import { acceleratorLocalStatus, removeLocalAccelerator, storeQualifiedAccelerator } from './acceleratorLocalStore';
import { acceleratorAssetLocation } from './acceleratorAssetLocation';

type Request = { action: 'status' | 'download' | 'remove' | 'cancel'; kind: number; profile: number; base: string };
const PROFILES = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const PRODUCTS = ['exact-legal-board', 'board-conditioned-reachability'];
const LIMITS = [64 * 1024 * 1024, 16 * 1024 * 1024];
const capabilities = {
  logicalProcessorCount: 1, webGpuAvailable: false, crossOriginIsolated: false,
  transferByteCap: 64 * 1024 * 1024, productRetentionByteCap: 128 * 1024 * 1024
};
let busy = false;
let controller: AbortController | null = null;
let displayed: AcceleratorCatalogPlan | null = null;

function selection(request: Request) {
  if (!Number.isInteger(request.kind) || !Number.isInteger(request.profile) ||
      !PRODUCTS[request.kind] || !PROFILES[request.profile]) throw new Error('accelerator_selection_invalid');
  return { product: PRODUCTS[request.kind], profile: PROFILES[request.profile] };
}

function repairableLocalAssetError(error: unknown) {
  if (error instanceof SyntaxError) return true;
  if (error instanceof DOMException) return error.name === 'NotFoundError' || error.name === 'NotReadableError';
  return error instanceof Error && [
    'accelerator_store_pointer_invalid',
    'accelerator_store_size_mismatch',
    'accelerator_store_digest_mismatch'
  ].includes(error.message);
}

async function planFor(request: Request) {
  const expected = selection(request);
  const wasm = await loadClearraWasmModule(undefined, capabilities);
  if (!wasm.accelerator_catalog || !wasm.accelerator_admit) throw new Error('accelerator_wasm_contract_unavailable');
  const plan = wasm.accelerator_catalog(request.kind, request.profile);
  if (plan.product !== expected.product || plan.profile !== expected.profile ||
      (plan.state !== 'qualified' && plan.state !== 'not_qualified') ||
      (plan.state === 'qualified' &&
        (!Number.isSafeInteger(plan.payload_bytes) || !plan.payload_bytes ||
          plan.payload_bytes > LIMITS[request.kind] || typeof plan.url !== 'string'))) {
    throw new Error('accelerator_catalog_plan_invalid');
  }
  return { wasm, plan };
}

async function download(plan: AcceleratorCatalogPlan, kind: number, profile: number, base: string, signal: AbortSignal) {
  if (plan.state !== 'qualified' || !plan.url || !plan.payload_bytes || !plan.payload_identity) {
    throw new Error('accelerator_not_qualified');
  }
  const location = acceleratorAssetLocation(plan, base, self.location.origin);
  const response = await fetch(location, { signal, cache: 'no-store', redirect: 'error' });
  if (!response.ok || !response.body) throw new Error('accelerator_download_response_invalid');
  const length = response.headers.get('content-length');
  if (length !== null && Number(length) !== plan.payload_bytes) throw new Error('accelerator_download_size_mismatch');
  const bytes = new Uint8Array(plan.payload_bytes);
  const reader = response.body.getReader();
  let offset = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (signal.aborted) throw new Error('accelerator_download_cancelled');
      if (offset + value.byteLength > bytes.byteLength) throw new Error('accelerator_download_size_mismatch');
      bytes.set(value, offset);
      offset += value.byteLength;
      postMessage({ type: 'progress', transferredBytes: offset, totalBytes: bytes.byteLength });
    }
  } finally { reader.releaseLock(); }
  if (offset !== bytes.byteLength) throw new Error('accelerator_download_size_mismatch');
  const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
    .map(value => value.toString(16).padStart(2, '0')).join('');
  if (digest !== plan.payload_identity) throw new Error('accelerator_download_digest_mismatch');
  if (signal.aborted) throw new Error('accelerator_download_cancelled');
  const wasm = await loadClearraWasmModule(undefined, capabilities);
  if (!wasm.accelerator_admit) throw new Error('accelerator_wasm_contract_unavailable');
  wasm.accelerator_admit(kind, profile, bytes.buffer, false);
  if (signal.aborted) throw new Error('accelerator_download_cancelled');
  return storeQualifiedAccelerator(plan, bytes, signal);
}

self.onmessage = async ({ data }: MessageEvent<Request>) => {
  if (data.action === 'cancel') { controller?.abort(); return; }
  if (busy) { postMessage({ type: 'error', code: 'accelerator_store_busy' }); return; }
  busy = true;
  controller = new AbortController();
  try {
    const expected = selection(data);
    if (data.action === 'remove') {
      await removeLocalAccelerator(expected.product, expected.profile);
      displayed = null;
      postMessage({ type: 'removed' });
    } else if (data.action === 'status') {
      const { plan } = await planFor(data);
      displayed = plan;
      try {
        postMessage({ type: 'status', plan, local: await acceleratorLocalStatus(expected.product, expected.profile, plan), localState: 'valid' });
      } catch (error) {
        if (!repairableLocalAssetError(error)) throw error;
        // A corrupt or unreadable local generation cannot confer authority,
        // but it must not hide the signed plan or block explicit repair.
        postMessage({ type: 'status', plan, local: null, localState: 'invalid_asset' });
      }
    } else if (data.action === 'download') {
      const { plan } = await planFor(data);
      if (plan.state !== 'qualified' || !displayed ||
          displayed.product !== plan.product || displayed.profile !== plan.profile ||
          displayed.catalog_identity !== plan.catalog_identity ||
          displayed.generation !== plan.generation) throw new Error('accelerator_download_check_required');
      const result = await download(plan, data.kind, data.profile, data.base, controller.signal);
      postMessage({ type: 'installed', local: await acceleratorLocalStatus(expected.product, expected.profile, plan), ...result });
    } else throw new Error('accelerator_action_invalid');
  } catch (error) {
    postMessage({ type: 'error', code: controller.signal.aborted ? 'accelerator_download_cancelled'
      : error instanceof Error ? error.message : 'accelerator_download_failed' });
  } finally { controller = null; busy = false; postMessage({ type: 'idle' }); }
};

import type { AcceleratorCatalogPlan } from './clearraWasmRuntime';

const PRODUCTS = new Set(['exact-legal-board', 'board-conditioned-reachability']);
const PROFILES = new Set(['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']);
const PRODUCT_PATH = { 'exact-legal-board': 'lb', 'board-conditioned-reachability': 'cr' } as const;

// The signed catalog still binds the immutable GitHub Release origin. Pages
// carries byte-identical, build-verified bytes at this same-origin location;
// the download worker independently checks length, digest and WASM admission.
export function acceleratorAssetLocation(plan: AcceleratorCatalogPlan, base: string, origin: string): URL {
  if (plan.state !== 'qualified' || !PRODUCTS.has(plan.product) || !PROFILES.has(plan.profile) ||
      !plan.url || !plan.payload_bytes || !plan.payload_identity ||
      !/^[a-f0-9]{64}$/.test(plan.payload_identity) ||
      (base !== '' && (!/^\/[A-Za-z0-9/_-]+$/.test(base) || base.endsWith('/')))) {
    throw new Error('accelerator_catalog_plan_invalid');
  }
  return new URL(`${base}/accel/${PRODUCT_PATH[plan.product]}/${plan.profile}/${plan.payload_identity}.bin`, origin);
}

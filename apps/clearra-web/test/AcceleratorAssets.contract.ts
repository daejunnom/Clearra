/// <reference types="vite/client" />
import assert from 'node:assert/strict';
import type { AcceleratorCatalogPlan, ClearraWasmModule } from '../src/workers/clearraWasmRuntime.ts';
import { acceleratorAssetLocation } from '../src/workers/acceleratorAssetLocation.ts';

// Pull the browser-only implementations through TypeScript without executing
// their Worker/OPFS globals in Node. The catalog and download products must
// remain independent of PC4's separate browser storage namespace.
type Store = typeof import('../src/workers/acceleratorLocalStore.ts');
type WorkerEntry = typeof import('../src/workers/acceleratorDownloadWorker.ts');
type SearchWorkerEntry = typeof import('../src/workers/clearraWorker.ts');
const supported: Store['acceleratorLocalStorageSupported'] = () => false;
const workerTypeExists: WorkerEntry | null = null;
const searchWorkerTypeExists: SearchWorkerEntry | null = null;
const plan: AcceleratorCatalogPlan = {
  product: 'exact-legal-board', profile: 'srs', state: 'not_qualified',
  payload_bytes: null, generation: null, payload_identity: null, url: null,
  catalog_identity: '0'.repeat(64)
};
const catalog: ClearraWasmModule['accelerator_catalog'] = undefined;
assert.equal(supported(), false);
assert.equal(workerTypeExists, null);
assert.equal(searchWorkerTypeExists, null);
assert.equal(catalog, undefined);
assert.equal(plan.state, 'not_qualified');
const qualified: AcceleratorCatalogPlan = {
  ...plan, state: 'qualified', payload_bytes: 123, generation: 'generation',
  payload_identity: 'a'.repeat(64), url: 'https://github.com/daejunnom/Clearra/releases/download/tag/asset.cllr'
};
assert.equal(acceleratorAssetLocation(qualified, '', 'http://127.0.0.1:4194').href,
  `http://127.0.0.1:4194/accel/lb/srs/${'a'.repeat(64)}.bin`);
assert.equal(acceleratorAssetLocation(qualified, '/Clearra', 'https://daejunnom.github.io').href,
  `https://daejunnom.github.io/Clearra/accel/lb/srs/${'a'.repeat(64)}.bin`);
assert.throws(() => acceleratorAssetLocation({ ...qualified, profile: '../other' }, '', 'https://example.org'));
assert.throws(() => acceleratorAssetLocation({ ...qualified, payload_identity: 'invalid' }, '', 'https://example.org'));

/// <reference types="vite/client" />
import assert from 'node:assert/strict';
import type { AcceleratorCatalogPlan, ClearraWasmModule } from '../src/workers/clearraWasmRuntime.ts';

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

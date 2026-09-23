/// <reference types="vite/client" />
import assert from 'node:assert/strict';
import type { AcceleratorCatalogPlan } from '../src/workers/clearraWasmRuntime.ts';
import {
  acceleratorLocalStatus,
  removeLocalAccelerator,
  storeQualifiedAccelerator
} from '../src/workers/acceleratorLocalStore.ts';

// An in-memory OPFS substitute exercises the product store's authority and
// recovery contract without using a real browser profile or remote asset.
let afterStagedWrite: (() => void) | null = null;
let afterPointerWrite: (() => void) | null = null;
class MemoryFile {
  bytes: Uint8Array<ArrayBuffer>;
  constructor(bytes: Uint8Array<ArrayBuffer>) { this.bytes = bytes; }
  get size() { return this.bytes.byteLength; }
  async arrayBuffer() { return this.bytes.slice().buffer; }
  async text() { return new TextDecoder().decode(this.bytes); }
}

class MemoryDirectory {
  readonly entries = new Map<string, MemoryFile | MemoryDirectory>();
  async getDirectoryHandle(name: string, options?: { create?: boolean }) {
    let entry = this.entries.get(name);
    if (!entry && options?.create) {
      entry = new MemoryDirectory();
      this.entries.set(name, entry);
    }
    if (!(entry instanceof MemoryDirectory)) throw new DOMException('missing directory', 'NotFoundError');
    return entry;
  }
  async getFileHandle(name: string, options?: { create?: boolean }) {
    let entry = this.entries.get(name);
    if (!entry && options?.create) {
      entry = new MemoryFile(new Uint8Array());
      this.entries.set(name, entry);
    }
    if (!(entry instanceof MemoryFile)) throw new DOMException('missing file', 'NotFoundError');
    return {
      getFile: async () => entry as MemoryFile,
      createWritable: async () => {
        let staged: Uint8Array<ArrayBuffer> | null = null;
        return {
          write: async (value: string | Uint8Array<ArrayBuffer>) => {
            staged = typeof value === 'string' ? new TextEncoder().encode(value) : value.slice();
            if (name.startsWith('gen-')) afterStagedWrite?.();
            if (name === 'active.json') afterPointerWrite?.();
          },
          close: async () => { if (staged) (entry as MemoryFile).bytes = staged; },
          abort: async () => { staged = null; }
        };
      }
    };
  }
  async removeEntry(name: string) {
    if (!this.entries.delete(name)) throw new DOMException('missing entry', 'NotFoundError');
  }
  async *keys() { yield* this.entries.keys(); }
}

const origin = new MemoryDirectory();
Object.defineProperty(globalThis, 'navigator', {
  configurable: true,
  value: {
    storage: { getDirectory: async () => origin, estimate: async () => ({ quota: 1_000_000_000, usage: 0 }) },
    locks: { request: async (_key: string, _options: unknown, callback: (lock: object) => Promise<void>) => callback({}) }
  }
});
const bytes = new Uint8Array([1, 2, 3, 4]);
const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
  .map(value => value.toString(16).padStart(2, '0')).join('');
const plan: AcceleratorCatalogPlan = {
  product: 'exact-legal-board', profile: 'srs', state: 'qualified',
  payload_bytes: bytes.byteLength, payload_identity: digest,
  generation: 'a'.repeat(64), catalog_identity: 'b'.repeat(64), url: 'https://example.invalid/asset'
};
const profile = async () => (await (await origin.getDirectoryHandle('clearra-exact-accelerators-v1'))
  .getDirectoryHandle('exact-legal-board')).getDirectoryHandle('srs');

await assert.rejects(storeQualifiedAccelerator(plan, new Uint8Array([4, 3, 2, 1])), /digest_mismatch/);
assert.equal(await acceleratorLocalStatus(plan.product, plan.profile, plan), null);

await storeQualifiedAccelerator(plan, bytes);
assert.equal((await acceleratorLocalStatus(plan.product, plan.profile, plan))?.current, true);
const local = await profile();
const pointer = JSON.parse(await (await (await local.getFileHandle('active.json')).getFile()).text());
const controller = new AbortController();
afterStagedWrite = () => controller.abort();
await assert.rejects(storeQualifiedAccelerator(plan, bytes, controller.signal), /accelerator_download_cancelled/);
afterStagedWrite = null;
const afterCancel = JSON.parse(await (await (await local.getFileHandle('active.json')).getFile()).text());
assert.equal(afterCancel.file, pointer.file);
assert.deepEqual([...local.entries.keys()].sort(), ['active.json', pointer.file].sort());
assert.equal((await acceleratorLocalStatus(plan.product, plan.profile, plan))?.current, true);
((await (await local.getFileHandle(pointer.file)).getFile()) as MemoryFile).bytes[0] ^= 1;
await assert.rejects(acceleratorLocalStatus(plan.product, plan.profile, plan), /digest_mismatch/);
await removeLocalAccelerator(plan.product, plan.profile);
assert.equal(await acceleratorLocalStatus(plan.product, plan.profile, plan), null);

// Cancellation while creating the first pointer must leave no invalid empty
// marker or candidate file behind.
const pointerController = new AbortController();
afterPointerWrite = () => pointerController.abort();
await assert.rejects(
  storeQualifiedAccelerator(plan, bytes, pointerController.signal),
  /accelerator_download_cancelled/
);
afterPointerWrite = null;
assert.deepEqual([...local.entries.keys()], []);
assert.equal(await acceleratorLocalStatus(plan.product, plan.profile, plan), null);

await storeQualifiedAccelerator(plan, bytes);
const pointer2 = JSON.parse(await (await (await local.getFileHandle('active.json')).getFile()).text());
await local.removeEntry(pointer2.file);
await assert.rejects(acceleratorLocalStatus(plan.product, plan.profile, plan), /NotFoundError/);
await removeLocalAccelerator(plan.product, plan.profile);
assert.equal(await acceleratorLocalStatus(plan.product, plan.profile, plan), null);

// An invalid pointer is still removable; its content is not used as a path.
const pointerHandle = await local.getFileHandle('active.json', { create: true });
const writable = await pointerHandle.createWritable();
await writable.write('{corrupt');
await writable.close();
await removeLocalAccelerator(plan.product, plan.profile);
assert.equal(await acceleratorLocalStatus(plan.product, plan.profile, plan), null);

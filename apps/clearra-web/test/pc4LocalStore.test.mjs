import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import { downloadPc4Profile } from '../../../scripts/release/pc4/pc4-download.mjs';
import { PC4_READER_CONTRACT } from '../../../scripts/release/pc4/qualify-upstream-generation.mjs';

const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/workers/pc4LocalStore.ts', import.meta.url))],
  bundle: true, write: false, platform: 'node', format: 'esm', logLevel: 'silent' });
const api = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text + '\n//# sourceURL=pc4LocalStore.contract.mjs').toString('base64')}`);
const hostBundle = await build({ entryPoints: [fileURLToPath(new URL('../src/workers/WasmJobRunner.ts', import.meta.url))],
  bundle: true, write: false, platform: 'node', format: 'esm', logLevel: 'silent' });
const { WasmJobRunner } = await import(`data:text/javascript;base64,${Buffer.from(hostBundle.outputFiles[0].text + '\n//# sourceURL=pc4LocalHost.contract.mjs').toString('base64')}`);
const absent = () => { throw new DOMException('missing', 'NotFoundError'); };
class Directory {
  entries = new Map();
  async getDirectoryHandle(name, { create = false } = {}) {
    if (!this.entries.has(name)) { if (!create) absent(); this.entries.set(name, new Directory()); }
    return this.entries.get(name);
  }
  async getFileHandle(name, { create = false } = {}) {
    if (!this.entries.has(name)) { if (!create) absent(); this.entries.set(name, new FileEntry()); }
    return this.entries.get(name);
  }
  async removeEntry(name) { if (!this.entries.delete(name)) absent(); }
  async *keys() { yield* [...this.entries.keys()]; }
}
class FileEntry {
  bytes = new Uint8Array();
  async getFile() { return new File([this.bytes], 'fixture'); }
  async createWritable() {
    let chunks = [];
    return { write: async value => { chunks.push(typeof value === 'string' ? new TextEncoder().encode(value) : new Uint8Array(value)); },
      close: async () => { this.bytes = Uint8Array.from(chunks.flatMap(c => [...c])); }, abort: async () => { chunks = []; } };
  }
}
function fixture() {
  const root = new Directory(), locks = new Map();
  const navigator = { storage: { getDirectory: async () => root, estimate: async () => ({ quota: 10_000_000, usage: 0 }) },
    locks: { async request(name, { mode }, callback) {
      const state = locks.get(name) ?? { shared: 0, exclusive: false }; locks.set(name, state);
      if (state.exclusive || (mode === 'exclusive' && state.shared)) return callback(null);
      if (mode === 'shared') state.shared++; else state.exclusive = true;
      try { return await callback({ name, mode }); }
      finally { if (mode === 'shared') state.shared--; else state.exclusive = false; }
    } } };
  const files = new Map(['field_hash_to_id.v1.bin','graph_offsets.u32.bin','graph.bin'].map((p, i) => [p, Uint8Array.from({ length: 50 + i }, (_, j) => j)]));
  const desc = [...files].map(([path, bytes]) => ({ path, byte_length: bytes.length, content_identity: 'sha256:' + createHash('sha256').update(bytes).digest('hex') }));
  const generation = { schema: 'clearra.pc4.host-generation.v1', repository: 'muse918/tetris-4lpc-mdp-vstar-policy', revision: 'a'.repeat(40), profiles:
    ['srs','srs-plus','srs-x','jstris-180','no-kick'].map(profile => profile === 'jstris-180'
      ? { profile, status: 'ready', upstream_complete: true, reader_contract: PC4_READER_CONTRACT,
        artifacts: { fields: desc[0], offsets: desc[1], graph: desc[2] } } : { profile, status: 'unavailable' }) };
  const install = async (value = generation) => downloadPc4Profile(value, api.pc4LocalDownloadStore, {
    intent: 'explicit-download', profile: 'jstris-180', fetcher: async url => new Response(files.get(url.split('/').at(-1))) });
  return { root, navigator, files, generation, desc, install };
}
async function withStorage(run) {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const f = fixture(); Object.defineProperty(globalThis, 'navigator', { value: f.navigator, configurable: true });
  try { await run(f); } finally { if (original) Object.defineProperty(globalThis, 'navigator', original); else delete globalThis.navigator; }
}

test('explicit OPFS install supplies local slices and prevents deletion/update during a search lease', async () => withStorage(async f => {
  assert.equal(await api.localPc4Status(), null);
  await f.install();
  assert.equal((await api.localPc4Status()).storedBytes, 153);
  const reader = await api.openLocalPc4Reader(f.generation);
  try {
    assert.equal(reader.provider, 'local-graph');
    assert.deepEqual(await reader.read(f.desc[2], 4, 8), f.files.get('graph.bin').slice(4, 12));
    assert.equal(reader.requests, 0); assert.equal(reader.bytes, 0); assert.equal(reader.localBytes, 8);
    await assert.rejects(api.removeLocalPc4(), { code: 'pc4_download_storage_busy' });
    await assert.rejects(f.install(), { code: 'pc4_download_storage_busy' });
    await assert.rejects(reader.read({ ...f.desc[2], path: 'graph_srsx.bin' }, 4, 8), { code: 'pc4_online_range_invalid' });
  } finally { await reader.dispose(); }
  await api.removeLocalPc4(); assert.equal(await api.localPc4Status(), null);
}));

test('per-profile lifecycle cannot use or remove another kick table data; updates keep one generation', async () => withStorage(async f => {
  await f.install();
  assert.equal(await api.localPc4Status('srs'), null);
  await api.removeLocalPc4('srs');
  assert.ok(await api.localPc4Status('jstris-180'));
  await f.install({ ...f.generation, revision: 'b'.repeat(40) });
  const profile = f.root.entries.get('clearra-pc4-v1').entries.get('jstris-180');
  assert.equal([...profile.entries.keys()].filter(k => k.startsWith('gen-')).length, 1);
  assert.equal(await api.openLocalPc4Reader(f.generation), null);
  assert.equal((await api.localPc4Status()).generation.revision, 'b'.repeat(40));
}));

test('quota/cancellation preserve the previous generation and release storage leases', async () => withStorage(async f => {
  await f.install();
  f.navigator.storage.estimate = async () => ({ quota: 2, usage: 1 });
  await assert.rejects(f.install({ ...f.generation, revision: 'b'.repeat(40) }), { code: 'pc4_download_insufficient_space' });
  assert.equal((await api.localPc4Status()).generation.revision, f.generation.revision);
  const controller = new AbortController(), reader = await api.openLocalPc4Reader(f.generation, controller.signal);
  controller.abort(); await reader.dispose();
  await assert.rejects(reader.read(f.desc[2], 0, 5), { code: 'pc4_online_cancelled' });
  await api.removeLocalPc4(); assert.equal(await api.localPc4Status(), null);
}));

test('WASM host uses typed local admission with zero HTTP and releases files before returning', async () => withStorage(async f => {
  await f.install();
  const original = globalThis.fetch;
  globalThis.fetch = async () => { throw new Error('a downloaded local generation must not issue HTTP'); };
  const events = [], observed = [];
  let admitted = false;
  const wasm = {
    start_job: () => 7,
    advance_job: () => {
      if (!admitted) return 'pending';
      events.push({ schema_version: 1, runtime: 'clearra-wasm', event: 'final_response', job_id: 7, response: { status: 'success' } });
      return 'completed';
    },
    online_pc4_pending: () => admitted ? null : { lookup_session: 9, request_id: 1, offset: 4, length: 8, artifact: f.desc[2] },
    online_pc4_admit: (job, response) => {
      assert.equal(job, 7);
      assert.deepEqual(response, { lookup_session: 9, request_id: 1, source: 'verified-local-file', bytes: [...f.files.get('graph.bin').slice(4, 12)] });
      admitted = true;
    },
    drain_job_events_json: () => JSON.stringify(events.splice(0)),
    cancel_job: () => { throw new Error('successful fixture must not cancel'); }
  };
  try {
    const result = await new WasmJobRunner(wasm, f.generation).run('clearra pc --tablebase', event => observed.push(event));
    assert.equal(result.event, 'final_response');
    const io = observed.at(-1).pc4_online;
    assert.equal(io.provider, 'local-graph'); assert.equal(io.requests, 0);
    assert.equal(io.transferred_bytes, 0); assert.equal(io.local_bytes, 8);
    await api.removeLocalPc4();
  } finally { globalThis.fetch = original; }
}));

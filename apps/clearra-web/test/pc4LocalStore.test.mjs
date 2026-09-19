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
  assert.equal(await api.openLocalPc4Reader(f.generation, undefined, 'srs'), null,
    'a different profile must never borrow the installed Jstris files');
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
    online_pc4_pending: () => admitted ? null : { lookup_session: 9, request_id: 1, profile: 'jstris-180', offset: 4, length: 8, artifact: f.desc[2] },
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
    assert.equal(io.local_file_reads, 1);
    assert.equal(io.local_file_access, 'blob-slice');
    await api.removeLocalPc4();
  } finally { globalThis.fetch = original; }
}));

async function withSyncStorage(run) {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'FileSystemSyncAccessHandle');
  const constructor = { prototype: { mode: 'read-only' } };
  Object.defineProperty(globalThis, 'FileSystemSyncAccessHandle', { value: constructor, configurable: true });
  try { await withStorage(async f => {
    await f.install();
    const profile = f.root.entries.get('clearra-pc4-v1').entries.get('jstris-180');
    const directory = [...profile.entries].find(([name]) => name.startsWith('gen-'))[1];
    const state = { opened: 0, closed: 0, active: 0, reads: 0, bytes: 0, blobCalls: 0 };
    for (const file of directory.entries.values()) {
      const getFile = file.getFile.bind(file);
      file.getFile = async () => { state.blobCalls++; return getFile(); };
      file.createSyncAccessHandle = async options => {
        assert.deepEqual(options, { mode: 'read-only' });
        state.opened++; state.active++;
        let closed = false;
        return { mode: 'read-only', getSize: () => file.bytes.length,
          read: (output, { at }) => {
            assert.equal(closed, false); state.reads++;
            const bytes = file.bytes.subarray(at, at + output.length);
            output.set(bytes); state.bytes += bytes.length; return bytes.length;
          },
          close: () => { assert.equal(closed, false, 'each OS handle closes exactly once'); closed = true; state.closed++; state.active--; }
        };
      };
    }
    await run({ ...f, directory, state, constructor });
  }); } finally {
    if (original) Object.defineProperty(globalThis, 'FileSystemSyncAccessHandle', original);
    else delete globalThis.FileSystemSyncAccessHandle;
  }
}

test('read-only OPFS handles are opened once and shared by concurrent search leases without Blob reads', async () => withSyncStorage(async f => {
  const first = await api.openLocalPc4Reader(f.generation), second = await api.openLocalPc4Reader(f.generation);
  assert.equal(first.fileAccess, 'sync-access-handle');
  try {
    for (let i = 0; i < 10000; i++) {
      const offset = i * 17 % 40;
      assert.deepEqual(await first.read(f.desc[2], offset, 12), f.files.get('graph.bin').slice(offset, offset + 12));
    }
    assert.deepEqual(await second.read(f.desc[1], 2, 16), f.files.get('graph_offsets.u32.bin').slice(2, 18));
    assert.equal(f.state.opened, 6); assert.equal(f.state.active, 6); assert.equal(f.state.blobCalls, 0);
    assert.equal(f.state.reads, 10001); assert.equal(first.requests, 0);
    await first.dispose(); await first.dispose();
    assert.equal(f.state.active, 3);
    await assert.rejects(api.removeLocalPc4(), { code: 'pc4_download_storage_busy' });
  } finally { await Promise.all([first.dispose(), second.dispose()]); }
  assert.equal(f.state.active, 0); assert.equal(f.state.closed, 6);
  await api.removeLocalPc4();
}));

test('an older exclusive-only API is never opened and unsupported optional mode stays local', async () => withSyncStorage(async f => {
  delete f.constructor.prototype.mode;
  const old = await api.openLocalPc4Reader(f.generation);
  assert.equal(old.fileAccess, 'blob-slice'); assert.equal(f.state.opened, 0);
  await old.dispose();
  f.constructor.prototype.mode = 'read-only';
  for (const file of f.directory.entries.values()) file.createSyncAccessHandle = async () => { throw new TypeError('unsupported mode'); };
  const reader = await api.openLocalPc4Reader(f.generation);
  try {
    assert.equal(reader.fileAccess, 'blob-slice');
    assert.deepEqual(await reader.read(f.desc[2], 0, 12), f.files.get('graph.bin').slice(0, 12));
    assert.equal(reader.requests, 0);
  } finally { await reader.dispose(); }
}));

test('partial read-only open failure closes earlier files and releases the generation lease', async () => withSyncStorage(async f => {
  f.directory.entries.get(f.desc[1].path).createSyncAccessHandle = async () => { throw new DOMException('denied', 'NotAllowedError'); };
  await assert.rejects(api.openLocalPc4Reader(f.generation), { name: 'NotAllowedError' });
  assert.equal(f.state.opened, 1); assert.equal(f.state.closed, 1); assert.equal(f.state.blobCalls, 0);
  await api.removeLocalPc4();
}));

test('cancellation during asynchronous handle opening closes every acquired file before returning', async () => withSyncStorage(async f => {
  const abort = new AbortController(), file = f.directory.entries.get(f.desc[1].path), create = file.createSyncAccessHandle;
  file.createSyncAccessHandle = async options => { const result = await create(options); abort.abort(); return result; };
  await assert.rejects(api.openLocalPc4Reader(f.generation, abort.signal), { code: 'pc4_online_cancelled' });
  assert.equal(f.state.opened, 2); assert.equal(f.state.closed, 2); assert.equal(f.state.active, 0);
  await api.removeLocalPc4();
}));

test('short sync reads are completed exactly, but a zero read never becomes padded data or HTTP', async () => withSyncStorage(async f => {
  const file = f.directory.entries.get(f.desc[2].path), create = file.createSyncAccessHandle;
  let stop = false;
  file.createSyncAccessHandle = async options => {
    const access = await create(options), read = access.read;
    access.read = (bytes, at) => stop ? 0 : read(bytes.subarray(0, Math.min(bytes.length, 3)), at);
    return access;
  };
  const reader = await api.openLocalPc4Reader(f.generation);
  try {
    assert.deepEqual(await reader.read(f.desc[2], 4, 8), f.files.get('graph.bin').slice(4, 12));
    stop = true;
    await assert.rejects(reader.read(f.desc[2], 8, 8), { code: 'pc4_online_truncated_range' });
    assert.equal(reader.requests, 0); assert.equal(f.state.blobCalls, 0);
  } finally { await reader.dispose(); }
  assert.equal(f.state.active, 0);
}));

test('same revision cannot lease local files with a different pinned artifact identity', async () => withSyncStorage(async f => {
  const changed = structuredClone(f.generation);
  changed.profiles.find(p => p.profile === 'jstris-180').artifacts.graph.content_identity = 'sha256:' + 'f'.repeat(64);
  await assert.rejects(api.openLocalPc4Reader(changed), { code: 'pc4_download_local_manifest_invalid' });
  assert.equal(f.state.opened, 0);
  await api.removeLocalPc4();
}));

test('a close error still closes other files, releases the lease and retires the failed WASM job', async () => withSyncStorage(async f => {
  const file = f.directory.entries.get(f.desc[0].path), create = file.createSyncAccessHandle;
  file.createSyncAccessHandle = async options => {
    const access = await create(options), close = access.close;
    access.close = () => { close(); throw new Error('close-failed'); };
    return access;
  };
  let cancelled = 0, admitted = false;
  const wasm = { start_job: () => 7, advance_job: () => { if (!admitted) return 'pending'; throw new Error('fixture-search-failed'); },
    online_pc4_pending: () => ({ lookup_session: 9, request_id: 1, profile: 'jstris-180', offset: 4, length: 8, artifact: f.desc[2] }),
    online_pc4_admit: () => { admitted = true; },
    drain_job_events_json: () => '[]', cancel_job: () => { cancelled++; } };
  const runner = new WasmJobRunner(wasm, f.generation);
  await assert.rejects(runner.run('clearra pc --tablebase', () => {}), /close-failed/);
  assert.equal(f.state.opened, 3); assert.equal(f.state.closed, 3); assert.equal(f.state.active, 0);
  assert.equal(cancelled, 1);
  runner.dispose(); assert.equal(cancelled, 1);
  await api.removeLocalPc4();
}));

test('disposing a Blob reader holds the generation lease until pending file reads settle', async () => withStorage(async f => {
  await f.install();
  const profile = f.root.entries.get('clearra-pc4-v1').entries.get('jstris-180');
  const directory = [...profile.entries].find(([name]) => name.startsWith('gen-'))[1];
  const file = directory.entries.get('graph.bin');
  let finish;
  file.getFile = async () => ({ size: file.bytes.length, slice: (offset, end) => ({
    arrayBuffer: () => new Promise(resolve => { finish = () => resolve(file.bytes.slice(offset, end).buffer); })
  }) });
  const reader = await api.openLocalPc4Reader(f.generation);
  const reading = reader.read(f.desc[2], 0, 8);
  const rejected = assert.rejects(reading, { code: 'pc4_online_cancelled' });
  let disposed = false;
  const disposing = reader.dispose().then(() => { disposed = true; });
  await assert.rejects(api.removeLocalPc4(), { code: 'pc4_download_storage_busy' });
  assert.equal(disposed, false);
  finish(); await rejected; await disposing;
  await api.removeLocalPc4();
}));

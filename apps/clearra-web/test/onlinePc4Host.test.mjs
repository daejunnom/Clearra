import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

// In-memory contract only: no build file or alternate cache root is created.
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/workers/WasmJobRunner.ts', import.meta.url))],
  bundle: true, write: false, platform: 'node', format: 'esm', logLevel: 'silent' });
const moduleUrl = `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text + '\n//# sourceURL=pc4OnlineHost.contract.mjs').toString('base64')}`;
const { WasmJobRunner } = await import(moduleUrl);
const generation = { schema: 'clearra.pc4.host-generation.v1', repository: 'muse918/tetris-4lpc-mdp-vstar-policy',
  revision: 'a'.repeat(40), profiles: [{ profile: 'jstris-180', status: 'ready', artifacts: {
    graph: { path: 'graph.bin', byte_length: 64000, content_identity: 'sha256:' + 'c'.repeat(64) }
  } }], transferred_bytes: 0 };
const range = { lookup_session: 9, request_id: 1, offset: 16, length: 8,
  artifact: { path: 'field_hash_to_id.v1.bin', byte_length: 32, content_identity: 'sha256:' + 'b'.repeat(64) } };

function fixture() {
  let admitted = false;
  const events = [], observed = [];
  const wasm = {
    start_job() { return 7; },
    advance_job() {
      if (!admitted) return 'pending';
      events.push({ schema_version: 1, runtime: 'clearra-wasm', event: 'final_response', job_id: 7,
        response: { status: 'success' } });
      return 'completed';
    },
    online_pc4_pending() { return admitted ? null : range; },
    online_pc4_admit(jobId, response) {
      assert.equal(jobId, 7);
      assert.deepEqual(response, { lookup_session: 9, request_id: 1, status: 206,
        content_range: 'bytes 16-23/32', bytes: [1,2,3,4,5,6,7,8] });
      admitted = true;
    },
    drain_job_events_json() { return JSON.stringify(events.splice(0)); },
    cancel_job() { events.push({ schema_version: 1, runtime: 'clearra-wasm', event: 'cancelled',
      job_id: 7, scope_released: true }); }
  };
  return { wasm, observed, emit: event => observed.push(event) };
}

test('online host forwards only verified partial bytes and records actual I/O', async () => {
  const original = fetch;
  try {
    globalThis.fetch = async (url, init) => {
      assert.match(url, /\/resolve\/a{40}\/field_hash_to_id.v1.bin$/);
      assert.equal(init.credentials, 'omit');
      assert.equal(init.headers.Range, 'bytes=16-23');
      return new Response(new Uint8Array([1,2,3,4,5,6,7,8]), { status: 206, headers: { 'content-range': 'bytes 16-23/32' } });
    };
    const f = fixture();
    const terminal = await new WasmJobRunner(f.wasm, generation).run('clearra pc --tablebase', f.emit);
    assert.equal(terminal.event, 'final_response');
    assert.equal(f.observed.at(-1).pc4_online.requests, 1);
    assert.equal(f.observed.at(-1).pc4_online.transferred_bytes, 8);
    assert.equal(f.observed.at(-1).pc4_online.provider, 'hf-graph');
  } finally { globalThis.fetch = original; }
});

test('online host consumes bounded WASM frontier hints but admits only each real pending range', async () => {
  const original = fetch;
  const count = 4096;
  const a = (path, byte_length, hash) => ({ path, byte_length, content_identity: 'sha256:' + hash.repeat(64) });
  const offsets = a('graph_offsets.u32.bin', 16 + 4 * (count + 1), 'b');
  const graph = a('graph.bin', count * 12, 'c');
  const bytes = new Uint8Array(offsets.byte_length), graphBytes = Uint8Array.from({ length: graph.byte_length }, (_, i) => i % 251);
  for (let id = 0; id <= count; id++) new DataView(bytes.buffer).setUint32(16 + id * 4, id * 12, true);
  const selected = { ...generation, profiles: [{ profile: 'jstris-180', status: 'ready', field_count: count,
    reader_contract: 'hydra-jstris-180-complete-graph-v1', artifacts: { offsets, graph } }] };
  const frontier = Array.from({ length: 32 }, (_, i) => i + 10);
  const demands = frontier.flatMap(id => [[offsets, 16 + id * 4, 8], [graph, id * 12, 12]])
    .map(([artifact, offset, length], i) => ({ artifact, offset, length, profile: 'jstris-180',
      lookup_session: 9, request_id: i + 1, lookup_frontier: frontier }));
  let cursor = 0, calls = 0;
  const events = [], observed = [];
  const wasm = {
    start_job: () => 7,
    advance_job: () => {
      if (cursor < demands.length) return 'pending';
      events.push({ event: 'final_response', job_id: 7, response: { status: 'success' } });
      return 'completed';
    },
    online_pc4_pending: () => demands[cursor],
    online_pc4_admit: (job, response) => {
      const r = demands[cursor++];
      assert.equal(job, 7);
      assert.deepEqual(response, { lookup_session: 9, request_id: r.request_id, status: 206,
        content_range: `bytes ${r.offset}-${r.offset + r.length - 1}/${r.artifact.byte_length}`,
        bytes: Array.from((r.artifact.path === offsets.path ? bytes : graphBytes).slice(r.offset, r.offset + r.length)) });
    },
    drain_job_events_json: () => JSON.stringify(events.splice(0)),
    cancel_job: () => assert.fail('successful frontier transport must not cancel')
  };
  try {
    globalThis.fetch = async (url, init) => {
      const file = url.endsWith('/' + offsets.path) ? offsets : graph;
      assert.ok(url.endsWith('/' + file.path));
      const [, start, last] = /^bytes=(\d+)-(\d+)$/.exec(init.headers.Range);
      const o = Number(start), end = Number(last) + 1;
      calls++;
      return new Response((file === offsets ? bytes : graphBytes).slice(o, end), {
        status: 206, headers: { 'content-range': `bytes ${o}-${end - 1}/${file.byte_length}` }
      });
    };
    await new WasmJobRunner(wasm, selected).run('clearra pc --tablebase', event => observed.push(event));
    assert.equal(cursor, 64, 'no pending request may be skipped or replaced with prefetched admission');
    assert.equal(calls, 2);
    assert.equal(observed.at(-1).pc4_online.requests, 2);
    assert.equal(observed.at(-1).pc4_online.transferred_bytes, 4480);
  } finally { globalThis.fetch = original; }
});

test('whole-body response fails without entering an offline solver', async () => {
  const original = fetch;
  try {
    globalThis.fetch = async () => new Response(new Uint8Array(32), { status: 200 });
    const f = fixture();
    await assert.rejects(new WasmJobRunner(f.wasm, generation).run('clearra pc --tablebase', f.emit),
      { code: 'pc4_online_whole_content_rejected' });
    assert.equal(f.observed.some(event => event.event === 'final_response'), false);
  } finally { globalThis.fetch = original; }
});

test('cancelling an in-flight Range drains cancellation instead of reporting a network failure', async () => {
  const original = fetch;
  let started;
  const waiting = new Promise(resolve => { started = resolve; });
  try {
    globalThis.fetch = (_url, { signal }) => new Promise((_resolve, reject) => {
      signal.addEventListener('abort', () => reject(new DOMException('cancelled', 'AbortError')), { once: true });
      started();
    });
    const f = fixture();
    const runner = new WasmJobRunner(f.wasm, generation);
    const run = runner.run('clearra pc --tablebase', f.emit);
    await waiting;
    runner.cancel();
    assert.equal((await run).event, 'cancelled');
    assert.equal(f.observed.at(-1).event, 'cancelled');
  } finally { globalThis.fetch = original; }
});

test('an outstanding host yield keeps Node alive but an idle runner does not', () => {
  // Isolated child: no unrelated test/network timer can mask an unref race.
  // A synthetic monotonic clock forces the yield without a timing threshold.
  const script = `
    import { WasmJobRunner } from ${JSON.stringify(moduleUrl)};
    let ticks = 0, advances = 0;
    Object.defineProperty(globalThis, 'performance', { value: { now: () => ticks += 100 } });
    const events = [];
    const runner = new WasmJobRunner({ start_job: () => 1,
      advance_job: () => { if (++advances < 3) return 'pending';
        events.push({ event: 'final_response', job_id: 1 }); return 'completed'; },
      drain_job_events_json: () => JSON.stringify(events.splice(0)), cancel_job: () => {} });
    const result = await runner.run('fixture', () => {});
    if (result.event !== 'final_response' || advances !== 3) throw Error('yield did not drain');
    console.log('host_yield_drained=1');
  `;
  const result = spawnSync(process.execPath, ['--input-type=module'], { input: script, encoding: 'utf8', timeout: 10_000, windowsHide: true });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /host_yield_drained=1/);
});

function batchFixture(count = 3) {
  const requests = Array.from({ length: count }, (_, i) => ({ lookup_session: i + 10, request_id: 1,
    profile: 'jstris-180', offset: i * 8192, length: 1, artifact: generation.profiles[0].artifacts.graph }));
  const admitted = new Set(), order = [], events = [];
  let cpu = 0, cancelled = false;
  const wasm = {
    start_job: () => 7,
    advance_job() {
      if (cpu < 16) cpu++;
      if (admitted.size < count) return 'pending';
      events.push({ event: 'final_response', job_id: 7, response: { status: 'success' } });
      return 'completed';
    },
    online_pc4_pending() {
      const batch = requests.filter(r => !admitted.has(r.lookup_session));
      return batch.length ? { ...batch[0], batch, can_advance: cpu < 16 } : null;
    },
    online_pc4_admit(job, response) {
      assert.equal(job, 7); assert.equal(cancelled, false);
      assert.equal(admitted.has(response.lookup_session), false, 'no duplicate admission');
      const range = requests.find(r => r.lookup_session === response.lookup_session);
      assert.deepEqual(response.bytes, [range.lookup_session]);
      admitted.add(response.lookup_session); order.push(response.lookup_session);
    },
    drain_job_events_json: () => JSON.stringify(events.splice(0)),
    cancel_job() { cancelled = true; events.push({ event: 'cancelled', job_id: 7 }); }
  };
  return { wasm, requests, order, get cpu() { return cpu; }, get cancelled() { return cancelled; } };
}

function deferredFetch(f) {
  const pending = new Map(), starts = [];
  let active = 0, peak = 0, cpuAtFirst = -1, afterStart = () => {};
  const fetch = (_url, { headers, signal }) => new Promise((resolve, reject) => {
    const offset = Number(/^bytes=(\d+)-/.exec(headers.Range)[1]);
    const request = f.requests.find(r => r.offset === offset);
    assert.ok(request, 'only known demands are transmitted');
    if (cpuAtFirst < 0) cpuAtFirst = f.cpu;
    starts.push(request.lookup_session); active++; peak = Math.max(peak, active);
    const abort = () => { active--; pending.delete(request.lookup_session); reject(new DOMException('cancelled', 'AbortError')); };
    signal.addEventListener('abort', abort, { once: true });
    pending.set(request.lookup_session, (status = 206) => {
      signal.removeEventListener('abort', abort); active--; pending.delete(request.lookup_session);
      resolve(new Response(Uint8Array.of(request.lookup_session), { status,
        headers: { 'content-range': `bytes ${offset}-${offset}/64000` } }));
    });
    afterStart();
  });
  return { fetch, pending, starts, get peak() { return peak; }, get cpuAtFirst() { return cpuAtFirst; },
    set afterStart(callback) { afterStart = callback; } };
}

test('batch host advances CPU while the first HTTP waits and admits faster responses without a batch barrier', { timeout: 5000 }, async () => {
  const original = fetch, f = batchFixture(), transport = deferredFetch(f);
  let firstDone = false;
  try {
    globalThis.fetch = transport.fetch;
    transport.afterStart = () => {
      if (transport.starts.length === 3) {
        transport.pending.get(12)(); transport.pending.get(11)();
      }
    };
    const admit = f.wasm.online_pc4_admit;
    f.wasm.online_pc4_admit = (job, response) => {
      assert.equal(firstDone, response.lookup_session === 10);
      admit(job, response);
      if (f.order.length === 2) { firstDone = true; transport.pending.get(10)(); }
    };
    const result = await new WasmJobRunner(f.wasm, generation).run('fixture', () => {});
    assert.equal(result.event, 'final_response');
    // Stream-body completion can reorder the two fast responses. Both must
    // be admitted before the slow request; their mutual order is immaterial.
    assert.deepEqual(f.order.slice(0, 2).sort(), [11, 12]);
    assert.equal(f.order[2], 10);
    assert.equal(transport.peak, 3);
    assert.ok(f.cpu > transport.cpuAtFirst, 'CPU advances while real fetch promises are pending');
    assert.equal(transport.starts.length, 3, 'repeated pending snapshots do not reissue requests');
  } finally { globalThis.fetch = original; }
});

test('batch host retains the reader concurrency cap and cancellation discards every late response', { timeout: 5000 }, async () => {
  const original = fetch, f = batchFixture(8), transport = deferredFetch(f);
  let started;
  const ready = new Promise(resolve => { started = resolve; });
  transport.afterStart = () => { if (transport.starts.length === 4) started(); };
  try {
    globalThis.fetch = transport.fetch;
    const runner = new WasmJobRunner(f.wasm, generation);
    const run = runner.run('fixture', () => {});
    await ready;
    assert.equal(transport.peak, 4, 'CPU requests are not physical HTTP concurrency');
    runner.cancel();
    assert.equal((await run).event, 'cancelled');
    assert.deepEqual(f.order, []);
    assert.equal(transport.pending.size, 0);
    assert.equal(transport.starts.length, 4, 'queued work is not sent after cancellation');
  } finally { globalThis.fetch = original; }
});

test('one bad parallel HTTP response cancels peers and cannot publish partial completion', { timeout: 5000 }, async () => {
  const original = fetch, f = batchFixture(), transport = deferredFetch(f);
  transport.afterStart = () => { if (transport.starts.length === 3) transport.pending.get(11)(200); };
  try {
    globalThis.fetch = transport.fetch;
    await assert.rejects(new WasmJobRunner(f.wasm, generation).run('fixture', () => {}),
      { code: 'pc4_online_whole_content_rejected' });
    assert.equal(transport.pending.size, 0);
    assert.equal(f.cancelled, true);
    assert.deepEqual(f.order, []);
  } finally { globalThis.fetch = original; }
});

test('an oversized pending batch is rejected before any HTTP is emitted', async () => {
  const original = fetch, f = batchFixture(17);
  let calls = 0;
  try {
    globalThis.fetch = () => { calls++; assert.fail('invalid batch must not send'); };
    await assert.rejects(new WasmJobRunner(f.wasm, generation).run('fixture', () => {}), { code: 'pc4_online_pending_limit' });
    assert.equal(calls, 0); assert.equal(f.cancelled, true);
  } finally { globalThis.fetch = original; }
});

test('known nearby batch ranges use one HTTP span but retain independent request admission', async () => {
  const original = fetch, f = batchFixture();
  f.requests.forEach((range, i) => { range.offset = i * 32; });
  let calls = 0;
  const events = [];
  try {
    globalThis.fetch = async (_url, init) => {
      calls++; assert.equal(init.headers.Range, 'bytes=0-64');
      const bytes = new Uint8Array(65);
      for (const range of f.requests) bytes[range.offset] = range.lookup_session;
      return new Response(bytes, { status: 206, headers: { 'content-range': 'bytes 0-64/64000' } });
    };
    await new WasmJobRunner(f.wasm, generation).run('fixture', event => events.push(event));
    assert.equal(calls, 1);
    assert.deepEqual(f.order, [10, 11, 12]);
    assert.equal(events.at(-1).pc4_online.transferred_bytes, 65);
    assert.equal(events.at(-1).pc4_online.requests, 1);
  } finally { globalThis.fetch = original; }
});

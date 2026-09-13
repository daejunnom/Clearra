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
  revision: 'a'.repeat(40), profiles: [], transferred_bytes: 0 };
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

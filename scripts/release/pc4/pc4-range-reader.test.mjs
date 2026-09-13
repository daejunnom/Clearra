import assert from 'node:assert/strict';
import test from 'node:test';
import { setImmediate } from 'node:timers/promises';
import { createPc4RangeReader } from './pc4-range-reader.mjs';

const generation = { repository: 'example/pc4', revision: 'a'.repeat(40) };
const artifact = { path: 'graph.bin', byte_length: 1_048_581, content_identity: 'sha256:' + 'b'.repeat(64) };
const values = (offset, length) => Uint8Array.from({ length }, (_, i) => ((offset + i) * 17 + 9) % 251);
function fixture(options = {}) {
  const ranges = [];
  let active = 0, peak = 0;
  const reader = createPc4RangeReader(generation, { ...options, fetcher: async (url, init) => {
    const [, a, b] = /^bytes=(\d+)-(\d+)$/.exec(init.headers.Range);
    const start = Number(a), end = Number(b);
    assert.equal(init.credentials, 'omit');
    assert.ok(end - start < 65_536);
    ranges.push({ url, start, end }); active++; peak = Math.max(peak, active);
    await setImmediate(); active--;
    return new Response(values(start, end - start + 1), {
      status: 206, headers: { 'content-range': `bytes ${start}-${end}/${artifact.byte_length}` }
    });
  } });
  return { reader, ranges, get peak() { return peak; } };
}

test('demand windows reuse contained bytes and join identical in-flight HTTP, without aliasing callers', async () => {
  const f = fixture();
  try {
    const result = await Promise.all([f.reader.read(artifact, 16, 8), f.reader.read(artifact, 80, 9)]);
    assert.deepEqual(result[0], values(16, 8)); assert.deepEqual(result[1], values(80, 9));
    assert.equal(f.reader.requests, 1); assert.equal(f.reader.joinedRequests, 1);
    result[0][0] = 255;
    assert.deepEqual(await f.reader.read(artifact, 16, 8), values(16, 8));
    assert.equal(f.reader.cacheHits, 1); assert.equal(f.reader.retainedBytes, 16_384);
    assert.equal(f.ranges[0].start, 0); assert.equal(f.ranges[0].end, 16_383);
  } finally { f.reader.dispose(); }
});

test('window seams, maximum demand and short EOF window return only exact requested bytes', async () => {
  const f = fixture();
  try {
    for (const [offset, length] of [[16_380, 20], [65_533, 65_536], [artifact.byte_length - 11, 11]]) {
      assert.deepEqual(await f.reader.read(artifact, offset, length), values(offset, length));
    }
    assert.ok(f.peak <= 4);
    assert.ok(f.ranges.every(({ start, end }) => end < artifact.byte_length && end >= start));
  } finally { f.reader.dispose(); }
});

test('LRU obeys both its byte bound and immutable artifact identity', async () => {
  const f = fixture({ cacheBytes: 32_768 });
  try {
    for (const offset of [0, 16_384, 0, 32_768, 16_384]) await f.reader.read(artifact, offset, 8);
    assert.equal(f.reader.requests, 4); assert.equal(f.reader.cacheHits, 1);
    assert.equal(f.reader.retainedBytes, 32_768);
    await f.reader.read({ ...artifact, content_identity: 'sha256:' + 'c'.repeat(64) }, 16_384, 8);
    assert.equal(f.reader.requests, 5);
    assert.ok(f.reader.retainedBytes <= 32_768);
  } finally { f.reader.dispose(); }
});

test('parallel I/O reserves the shared byte budget before starting each request', async () => {
  const f = fixture({ maxBytes: 32_768 });
  try {
    const results = await Promise.allSettled([0, 16_384, 32_768].map(offset => f.reader.read(artifact, offset, 8)));
    assert.equal(results.filter(result => result.status === 'fulfilled').length, 2);
    assert.equal(results.find(result => result.status === 'rejected').reason.code, 'pc4_online_transfer_limit');
    assert.equal(f.reader.requests, 2); assert.equal(f.reader.bytes, 32_768);
    // An exhausted transfer budget does not invalidate already verified bytes.
    assert.deepEqual(await f.reader.read(artifact, 16, 8), values(16, 8));
    assert.equal(f.reader.requests, 2);
  } finally { f.reader.dispose(); }
});

test('cancellation rejects active and queued windows without starting the queued I/O', async () => {
  const controller = new AbortController();
  let calls = 0;
  const reader = createPc4RangeReader(generation, { signal: controller.signal, maxConcurrent: 1,
    fetcher: (_url, { signal }) => new Promise((_resolve, reject) => {
      calls++;
      signal.addEventListener('abort', () => reject(new DOMException('cancelled', 'AbortError')), { once: true });
    }) });
  const pending = [0, 16_384, 32_768].map(offset => reader.read(artifact, offset, 8));
  controller.abort();
  const results = await Promise.allSettled(pending);
  assert.equal(calls, 1);
  assert.ok(results.every(result => result.status === 'rejected' && result.reason.code === 'pc4_online_cancelled'));
  assert.equal(reader.retainedBytes, 0); reader.dispose();
});

test('no cache, tiny budgets and small artifacts retain exact partial access', async () => {
  for (const options of [{ cacheBytes: 0 }, { windowBytes: 0 }, { maxBytes: 8 }]) {
    const f = fixture(options);
    try {
      assert.deepEqual(await f.reader.read(artifact, 16, 8), values(16, 8));
      assert.equal(f.ranges[0].start, 16); assert.equal(f.ranges[0].end, 23);
    } finally { f.reader.dispose(); }
  }
  const tiny = { ...artifact, byte_length: 20 };
  const reader = createPc4RangeReader(generation, { fetcher: async (_url, init) => {
    assert.equal(init.headers.Range, 'bytes=3-5');
    return new Response(values(3, 3), { status: 206, headers: { 'content-range': 'bytes 3-5/20' } });
  } });
  try { assert.deepEqual(await reader.read(tiny, 3, 3), values(3, 3)); }
  finally { reader.dispose(); }
});

test('queued transport snapshots descriptors and keeps the active-request bound', async () => {
  const f = fixture({ maxConcurrent: 2 });
  try {
    const descriptor = { ...artifact };
    const pending = Array.from({ length: 10 }, (_, i) => f.reader.read(descriptor, i * 16_384, 8));
    descriptor.path = 'other.bin'; descriptor.byte_length = 5;
    const results = await Promise.all(pending);
    assert.equal(results.length, 10); assert.equal(f.peak, 2);
    assert.ok(f.ranges.every(({ url }) => url.endsWith('/graph.bin')));
  } finally { f.reader.dispose(); }
});

test('exact/window A/B returns identical bytes for a clustered lookup demand trace', async t => {
  const exact = fixture({ windowBytes: 0 }), windowed = fixture();
  try {
    const trace = Array.from({ length: 320 }, (_, i) => [Math.floor(i / 40) * 16_384 + (i % 40) * 8, 8]);
    for (const [offset, length] of trace) {
      assert.deepEqual(await windowed.reader.read(artifact, offset, length), await exact.reader.read(artifact, offset, length));
    }
    assert.equal(exact.reader.requests, 320); assert.equal(windowed.reader.requests, 8);
    assert.equal(windowed.reader.cacheHits, 312);
    t.diagnostic(JSON.stringify({ evidence: 'synthetic-byte-trace-not-search-timing', logical_reads: 320,
      exact_requests: exact.reader.requests, window_requests: windowed.reader.requests,
      exact_bytes: exact.reader.bytes, window_bytes: windowed.reader.bytes }));
  } finally { exact.reader.dispose(); windowed.reader.dispose(); }
});

test('invalid limits and identities fail before opening a connection', async () => {
  for (const options of [{ windowBytes: 3 }, { maxConcurrent: 0 }, { maxConcurrent: 17 }, { cacheBytes: -1 }]) {
    assert.throws(() => createPc4RangeReader(generation, options), { code: 'pc4_online_limits_invalid' });
  }
  const reader = createPc4RangeReader(generation, { fetcher: () => assert.fail('must not fetch') });
  try {
    await assert.rejects(reader.read({ ...artifact, content_identity: 'mutable' }, 0, 8), { code: 'pc4_online_range_invalid' });
  } finally { reader.dispose(); }
});

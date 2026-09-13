import assert from 'node:assert/strict';
import test from 'node:test';
import { createPc4RangeReader } from './pc4-range-reader.mjs';
import { prefetchPc4LookupFrontier } from './pc4-frontier-reader.mjs';
import { PC4_READER_CONTRACT } from './qualify-upstream-generation.mjs';

function fixture({ mutate, count = 128, ...options } = {}) {
  const a = (path, byte_length, hash) => ({ path, byte_length, content_identity: 'sha256:' + hash.repeat(64) });
  const offsets = a('graph_offsets.u32.bin', 16 + 4 * (count + 1), 'b');
  const graph = a('graph.bin', count * 12, 'c');
  const indexBytes = new Uint8Array(offsets.byte_length);
  for (let i = 0; i <= count; i++) new DataView(indexBytes.buffer).setUint32(16 + i * 4, i * 12, true);
  const graphBytes = Uint8Array.from({ length: graph.byte_length }, (_, i) => i % 251);
  const generation = { repository: 'example/pc4', revision: 'a'.repeat(40), profiles: [{
    profile: 'jstris-180', status: 'ready', reader_contract: PC4_READER_CONTRACT,
    field_count: count, artifacts: { offsets, graph }
  }] };
  const calls = [];
  const reader = createPc4RangeReader(generation, { windowBytes: 4096, directPaths: [graph.path], ...options,
    fetcher: async (url, init) => {
      const record = [[offsets, indexBytes], [graph, graphBytes]].find(([file]) => url.endsWith('/' + file.path));
      assert.ok(record, 'only the selected profile files may be read');
      const [file, data] = record;
      const [, begin, stop] = /^bytes=(\d+)-(\d+)$/.exec(init.headers.Range);
      const start = Number(begin), end = Number(stop) + 1;
      calls.push({ path: file.path, start, end });
      const bytes = data.slice(start, end);
      mutate?.({ generation, file, bytes, start });
      return new Response(bytes, { status: 206, headers: { 'content-range': `bytes ${start}-${end - 1}/${file.byte_length}` } });
    } });
  const range = { profile: 'jstris-180', artifact: { ...offsets }, offset: 56, length: 8,
    lookup_frontier: Array.from({ length: 32 }, (_, i) => i + 10) };
  return { reader, generation, calls, range, graphBytes, indexBytes };
}

test('a known 32-record frontier reuses cached offsets and needs only the current graph transfer', async t => {
  const serial = fixture({ count: 4096 }), batch = fixture({ count: 4096 });
  try {
    await prefetchPc4LookupFrontier(batch.reader, batch.generation, batch.range);
    for (const id of batch.range.lookup_frontier) {
      for (const f of [serial, batch]) {
        assert.deepEqual(await f.reader.read(f.range.artifact, 16 + id * 4, 8), f.indexBytes.slice(16 + id * 4, 24 + id * 4));
        const graph = f.generation.profiles[0].artifacts.graph;
        const bytes = await f.reader.read(graph, id * 12, 12);
        assert.deepEqual(bytes, f.graphBytes.slice(id * 12, (id + 1) * 12));
        bytes.fill(255); // Caller mutation must not contaminate a batched span.
      }
    }
    assert.equal(serial.reader.requests, 33);
    assert.equal(batch.reader.requests, 2);
    assert.equal(batch.reader.bytes, 4096 + 384);
    await prefetchPc4LookupFrontier(batch.reader, batch.generation, batch.range);
    assert.equal(batch.reader.requests, 2, 'repeated hints cannot refetch known byte spans');
    assert.ok(batch.reader.retainedBytes <= 8 * 1024 * 1024);
    t.diagnostic('synthetic cached-index frontier transport A/B: 33 -> 2 requests; not a complete PC timing');
  } finally { serial.reader.dispose(); batch.reader.dispose(); }
});

test('large-index frontiers preserve containing-page reuse including an offset-pair seam', async () => {
  const f = fixture({ count: 4096 });
  const ids = [1019, 1020]; // 4092..4100 crosses the 4 KiB index seam.
  const range = { ...f.range, offset: 16 + ids[0] * 4, lookup_frontier: ids };
  try {
    await prefetchPc4LookupFrontier(f.reader, f.generation, range);
    assert.equal(f.calls.length, 3, 'the normal seam read needs two index pages, followed by one graph span');
    assert.deepEqual(f.calls[0], { path: range.artifact.path, start: 0, end: 4096 });
    assert.deepEqual(f.calls[1], { path: range.artifact.path, start: 4096, end: 8192 });
    assert.deepEqual(await f.reader.read(range.artifact, 100 * 4 + 16, 8), f.indexBytes.slice(416, 424));
    assert.equal(f.calls.length, 3, 'a later sibling reuses the containing index pages');
  } finally { f.reader.dispose(); }
});

test('frontier validation is bounded, profile-specific and never triggers on header or single-record requests', async () => {
  const f = fixture();
  try {
    for (const range of [{ ...f.range, lookup_frontier: undefined }, { ...f.range, lookup_frontier: [10] }, { ...f.range, offset: 0, length: 16 }]) {
      await prefetchPc4LookupFrontier(f.reader, f.generation, range);
    }
    for (const ids of [Array(33).fill(10), [10, 128], [10, -1], [10, 0.1], [11, 12], 'invalid']) {
      await assert.rejects(prefetchPc4LookupFrontier(f.reader, f.generation, { ...f.range, lookup_frontier: ids }), { code: 'pc4_online_frontier_invalid' });
    }
    await assert.rejects(prefetchPc4LookupFrontier(f.reader, f.generation, { ...f.range, profile: 'srs-x' }), { code: 'pc4_online_profile_not_qualified' });
    assert.equal(f.calls.length, 0);
  } finally { f.reader.dispose(); }
});

test('malformed offset pairs, exhausted transfer budget and cancellation never start the graph stage', async () => {
  const bad = fixture({ mutate: ({ bytes }) => new DataView(bytes.buffer).setUint32(4, 0, true) });
  const budget = fixture({ maxBytes: 7 });
  const controller = new AbortController();
  const cancelled = fixture({ signal: controller.signal, mutate: () => controller.abort() });
  for (const [f, code] of [[bad, 'pc4_online_record_bounds'], [budget, 'pc4_online_transfer_limit'], [cancelled, 'pc4_online_cancelled']]) {
    try {
      await assert.rejects(prefetchPc4LookupFrontier(f.reader, f.generation, f.range), { code });
      assert.ok(f.calls.every(call => call.path === 'graph_offsets.u32.bin'));
    } finally { f.reader.dispose(); }
  }
});

test('a frontier snapshots graph identity before the index await', async () => {
  let original;
  const f = fixture({ count: 4096, mutate: ({ generation, file }) => {
    if (file.path !== 'graph_offsets.u32.bin') return;
    original = generation.profiles[0].artifacts.graph;
    generation.profiles[0].artifacts.graph = { ...original, path: 'other-profile.bin' };
  } });
  try {
    await prefetchPc4LookupFrontier(f.reader, f.generation, f.range);
    assert.equal(f.calls.length, 2);
    assert.equal(f.calls[1].path, 'graph.bin');
  } finally { f.reader.dispose(); }
});

test('uncached or distant sibling offsets never cause speculative HTTP requests', async () => {
  const f = fixture({ count: 4096 });
  const range = { ...f.range, lookup_frontier: [10, 2048, 3000] };
  try {
    await prefetchPc4LookupFrontier(f.reader, f.generation, range);
    assert.equal(f.calls.length, 0, 'distant IDs are rejected before any prefetch preparation');
    assert.equal(f.reader.readCached(range.artifact, 16 + 2048 * 4, 8), null);
    assert.equal(f.calls.length, 0, 'cache inspection itself must not start I/O');
  } finally { f.reader.dispose(); }
});

test('the 12-byte record lower bound keeps the 86-ID seam and its connected chains', async () => {
  for (const [ids, calls] of [[[10, 96], 2], [[10, 97], 0], [[10, 96, 182], 2]]) {
    const f = fixture({ count: 4096 });
    try {
      await prefetchPc4LookupFrontier(f.reader, f.generation, { ...f.range, lookup_frontier: ids }, { maxGapBytes: 1024 });
      assert.equal(f.calls.length, calls, JSON.stringify(ids));
      if (calls) assert.equal(f.calls[1].end, (ids.at(-1) + 1) * 12);
    } finally { f.reader.dispose(); }
  }
});

test('the larger measured gap keeps the 342-ID seam, bounded chains and exact bytes', async () => {
  for (const [ids, calls] of [[[10, 352], 2], [[10, 353], 0], [[10, 352, 694], 2]]) {
    const f = fixture({ count: 4096 });
    try {
      await prefetchPc4LookupFrontier(f.reader, f.generation, { ...f.range, lookup_frontier: ids });
      assert.equal(f.calls.length, calls, JSON.stringify(ids));
      if (calls) {
        assert.equal(f.calls[1].end, (ids.at(-1) + 1) * 12);
        for (const id of ids) {
          assert.deepEqual(await f.reader.read(f.generation.profiles[0].artifacts.graph, id * 12, 12), f.graphBytes.slice(id * 12, id * 12 + 12));
        }
        assert.equal(f.calls.length, 2, 'known siblings use one contiguous graph transfer');
      }
    } finally { f.reader.dispose(); }
  }
  const f = fixture();
  try {
    for (const maxGapBytes of [-1, 4097, 0.5, Infinity]) {
      await assert.rejects(prefetchPc4LookupFrontier(f.reader, f.generation, f.range, { maxGapBytes }), { code: 'pc4_online_batch_invalid' });
    }
    assert.equal(f.calls.length, 0);
  } finally { f.reader.dispose(); }
});

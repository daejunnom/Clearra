import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPc4LocalReader } from './pc4-local-reader.mjs';

const artifacts = ['fields.bin', 'offsets.bin', 'graph.bin'].map((path, i) => ({ path, byte_length: 131089,
  content_identity: `sha256:${String(i).repeat(64)}` }));
const bytesAt = (offset, length) => Uint8Array.from({ length }, (_, i) => (offset + i) % 251);
const fixture = options => createPc4LocalReader(artifacts, async (_a, o, n) => bytesAt(o, n), options);

test('large repeated graph demands use bounded pages and preserve every byte', async () => {
  const reader = fixture({ pageBytes: 65536 });
  for (let i = 0; i < 20000; i++) {
    const a = artifacts[i % 3], offset = (i * 17) % (a.byte_length - 34);
    assert.deepEqual(await reader.read(a, offset, 34), bytesAt(offset, 34));
  }
  assert.equal(reader.reads, 20000);
  assert.ok(reader.fileReads <= 9);
  assert.ok(reader.cacheHits >= 19991);
  assert.equal(reader.requests, 0);
  assert.equal(reader.bytes, 0);
  reader.dispose(); assert.equal(reader.retainedBytes, 0);
});
test('boundaries, LRU eviction and returned buffer ownership remain exact', async () => {
  const reader = fixture({ pageBytes: 65536, cacheBytes: 65536 });
  const first = await reader.read(artifacts[0], 65530, 30);
  assert.deepEqual(first, bytesAt(65530, 30));
  first.fill(0);
  assert.deepEqual(await reader.read(artifacts[0], 65530, 30), bytesAt(65530, 30));
  assert.ok(reader.retainedBytes <= 65536);
  assert.deepEqual(await reader.read(artifacts[0], 131080, 9), bytesAt(131080, 9));
  assert.ok(reader.fileReads >= 4);
});
test('same pending page is read once, and abort never admits late bytes', async () => {
  let finish;
  const abort = new AbortController();
  const reader = createPc4LocalReader(artifacts, (_a, o, n) => new Promise(resolve => { finish = () => resolve(bytesAt(o, n)); }), { signal: abort.signal });
  const a = reader.read(artifacts[1], 10, 8), b = reader.read(artifacts[1], 100, 8);
  await Promise.resolve();
  assert.equal(reader.fileReads, 1); assert.equal(reader.joinedRequests, 1);
  abort.abort(); finish();
  await assert.rejects(a, /cancelled/); await assert.rejects(b, /cancelled/);
  assert.equal(reader.retainedBytes, 0);
  await assert.rejects(reader.read(artifacts[1], 10, 8), /cancelled/);
});
test('invalid or cross-generation batch fails before I/O; truncated page is never cached', async () => {
  const reader = fixture();
  await assert.rejects(reader.readMany([{ artifact: artifacts[0], offset: 0, length: 8 },
    { artifact: { ...artifacts[1], content_identity: `sha256:${'f'.repeat(64)}` }, offset: 0, length: 8 }]), /range_invalid/);
  assert.equal(reader.fileReads, 0);
  await assert.rejects(reader.read(artifacts[0], 131089, 1), /range_invalid/);
  const short = createPc4LocalReader(artifacts, async () => new Uint8Array(3));
  await assert.rejects(short.read(artifacts[0], 0, 1), /truncated/);
  assert.equal(short.retainedBytes, 0);
  await assert.rejects(short.read(artifacts[0], 0, 1), /truncated/);
  assert.equal(short.fileReads, 2);
});
test('random graph records stay exact while index reads share pages', async () => {
  const reader = fixture({ directPaths: [artifacts[2].path] });
  await reader.read(artifacts[0], 0, 8);
  await reader.read(artifacts[0], 16, 8);
  await reader.read(artifacts[2], 70000, 12);
  assert.equal(reader.fileReads, 2);
  assert.equal(reader.localBytes, 4096 + 12);
  assert.equal(reader.retainedBytes, 4096);
  assert.equal(reader.cacheHits, 1);
});
test('concurrent direct reads and pages share the same allocation bound', async () => {
  const finish = [];
  const reader = createPc4LocalReader(artifacts, (_a, _o, n) => new Promise(resolve => finish.push(() => resolve(new Uint8Array(n)))),
    { directPaths: [artifacts[2].path] });
  const pending = Array.from({ length: 128 }, () => reader.read(artifacts[2], 0, 1));
  await assert.rejects(reader.read(artifacts[2], 0, 1), /pending_limit/);
  await assert.rejects(reader.read(artifacts[0], 0, 1), /pending_limit/);
  assert.equal(reader.fileReads, 128);
  for (const resolve of finish) resolve();
  await Promise.all(pending);
  assert.equal(reader.retainedBytes, 0);
});

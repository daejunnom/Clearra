import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPc4LocalReader } from './pc4-local-reader.mjs';
import { buildPc4GraphBlockDirectory } from './pc4-graph-block.mjs';

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

test('qualified local graph blocks synthesize exact offset pairs and retain sibling records', async () => {
  const fieldCount = 32, width = 3, recordBytes = 12;
  const offsetsBytes = new Uint8Array(16 + 4 * (fieldCount + 1));
  const offsetView = new DataView(offsetsBytes.buffer);
  offsetsBytes.set(new TextEncoder().encode('GOFFIDX1'));
  offsetView.setUint32(8, 1, true); offsetView.setUint32(12, fieldCount, true);
  for (let id = 0; id <= fieldCount; id++) offsetView.setUint32(16 + id * 4, id * recordBytes, true);
  const graphBytes = new Uint8Array(fieldCount * recordBytes);
  for (let id = 0; id < fieldCount; id++) {
    const at = id * recordBytes;
    graphBytes[at + 4] = id;
    // Seven zero-degree piece buckets follow the five-byte source hash.
  }
  const directory = await buildPc4GraphBlockDirectory(async (offset, length) =>
    offsetsBytes.slice(offset, offset + length), { fieldCount, graphBytes: graphBytes.length, blockRecords: 16 });
  const localArtifacts = [
    { path: 'fields.bin', byte_length: 1, content_identity: `sha256:${'1'.repeat(64)}` },
    { path: 'offsets.bin', byte_length: offsetsBytes.length, content_identity: `sha256:${'2'.repeat(64)}` },
    { path: 'graph.bin', byte_length: graphBytes.length, content_identity: `sha256:${'3'.repeat(64)}` },
    { path: 'blocks.bin', byte_length: directory.bytes.length, content_identity: directory.contentIdentity }
  ];
  const bodies = new Map([['fields.bin', new Uint8Array(1)], ['offsets.bin', offsetsBytes],
    ['graph.bin', graphBytes], ['blocks.bin', directory.bytes]]);
  const reader = createPc4LocalReader(localArtifacts, async (artifact, offset, length) =>
    bodies.get(artifact.path).slice(offset, offset + length), {
      directPaths: ['graph.bin'],
      graphBlock: { directoryPath: 'blocks.bin', offsetsPath: 'offsets.bin', graphPath: 'graph.bin',
        fieldCount, targetWidth: width,
        blockRecords: 16 }
    });

  const pair5 = await reader.read(localArtifacts[1], 16 + 5 * 4, 8);
  assert.deepEqual([...pair5], [60, 0, 0, 0, 72, 0, 0, 0]);
  assert.deepEqual(await reader.read(localArtifacts[2], 60, recordBytes), graphBytes.slice(60, 72));
  const pair10 = await reader.read(localArtifacts[1], 16 + 10 * 4, 8);
  assert.deepEqual([...pair10], [120, 0, 0, 0, 132, 0, 0, 0]);
  assert.deepEqual(await reader.read(localArtifacts[2], 120, recordBytes), graphBytes.slice(120, 132));
  assert.equal(reader.fileReads, 2, 'one offsets page and one graph block serve both records');

  const pair20 = await reader.read(localArtifacts[1], 16 + 20 * 4, 8);
  assert.deepEqual([...pair20], [240, 0, 0, 0, 252, 0, 0, 0]);
  assert.deepEqual(await reader.read(localArtifacts[2], 240, recordBytes), graphBytes.slice(240, 252));
  assert.equal(reader.fileReads, 3, 'the second block reuses the offsets page and reads one graph span');
  assert.equal(reader.localBytes, directory.bytes.length + graphBytes.length);
  assert.ok(reader.cacheHits >= 6);
});

test('local graph block parsing fails closed on a malformed qualified span', async () => {
  const offsetsBytes = new Uint8Array(16 + 4 * 3);
  const view = new DataView(offsetsBytes.buffer);
  offsetsBytes.set(new TextEncoder().encode('GOFFIDX1'));
  view.setUint32(8, 1, true); view.setUint32(12, 2, true);
  view.setUint32(16, 0, true); view.setUint32(20, 12, true); view.setUint32(24, 24, true);
  const directory = await buildPc4GraphBlockDirectory(async (offset, length) =>
    offsetsBytes.slice(offset, offset + length), { fieldCount: 2, graphBytes: 24, blockRecords: 2 });
  const localArtifacts = [
    { path: 'fields.bin', byte_length: 1, content_identity: `sha256:${'4'.repeat(64)}` },
    { path: 'offsets.bin', byte_length: offsetsBytes.length, content_identity: `sha256:${'5'.repeat(64)}` },
    { path: 'graph.bin', byte_length: 24, content_identity: `sha256:${'6'.repeat(64)}` },
    { path: 'blocks.bin', byte_length: directory.bytes.length, content_identity: directory.contentIdentity }
  ];
  const bodies = new Map([['fields.bin', new Uint8Array(1)], ['offsets.bin', offsetsBytes],
    ['graph.bin', new Uint8Array(24).fill(255)], ['blocks.bin', directory.bytes]]);
  const reader = createPc4LocalReader(localArtifacts, async (artifact, offset, length) =>
    bodies.get(artifact.path).slice(offset, offset + length), {
      directPaths: ['graph.bin'],
      graphBlock: { directoryPath: 'blocks.bin', offsetsPath: 'offsets.bin', graphPath: 'graph.bin',
        fieldCount: 2, targetWidth: 3,
        blockRecords: 2 }
    });
  await assert.rejects(reader.read(localArtifacts[1], 16, 8), /pc4_local_graph_block_invalid/);
});

test('graph block directory generation validates every sampled bound and source header', async () => {
  const fieldCount = 33, recordBytes = 12, offsets = new Uint8Array(16 + 4 * (fieldCount + 1));
  offsets.set(new TextEncoder().encode('GOFFIDX1'));
  const view = new DataView(offsets.buffer);
  view.setUint32(8, 1, true); view.setUint32(12, fieldCount, true);
  for (let id = 0; id <= fieldCount; id++) view.setUint32(16 + id * 4, id * recordBytes, true);
  const result = await buildPc4GraphBlockDirectory(async (offset, length) =>
    offsets.slice(offset, offset + length), { fieldCount, graphBytes: fieldCount * recordBytes, blockRecords: 16 });
  assert.equal(new TextDecoder().decode(result.bytes.subarray(0, 8)), 'GBLKIDX1');
  assert.equal(result.bytes.length, 32);
  const output = new DataView(result.bytes.buffer);
  assert.deepEqual([output.getUint32(16, true), output.getUint32(20, true),
    output.getUint32(24, true), output.getUint32(28, true)], [0, 192, 384, 396]);
  const descending = offsets.slice();
  new DataView(descending.buffer).setUint32(16 + 32 * 4, 1, true);
  await assert.rejects(buildPc4GraphBlockDirectory(async (offset, length) =>
    descending.slice(offset, offset + length), { fieldCount, graphBytes: fieldCount * recordBytes,
      blockRecords: 16 }), /pc4_local_graph_block_invalid/);
});

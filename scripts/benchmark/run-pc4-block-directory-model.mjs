// Read-only, local model for an optional sparse graph-block directory. This
// consumes a previously recorded real logical-demand trace and an explicitly
// downloaded generation. It does not create a sidecar, contact upstream,
// alter product storage, or claim network/search timing.
import { createHash } from 'node:crypto';
import { lstat, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { checkedDatasetRoot, openBenchmarkDataset } from './pc4-local-dataset.mjs';

const CACHE_BYTES = 8 * 1024 * 1024;
const CACHE_ENTRIES = 2_048;
const INDEX_HEADER_BYTES = 16;
const PAGE_BYTES = 4_096;
const MAX_RANGE_BYTES = 65_536;

const { values } = parseArgs({ options: {
  directory: { type: 'string' },
  profile: { type: 'string' },
  blocks: { type: 'string', default: '1,2,4,8,16,32,64,128' },
  'verify-block': { type: 'string' }
} });
const blockSizes = values.blocks.split(',').map(Number);
const verifyBlock = values['verify-block'] === undefined ? null : Number(values['verify-block']);
if (!values.directory || !values.profile || !blockSizes.length || blockSizes.length > 12 ||
    blockSizes.some(value => !Number.isSafeInteger(value) || value < 1 || value > 1_024 ||
      (value & (value - 1)) !== 0) || (verifyBlock !== null &&
      (!Number.isSafeInteger(verifyBlock) || !blockSizes.includes(verifyBlock)))) {
  throw new Error('Supply a dataset directory, profile and bounded power-of-two block sizes');
}

const dataset = await openBenchmarkDataset(values.directory, values.profile);
try {
  const profile = dataset.generation.profiles.find(slot => slot.profile === values.profile);
  if (!profile || profile.status !== 'ready' || !Number.isSafeInteger(profile.field_count) ||
      profile.field_count < 1) throw new Error('Selected profile is not a qualified generation');
  const fieldCount = profile.field_count;
  const offsetsArtifact = dataset.plan.files[1];
  const graphArtifact = dataset.plan.files[2];
  if (offsetsArtifact.byte_length !== INDEX_HEADER_BYTES + 4 * (fieldCount + 1)) {
    throw new Error('Qualified offsets layout does not match the declared field count');
  }

  const header = await dataset.read(offsetsArtifact, 0, INDEX_HEADER_BYTES);
  const headerView = new DataView(header.buffer, header.byteOffset, header.byteLength);
  if (new TextDecoder().decode(header.subarray(0, 8)) !== 'GOFFIDX1' ||
      headerView.getUint32(8, true) !== 1 || headerView.getUint32(12, true) !== fieldCount) {
    throw new Error('Qualified offsets header mismatch');
  }
  const offsets = new Uint32Array(fieldCount + 1);
  let ordinal = 0;
  for (let at = INDEX_HEADER_BYTES; at < offsetsArtifact.byte_length; at += MAX_RANGE_BYTES) {
    const length = Math.min(MAX_RANGE_BYTES, offsetsArtifact.byte_length - at);
    const bytes = await dataset.read(offsetsArtifact, at, length);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    for (let cursor = 0; cursor < length; cursor += 4) {
      const offset = view.getUint32(cursor, true);
      if (ordinal && offset < offsets[ordinal - 1]) throw new Error('Descending graph offset');
      offsets[ordinal++] = offset;
    }
  }
  if (ordinal !== fieldCount + 1 || offsets[fieldCount] !== graphArtifact.byte_length) {
    throw new Error('Graph terminal offset mismatch');
  }

  const tracePath = join(await checkedDatasetRoot(values.directory, values.profile), 'lookup-trace.bin');
  const stat = await lstat(tracePath);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size < 4 || stat.size > 2_531_076) {
    throw new Error('Invalid bounded trace file');
  }
  const trace = await readFile(tracePath);
  const traceHeaderBytes = trace.readUInt32LE(0);
  if (traceHeaderBytes > 131_072 || 4 + traceHeaderBytes > trace.length) {
    throw new Error('Invalid trace header');
  }
  const traceHeader = JSON.parse(trace.subarray(4, 4 + traceHeaderBytes));
  const body = trace.subarray(4 + traceHeaderBytes);
  if (traceHeader.schema !== 'clearra.pc4.lookup-trace.v1' ||
      !Number.isSafeInteger(traceHeader.record_count) || traceHeader.record_count < 1 ||
      traceHeader.record_count > 200_000 || body.length !== traceHeader.record_count * 12 ||
      traceHeader.body_sha256 !== createHash('sha256').update(body).digest('hex') ||
      JSON.stringify(traceHeader.generation) !== JSON.stringify(dataset.generation)) {
    throw new Error('Trace identity mismatch');
  }

  const graphFieldIds = [];
  for (let record = 0; record < traceHeader.record_count; record++) {
    if (body.readUInt32LE(record * 12) !== 2) continue;
    const start = body.readUInt32LE(record * 12 + 4);
    const length = body.readUInt32LE(record * 12 + 8);
    let low = 0, high = fieldCount;
    while (low < high) {
      const middle = low + Math.floor((high - low) / 2);
      if (offsets[middle] < start) low = middle + 1;
      else high = middle;
    }
    if (low >= fieldCount || offsets[low] !== start || offsets[low + 1] - start !== length) {
      throw new Error('Trace graph demand is not one qualified exact record');
    }
    graphFieldIds.push(low);
  }
  if (!graphFieldIds.length) throw new Error('Trace contains no graph demand');

  const models = blockSizes.map(blockRecords => simulateBlockDirectory(
    fieldCount, offsets, graphFieldIds, blockRecords
  ));
  const verification = verifyBlock === null ? null : await verifyBlockDirectory(
    dataset, profile, offsets, graphArtifact, graphFieldIds, verifyBlock
  );
  console.log(JSON.stringify({
    evidence: 'recorded-real-demand-local-sidecar-model-not-network-timing',
    profile: values.profile,
    revision: dataset.plan.revision,
    trace_records: traceHeader.record_count,
    graph_demands: graphFieldIds.length,
    field_count: fieldCount,
    cache_bytes: CACHE_BYTES,
    cache_entries: CACHE_ENTRIES,
    models,
    ...(verification ? { verification } : {})
  }));
} finally {
  await dataset.close();
}

function simulateBlockDirectory(fieldCount, offsets, fieldIds, blockRecords) {
  const blockCount = Math.ceil(fieldCount / blockRecords);
  const sidecarBytes = INDEX_HEADER_BYTES + 4 * (blockCount + 1);
  const cache = new Map();
  let retainedBytes = 0, requests = 0, transferredBytes = 0;
  let sidecarRequests = 0, graphRequests = 0, maximumBlockBytes = 0;

  const touch = key => {
    const retained = cache.get(key);
    if (retained !== undefined) {
      cache.delete(key);
      cache.set(key, retained);
      return true;
    }
    return false;
  };
  const admit = (key, bytes, kind) => {
    requests++;
    transferredBytes += bytes;
    if (kind === 'sidecar') sidecarRequests++;
    else graphRequests++;
    if (bytes > CACHE_BYTES) return;
    while (cache.size &&
        (retainedBytes + bytes > CACHE_BYTES || cache.size >= CACHE_ENTRIES)) {
      const oldest = cache.keys().next().value;
      retainedBytes -= cache.get(oldest);
      cache.delete(oldest);
    }
    cache.set(key, bytes);
    retainedBytes += bytes;
  };
  const use = (key, bytes, kind) => {
    if (!touch(key)) admit(key, bytes, kind);
  };

  // A real reader validates the header through its containing cache page. Do
  // not model the first pair as if this immutable format witness were free.
  use('sidecar:0', Math.min(PAGE_BYTES, sidecarBytes), 'sidecar');

  for (const fieldId of fieldIds) {
    const block = Math.floor(fieldId / blockRecords);
    const first = block * blockRecords;
    const last = Math.min(fieldCount, first + blockRecords);
    const blockBytes = offsets[last] - offsets[first];
    maximumBlockBytes = Math.max(maximumBlockBytes, blockBytes);
    if (blockBytes > MAX_RANGE_BYTES) {
      return { block_records: blockRecords, sidecar_bytes: sidecarBytes,
        invalid: 'graph_block_exceeds_single_range_limit', maximum_block_bytes: maximumBlockBytes };
    }

    // The adapter can derive the block id from the field id. A retained graph
    // block therefore bypasses the directory completely; only graph misses
    // need the immutable offset pair before the block is admitted.
    const graphKey = `graph:${block}`;
    if (touch(graphKey)) continue;
    const sidecarOffset = INDEX_HEADER_BYTES + block * 4;
    const pairEnd = sidecarOffset + 8;
    for (let pageOffset = Math.floor(sidecarOffset / PAGE_BYTES) * PAGE_BYTES;
      pageOffset < pairEnd; pageOffset += PAGE_BYTES) {
      use(`sidecar:${pageOffset}`, Math.min(PAGE_BYTES, sidecarBytes - pageOffset), 'sidecar');
    }
    admit(graphKey, blockBytes, 'graph');
  }
  return {
    block_records: blockRecords,
    sidecar_bytes: sidecarBytes,
    modeled_requests: requests,
    sidecar_requests: sidecarRequests,
    graph_requests: graphRequests,
    modeled_bytes: transferredBytes,
    maximum_block_bytes: maximumBlockBytes,
    retained_bytes: retainedBytes,
    retained_entries: cache.size
  };
}

async function verifyBlockDirectory(dataset, profile, offsets, graphArtifact, fieldIds, blockRecords) {
  if (![3, 4].includes(profile.target_width)) throw new Error('Unsupported qualified target width');
  const fieldCount = profile.field_count;
  const blockCount = Math.ceil(fieldCount / blockRecords);
  const sidecar = new Uint8Array(INDEX_HEADER_BYTES + 4 * (blockCount + 1));
  sidecar.set(new TextEncoder().encode('GBLKIDX1'));
  const sidecarView = new DataView(sidecar.buffer);
  sidecarView.setUint32(8, 1, true);
  sidecarView.setUint32(12, fieldCount, true);
  for (let block = 0; block <= blockCount; block++) {
    sidecarView.setUint32(INDEX_HEADER_BYTES + block * 4,
      offsets[Math.min(fieldCount, block * blockRecords)], true);
  }

  const cache = new Map();
  let retainedBytes = 0, sidecarReads = 0, sidecarBytes = 0;
  let graphReads = 0, graphBytes = 0, cacheHits = 0;
  const get = key => {
    const entry = cache.get(key);
    if (!entry) return null;
    cache.delete(key); cache.set(key, entry); cacheHits++;
    return entry.value;
  };
  const put = (key, size, value) => {
    if (size > CACHE_BYTES) return value;
    while (cache.size && (retainedBytes + size > CACHE_BYTES || cache.size >= CACHE_ENTRIES)) {
      const oldest = cache.keys().next().value;
      retainedBytes -= cache.get(oldest).size;
      cache.delete(oldest);
    }
    cache.set(key, { size, value }); retainedBytes += size;
    return value;
  };
  const sidecarPage = pageOffset => {
    const key = `sidecar:${pageOffset}`;
    const cached = get(key);
    if (cached) return cached;
    const bytes = sidecar.slice(pageOffset, Math.min(sidecar.length, pageOffset + PAGE_BYTES));
    sidecarReads++; sidecarBytes += bytes.length;
    return put(key, bytes.length, bytes);
  };
  const readSidecar = (offset, length) => {
    const result = new Uint8Array(length);
    for (let pageOffset = Math.floor(offset / PAGE_BYTES) * PAGE_BYTES;
      pageOffset < offset + length; pageOffset += PAGE_BYTES) {
      const page = sidecarPage(pageOffset);
      const begin = Math.max(offset, pageOffset), end = Math.min(offset + length, pageOffset + page.length);
      result.set(page.subarray(begin - pageOffset, end - pageOffset), begin - offset);
    }
    return result;
  };
  const header = readSidecar(0, INDEX_HEADER_BYTES);
  const headerView = new DataView(header.buffer, header.byteOffset, header.byteLength);
  if (new TextDecoder().decode(header.subarray(0, 8)) !== 'GBLKIDX1' ||
      headerView.getUint32(8, true) !== 1 || headerView.getUint32(12, true) !== fieldCount) {
    throw new Error('Generated block directory header mismatch');
  }
  const graphBlock = async block => {
    const key = `graph:${block}`;
    const cached = get(key);
    if (cached) return cached;
    const pair = readSidecar(INDEX_HEADER_BYTES + block * 4, 8);
    const view = new DataView(pair.buffer, pair.byteOffset, pair.byteLength);
    const start = view.getUint32(0, true), end = view.getUint32(4, true);
    if (end <= start || end - start > MAX_RANGE_BYTES || end > graphArtifact.byte_length) {
      throw new Error('Block directory graph span mismatch');
    }
    const bytes = await dataset.read(graphArtifact, start, end - start);
    graphReads++; graphBytes += bytes.length;
    const first = block * blockRecords, count = Math.min(blockRecords, fieldCount - first);
    const bounds = parseHydraBlock(bytes, count, profile.target_width, fieldCount);
    return put(key, bytes.length, { bytes, bounds, first });
  };

  const expectedDigest = createHash('sha256'), actualDigest = createHash('sha256');
  let directReads = 0, directBytes = 0;
  for (const fieldId of fieldIds) {
    const block = Math.floor(fieldId / blockRecords);
    const loaded = await graphBlock(block);
    const [start, end] = loaded.bounds[fieldId - loaded.first];
    const actual = loaded.bytes.subarray(start, end);
    const expected = await dataset.read(graphArtifact, offsets[fieldId], offsets[fieldId + 1] - offsets[fieldId]);
    directReads++; directBytes += expected.length;
    if (actual.length !== expected.length || actual.some((byte, index) => byte !== expected[index])) {
      throw new Error(`Block directory record mismatch at field ${fieldId}`);
    }
    expectedDigest.update(expected); actualDigest.update(actual);
  }
  const expectedSha256 = expectedDigest.digest('hex'), actualSha256 = actualDigest.digest('hex');
  if (expectedSha256 !== actualSha256) throw new Error('Block directory aggregate digest mismatch');
  return {
    evidence: 'actual-local-block-scan-versus-qualified-exact-records-not-network-timing',
    block_records: blockRecords,
    compared_records: fieldIds.length,
    exact_record_sha256: expectedSha256,
    block_record_sha256: actualSha256,
    sidecar_file_bytes: sidecar.length,
    sidecar_page_reads: sidecarReads,
    sidecar_page_bytes: sidecarBytes,
    graph_block_reads: graphReads,
    graph_block_bytes: graphBytes,
    direct_reference_reads: directReads,
    direct_reference_bytes: directBytes,
    cache_hits: cacheHits,
    retained_bytes: retainedBytes,
    retained_entries: cache.size
  };
}

function parseHydraBlock(bytes, recordCount, targetWidth, fieldCount) {
  const bounds = [];
  let cursor = 0;
  for (let record = 0; record < recordCount; record++) {
    const start = cursor;
    if (cursor + 5 > bytes.length) throw new Error('Truncated graph source hash in block');
    cursor += 5;
    let cumulative = 0;
    for (let piece = 0; piece < 7; piece++) {
      if (cursor >= bytes.length) throw new Error('Truncated graph degree in block');
      const degree = bytes[cursor++];
      cumulative += degree;
      if (cumulative > 255) throw new Error('Noncanonical graph degree in block');
      const end = cursor + degree * targetWidth;
      if (end > bytes.length) throw new Error('Truncated graph targets in block');
      for (let at = cursor; at < end; at += targetWidth) {
        const target = targetWidth === 3
          ? bytes[at] + bytes[at + 1] * 256 + bytes[at + 2] * 65_536
          : bytes[at] + bytes[at + 1] * 256 + bytes[at + 2] * 65_536 + bytes[at + 3] * 16_777_216;
        if (target >= fieldCount) throw new Error('Graph target outside qualified field domain');
      }
      cursor = end;
    }
    bounds.push([start, cursor]);
  }
  if (cursor !== bytes.length) throw new Error('Trailing graph bytes in block');
  return bounds;
}

// SRP: generation-pinned, bounded local-file paging. No network, file ownership,
// graph decoding or fallback. The store keeps its shared lease until disposal.
import { checkedPc4Read } from './pc4-range-plan.mjs';
import { parsePc4HydraGraphBlock } from './pc4-graph-block.mjs';

export function createPc4LocalReader(artifacts, readSlice, { signal,
  pageBytes = 4096, cacheBytes = 8 * 1024 * 1024, directPaths = [], graphBlock } = {}) {
  const fail = code => { throw Object.assign(new Error(code), { code }); };
  if (!Array.isArray(artifacts) || ![3, 4].includes(artifacts.length) || typeof readSlice !== 'function' ||
      !Number.isSafeInteger(pageBytes) || pageBytes < 512 || pageBytes > 65536 || (pageBytes & (pageBytes - 1)) ||
      !Number.isSafeInteger(cacheBytes) || cacheBytes < pageBytes || cacheBytes > 64 * 1024 * 1024) fail('pc4_local_limits_invalid');
  const files = new Map();
  for (const input of artifacts) {
    const { artifact } = checkedPc4Read(input, 0, 1);
    if (files.has(artifact.path)) fail('pc4_local_identity_invalid');
    files.set(artifact.path, Object.freeze(artifact));
  }
  if (!Array.isArray(directPaths) || directPaths.some(path => !files.has(path))) fail('pc4_local_identity_invalid');
  const direct = new Set(directPaths);
  let blockConfig = null;
  if (graphBlock !== undefined) {
    const { directoryPath, offsetsPath, graphPath, fieldCount, targetWidth, blockRecords = 16,
      maxBlockBytes = 65_536 } = graphBlock ?? {};
    const directory = files.get(directoryPath), offsets = files.get(offsetsPath), graph = files.get(graphPath);
    const blockCount = Math.ceil(fieldCount / blockRecords);
    if (!directory || !offsets || !graph || offsets.byte_length !== 16 + 4 * (fieldCount + 1) ||
        directory.byte_length !== 16 + 4 * (blockCount + 1) ||
        !Number.isSafeInteger(fieldCount) || fieldCount < 1 || fieldCount > 2 ** 24 ||
        ![3, 4].includes(targetWidth) || !Number.isSafeInteger(blockRecords) ||
        blockRecords < 1 || blockRecords > 1024 || (blockRecords & (blockRecords - 1)) ||
        !Number.isSafeInteger(maxBlockBytes) || maxBlockBytes < 12 || maxBlockBytes > 65_536) {
      fail('pc4_local_graph_block_invalid');
    }
    blockConfig = Object.freeze({ directory, offsets, graph, fieldCount, targetWidth, blockRecords, maxBlockBytes });
  }
  const cache = new Map(), pending = new Map(), blockPending = new Map(), recordBlocks = new Map();
  let closed = false, retained = 0, reads = 0, fileReads = 0, localBytes = 0, hits = 0, joins = 0, directActive = 0;
  const checkOpen = () => { if (closed || signal?.aborted) fail('pc4_online_cancelled'); };
  const dispose = () => {
    closed = true; cache.clear(); recordBlocks.clear(); retained = 0;
    signal?.removeEventListener('abort', dispose);
  };
  signal?.addEventListener('abort', dispose, { once: true });
  const checked = (input, offset, length) => {
    checkOpen();
    let artifact;
    try { ({ artifact } = checkedPc4Read(input, offset, length)); }
    catch (error) { fail(error.message); }
    const pinned = files.get(artifact.path);
    if (!pinned || pinned.byte_length !== artifact.byte_length || pinned.content_identity !== artifact.content_identity) fail('pc4_online_range_invalid');
    return pinned;
  };
  const touch = key => {
    const found = cache.get(key);
    if (!found) return null;
    hits++; cache.delete(key); cache.set(key, found);
    return found;
  };
  const remove = key => {
    const found = cache.get(key);
    if (!found) return;
    retained -= found.size; cache.delete(key);
    for (const recordKey of found.recordKeys ?? []) {
      if (recordBlocks.get(recordKey) === key) recordBlocks.delete(recordKey);
    }
  };
  const retain = (key, entry) => {
    if (entry.size > cacheBytes) return entry;
    remove(key);
    while (cache.size && (retained + entry.size > cacheBytes || cache.size >= 2048)) {
      remove(cache.keys().next().value);
    }
    cache.set(key, entry); retained += entry.size;
    return entry;
  };
  async function page(artifact, offset) {
    checkOpen();
    const key = `page:${artifact.path}:${offset}`, found = touch(key);
    if (found) return found.bytes;
    if (pending.has(key)) { joins++; return pending.get(key); }
    // Bound simultaneous allocations even for independently concurrent callers.
    if (pending.size + directActive >= 128) fail('pc4_local_pending_limit');
    const length = Math.min(pageBytes, artifact.byte_length - offset);
    const work = Promise.resolve().then(async () => {
      checkOpen(); fileReads++;
      const result = await readSlice(artifact, offset, length);
      checkOpen();
      if (!(result instanceof Uint8Array) || result.length !== length) fail('pc4_online_truncated_range');
      localBytes += result.length;
      // Own the page: a reused host scratch buffer cannot change prior reads.
      const bytes = result.slice();
      retain(key, { bytes, size: bytes.length });
      return bytes;
    }).finally(() => pending.delete(key));
    pending.set(key, work);
    return work;
  }
  async function paged(artifact, offset, length) {
    const result = new Uint8Array(length), end = offset + length;
    for (let start = Math.floor(offset / pageBytes) * pageBytes; start < end; start += pageBytes) {
      const bytes = await page(artifact, start);
      checkOpen();
      const begin = Math.max(start, offset), finish = Math.min(start + bytes.length, end);
      result.set(bytes.subarray(begin - start, finish - start), begin - offset);
    }
    return result;
  }
  async function directRead(artifact, offset, length) {
    if (pending.size + directActive >= 128) fail('pc4_local_pending_limit');
    directActive++; fileReads++;
    try {
      const bytes = await readSlice(artifact, offset, length);
      checkOpen();
      if (!(bytes instanceof Uint8Array) || bytes.length !== length) fail('pc4_online_truncated_range');
      localBytes += bytes.length;
      return bytes.slice();
    } finally { directActive--; }
  }
  let graphBlockHeader;
  async function requireGraphBlockHeader() {
    if (!graphBlockHeader) graphBlockHeader = paged(blockConfig.directory, 0, 16).then(bytes => {
      const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      if (new TextDecoder().decode(bytes.subarray(0, 8)) !== 'GBLKIDX1' ||
          view.getUint32(8, true) !== 1 || view.getUint32(12, true) !== blockConfig.fieldCount) {
        fail('pc4_local_graph_block_invalid');
      }
      return true;
    });
    return graphBlockHeader;
  }
  async function loadGraphBlock(fieldId) {
    const { directory, graph, fieldCount, targetWidth, blockRecords, maxBlockBytes } = blockConfig;
    const block = Math.floor(fieldId / blockRecords), first = block * blockRecords;
    const count = Math.min(blockRecords, fieldCount - first), key = `block:${graph.path}:${block}`;
    const known = touch(key);
    if (known) return known;
    if (blockPending.has(key)) { joins++; return blockPending.get(key); }
    if (blockPending.size >= 128) fail('pc4_local_pending_limit');
    const work = Promise.resolve().then(async () => {
      await requireGraphBlockHeader();
      const pair = await paged(directory, 16 + block * 4, 8);
      const start = u32(pair), end = u32(pair.subarray(4));
      if (end <= start || end > graph.byte_length || end - start > maxBlockBytes) {
        fail('pc4_local_graph_block_invalid');
      }
      const bytes = await directRead(graph, start, end - start);
      const bounds = parsePc4HydraGraphBlock(bytes, { recordCount: count, targetWidth, fieldCount });
      const recordKeys = bounds.map(([begin, finish]) =>
        `${graph.path}:${start + begin}:${finish - begin}`);
      const entry = { bytes, size: bytes.length, first, start, bounds, recordKeys };
      retain(key, entry);
      if (cache.has(key)) for (const recordKey of recordKeys) recordBlocks.set(recordKey, key);
      return entry;
    }).finally(() => blockPending.delete(key));
    blockPending.set(key, work);
    return work;
  }
  async function optimizedOffsetPair(artifact, offset, length) {
    if (!blockConfig || artifact.path !== blockConfig.offsets.path || length !== 8 ||
        offset < 16 || (offset - 16) % 4 !== 0) return null;
    const fieldId = (offset - 16) / 4;
    if (fieldId >= blockConfig.fieldCount) return null;
    const loaded = await loadGraphBlock(fieldId);
    const [begin, end] = loaded.bounds[fieldId - loaded.first];
    const pair = new Uint8Array(8), view = new DataView(pair.buffer);
    view.setUint32(0, loaded.start + begin, true);
    view.setUint32(4, loaded.start + end, true);
    return pair;
  }
  function cachedGraphRecord(artifact, offset, length) {
    if (!blockConfig || artifact.path !== blockConfig.graph.path) return null;
    const recordKey = `${artifact.path}:${offset}:${length}`, blockKey = recordBlocks.get(recordKey);
    if (!blockKey) return null;
    const loaded = touch(blockKey);
    if (!loaded) { recordBlocks.delete(recordKey); return null; }
    const record = loaded.bounds.find(([begin, end]) =>
      loaded.start + begin === offset && end - begin === length);
    return record ? loaded.bytes.slice(record[0], record[1]) : null;
  }
  async function read(input, offset, length) {
    const artifact = checked(input, offset, length);
    reads++;
    if (blockConfig && artifact.path === blockConfig.offsets.path && length === 8 &&
        offset >= 16 && (offset - 16) % 4 === 0) {
      const pair = await optimizedOffsetPair(artifact, offset, length);
      if (pair) return pair;
    }
    if (blockConfig) {
      const blockRecord = cachedGraphRecord(artifact, offset, length);
      if (blockRecord) return blockRecord;
    }
    // The graph owner already retains decoded records. Random graph reads
    // should not pull in 4/64 KiB of unrelated records just to copy 12 bytes.
    // Index pages, in contrast, amortize headers and nearby offset pairs.
    if (direct.has(artifact.path)) {
      return directRead(artifact, offset, length);
    }
    return paged(artifact, offset, length);
  }
  return {
    provider: 'local-graph', requests: 0, bytes: 0,
    get reads() { return reads; }, get fileReads() { return fileReads; }, get localBytes() { return localBytes; },
    get cacheHits() { return hits; }, get joinedRequests() { return joins; }, get retainedBytes() { return retained; },
    read, dispose,
    async readMany(demands) {
      if (!Array.isArray(demands) || demands.length > 512) fail('pc4_online_batch_invalid');
      // Validate the entire explicit batch before doing any I/O.
      const pinned = demands.map(d => ({ artifact: checked(d?.artifact, d?.offset, d?.length), offset: d.offset, length: d.length }));
      const results = [];
      // A local page is cheap; serial reads avoid flooding the file service.
      for (const d of pinned) results.push(await read(d.artifact, d.offset, d.length));
      return results;
    }
  };
}

function u32(bytes) {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, true);
}

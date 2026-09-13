// SRP: generation-pinned, bounded local-file paging. No network, file ownership,
// graph decoding or fallback. The store keeps its shared lease until disposal.
import { checkedPc4Read } from './pc4-range-plan.mjs';

export function createPc4LocalReader(artifacts, readSlice, { signal,
  pageBytes = 4096, cacheBytes = 8 * 1024 * 1024, directPaths = [] } = {}) {
  const fail = code => { throw Object.assign(new Error(code), { code }); };
  if (!Array.isArray(artifacts) || artifacts.length !== 3 || typeof readSlice !== 'function' ||
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
  const cache = new Map(), pending = new Map();
  let closed = false, retained = 0, reads = 0, fileReads = 0, localBytes = 0, hits = 0, joins = 0, directActive = 0;
  const checkOpen = () => { if (closed || signal?.aborted) fail('pc4_online_cancelled'); };
  const dispose = () => { closed = true; cache.clear(); retained = 0; signal?.removeEventListener('abort', dispose); };
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
  async function page(artifact, offset) {
    checkOpen();
    const key = `${artifact.path}:${offset}`, found = cache.get(key);
    if (found) { hits++; cache.delete(key); cache.set(key, found); return found; }
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
      while (cache.size && (retained + bytes.length > cacheBytes || cache.size >= 2048)) {
        const oldest = cache.keys().next().value;
        retained -= cache.get(oldest).length; cache.delete(oldest);
      }
      cache.set(key, bytes); retained += bytes.length;
      return bytes;
    }).finally(() => pending.delete(key));
    pending.set(key, work);
    return work;
  }
  async function read(input, offset, length) {
    const artifact = checked(input, offset, length);
    reads++;
    // The graph owner already retains decoded records. Random graph reads
    // should not pull in 4/64 KiB of unrelated records just to copy 12 bytes.
    // Index pages, in contrast, amortize headers and nearby offset pairs.
    if (direct.has(artifact.path)) {
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
    const result = new Uint8Array(length), end = offset + length;
    for (let start = Math.floor(offset / pageBytes) * pageBytes; start < end; start += pageBytes) {
      const bytes = await page(artifact, start);
      checkOpen();
      const begin = Math.max(start, offset), finish = Math.min(start + bytes.length, end);
      result.set(bytes.subarray(begin - start, finish - start), begin - offset);
    }
    return result;
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

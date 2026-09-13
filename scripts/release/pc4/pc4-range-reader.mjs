// SRP: bounded immutable HTTP byte transport. Graph meaning, candidate
// completeness and rule-profile qualification belong to other owners.
import { checkedPc4Read, planPc4ReadBatch } from './pc4-range-plan.mjs';
import { createPc4SpanCache } from './pc4-span-cache.mjs';
export class Pc4OnlineError extends Error {
  constructor(code, detail = code) { super(detail); this.name = 'Pc4OnlineError'; this.code = code; }
}

const MAX_RANGE = 65_536;
const MAX_CACHE_ENTRIES = 2_048;
const MAX_WAITERS = 512;

export function createPc4RangeReader(discovery, { signal, onProgress, fetcher = fetch,
  maxBytes = 64 * 1024 * 1024, maxRequests = 100_000, cacheBytes = 8 * 1024 * 1024,
  windowBytes = 16_384, maxConcurrent = 4, directPaths = [] } = {}) {
  const revision = discovery.resolved_revision ?? discovery.revision;
  const repository = discovery.repository;
  if (!/^[0-9a-f]{40}$/.test(revision) || !/^[\w.-]+\/[\w.-]+$/.test(repository)) fail('pc4_online_identity_invalid');
  for (const value of [maxBytes, maxRequests, cacheBytes, windowBytes, maxConcurrent]) {
    if (!Number.isSafeInteger(value) || value < 0) fail('pc4_online_limits_invalid');
  }
  if (!maxConcurrent || maxConcurrent > 16 || windowBytes > MAX_RANGE ||
      (windowBytes && (windowBytes < 512 || (windowBytes & (windowBytes - 1)) !== 0))) fail('pc4_online_limits_invalid');
  if (!Array.isArray(directPaths) || directPaths.length > 16 ||
      directPaths.some(path => typeof path !== 'string' || !/^[A-Za-z0-9_.-]+\.bin$/.test(path))) fail('pc4_online_limits_invalid');
  const direct = new Set(directPaths);

  const pageSize = cacheBytes >= windowBytes && maxBytes >= windowBytes ? windowBytes : 0;
  const cache = createPc4SpanCache(cacheBytes, MAX_CACHE_ENTRIES), inFlight = new Map(), controllers = new Set(), queue = [];
  let transferred = 0, requests = 0, reservedTotal = 0;
  let active = 0, closed = false, reads = 0, cacheHits = 0, joined = 0;
  const cancel = () => {
    closed = true;
    for (const controller of controllers) controller.abort();
    for (const entry of queue.splice(0)) entry.reject(new Pc4OnlineError('pc4_online_cancelled'));
    cache.clear();
  };
  signal?.addEventListener('abort', cancel, { once: true });

  function pump() {
    while (!closed && active < maxConcurrent && queue.length) {
      const entry = queue.shift();
      active++;
      transport(entry.artifact, entry.offset, entry.length).then(entry.resolve, entry.reject).finally(() => {
        active--; pump();
      });
    }
  }

  function span(artifact, offset, length, explicitBatch = false) {
    if (closed || signal?.aborted) return Promise.reject(new Pc4OnlineError('pc4_online_cancelled'));
    const key = `${artifact.content_identity}:${artifact.path}:${artifact.byte_length}:${offset}:${length}`;
    const cached = cache.get(artifact, offset, length);
    if (cached) {
      cacheHits++;
      return Promise.resolve(cached);
    }
    const pending = inFlight.get(key);
    if (pending) { joined++; return pending; }
    if (queue.length >= MAX_WAITERS) return Promise.reject(new Pc4OnlineError('pc4_online_request_queue_limit'));
    const request = new Promise((resolve, reject) => {
      queue.push({ artifact, offset, length, resolve, reject }); pump();
    }).then(bytes => {
      inFlight.delete(key);
      // Decoded graph records already belong to the generation-bound App
      // cache. Retaining their one-shot raw bytes here evicts reusable index
      // pages and increases subsequent HTTP calls for the same known indices.
      if (!closed && (explicitBatch || !direct.has(artifact.path))) cache.put(artifact, offset, bytes);
      return bytes;
    }, error => { inFlight.delete(key); throw error; });
    inFlight.set(key, request);
    return request;
  }

  async function transport(artifact, offset, length) {
    if (closed || signal?.aborted) fail('pc4_online_cancelled');
    // Reserve before I/O, not after streaming: parallel requests cannot each
    // spend the same remaining budget. Failed reservations are not refunded.
    if (requests >= maxRequests || reservedTotal + length > maxBytes) fail('pc4_online_transfer_limit');
    requests++; reservedTotal += length;
    const controller = new AbortController(); controllers.add(controller);
    const timer = setTimeout(() => controller.abort(), 30_000);
    try {
      const response = await fetcher(`https://huggingface.co/datasets/${repository}/resolve/${revision}/${artifact.path}`,
        { headers: { Range: `bytes=${offset}-${offset + length - 1}` }, credentials: 'omit', signal: controller.signal });
      const expected = `bytes ${offset}-${offset + length - 1}/${artifact.byte_length}`;
      if (response.status !== 206 || response.headers.get('content-range') !== expected) {
        await response.body?.cancel();
        fail(response.status === 429 ? 'pc4_online_rate_limited' : response.status === 416 ? 'pc4_online_range_unsatisfiable' :
          response.status === 200 ? 'pc4_online_whole_content_rejected' : 'pc4_online_range_response_invalid');
      }
      const stream = response.body?.getReader();
      if (!stream) fail('pc4_online_empty_body');
      const bytes = new Uint8Array(length);
      let cursor = 0;
      while (true) {
        const { done, value } = await stream.read();
        if (done) break;
        transferred += value.length;
        if (cursor + value.length > length || transferred > maxBytes) {
          await stream.cancel(); fail('pc4_online_response_too_large');
        }
        bytes.set(value, cursor); cursor += value.length;
      }
      if (closed || signal?.aborted) fail('pc4_online_cancelled');
      if (cursor !== length) fail('pc4_online_truncated_range');
      onProgress?.({ transferredBytes: transferred, requests });
      return bytes;
    } catch (error) {
      if (error instanceof Pc4OnlineError) throw error;
      throw new Pc4OnlineError(closed || signal?.aborted ? 'pc4_online_cancelled' :
        controller.signal.aborted ? 'pc4_online_timeout' : 'pc4_online_offline', String(error));
    } finally { clearTimeout(timer); controllers.delete(controller); }
  }

  return {
    get bytes() { return transferred; },
    get requests() { return requests; },
    get reads() { return reads; },
    get cacheHits() { return cacheHits; },
    get joinedRequests() { return joined; },
    get retainedBytes() { return cache.bytes; },
    dispose() { cancel(); signal?.removeEventListener('abort', cancel); },
    readCached(input, offset, length) {
      if (closed || signal?.aborted) fail('pc4_online_cancelled');
      let artifact;
      try { ({ artifact } = checkedPc4Read(input, offset, length)); }
      catch (error) { fail(error.message); }
      return cache.get(artifact, offset, length)?.slice() ?? null;
    },
    async readMany(demands, options) {
      if (closed || signal?.aborted) fail('pc4_online_cancelled');
      let plan;
      try { plan = planPc4ReadBatch(demands, options); }
      catch (error) { fail(error.message); }
      // Plan the whole explicit demand before starting any I/O. Batch spans
      // follow known record boundaries instead of expanding every read to a
      // cache window. The same transport limits/accounting apply to each span.
      reads += demands.length;
      const result = new Array(demands.length);
      const missing = [], originalIndices = [];
      // Remove cached sub-demands BEFORE merging. Otherwise a partly cached
      // frontier can repeatedly transfer its old prefix in a larger new span.
      for (const transfer of plan) for (const demand of transfer.demands) {
        const known = cache.get(transfer.artifact, demand.offset, demand.length);
        if (known) { result[demand.index] = known.slice(); cacheHits++; }
        else {
          originalIndices.push(demand.index);
          missing.push({ artifact: transfer.artifact, offset: demand.offset, length: demand.length });
        }
      }
      plan = planPc4ReadBatch(missing, options);
      await Promise.all(plan.map(async transfer => {
        const bytes = await span(transfer.artifact, transfer.offset, transfer.length, true);
        for (const demand of transfer.demands) {
          const begin = demand.offset - transfer.offset;
          result[originalIndices[demand.index]] = bytes.slice(begin, begin + demand.length);
        }
      }));
      if (closed || signal?.aborted) fail('pc4_online_cancelled');
      return result;
    },
    async read(input, offset, length) {
      if (closed || signal?.aborted) fail('pc4_online_cancelled');
      // A queued transport must not observe later mutation of its descriptor.
      let artifact;
      try { ({ artifact } = checkedPc4Read(input, offset, length)); }
      catch (error) { fail(error.message); }
      reads++;
      // A previous explicit frontier batch may cover this exact record without
      // matching the fixed window key. Reuse it before expanding the demand.
      const known = cache.get(artifact, offset, length);
      if (known) {
        cacheHits++;
        const result = known.slice();
        // App retains the decoded graph once this real demand is admitted.
        // Drop a consumed one-record prefetch; otherwise thousands of dead
        // graph entries evict the index pages that batching must preserve.
        // A containing multi-record span stays until LRU eviction so unread
        // siblings in that same transfer are never discarded prematurely.
        if (direct.has(artifact.path)) cache.releaseExact(artifact, offset, length);
        return result;
      }
      // No speculative/background scan: only windows intersecting this demand.
      // Small files and byte-budget-constrained reads keep exact access, never
      // expand a small request into a whole-artifact download.
      if (direct.has(artifact.path) || !pageSize || artifact.byte_length <= pageSize) {
        const bytes = await span(artifact, offset, length);
        if (closed || signal?.aborted) fail('pc4_online_cancelled');
        return bytes.slice();
      }
      const end = offset + length, parts = [];
      for (let start = Math.floor(offset / pageSize) * pageSize; start < end; start += pageSize) {
        const size = Math.min(pageSize, artifact.byte_length - start);
        const begin = Math.max(offset, start), finish = Math.min(end, start + size);
        parts.push(span(artifact, start, size).then(bytes => ({ bytes, start, begin, finish })));
      }
      const result = new Uint8Array(length);
      for (const part of await Promise.all(parts)) {
        result.set(part.bytes.subarray(part.begin - part.start, part.finish - part.start), part.begin - offset);
      }
      if (closed || signal?.aborted) fail('pc4_online_cancelled');
      return result;
    }
  };
}

function fail(code) { throw new Pc4OnlineError(code); }

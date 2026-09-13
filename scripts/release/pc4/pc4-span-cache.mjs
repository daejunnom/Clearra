// SRP: bounded immutable byte-span reuse. Bucket lookup avoids scanning every
// retained graph record when a later exact demand falls inside a batched span.
const BUCKET = 65536;
const identity = a => `${a.content_identity}:${a.path}:${a.byte_length}`;
export function createPc4SpanCache(maxBytes, maxEntries) {
  const entries = new Map(), files = new Map(); let retained = 0;
  const key = (file, offset, length) => `${file}:${offset}:${length}`;
  function remove(k) {
    const e = entries.get(k); if (!e) return;
    entries.delete(k); retained -= e.bytes.length;
    const buckets = files.get(e.file);
    for (const bucket of e.buckets) {
      const keys = buckets.get(bucket); keys.delete(k);
      if (!keys.size) buckets.delete(bucket);
    }
    if (!buckets.size) files.delete(e.file);
  }
  return {
    get bytes() { return retained; },
    clear() { entries.clear(); files.clear(); retained = 0; },
    get(artifact, offset, length) {
      const file = identity(artifact), exact = key(file, offset, length);
      let k = entries.has(exact) ? exact : null;
      if (k === null) {
        const keys = files.get(file)?.get(Math.floor(offset / BUCKET));
        if (keys) for (const candidate of keys) {
          const e = entries.get(candidate);
          if (e.offset <= offset && e.offset + e.bytes.length >= offset + length) { k = candidate; break; }
        }
      }
      if (k === null) return null;
      const e = entries.get(k); entries.delete(k); entries.set(k, e);
      return e.bytes.subarray(offset - e.offset, offset - e.offset + length);
    },
    put(artifact, offset, bytes) {
      // All transport spans are at most one bucket wide (possibly crossing a
      // seam); keep this invariant explicit in the cache as well.
      if (!bytes.length || bytes.length > BUCKET || bytes.length > maxBytes || maxEntries < 1) return;
      const file = identity(artifact), k = key(file, offset, bytes.length);
      remove(k);
      while (entries.size && (retained + bytes.length > maxBytes || entries.size >= maxEntries)) remove(entries.keys().next().value);
      const buckets = [Math.floor(offset / BUCKET)];
      const end = Math.floor((offset + bytes.length - 1) / BUCKET);
      if (end !== buckets[0]) buckets.push(end);
      if (!files.has(file)) files.set(file, new Map());
      const index = files.get(file);
      for (const bucket of buckets) {
        if (!index.has(bucket)) index.set(bucket, new Set());
        index.get(bucket).add(k);
      }
      entries.set(k, { file, offset, bytes, buckets }); retained += bytes.length;
    }
  };
}

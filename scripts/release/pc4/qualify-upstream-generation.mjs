// SRP: qualify the host's supported graph/index format against one immutable
// upstream generation. Upstream completeness declarations and reader evidence
// are distinct: bounded samples are never described as a whole-graph proof.
import { discoverPc4UpstreamGeneration } from './discover-upstream-generation.mjs';

export const PC4_READER_CONTRACT = 'hydra-jstris-180-complete-graph-v1';
export const PC4_PROFILE_ARTIFACTS = Object.freeze([
  { profile: 'srs', graph: 'graph_no180.bin', suffix: '_no180', width: 3 },
  { profile: 'srs-plus', graph: 'graph_srsplus.bin', suffix: '_srsplus', width: 3 },
  { profile: 'srs-x', graph: 'graph_srsx.bin', suffix: '_srsx', width: 4 },
  { profile: 'jstris-180', graph: 'graph.bin', suffix: '', width: 3 },
  { profile: 'no-kick', graph: 'graph_nokick.bin', suffix: '_nokick', width: 3 }
]);

export class Pc4OnlineError extends Error {
  constructor(code, detail = code) { super(detail); this.name = 'Pc4OnlineError'; this.code = code; }
}

// No disk persistence. A job pins the returned immutable revision; a subsequent
// refresh can discover new upstream content without changing an active job.
export async function qualifyPc4UpstreamGeneration({ signal, onProgress } = {}, dependencies = {}) {
  const discovery = await (dependencies.discover ?? discoverPc4UpstreamGeneration)({ signal });
  if (signal?.aborted) throw new Pc4OnlineError('pc4_online_cancelled');
  const reader = dependencies.reader ?? createPc4RangeReader(discovery, { signal, onProgress });
  const entries = new Map(discovery.candidates.map(entry => [entry.path, entry]));
  const profiles = [];
  for (const model of PC4_PROFILE_ARTIFACTS) {
    const graph = entries.get(model.graph);
    const fields = entries.get(`field_hash_to_id${model.suffix}.v1.bin`);
    const offsets = entries.get(`graph_offsets${model.suffix}.u32.bin`);
    const base = { profile: model.profile, upstream_complete: model.profile === 'jstris-180' };
    if (!graph || !fields || !offsets) {
      profiles.push({ ...base, status: 'unavailable', reason: !graph
        ? 'missing-profile-artifacts' : 'missing-profile-specific-index' });
      continue;
    }
    // Other graph variants remain independent. Do not substitute the canonical
    // offsets, infer a completion statement from filenames, or mark them ready.
    if (!base.upstream_complete) {
      profiles.push({ ...base, status: 'unavailable', reason: 'missing-completion-declaration' });
      continue;
    }
    try {
      const [fh, oh] = await Promise.all([reader.read(fields, 0, 16), reader.read(offsets, 0, 16)]);
      const count = header(fh, 'FHIDIDX1');
      if (count !== header(oh, 'GOFFIDX1') || fields.byte_length !== 16 + 8 * count ||
          offsets.byte_length !== 16 + 4 * (count + 1)) fail('pc4_online_index_layout_mismatch');
      const [first, sentinel] = await Promise.all([
        reader.read(offsets, 16, 4), reader.read(offsets, 16 + 4 * count, 4)
      ]);
      if (u32(first) !== 0 || u32(sentinel) !== graph.byte_length) fail('pc4_online_index_graph_mismatch');
      const evidence = [];
      for (const id of [...new Set([0, 1, 100, 10_000, Math.floor(count / 2), count - 1])]) {
        if (id >= count) continue;
        const [field, pair] = await Promise.all([
          reader.read(fields, 16 + id * 8, 8), reader.read(offsets, 16 + id * 4, 8)
        ]);
        const hash = little(field.subarray(0, 5));
        if (little(field.subarray(5)) !== id) fail('pc4_online_index_order_mismatch');
        const start = u32(pair), end = u32(pair, 4);
        if (end <= start || end > graph.byte_length || end - start > 16_384) fail('pc4_online_record_bounds');
        const record = await reader.read(graph, start, end - start);
        validateGraphRecord(record, hash, model.width, count);
        evidence.push({ id, hash, start, end });
      }
      if (evidence[0].hash !== 0 || evidence.at(-1).hash !== 2 ** 40 - 1) fail('pc4_online_terminal_mismatch');
      profiles.push({ ...base, status: 'ready', reader_contract: PC4_READER_CONTRACT,
        field_count: count, target_width: model.width, target_lines: [4],
        terminal_id: count - 1, artifacts: { fields, offsets, graph }, evidence });
    } catch (error) {
      if (signal?.aborted) throw new Pc4OnlineError('pc4_online_cancelled');
      profiles.push({ ...base, status: 'unavailable', reason: error.code ?? 'pc4_online_unavailable' });
    }
  }
  return Object.freeze({ schema: 'clearra.pc4.host-generation.v1', repository: discovery.repository,
    revision: discovery.resolved_revision, profiles, transferred_bytes: reader.bytes ?? 0 });
}

export function createPc4RangeReader(discovery, { signal, onProgress, fetcher = fetch,
  maxBytes = 64 * 1024 * 1024, maxRequests = 100_000, cacheBytes = 8 * 1024 * 1024 } = {}) {
  const revision = discovery.resolved_revision ?? discovery.revision;
  if (!/^[0-9a-f]{40}$/.test(revision) || !/^[\w.-]+\/[\w.-]+$/.test(discovery.repository)) fail('pc4_online_identity_invalid');
  let transferred = 0, requests = 0, retained = 0;
  const cache = new Map();
  return {
    get bytes() { return transferred; },
    get requests() { return requests; },
    async read(artifact, offset, length) {
      if (signal?.aborted) fail('pc4_online_cancelled');
      if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(length) || length < 1 ||
          length > 65_536 || offset + length > artifact.byte_length ||
          !/^[A-Za-z0-9_.-]+\.bin$/.test(artifact.path)) fail('pc4_online_range_invalid');
      const key = `${artifact.content_identity}:${artifact.path}:${offset}:${length}`;
      const cached = cache.get(key);
      if (cached) return cached.slice();
      if (requests >= maxRequests || transferred + length > maxBytes) fail('pc4_online_transfer_limit');
      requests++;
      const controller = new AbortController();
      const abort = () => controller.abort();
      signal?.addEventListener('abort', abort, { once: true });
      const timer = setTimeout(abort, 30_000);
      try {
        const url = `https://huggingface.co/datasets/${discovery.repository}/resolve/${revision}/${artifact.path}`;
        const response = await fetcher(url, { headers: { Range: `bytes=${offset}-${offset + length - 1}` },
          credentials: 'omit', signal: controller.signal });
        const expected = `bytes ${offset}-${offset + length - 1}/${artifact.byte_length}`;
        if (response.status !== 206 || response.headers.get('content-range') !== expected) {
          await response.body?.cancel();
          fail(response.status === 429 ? 'pc4_online_rate_limited' :
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
        if (cursor !== length) fail('pc4_online_truncated_range');
        while (retained + length > cacheBytes && cache.size) {
          const oldest = cache.keys().next().value;
          retained -= cache.get(oldest).length; cache.delete(oldest);
        }
        if (length <= cacheBytes) { cache.set(key, bytes); retained += length; }
        onProgress?.({ transferredBytes: transferred, requests });
        return bytes.slice();
      } catch (error) {
        if (error instanceof Pc4OnlineError) throw error;
        throw new Pc4OnlineError(signal?.aborted ? 'pc4_online_cancelled' :
          controller.signal.aborted ? 'pc4_online_timeout' : 'pc4_online_offline', String(error));
      } finally {
        clearTimeout(timer); signal?.removeEventListener('abort', abort);
      }
    }
  };
}

function header(bytes, magic) {
  if (new TextDecoder().decode(bytes.subarray(0, 8)) !== magic || u32(bytes, 8) !== 1) fail('pc4_online_index_header');
  const count = u32(bytes, 12);
  if (!count || count > 2 ** 24) fail('pc4_online_index_count');
  return count;
}
function u32(bytes, offset = 0) { return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(offset, true); }
function little(bytes) { let result = 0; for (let i = bytes.length - 1; i >= 0; i--) result = result * 256 + bytes[i]; return result; }
function fail(code) { throw new Pc4OnlineError(code); }
function validateGraphRecord(bytes, hash, width, count) {
  if (bytes.length < 12) fail('pc4_online_record_truncated');
  let actual = 0; for (let i = 0; i < 5; i++) actual = actual * 256 + bytes[i];
  if (actual !== hash) fail('pc4_online_graph_field_mismatch');
  let cursor = 5;
  for (let p = 0; p < 7; p++) {
    if (cursor >= bytes.length) fail('pc4_online_record_truncated');
    const degree = bytes[cursor++];
    for (let edge = 0; edge < degree; edge++) {
      if (cursor + width > bytes.length) fail('pc4_online_record_truncated');
      if (little(bytes.subarray(cursor, cursor + width)) >= count) fail('pc4_online_target_outside_domain');
      cursor += width;
    }
  }
  if (cursor !== bytes.length) fail('pc4_online_record_trailing_bytes');
}

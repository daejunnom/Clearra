// SRP: qualify the host's supported graph/index format against one immutable
// upstream generation. Upstream completeness declarations and reader evidence
// are distinct: bounded samples are never described as a whole-graph proof.
import { discoverPc4UpstreamGeneration } from './discover-upstream-generation.mjs';
import { createPc4RangeReader, Pc4OnlineError } from './pc4-range-reader.mjs';
export { createPc4RangeReader, Pc4OnlineError } from './pc4-range-reader.mjs';

export const PC4_READER_CONTRACT = 'hydra-jstris-180-complete-graph-v1';
export const PC4_PROFILE_ARTIFACTS = Object.freeze([
  { profile: 'srs', graph: 'graph_no180.bin', suffix: '_no180', width: 3 },
  { profile: 'srs-plus', graph: 'graph_srsplus.bin', suffix: '_srsplus', width: 3 },
  { profile: 'srs-x', graph: 'graph_srsx.bin', suffix: '_srsx', width: 4 },
  { profile: 'jstris-180', graph: 'graph.bin', suffix: '', width: 3 },
  { profile: 'no-kick', graph: 'graph_nokick.bin', suffix: '_nokick', width: 3 }
]);

// No disk persistence. A job pins the returned immutable revision; a subsequent
// refresh can discover new upstream content without changing an active job.
export async function qualifyPc4UpstreamGeneration({ signal, onProgress } = {}, dependencies = {}) {
  const discovery = await (dependencies.discover ?? discoverPc4UpstreamGeneration)({ signal });
  if (signal?.aborted) throw new Pc4OnlineError('pc4_online_cancelled');
  const reader = dependencies.reader ?? createPc4RangeReader(discovery, { signal, onProgress });
  try { return await qualifyProfiles(discovery, reader, signal); }
  finally { if (!dependencies.reader) reader.dispose(); }
}

async function qualifyProfiles(discovery, reader, signal) {
  const readMany = demands => reader.readMany ? reader.readMany(demands)
    : Promise.all(demands.map(d => reader.read(d.artifact, d.offset, d.length)));
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
      const [fh, oh] = await readMany([
        { artifact: fields, offset: 0, length: 16 }, { artifact: offsets, offset: 0, length: 16 }
      ]);
      const count = header(fh, 'FHIDIDX1');
      if (count !== header(oh, 'GOFFIDX1') || fields.byte_length !== 16 + 8 * count ||
          offsets.byte_length !== 16 + 4 * (count + 1)) fail('pc4_online_index_layout_mismatch');
      const ids = [...new Set([0, 1, 100, 10_000, Math.floor(count / 2), count - 1])].filter(id => id < count);
      // All sample addresses are known after the two headers. Fetch their
      // necessary index intervals as one planned stage, not six serial lookups.
      const indices = await readMany(ids.flatMap(id => [
        { artifact: fields, offset: 16 + id * 8, length: 8 },
        { artifact: offsets, offset: 16 + id * 4, length: 8 }
      ]));
      const evidence = [];
      for (const [index, id] of ids.entries()) {
        const [field, pair] = indices.slice(index * 2, index * 2 + 2);
        const hash = little(field.subarray(0, 5));
        if (little(field.subarray(5)) !== id) fail('pc4_online_index_order_mismatch');
        const start = u32(pair), end = u32(pair, 4);
        // These boundary pairs already contain the first offset and sentinel;
        // requesting those separately would reread the same index cells.
        if ((id === 0 && start !== 0) || (id === count - 1 && end !== graph.byte_length)) {
          fail('pc4_online_index_graph_mismatch');
        }
        if (end <= start || end > graph.byte_length || end - start > 16_384) fail('pc4_online_record_bounds');
        evidence.push({ id, hash, start, end });
      }
      const records = await readMany(evidence.map(e => ({ artifact: graph, offset: e.start, length: e.end - e.start })));
      for (const [index, e] of evidence.entries()) validateGraphRecord(records[index], e.hash, model.width, count);
      if (evidence[0].hash !== 0 || evidence.at(-1).hash !== 2 ** 40 - 1) fail('pc4_online_terminal_mismatch');
      profiles.push({ ...base, status: 'ready', reader_contract: PC4_READER_CONTRACT,
        field_count: count, target_width: model.width, target_lines: [4],
        pc_search_target_lines: [4], setup_search_target_lines: [],
        terminal_id: count - 1, artifacts: { fields, offsets, graph }, evidence });
    } catch (error) {
      if (signal?.aborted) throw new Pc4OnlineError('pc4_online_cancelled');
      profiles.push({ ...base, status: 'unavailable', reason: error.code ?? 'pc4_online_unavailable' });
    }
  }
  return Object.freeze({ schema: 'clearra.pc4.host-generation.v1', repository: discovery.repository,
    revision: discovery.resolved_revision, profiles, transferred_bytes: reader.bytes ?? 0 });
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

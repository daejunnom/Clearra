// SRP: byte-layout planning for an already queued, bounded graph-ID frontier.
// No new graph traversal, graph interpretation, candidate admission or retry.
import { PC4_READER_CONTRACT, Pc4OnlineError } from './qualify-upstream-generation.mjs';
import { checkedPc4Read } from './pc4-range-plan.mjs';

export async function prefetchPc4LookupFrontier(reader, generation, range) {
  const ids = range.lookup_frontier;
  if (ids === undefined || Array.isArray(ids) && ids.length < 2) return;
  const fail = code => { throw new Pc4OnlineError(code); };
  if (!Array.isArray(ids) || ids.length > 32) fail('pc4_online_frontier_invalid');
  const matches = generation.profiles.filter(p => p.profile === range.profile);
  if (matches.length !== 1 || matches[0].status !== 'ready' || matches[0].reader_contract !== PC4_READER_CONTRACT) {
    fail('pc4_online_profile_not_qualified');
  }
  const { artifacts, field_count: count } = matches[0];
  if (!Number.isSafeInteger(count) || count < 1 || count > 2 ** 24 ||
      artifacts?.offsets?.byte_length !== 16 + 4 * (count + 1)) fail('pc4_online_index_layout_mismatch');
  // Wait until the real LookupMachine has requested its offset pair, after
  // header validation. Earlier hash/header demands never trigger read-ahead.
  const a = checkedPc4Read(artifacts.offsets, 0, 16).artifact;
  const graph = checkedPc4Read(artifacts.graph, 0, 1).artifact;
  const requested = range.artifact;
  if (requested.path !== a.path || requested.content_identity !== a.content_identity || requested.byte_length !== a.byte_length ||
      range.offset < 16 || (range.offset - 16) % 4 || range.length !== 8) return;
  const current = (range.offset - 16) / 4;
  if (!ids.includes(current) || ids.some(id => !Number.isSafeInteger(id) || id < 0 || id >= count)) fail('pc4_online_frontier_invalid');
  const unique = [...new Set(ids)];
  const pairs = await reader.readMany(unique.map(id => checkedPc4Read(a, 16 + id * 4, 8)));
  const demands = pairs.map((pair, i) => {
    if (pair.length !== 8) fail('pc4_online_truncated_range');
    const view = new DataView(pair.buffer, pair.byteOffset, pair.byteLength);
    const start = view.getUint32(0, true), end = view.getUint32(4, true);
    if (end <= start || end - start > 65536 || end > graph.byte_length ||
        unique[i] === 0 && start !== 0 || unique[i] === count - 1 && end !== graph.byte_length) fail('pc4_online_record_bounds');
    return checkedPc4Read(graph, start, end - start);
  });
  // Explicit graph spans are retained in the SAME bounded transport cache.
  // Later machine demands still obtain exact slices and use normal admission.
  await reader.readMany(demands);
}

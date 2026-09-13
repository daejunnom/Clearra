// SRP: byte-layout planning for an already queued, bounded graph-ID frontier.
// No new graph traversal, graph interpretation, candidate admission or retry.
import { PC4_READER_CONTRACT, Pc4OnlineError } from './qualify-upstream-generation.mjs';
import { checkedPc4Read, planPc4ReadBatch } from './pc4-range-plan.mjs';

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
  // A qualified graph record has at least 5 bitmap + 7 degree bytes. With a
  // 1,024-byte merge gap, an ID gap above 86 cannot connect two known records.
  // Most DFS frontiers are far apart: reject those optional hints arithmetically
  // before copying cached bytes or planning transfers. Required lookup is
  // untouched, so this filter grants no graph validity/completeness authority.
  if (!ids.some(id => id !== current && Math.abs(id - current) <= 86)) return;
  const sorted = [...new Set(ids)].sort((x, y) => x - y), at = sorted.indexOf(current);
  let lo = at, hi = at + 1;
  while (lo > 0 && sorted[lo] - sorted[lo - 1] <= 86) lo--;
  while (hi < sorted.length && sorted[hi] - sorted[hi - 1] <= 86) hi++;
  // Pay only for the index range the real machine needs NOW. Other queued
  // IDs may participate only when their offset bytes are already cached.
  // Fetching every sibling's index early increased real P7P4 request counts.
  const requiredPair = await reader.read(a, range.offset, range.length);
  const unique = [current, ...sorted.slice(lo, hi).filter(id => id !== current)];
  const demands = [];
  for (const id of unique) {
    const pair = id === current ? requiredPair : reader.readCached(a, 16 + id * 4, 8);
    if (pair === null) continue;
    if (pair.length !== 8) fail('pc4_online_truncated_range');
    const view = new DataView(pair.buffer, pair.byteOffset, pair.byteLength);
    const start = view.getUint32(0, true), end = view.getUint32(4, true);
    if (end <= start || end - start > 65536 || end > graph.byte_length ||
        id === 0 && start !== 0 || id === count - 1 && end !== graph.byte_length) fail('pc4_online_record_bounds');
    if (reader.readCached(graph, start, end - start) !== null) {
      if (id === current) return;
      continue;
    }
    demands.push(checkedPc4Read(graph, start, end - start));
  }
  // Only enlarge the transfer that is required now, and only if it covers at
  // least two distinct missing records. Distant siblings cause zero extra
  // index or graph HTTP calls; cached bytes remain transport-only evidence.
  const currentSpan = planPc4ReadBatch(demands).find(span => span.demands.some(d => d.index === 0));
  if (currentSpan && new Set(currentSpan.demands.map(d => `${d.offset}:${d.length}`)).size > 1) {
    await reader.readMany(currentSpan.demands.map(d => demands[d.index]));
  }
}

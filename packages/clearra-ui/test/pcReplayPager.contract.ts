import assert from 'node:assert/strict';
import type { ClearraPcPathWitnessPayload, ClearraPcReplayRuntimePage, ClearraProductResultPayload } from '../src/lib/wasm/wasmCommandClient';
import { collectPcReplayGeometryExportPages, createReplayExportBudget, loadPcReplayPage, validatePcReplayPage } from '../src/lib/workspace/pcReplayPager';
import { productResultIdentity, validateProductResultPayload, type ProductMemberPageLoader } from '../src/lib/workspace/productResultPager';

function witness(pattern: number, candidate = '3'): ClearraPcPathWitnessPayload {
  return {
    candidate_id: candidate, producer_candidate_id: '1', pattern_id: String(pattern),
    trace_identity: `identity-${pattern}`, normalized_trace_key: `trace-${pattern}`,
    consumed_piece_count: '1', terminal_hold_piece: null,
    steps: [{
      step_index: '0', operation_id: 'operation', active_piece: 'I', input_cursor: '0', output_cursor: '1',
      input_hold_piece: null, output_hold_piece: null, hold_decision: 'no-hold', rotation: 'north', x: '0', y: '0',
      placement_mask: '0x000000000000000f', board_before_mask: '0x00000000000003f0',
      board_after_placement_mask: '0x00000000000003ff', board_after_line_clear_mask: '0x0000000000000000',
      cleared_row_mask: '0x0000000000000001', cleared_lines: '1', line_clear_identity: 'clear'
    }]
  };
}
function page(member = 1): ClearraPcReplayRuntimePage {
  const start = (member - 1) * 100;
  return {
    page_contract: 'pc-replay-member-page.v2', page_source_available: true,
    page_source_identity_sha256: 'a'.repeat(64), geometry_count: '2', geometry_page_number: '1', candidate_id: '3',
    geometry_witness_count: '201', geometry_pattern_count: '201', member_page_number: String(member), member_page_count: '3',
    witness_count: '202', materialized_pattern_count: '201',
    witnesses: Array.from({ length: Math.min(100, 201 - start) }, (_, i) => witness(start + i))
  };
}
function envelope(value: ClearraPcReplayRuntimePage) {
  return { schema_version: 1 as const, runtime: 'clearra-wasm' as const, product_page_kind: 'pc-replay' as const, state: 'page' as const, page: value };
}
function result(value = page()): ClearraProductResultPayload {
  return { contract: 'pc.path', result_kind: 'pc-path-family.v2', content: { payload_kind: 'pc-path-family', payload: {
    ...value, witness_contract: 'pc-path-witness.v2',
    ordering: 'candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending',
    problem_id: 'fixture', complete: true, canonical_selection: 'smallest-canonical-candidate-id', canonical_witness: value.witnesses[0]!
  } } };
}

assert.equal(validatePcReplayPage(page()), null);
assert.equal(validateProductResultPayload(result()), null, 'global witness count is distinct from the bounded materialized member page');
const traceTiePage = { ...page(), geometry_count: '1', geometry_witness_count: '2', geometry_pattern_count: '1',
  member_page_count: '1', witness_count: '2', witnesses: [
    { ...witness(0), trace_identity: 'identity-a' }, { ...witness(0), trace_identity: 'identity-b' }
  ] };
assert.equal(validatePcReplayPage(traceTiePage), null, 'equal trace keys retain distinct canonical execution identities');
assert.equal(validateProductResultPayload(result(traceTiePage)), null, 'initial family follows the same Rust four-part ordering');
assert.notEqual(validatePcReplayPage({ ...traceTiePage, witnesses: [traceTiePage.witnesses[0]!, traceTiePage.witnesses[0]!] }), null);
const empty: ClearraProductResultPayload = { contract: 'pc.path', result_kind: 'pc-path-family.v2', content: {
  payload_kind: 'pc-path-family', payload: { witness_contract: 'pc-path-witness.v2',
    ordering: 'candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending', problem_id: 'empty',
    materialized_pattern_count: '0', witness_count: '0', complete: true,
    canonical_selection: 'smallest-canonical-candidate-id', canonical_witness: null, witnesses: [] }
} };
assert.equal(validateProductResultPayload(empty), null, 'empty complete families do not require a dead lazy handle');
const changedIdentity = result();
if (changedIdentity.content.payload_kind === 'pc-path-family') changedIdentity.content.payload.page_source_identity_sha256 = 'b'.repeat(64);
assert.notEqual(productResultIdentity(result()), productResultIdentity(changedIdentity));
for (const invalid of [
  { ...page(), member_page_count: '2' }, { ...page(), geometry_page_number: '3' },
  { ...page(), page_source_identity_sha256: 'bad' }, { ...page(), geometry_witness_count: '099' },
  { ...page(), witnesses: [...page().witnesses, witness(100)] },
  { ...page(), witnesses: [witness(0, '8'), ...page().witnesses.slice(1)] }
]) assert.notEqual(validatePcReplayPage(invalid), null);

const requests: string[] = [];
const loader: ProductMemberPageLoader = async (geometry, member) => {
  requests.push(`${geometry}:${member}`);
  return envelope(page(Number(member)));
};
const exported = await collectPcReplayGeometryExportPages({ initialPage: page(), loadMemberPage: loader, targetLines: 4, isCurrent: () => true });
assert.equal(exported.length, 201, 'copy includes all members of the current geometry, not only its visible first100');
assert.deepEqual(requests, ['1:2', '1:3'], 'copy requests only the selected geometry and consumes one input page at a time');
assert.equal(exported.at(-1)?.placements.length, 1);

await assert.rejects(collectPcReplayGeometryExportPages({ initialPage: page(), loadMemberPage: async () => envelope({ ...page(2), page_source_identity_sha256: 'b'.repeat(64) }), targetLines: 4, isCurrent: () => true }), /source/);
await assert.rejects(collectPcReplayGeometryExportPages({ initialPage: page(), loadMemberPage: async () => envelope({ ...page(2), witnesses: Array.from({length:100}, (_, i) => witness(i + 50)) }), targetLines: 4, isCurrent: () => true }), /overlap|order/);
await assert.rejects(loadPcReplayPage({ reference: page(), geometryPageNumber: '1', memberPageNumber: '2', loadMemberPage: async () => envelope(page(3)), isCurrent: () => true }), /source/);
const nextGeometry = { ...page(3), geometry_page_number: '2', candidate_id: '9', geometry_witness_count: '1', geometry_pattern_count: '1', member_page_number: '1', member_page_count: '1', witnesses: [witness(0, '9')] };
assert.equal((await loadPcReplayPage({ reference: page(), geometryPageNumber: '2', memberPageNumber: '1', loadMemberPage: async () => envelope(nextGeometry), isCurrent: () => true })).candidate_id, '9');
await assert.rejects(loadPcReplayPage({ reference: page(), geometryPageNumber: '2', memberPageNumber: '1', loadMemberPage: async () => envelope({ ...nextGeometry, candidate_id: '2', witnesses: [witness(0, '2')] }), isCurrent: () => true }), /order/);

const cancelled = new AbortController();
cancelled.abort();
let called = false;
await assert.rejects(collectPcReplayGeometryExportPages({ initialPage: page(), loadMemberPage: async () => { called = true; return envelope(page(2)); }, targetLines: 4, signal: cancelled.signal, isCurrent: () => true }), { name: 'AbortError' });
assert.equal(called, false);
let current = true;
await assert.rejects(loadPcReplayPage({ reference: page(), geometryPageNumber: '1', memberPageNumber: '2', loadMemberPage: async () => { current = false; return envelope(page(2)); }, isCurrent: () => current }), { name: 'AbortError' });

assert.notEqual(validatePcReplayPage({ ...page(), page_contract: 'pc-replay-member-page.v1' } as unknown as ClearraPcReplayRuntimePage), null, 'v1 trace-stream source hashes must not be silently accepted as v2 immutable-source hashes');
const pending = (member = '2') => ({ schema_version: 1 as const, runtime: 'clearra-wasm' as const,
  product_page_kind: 'pc-replay' as const, state: 'pending' as const,
  page_contract: 'pc-replay-member-page.v2' as const, page_source_identity_sha256: 'a'.repeat(64),
  geometry_page_number: '1', member_page_number: member, work_steps: '64' });
let slices = 0;
const resumed = await loadPcReplayPage({ reference: page(), geometryPageNumber: '1', memberPageNumber: '2',
  isCurrent: () => true, loadMemberPage: async (geometry, member, _signal, work) => {
    assert.equal(geometry, '1'); assert.equal(member, '2'); assert.equal(work, 64);
    return ++slices < 4 ? pending() : envelope(page(2));
  } });
assert.equal(slices, 4);
assert.equal(resumed.member_page_number, '2');
for (const invalid of [
  { ...pending(), member_page_number: '3' }, { ...pending(), page_source_identity_sha256: 'b'.repeat(64) },
  { ...pending(), work_steps: '0' }, { ...pending(), work_steps: '65' }
]) await assert.rejects(loadPcReplayPage({ reference: page(), geometryPageNumber: '1', memberPageNumber: '2', isCurrent: () => true, loadMemberPage: async () => invalid }), /source|progress/);
await assert.rejects(loadPcReplayPage({ reference: page(), geometryPageNumber: '1', memberPageNumber: '2', isCurrent: () => true,
  loadMemberPage: async () => ({ ...pending(), state: 'cancelled' }) }), { name: 'AbortError' });
const betweenSlices = new AbortController();
let pendingCalls = 0;
await assert.rejects(loadPcReplayPage({ reference: page(), geometryPageNumber: '1', memberPageNumber: '2', isCurrent: () => true, signal: betweenSlices.signal,
  loadMemberPage: async () => { pendingCalls += 1; setTimeout(() => betweenSlices.abort(), 0); return pending(); } }), { name: 'AbortError' });
assert.equal(pendingCalls, 1, 'pending loop yields for cancellation instead of spinning through an entire language');
assert.throws(() => createReplayExportBudget('4294967296'), /required_memory_bytes=.*max_memory_bytes=/, 'huge exact count is rejected before clipboard accumulation');
assert.doesNotThrow(() => createReplayExportBudget('2', 128n));
assert.throws(() => createReplayExportBudget('2', 127n), /No partial export/);
const bounded = createReplayExportBudget('1', 64n);
assert.throws(() => bounded.admitConversion(witness(0)), /No partial export/, 'conversion is admitted before frames or BigInts are built');

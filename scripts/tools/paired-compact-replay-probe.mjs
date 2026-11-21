import assert from 'node:assert/strict';

// A single I clears the only line. This is the same bounded typed product
// on native and Chromium, not the exhaustive P7P4 incident reproduction.
export const COMPACT_REPLAY_PROBE_ARGS = Object.freeze([
  'pc', 'path', '--lines', '1', '--height', '1', '--board-mask', '0x3f0',
  '--pieces', '1', '--queue', 'I', '--no-hold', '--rule', 'srs-plus',
  '--backend', 'cpu', '--no-tablebase', '--no-legal-board', '--no-conditioned-reachability',
  '--workers', '1',
]);
export const COMPACT_REPLAY_PROBE_COMMAND = ['clearra', ...COMPACT_REPLAY_PROBE_ARGS].join(' ');

const REPLAY_STAGES = [
  'wasm.pc.path.execution_graphs',
  'wasm.pc.path.pattern_rows',
  'app.pc.path.manifest_prepare',
  'app.pc.path.manifest_count',
];

export function verifyCompactReplayProbeStages(stages) {
  for (const name of REPLAY_STAGES) {
    const rows = stages.filter(row => row.name === name);
    assert.equal(rows.length, 1, 'missing or duplicate replay stage: ' + name);
    assert.ok(rows[0].invocation_count > 0, 'empty replay stage: ' + name);
  }
}

export function compactReplayProbeIdentity(kind, payload) {
  assert.equal(kind, 'pc-path-family.v2');
  assert.equal(payload?.complete, true);
  assert.equal(payload.materialized_pattern_count, '1');
  assert.match(payload.witness_count ?? '', /^[1-9]\d*$/u);
  assert.equal(payload.geometry_count, '1');
  assert.match(payload.page_source_identity_sha256 ?? '', /^[0-9a-f]{64}$/u);
  const canonical = payload.canonical_witness;
  assert.equal(canonical?.candidate_id, '1');
  assert.equal(String(canonical.pattern_id), '0');
  assert.match(canonical.normalized_trace_key ?? '', /^trk1:/u);
  return {
    witnesses: payload.witness_count,
    geometry_count: payload.geometry_count,
    page_source: payload.page_source_identity_sha256,
    canonical_witness: canonical.normalized_trace_key,
  };
}

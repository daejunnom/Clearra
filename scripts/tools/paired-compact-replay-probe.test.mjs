import assert from 'node:assert/strict';
import test from 'node:test';

import {
  COMPACT_REPLAY_PROBE_ARGS, COMPACT_REPLAY_PROBE_COMMAND, compactReplayProbeIdentity,
  verifyCompactReplayProbeStages,
} from './paired-compact-replay-probe.mjs';

const kind = 'pc-path-family.v2';
const summary = {
  complete: true,
  materialized_pattern_count: '1',
  witness_count: '2',
  geometry_count: '1',
  page_source_identity_sha256: 'a'.repeat(64),
  canonical_witness: { candidate_id: '1', pattern_id: '0', normalized_trace_key: 'trk1:fixture' },
};

test('compact replay pairs the bounded typed product, not a unique count', () => {
  assert.equal(COMPACT_REPLAY_PROBE_COMMAND, ['clearra', ...COMPACT_REPLAY_PROBE_ARGS].join(' '));
  assert.deepEqual(compactReplayProbeIdentity(kind, summary), {
    witnesses: '2', geometry_count: '1', page_source: 'a'.repeat(64),
    canonical_witness: 'trk1:fixture',
  });
});

test('compact replay profiles reject unfinished, empty or different replay identity', () => {
  for (const incomplete of [
    { complete: false }, { witness_count: '0' }, { materialized_pattern_count: '5040' },
    { geometry_count: '2' }, { page_source_identity_sha256: 'missing' },
    { canonical_witness: null },
    { canonical_witness: { ...summary.canonical_witness, pattern_id: '1' } },
  ]) {
    assert.throws(() => compactReplayProbeIdentity(kind, { ...summary, ...incomplete }));
  }
  assert.throws(() => compactReplayProbeIdentity('pc', summary));
});

test('replay stage comparisons require work from both the Core and the shared pager', () => {
  const stages = ['wasm.pc.path.execution_graphs', 'wasm.pc.path.pattern_rows',
    'app.pc.path.manifest_prepare', 'app.pc.path.manifest_count']
    .map(name => ({ name, invocation_count: 1 }));
  verifyCompactReplayProbeStages(stages);
  assert.throws(() => verifyCompactReplayProbeStages(stages.slice(0, 2)));
  assert.throws(() => verifyCompactReplayProbeStages([...stages, stages[0]]));
  assert.throws(() => verifyCompactReplayProbeStages(
    stages.map(row => ({ ...row, invocation_count: 0 }))));
});

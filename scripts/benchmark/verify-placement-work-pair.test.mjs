import assert from 'node:assert/strict';
import test from 'node:test';
import { verifyPlacementWorkPair } from './verify-placement-work-pair.mjs';

const source = '1'.repeat(40);
const emptyTenI = 'empty-ten-i';
const groundedO = 'grounded-o-two-lines';
const groundedOUnique = 'grounded-o-two-lines-unique';

function reports() {
  const reference = {
    schema: 'clearra.placement-graph-benchmark.v1', compiled_source_commit: source,
    case: 'empty-4L-ten-I-jstris180', valid: true, output_mode: 'global-only',
    binary_sha256: '2'.repeat(64), supervisor_run_id: '1800000000000000000-100',
    minimum_domain_cutoff_mode: '0', minimum_domain_work_mode: '1', mode: '0',
    geometric_edge_reuse_mode: '0', input: { count: 'unique' }, cpu: { hard_limit: 4 },
    other_process_configuration: {}, summary: { workers_used: '1', cpu_parallel_execution: 'false' },
    geometry_domain: { completed_families: 1, minimum_compiles: 9, minimum_empty_origin: 9,
      minimum_incidence_rows_scanned: 210, minimum_incidence_rows_skipped: 0 },
  };
  const treatment = { ...structuredClone(reference), supervisor_run_id: '1800000000000000001-100',
    minimum_domain_cutoff_mode: '1', geometry_domain: { ...reference.geometry_domain,
      minimum_incidence_rows_scanned: 143, minimum_incidence_rows_skipped: 67 } };
  return [reference, treatment];
}

test('reports work conservation without giving timing adoption authority', () => {
  const [a, b] = reports();
  assert.deepEqual(verifyPlacementWorkPair(emptyTenI, source, a, b), {
    source, fixture: a.case, timing_adoption_authority: false,
    examined_before: 210, examined_after: 143, skipped: 67,
  });
  b.geometry_domain.minimum_incidence_rows_skipped--;
  assert.throws(() => verifyPlacementWorkPair(emptyTenI, source, a, b));
});

test('refuses changed source, reused supervisor run, worker downgrade, or secondary flag', () => {
  for (const mutate of [
    (a, b) => { b.compiled_source_commit = '3'.repeat(40); },
    (a, b) => { b.supervisor_run_id = a.supervisor_run_id; },
    (a, b) => { b.summary.workers_used = '2'; },
    (a, b) => { b.geometric_edge_reuse_mode = '1'; },
  ]) {
    const [a, b] = reports();
    mutate(a, b);
    assert.throws(() => verifyPlacementWorkPair(emptyTenI, source, a, b));
  }
});

test('distinguishes shared CountAll work from early-terminated CountUnique', () => {
  for (const [fixture, mode, hits] of [[groundedO, 'countall', 3], [groundedOUnique, 'countunique', 0]]) {
    const [a, b] = reports();
    for (const [sample, toggle] of [[a, '0'], [b, '1']]) {
      sample.case = `grounded-o-two-lines-${mode}-srs90`;
      sample.minimum_domain_cutoff_mode = '0';
      sample.minimum_domain_work_mode = '0';
      sample.geometric_edge_reuse_mode = toggle;
    }
    a.geometric_edges = { footprint_validations: 0, reused_validations: 0,
      line_clear_computations: 0, projection_validations: 0 };
    b.geometric_edges = { footprint_validations: 1, reused_validations: hits,
      line_clear_computations: 1, projection_validations: 1 };
    const result = verifyPlacementWorkPair(fixture, source, a, b);
    assert.equal(result.reused_validations, hits);
    assert.equal(result.reference_counters_disabled, true);
    assert.equal(result.timing_adoption_authority, false);
    b.geometric_edges.reused_validations = hits + 1;
    assert.throws(() => verifyPlacementWorkPair(fixture, source, a, b));
  }
});

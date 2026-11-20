// Deterministic work evidence only. The Rust --compare owns result parity;
// this validator binds the two supervised reports to one experiment axis.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const fixtures = {
  'empty-ten-i': { name: 'empty-4L-ten-I-jstris180', axis: 'minimum_domain_cutoff_mode' },
  'grounded-o-two-lines': { name: 'grounded-o-two-lines-countall-srs90', axis: 'geometric_edge_reuse_mode' },
  'grounded-o-two-lines-unique': { name: 'grounded-o-two-lines-countunique-srs90', axis: 'geometric_edge_reuse_mode' },
};

export function verifyPlacementWorkPair(fixture, source, reference, treatment) {
  assert.ok(Object.hasOwn(fixtures, fixture), 'unknown placement work fixture');
  assert.match(source, /^[0-9a-f]{40}$/u);
  const { name, axis } = fixtures[fixture];
  for (const [report, mode] of [[reference, '0'], [treatment, '1']]) {
    assert.equal(report.schema, 'clearra.placement-graph-benchmark.v1');
    assert.equal(report.compiled_source_commit, source);
    assert.equal(report.case, name);
    assert.equal(report.output_mode, 'global-only');
    assert.equal(report.valid, true);
    assert.equal(report[axis], mode);
    assert.match(report.binary_sha256, /^[0-9a-f]{64}$/u);
    assert.match(report.supervisor_run_id, /^\d{16,21}-[1-9]\d*$/u);
  }
  assert.equal(reference.binary_sha256, treatment.binary_sha256);
  assert.notEqual(reference.supervisor_run_id, treatment.supervisor_run_id);
  for (const key of ['input', 'cpu', 'other_process_configuration']) {
    assert.deepEqual(reference[key], treatment[key], key);
  }
  for (const key of ['workers_used', 'cpu_parallel_execution']) {
    assert.notEqual(reference.summary[key] ?? null, null, key);
    assert.equal(reference.summary[key], treatment.summary[key], key);
  }
  const allKeys = new Set([...Object.keys(reference), ...Object.keys(treatment)]);
  for (const key of allKeys) {
    if (key !== axis && (key === 'mode' || key.endsWith('_mode'))) {
      assert.equal(reference[key], treatment[key], 'non-target mode changed: ' + key);
    }
  }

  const result = { source, fixture: name, timing_adoption_authority: false };
  if (fixture === 'empty-ten-i') {
    assert.equal(reference.minimum_domain_work_mode, '1');
    const a = reference.geometry_domain, b = treatment.geometry_domain;
    for (const key of ['completed_families', 'minimum_compiles', 'minimum_empty_origin']) {
      assert.ok(Number.isSafeInteger(a[key]) && a[key] > 0, key);
      assert.equal(a[key], b[key], key);
    }
    assert.equal(a.minimum_incidence_rows_skipped, 0);
    assert.ok(Number.isSafeInteger(b.minimum_incidence_rows_skipped) && b.minimum_incidence_rows_skipped > 0);
    assert.equal(a.minimum_incidence_rows_scanned,
      b.minimum_incidence_rows_scanned + b.minimum_incidence_rows_skipped);
    return { ...result, examined_before: a.minimum_incidence_rows_scanned,
      examined_after: b.minimum_incidence_rows_scanned, skipped: b.minimum_incidence_rows_skipped };
  }

  const counters = ['footprint_validations', 'reused_validations',
    'line_clear_computations', 'projection_validations'];
  for (const key of counters) assert.equal(reference.geometric_edges[key], 0, key);
  const unique = fixture === 'grounded-o-two-lines-unique';
  const expected = [1, unique ? 0 : 3, 1, 1];
  for (const [index, key] of counters.entries()) {
    assert.equal(treatment.geometric_edges[key], expected[index], key);
  }
  return { ...result, reference_counters_disabled: true,
    ...Object.fromEntries(counters.map((key, index) => [key, expected[index]])) };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [fixture, source, directory] = process.argv.slice(2);
  assert.equal(process.argv.length, 5, 'usage: verify-placement-work-pair.mjs FIXTURE SHA RECEIPTS_DIR');
  const read = name => JSON.parse(readFileSync(join(directory, name + '.json'), 'utf8'));
  console.log(JSON.stringify(verifyPlacementWorkPair(fixture, source, read('reference'), read('treatment'))));
}

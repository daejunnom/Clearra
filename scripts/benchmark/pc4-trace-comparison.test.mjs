import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createPc4TraceComparison } from './pc4-trace-comparison.mjs';
const artifacts = ['fields.bin', 'offsets.bin', 'graph.bin'].map(path => ({ path, byte_length: 100000, content_identity: `sha256:${'a'.repeat(64)}` }));
const demands = [[1, 10, 8], [2, 40, 12], [1, 0, 16], [1, 60, 8], [2, 100, 18]];
const body = demands => { const b = Buffer.alloc(demands.length * 12); demands.flat().forEach((n, i) => b.writeUInt32LE(n, i * 4)); return b; };
const bytes = d => Buffer.alloc(d[2], d[1] % 256);
const hash = demands => { const h = createHash('sha256'); for (const d of demands) h.update(`${artifacts[d[0]].path}:${d[1]}:${d[2]}\n`).update(bytes(d)); return h.digest('hex'); };
test('independent two-record swaps normalize exact bytes, including the subsequent unrecorded suffix', () => {
  const compare = createPc4TraceComparison(body(demands), artifacts);
  for (const i of [3, 4, 2, 0, 1]) { const d = demands[i]; compare.observe(artifacts[d[0]], d[1], d[2], bytes(d)); }
  const suffix = [2, 160, 12]; compare.observe(artifacts[2], 160, 12, bytes(suffix));
  const report = compare.finish();
  assert.equal(report.comparison_mismatches, 4);
  assert.equal(report.comparison_read_count, 5);
  assert.equal(report.comparison_covers_measurement, false);
  assert.equal(report.comparison_multiset_equal, true);
  assert.equal(report.reference_ordered_demand_sha256, hash([...demands, suffix]));
});
test('changed bytes cannot pass the recorded checksum and missing demands are not equal', () => {
  const compare = createPc4TraceComparison(body(demands), artifacts);
  for (const d of demands) compare.observe(artifacts[d[0]], d[1], d[2], Buffer.alloc(d[2], 255));
  assert.notEqual(compare.finish().reference_ordered_demand_sha256, hash(demands));
  const missing = createPc4TraceComparison(body(demands), artifacts);
  missing.observe(artifacts[2], 100, 18, bytes(demands[4]));
  assert.equal(missing.finish().comparison_multiset_equal, false);
});

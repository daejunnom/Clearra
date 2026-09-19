import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pc4SearchSummary } from './pc4-search-summary.mjs';

test('keeps authoritative count/completeness/hash without dumping solution keys', () => {
  assert.deepEqual(pc4SearchSummary({ unique_solution_count: 456459, solution_count_calculated: true,
    count_complete: true, normalized_solution_set_hash: 'reference', normalized_solution_keys: ['not-logged'],
    unknown: 'not-logged' }), { unique_solution_count: 456459, solution_count_calculated: true,
    normalized_solution_set_hash: 'reference', count_complete: true });
});

test('does not infer success, zero results or completeness from a missing report', () => {
  assert.equal(pc4SearchSummary(null), null);
  assert.equal(pc4SearchSummary(undefined), null);
  assert.deepEqual(pc4SearchSummary({}), {});
});

test('retains incomplete/truncated evidence and ignores unexpected complex fields', () => {
  assert.deepEqual(pc4SearchSummary({ unique_solution_count: 0, solution_count_calculated: false,
    count_complete: false, resource_truncated: true, resource_truncation_reason: 'limit',
    normalized_solution_set_hash: ['not-a-hash'] }), { unique_solution_count: 0,
    solution_count_calculated: false, count_complete: false, resource_truncated: true,
    resource_truncation_reason: 'limit' });
});

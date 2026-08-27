import assert from 'node:assert/strict';
import test from 'node:test';

import {
  workspaceSolutionCount,
  workspaceSolutionCountCalculated,
  workspaceSolutionKeysComplete,
  workspaceSolutionPageAvailable,
  workspaceSolutionSetMaterialized
} from '../src/lib/workspace/solutionSetAvailability.ts';
import { workspaceMessage } from '../src/lib/workspace/workspaceI18n.ts';

test('explicit availability keeps not-calculated distinct from an actual zero', () => {
  const unavailable = {
    unique_solution_count: 0,
    solution_count_calculated: false,
    solution_set_materialized: false,
    solution_keys_materialized_count: 0,
    solution_keys_complete: false,
    solution_page_available: false
  };
  const calculatedZero = {
    ...unavailable,
    solution_count_calculated: true,
    solution_set_materialized: true,
    solution_keys_complete: true,
    count_complete: true,
    execution_availability: { state: 'available' },
    result_completeness: 'complete'
  };

  assert.equal(workspaceSolutionCount(unavailable), null);
  assert.equal(workspaceSolutionCount(calculatedZero), 0);
  assert.equal(workspaceSolutionCountCalculated(calculatedZero), true);
  assert.equal(workspaceSolutionSetMaterialized(calculatedZero), true);
  assert.equal(workspaceSolutionKeysComplete(calculatedZero), true);
});

test('bare legacy numeric reports fail closed while typed complete evidence permits counts', () => {
  assert.equal(
    workspaceSolutionCount({
      unique_solution_count: 0,
      summary_fields: [['solution_count_calculated', 'false']]
    }),
    null
  );
  assert.equal(workspaceSolutionCount({ unique_solution_count: 3 }), null);
  assert.equal(workspaceSolutionCount({
    unique_solution_count: 3,
    solution_count_calculated: true,
    count_complete: true,
    execution_availability: { state: 'available' },
    result_completeness: 'complete'
  }), 3);
  assert.equal(
    workspaceSolutionPageAvailable({
      unique_solution_count: 3,
      summary_fields: [['solution_page_available', 'true']]
    }),
    false
  );
});

test('all numeric counts require explicit available complete count authority', () => {
  const base = {
    search_output_policy: 'summary',
    unique_solution_count: 0,
    solution_count_calculated: true,
    count_complete: true,
    execution_availability: { state: 'available' },
    result_completeness: 'complete'
  };
  for (const [label, report] of [
    ['availability missing', { ...base, execution_availability: undefined }],
    ['completeness missing', { ...base, result_completeness: undefined }],
    ['count completeness missing', { ...base, count_complete: undefined }],
    ['availability contradictory', {
      ...base,
      execution_availability: { state: 'exhausted' }
    }],
    ['completeness contradictory', { ...base, result_completeness: 'incomplete' }]
  ]) {
    assert.equal(workspaceSolutionCount(report), null, label);
    assert.equal(workspaceSolutionCountCalculated(report), false, label);
  }
  assert.equal(workspaceSolutionCount(base), 0);
  assert.equal(workspaceSolutionCount({ ...base, unique_solution_count: 17 }), 17);
});

function canonicalCoverageSummaryReport() {
  return {
    unique_solution_count: 0,
    normalized_solution_keys: [],
    normalized_solution_set_hash: 'not-calculated',
    solution_count_calculated: false,
    solution_set_materialized: false,
    solution_keys_materialized_count: 0,
    solution_keys_complete: false,
    solution_page_available: false,
    summary_fields: [
      ['search_output_policy', 'coverage-summary'],
      ['unique_solution_count', 'not-calculated'],
      ['normalized_unique_solution_count', 'not-calculated'],
      ['solution_count_calculated', 'false'],
      ['solution_set_materialized', 'false'],
      ['solution_keys_materialized_count', '0'],
      ['solution_keys_complete', 'false'],
      ['solution_page_available', 'false'],
      ['normalized_solution_set_hash', 'not-calculated'],
      ['actual_normalized_solution_set_hash', 'not-calculated'],
      ['mirror_normalized_solution_set_hash', 'not-calculated']
    ]
  };
}

function assertUnavailable(report, label) {
  assert.equal(workspaceSolutionCount(report), null, `${label}: count`);
  assert.equal(workspaceSolutionCountCalculated(report), false, `${label}: calculated`);
  assert.equal(workspaceSolutionSetMaterialized(report), false, `${label}: materialized`);
  assert.equal(workspaceSolutionKeysComplete(report), false, `${label}: keys`);
  assert.equal(workspaceSolutionPageAvailable(report), false, `${label}: page`);
}

test('CoverageSummary availability is one atomic fail-closed tuple', () => {
  const canonical = canonicalCoverageSummaryReport();
  assertUnavailable(canonical, 'canonical');
  for (const [language, countLabel, explanation] of [
    ['en', 'Not calculated', 'The solution set was not calculated for this coverage-only result.'],
    ['ko', '계산하지 않음', '이 커버리지 전용 결과에서는 해법 집합을 계산하지 않았습니다.']
  ]) {
    assert.equal(workspaceMessage(language, 'notCalculated'), countLabel);
    assert.equal(workspaceMessage(language, 'solutionSetNotCalculated'), explanation);
  }

  for (const [key, malformed] of [
    ['search_output_policy', 'coverage_summary'],
    ['unique_solution_count', '0'],
    ['normalized_unique_solution_count', '0'],
    ['solution_count_calculated', 'true'],
    ['solution_set_materialized', 'true'],
    ['solution_keys_materialized_count', '1'],
    ['solution_keys_complete', 'true'],
    ['solution_page_available', 'true'],
    ['normalized_solution_set_hash', 'cts1:stale'],
    ['actual_normalized_solution_set_hash', 'cts1:stale'],
    ['mirror_normalized_solution_set_hash', 'cts1:stale']
  ]) {
    const report = canonicalCoverageSummaryReport();
    report.summary_fields = report.summary_fields.map(([field, value]) =>
      field === key ? [field, malformed] : [field, value]
    );
    assertUnavailable(report, `malformed ${key}`);
  }

  for (const [key] of canonical.summary_fields) {
    const report = canonicalCoverageSummaryReport();
    report.summary_fields = report.summary_fields.filter(([field]) => field !== key);
    assertUnavailable(report, `missing ${key}`);
  }

  const contradictoryProjection = {
    ...canonicalCoverageSummaryReport(),
    unique_solution_count: 17,
    normalized_solution_keys: ['stale-solution'],
    normalized_solution_set_hash: 'cts1:stale',
    solution_count_calculated: true,
    solution_set_materialized: true,
    solution_keys_materialized_count: 1,
    solution_keys_complete: true,
    solution_page_available: true
  };
  assertUnavailable(contradictoryProjection, 'contradictory projected fields');
});

test('numeric zero requires explicit complete authority outside CoverageSummary', () => {
  assert.equal(workspaceSolutionCount({ unique_solution_count: 0 }), null);
  assert.equal(
    workspaceSolutionCount({
      unique_solution_count: 0,
      solution_count_calculated: true,
      count_complete: true,
      execution_availability: { state: 'available' },
      result_completeness: 'complete',
      summary_fields: [['search_output_policy', 'summary']]
    }),
    0
  );
  assertUnavailable({
    unique_solution_count: 0,
    solution_count_calculated: true,
    summary_fields: [
      ['unique_solution_count', 'not-calculated'],
      ['normalized_unique_solution_count', 'not-calculated']
    ]
  }, 'missing CoverageSummary policy');
});

test('typed pc tiling keeps the complete family count while exposing its next page', () => {
  const report = {
    search_output_policy: 'tiling-only',
    unique_solution_count: 137,
    normalized_solution_keys: Array.from({ length: 100 }, (_, index) => `ctk1:${index}`),
    normalized_solution_set_hash: 'cts1:typed-family',
    solution_count_calculated: true,
    solution_set_materialized: true,
    solution_keys_materialized_count: 100,
    solution_keys_complete: false,
    solution_page_available: true,
    count_complete: true,
    execution_availability: { state: 'available' },
    result_completeness: 'complete',
    summary_fields: [
      ['search_output_policy', 'tiling-only'],
      ['unique_solution_count', '137'],
      ['normalized_unique_solution_count', '137'],
      ['solution_count_calculated', 'true'],
      ['solution_set_materialized', 'true'],
      ['solution_keys_materialized_count', '100'],
      ['solution_keys_complete', 'false'],
      ['solution_page_available', 'true'],
      ['normalized_solution_set_hash', 'cts1:typed-family'],
      ['actual_normalized_solution_set_hash', 'cts1:typed-family']
    ]
  };

  assert.equal(workspaceSolutionCount(report), 137);
  assert.equal(workspaceSolutionCountCalculated(report), true);
  assert.equal(workspaceSolutionSetMaterialized(report), true);
  assert.equal(workspaceSolutionKeysComplete(report), false);
  assert.equal(workspaceSolutionPageAvailable(report), true);

  assertUnavailable({
    ...report,
    search_output_policy: 'tiling_only',
    summary_fields: report.summary_fields.map(([key, value]) =>
      key === 'search_output_policy' ? [key, 'tiling_only'] : [key, value]
    )
  }, 'noncanonical tiling policy');
});

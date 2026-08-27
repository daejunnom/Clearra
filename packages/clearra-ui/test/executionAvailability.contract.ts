import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import {
  executionResultIsComplete,
  isExecutionAvailabilityReport,
  type ExecutionAvailabilityReport
} from '../src/lib/wasm/executionAvailability.ts';
import {
  workspaceSolutionCount,
  workspaceSolutionCountCalculated
} from '../src/lib/workspace/solutionSetAvailability.ts';

const fixture = JSON.parse(await readFile(
  resolve(process.cwd(), 'tests/fixtures/contracts/execution_resource_authority.v1.json'),
  'utf8'
));
assert.equal(fixture.schema_version, 'clearra.execution-resource-authority.v1');

const availabilityCases = fixture.availability_cases as Array<{
  state: ExecutionAvailabilityReport['state'];
  reason: ExecutionAvailabilityReport['reason'];
  result_completeness: 'not-executed' | 'complete' | 'incomplete';
}>;
for (const fixtureCase of availabilityCases) {
  const report = reportFrom(fixtureCase);
  assert.equal(isExecutionAvailabilityReport(report), true);
  assert.equal(
    executionResultIsComplete(report, fixtureCase.result_completeness),
    false,
    `${fixtureCase.state} must not imply a complete result`
  );
}

for (const fixtureCase of fixture.dense_pattern_cases as Array<{
  surface: ExecutionAvailabilityReport['surface'];
  state: ExecutionAvailabilityReport['state'];
  reason: ExecutionAvailabilityReport['reason'];
  descriptor_pattern_count: string;
  dense_pattern_count: string;
  required_dense_bytes: string;
  required_memory_bytes: string;
  result_completeness: 'not-executed' | 'complete' | 'incomplete';
}>) {
  const report = { ...fixtureCase };
  assert.equal(isExecutionAvailabilityReport(report), true, fixtureCase.surface);
  assert.equal(
    executionResultIsComplete(report, fixtureCase.result_completeness),
    false,
    `${fixtureCase.surface} 6L availability must not project empty complete`
  );
}

const completeAvailability = reportFrom({ state: 'available', reason: null });
assert.equal(executionResultIsComplete(completeAvailability, 'complete'), true);
assert.equal(executionResultIsComplete(completeAvailability, 'incomplete'), false);

assert.equal(isExecutionAvailabilityReport({
  ...completeAvailability,
  reason: 'partial-execution'
}), false);
assert.equal(isExecutionAvailabilityReport({
  ...completeAvailability,
  descriptor_pattern_count: '64',
  dense_pattern_count: null,
  required_dense_bytes: '8'
}), false);
assert.equal(isExecutionAvailabilityReport({
  ...completeAvailability,
  descriptor_pattern_count: '64',
  dense_pattern_count: '64',
  required_dense_bytes: '9'
}), false);
assert.equal(isExecutionAvailabilityReport({
  ...completeAvailability,
  descriptor_pattern_count: '63',
  dense_pattern_count: '64',
  required_dense_bytes: '8'
}), false);
assert.equal(isExecutionAvailabilityReport({
  ...completeAvailability,
  descriptor_pattern_count: '64',
  dense_pattern_count: '64',
  required_dense_bytes: '8',
  required_memory_bytes: '7'
}), false);
assert.equal(isExecutionAvailabilityReport({
  ...completeAvailability,
  required_memory_bytes: '01'
}), false);

assert.equal(
  workspaceSolutionCountCalculated({
    search_output_policy: 'summary',
    unique_solution_count: 0
  }),
  false,
  'legacy numeric zero without typed evidence remains unknown/incomplete'
);
const completedZero = {
  search_output_policy: 'summary',
  unique_solution_count: 0,
  solution_count_calculated: true,
  count_complete: true,
  execution_availability: completeAvailability,
  result_completeness: 'complete' as const
};
assert.equal(workspaceSolutionCountCalculated(completedZero), true);
assert.equal(workspaceSolutionCount(completedZero), 0);
assert.equal(workspaceSolutionCountCalculated({
  ...completedZero,
  count_complete: false
}), false);
assert.equal(workspaceSolutionCountCalculated({
  ...completedZero,
  execution_availability: reportFrom({
    state: 'unavailable',
    reason: 'dense-pattern-representation-unavailable'
  })
}), false);
assert.equal(workspaceSolutionCountCalculated({
  ...completedZero,
  result_completeness: 'incomplete'
}), false);

function reportFrom(value: {
  state: ExecutionAvailabilityReport['state'];
  reason: ExecutionAvailabilityReport['reason'];
}): ExecutionAvailabilityReport {
  return {
    state: value.state,
    reason: value.reason,
    surface: 'browser-wasm32',
    descriptor_pattern_count: null,
    dense_pattern_count: null,
    required_dense_bytes: null,
    required_memory_bytes: null
  };
}

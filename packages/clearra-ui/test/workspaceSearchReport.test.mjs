import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export { projectWorkspaceSearchReport } from './src/lib/workspace/workspaceSearchReport.ts';
      export { workspaceSolutionCount } from './src/lib/workspace/solutionSetAvailability.ts';
      export { workspacePublicFailureMessage } from './src/lib/workspace/workspacePublicFailure.ts';
      export {
        workspaceViewFromDesktop,
        workspaceViewFromWasm
      } from './src/lib/workspace/workspaceRuntime.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

const available = {
  state: 'available',
  reason: null,
  surface: 'browser-wasm32',
  descriptor_pattern_count: null,
  dense_pattern_count: null,
  required_dense_bytes: null,
  required_memory_bytes: null
};

function completeStandardPcReport() {
  return {
    unique_solution_count: 2,
    normalized_solution_keys: [
      'ctk1|initial=0000000000000000|placements=I:000000000000000f',
      'ctk1|initial=0000000000000000|placements=O:0000000000000033'
    ],
    normalized_solution_set_hash: 'cts1:standard-pc',
    solution_count_calculated: true,
    solution_set_materialized: true,
    solution_keys_materialized_count: 2,
    solution_keys_complete: true,
    solution_page_available: false,
    count_complete: true,
    summary_fields: [
      ['search_output_policy', 'trace'],
      ['solution_count_calculated', 'true'],
      ['solution_set_materialized', 'true']
    ]
  };
}

test('workspace projection joins host authority to standard PC solutions exactly once', () => {
  const raw = completeStandardPcReport();
  assert.equal(production.workspaceSolutionCount(raw), null, 'unjoined DTO fails closed');

  const projected = production.projectWorkspaceSearchReport(raw, available, 'complete');
  assert.notEqual(projected, raw);
  assert.equal(production.workspaceSolutionCount(projected), 2);
  assert.deepEqual(projected.normalized_solution_keys, raw.normalized_solution_keys);
  assert.equal(Object.hasOwn(raw, 'execution_availability'), false);
  assert.equal(Object.hasOwn(raw, 'result_completeness'), false);
});

test('workspace projection never invents missing or incomplete execution authority', () => {
  const raw = completeStandardPcReport();
  const missing = production.projectWorkspaceSearchReport(raw, null, null);
  const incomplete = production.projectWorkspaceSearchReport(raw, available, 'incomplete');

  assert.equal(missing, raw);
  assert.equal(production.workspaceSolutionCount(missing), null);
  assert.equal(production.workspaceSolutionCount(incomplete), null);
});

test('Web and desktop runtime adapters both expose complete standard PC solution families', () => {
  const report = completeStandardPcReport();
  const commonState = {
    status: 'completed',
    jobId: null,
    progressLabel: '',
    progressDone: 1,
    progressTotal: 1,
    diagnostics: [],
    searchReport: report,
    error: null
  };
  const web = production.workspaceViewFromWasm({
    ...commonState,
    terminationReason: null,
    forwardPatternDone: 0,
    forwardPatternTotal: 0,
    progressTelemetry: null,
    response: null,
    resourceReport: null,
    executionAvailability: available,
    resultCompleteness: 'complete',
    webgpuBackend: null
  });
  const desktop = production.workspaceViewFromDesktop({
    ...commonState,
    result: null,
    backendStatus: null,
    resourceStatus: {
      execution_availability: { ...available, surface: 'native' },
      result_completeness: 'complete'
    }
  });

  assert.equal(production.workspaceSolutionCount(web.searchReport), 2);
  assert.equal(production.workspaceSolutionCount(desktop.searchReport), 2);
  assert.deepEqual(web.searchReport.normalized_solution_keys, report.normalized_solution_keys);
  assert.deepEqual(desktop.searchReport.normalized_solution_keys, report.normalized_solution_keys);
});

test('Web and desktop use typed admission causes without publishing private diagnostic details', () => {
  for (const [state, reason, expected] of [
    ['unavailable', 'pattern-count-address-space-exceeded', 'resource-limit'],
    ['unavailable', 'dense-pattern-representation-unavailable', 'resource-limit'],
    ['exhausted', 'compute-budget-exceeded', 'resource-limit'],
    ['exhausted', 'memory-budget-exceeded', 'resource-limit'],
    ['unavailable', 'capability-unavailable', 'unsupported'],
    ['cancelled', 'cancelled-by-caller', 'request-cancelled'],
    ['unavailable', 'not-executed', 'execution-failed'],
    ['incomplete', 'partial-execution', 'runtime-trap']
  ]) {
    const diagnostics = [{ code: reason === 'partial-execution' ? 'E_WASM_RUNTIME_TRAP' : 'E_WASM_DISTRIBUTED_START',
      severity: 'error', message: 'private candidate_id=81 problem_id=fixture' }];
    const availability = { ...available, state, reason };
    const commonState = { status: 'failed', jobId: 1, progressLabel: '', progressDone: 0,
      progressTotal: 1, diagnostics, searchReport: null, error: 'private input=fixture' };
    const web = production.workspaceViewFromWasm({ ...commonState,
      terminationReason: null, forwardPatternDone: 0, forwardPatternTotal: 0,
      progressTelemetry: null, response: null, resourceReport: null,
      executionAvailability: availability, resultCompleteness: 'not-executed', webgpuBackend: null });
    const desktop = production.workspaceViewFromDesktop({ ...commonState,
      result: null, backendStatus: null, resourceStatus: {
        execution_availability: { ...availability, surface: 'native' }, result_completeness: 'not-executed' } });
    for (const view of [web, desktop]) {
      assert.deepEqual(view.publicFailures, [{ code: expected, severity: 'error' }], reason);
      assert.deepEqual(view.developerDiagnostics, diagnostics);
      assert.equal(view.developerError, commonState.error);
      for (const language of ['en', 'ko', 'ja']) {
        assert.doesNotMatch(production.workspacePublicFailureMessage(language, view.publicFailures[0]),
          /private|candidate_id|problem_id|E_WASM|fixture|pattern-count|dense-pattern/u);
      }
    }
  }
});

import assert from 'node:assert/strict';

import {
  buildWorkspaceProgressModel,
  type WorkspaceProgressInput
} from '../src/lib/workspace/workspaceProgressModel.ts';

function input(
  profile: WorkspaceProgressInput['profile'],
  telemetry: NonNullable<WorkspaceProgressInput['telemetry']>
): WorkspaceProgressInput {
  return {
    profile,
    status: 'running',
    progressLabel: '',
    progressDone: 0,
    progressTotal: 0,
    telemetry
  };
}

function telemetry(
  overrides: Partial<NonNullable<WorkspaceProgressInput['telemetry']>> = {}
): NonNullable<WorkspaceProgressInput['telemetry']> {
  return {
    phase: 'searching',
    producer_complete: false,
    geometry_nodes: 10,
    candidates_emitted: 1,
    geometry_family_count: null,
    candidates_verified: 0,
    producer_build_nodes: 0,
    producer_coverage_checks: 0,
    build_nodes: 0,
    coverage_checks: 0,
    ready_workers: 1,
    active_workers: 0,
    worker_count: 1,
    oldest_batch_ms: 0,
    pass_index: 0,
    pass_count: 1,
    layer_index: 0,
    layer_count: 0,
    layer_done: 0,
    layer_total: 0,
    ...overrides
  };
}

const producing = buildWorkspaceProgressModel(input('pc', telemetry()));
assert.equal(producing.stages.find((stage) => stage.id === 'geometry')?.status, 'running');
assert.equal(producing.stages.find((stage) => stage.id === 'verify')?.status, 'pending');

const verifying = buildWorkspaceProgressModel(
  input('pc', telemetry({ candidates_verified: 1, build_nodes: 12 }))
);
assert.equal(verifying.stages.find((stage) => stage.id === 'geometry')?.status, 'complete');
assert.equal(verifying.stages.find((stage) => stage.id === 'verify')?.status, 'running');
assert.equal(
  verifying.stages.filter((stage) => stage.status === 'running').length,
  1
);

const tiling = buildWorkspaceProgressModel(
  input('tiling', telemetry({ candidates_emitted: 20 }))
);
assert.equal(tiling.stages.some((stage) => stage.id === 'verify'), false);
assert.equal(tiling.stages.find((stage) => stage.id === 'geometry')?.status, 'running');

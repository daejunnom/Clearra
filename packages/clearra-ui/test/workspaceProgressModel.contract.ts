import assert from 'node:assert/strict';

import {
  buildWorkspaceProgressModel,
  type WorkspaceProgressInput
} from '../src/lib/workspace/workspaceProgressModel.ts';

function input(
  profile: WorkspaceProgressInput['profile'],
  telemetry: NonNullable<WorkspaceProgressInput['telemetry']>,
  mode: WorkspaceProgressInput['mode'] = 'default'
): WorkspaceProgressInput {
  return {
    profile,
    mode,
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
    availability: telemetryFlags(true),
    exactness: telemetryFlags(true),
    ...overrides
  };
}

function telemetryFlags(value: boolean) {
  return {
    geometry_nodes: value,
    candidates_emitted: value,
    geometry_family_count: value,
    candidates_verified: value,
    producer_build_nodes: value,
    producer_coverage_checks: value,
    build_nodes: value,
    coverage_checks: value,
    ready_workers: value,
    active_workers: value,
    worker_count: value,
    oldest_batch_ms: value,
    pass_index: value,
    pass_count: value,
    layer_index: value,
    layer_count: value,
    layer_done: value,
    layer_total: value
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

const setupGeometry = buildWorkspaceProgressModel(
  input(
    'setup',
    telemetry({
      candidates_emitted: 0,
      geometry_family_count: '80',
      pass_index: 0,
      pass_count: 4
    })
  )
);
assert.equal(setupGeometry.stages.find((stage) => stage.id === 'geometry')?.status, 'running');
assert.equal(setupGeometry.stages.find((stage) => stage.id === 'graph')?.status, 'pending');

const setupPartialBuild = buildWorkspaceProgressModel(
  input(
    'setup',
    telemetry({
      candidates_emitted: 0,
      geometry_family_count: '80',
      producer_build_nodes: 1_024,
      pass_index: 1,
      pass_count: 4,
      layer_index: 2,
      layer_count: 5,
      layer_done: 37,
      layer_total: 120
    })
  )
);
const partialBuildGraph = setupPartialBuild.stages.find((stage) => stage.id === 'graph');
assert.equal(setupPartialBuild.stages.find((stage) => stage.id === 'geometry')?.status, 'complete');
assert.equal(partialBuildGraph?.status, 'running');
assert.equal(partialBuildGraph?.done, '37');
assert.equal(partialBuildGraph?.total, '120');

const setupDispatch = buildWorkspaceProgressModel(
  input(
    'setup',
    telemetry({
      candidates_emitted: 1,
      geometry_family_count: '80',
      producer_build_nodes: 1_024,
      pass_index: 3,
      pass_count: 4
    })
  )
);
assert.equal(setupDispatch.stages.find((stage) => stage.id === 'graph')?.status, 'complete');
assert.equal(setupDispatch.stages.find((stage) => stage.id === 'tasks')?.status, 'running');

const minimumCoverPostprocess = buildWorkspaceProgressModel(
  input(
    'pc',
    telemetry({
      phase: 'postprocessing',
      producer_complete: true,
      candidates_emitted: 80,
      geometry_family_count: '80',
      candidates_verified: 80
    }),
    'pc-minimum-cover'
  )
);
assert.equal(minimumCoverPostprocess.stages.find((stage) => stage.id === 'verify')?.status, 'complete');
assert.equal(minimumCoverPostprocess.stages.find((stage) => stage.id === 'finalize')?.status, 'running');
assert.equal(
  minimumCoverPostprocess.stages.find((stage) => stage.id === 'finalize')?.labelKey,
  'progressStageMinimumCover'
);

const scoreMerge = buildWorkspaceProgressModel(
  input(
    'pc',
    telemetry({ phase: 'merging', producer_complete: true, candidates_verified: 80 }),
    'pc-score'
  )
);
assert.equal(scoreMerge.stages.find((stage) => stage.id === 'finalize')?.status, 'running');
assert.equal(
  scoreMerge.stages.find((stage) => stage.id === 'finalize')?.labelKey,
  'progressStageScore'
);

const setupPostprocess = buildWorkspaceProgressModel(
  input(
    'setup',
    telemetry({ phase: 'postprocessing', producer_complete: true, pass_index: 3, pass_count: 4 }),
    'setup-qb'
  )
);
assert.equal(setupPostprocess.stages.find((stage) => stage.id === 'tasks')?.status, 'complete');
assert.equal(setupPostprocess.stages.find((stage) => stage.id === 'finalize')?.status, 'running');

const buildSpinPostprocess = buildWorkspaceProgressModel(
  input(
    'build',
    telemetry({ phase: 'postprocessing', producer_complete: true, candidates_verified: 12 }),
    'build-spin'
  )
);
assert.equal(buildSpinPostprocess.stages.find((stage) => stage.id === 'verify')?.status, 'complete');
assert.equal(
  buildSpinPostprocess.stages.find((stage) => stage.id === 'finalize')?.labelKey,
  'progressStageSpinCoverage'
);

const damagePostprocess = buildWorkspaceProgressModel(
  input(
    'damage',
    telemetry({ phase: 'postprocessing', producer_complete: true }),
    'damage-at-least'
  )
);
assert.equal(damagePostprocess.stages.find((stage) => stage.id === 'forward')?.status, 'complete');
assert.equal(damagePostprocess.stages.find((stage) => stage.id === 'classify')?.status, 'running');

const spinPostprocess = buildWorkspaceProgressModel(
  input('spin', telemetry({ phase: 'postprocessing', producer_complete: true }), 'spin')
);
assert.equal(spinPostprocess.stages.find((stage) => stage.id === 'forward')?.status, 'complete');
assert.equal(spinPostprocess.stages.find((stage) => stage.id === 'classify')?.status, 'running');

const serialScorePostprocess = buildWorkspaceProgressModel({
  profile: 'pc',
  mode: 'pc-score',
  status: 'running',
  progressLabel: 'postprocess',
  progressDone: 0,
  progressTotal: 1,
  telemetry: null
});
assert.equal(serialScorePostprocess.stages.find((stage) => stage.id === 'verify')?.status, 'complete');
assert.equal(serialScorePostprocess.stages.find((stage) => stage.id === 'finalize')?.status, 'running');

import type { ClearraSearchProgressTelemetry } from '../wasm/wasmCommandClient';
import type { WorkspaceMessageKey } from './workspaceI18n';
import type { WorkspaceRuntimeStatus } from './workspaceRuntime';

export type WorkspaceProgressProfile = 'pc' | 'tiling' | 'setup' | 'build' | 'damage' | 'spin';
export type WorkspaceProgressMode =
  | 'default'
  | 'pc-all'
  | 'pc-minimum-cover'
  | 'pc-score'
  | 'pc-failed-queue'
  | 'setup-oracle'
  | 'setup-qb'
  | 'buildability'
  | 'build-spin'
  | 'damage-maximum'
  | 'damage-at-least'
  | 'spin';
export type WorkspaceProgressStageStatus = 'pending' | 'running' | 'complete' | 'stopped';

export type WorkspaceProgressStage = {
  id: string;
  labelKey: WorkspaceMessageKey;
  status: WorkspaceProgressStageStatus;
  done: string | null;
  total: string | null;
  percent: number | null;
  metrics: WorkspaceProgressMetric[];
};

export type WorkspaceProgressMetric = {
  labelKey: WorkspaceMessageKey;
  value: string;
  total: string | null;
  kind: 'count';
};

export type WorkspaceProgressModel = {
  stages: WorkspaceProgressStage[];
  completedStages: number;
  totalStages: number;
  overallPercent: number;
};

export type WorkspaceProgressInput = {
  profile: WorkspaceProgressProfile;
  mode?: WorkspaceProgressMode;
  status: WorkspaceRuntimeStatus;
  progressLabel: string;
  progressDone: number;
  progressTotal: number;
  forwardPatternDone?: number;
  forwardPatternTotal?: number;
  telemetry: ClearraSearchProgressTelemetry | null;
};

type StageDefinition = Pick<WorkspaceProgressStage, 'id' | 'labelKey'>;

const COMMON_PREPARE: StageDefinition = {
  id: 'prepare',
  labelKey: 'progressStagePrepare'
};
const COMMON_AGGREGATE: StageDefinition = {
  id: 'finalize',
  labelKey: 'progressStageAggregate'
};

const GEOMETRY_STAGE: StageDefinition = {
  id: 'geometry',
  labelKey: 'progressStageGeometry'
};
const VERIFY_STAGE: StageDefinition = {
  id: 'verify',
  labelKey: 'progressStageBuildVerify'
};

function profileStages(
  profile: WorkspaceProgressProfile,
  mode: WorkspaceProgressMode
): StageDefinition[] {
  if (profile === 'tiling') return [COMMON_PREPARE, GEOMETRY_STAGE, COMMON_AGGREGATE];
  if (profile === 'pc') {
    const labelKey: WorkspaceMessageKey =
      mode === 'pc-minimum-cover'
        ? 'progressStageMinimumCover'
        : mode === 'pc-score'
          ? 'progressStageScore'
          : mode === 'pc-failed-queue'
            ? 'progressStageFailedQueues'
            : 'progressStageSolutions';
    return [COMMON_PREPARE, GEOMETRY_STAGE, VERIFY_STAGE, { id: 'finalize', labelKey }];
  }
  if (profile === 'setup') {
    return [
      COMMON_PREPARE,
      GEOMETRY_STAGE,
      { id: 'graph', labelKey: 'progressStageSetupGraph' },
      { id: 'tasks', labelKey: 'progressStageSetupCoverage' },
      { id: 'finalize', labelKey: 'progressStageSetupFinalize' }
    ];
  }
  if (profile === 'build') {
    return [
      COMMON_PREPARE,
      GEOMETRY_STAGE,
      VERIFY_STAGE,
      {
        id: 'finalize',
        labelKey:
          mode === 'build-spin'
            ? 'progressStageSpinCoverage'
            : 'progressStageBuildProbability'
      }
    ];
  }
  if (profile === 'damage') {
    return [
      COMMON_PREPARE,
      { id: 'forward', labelKey: 'progressStageForwardSearch' },
      { id: 'classify', labelKey: 'progressStageDamage' }
    ];
  }
  return [
    COMMON_PREPARE,
    { id: 'patterns', labelKey: 'progressStagePatterns' },
    { id: 'forward', labelKey: 'progressStageForwardSearch' },
    { id: 'classify', labelKey: 'progressStageSpin' }
  ];
}

export function buildWorkspaceProgressModel(
  input: WorkspaceProgressInput
): WorkspaceProgressModel {
  const stages = profileStages(input.profile, input.mode ?? 'default').map<WorkspaceProgressStage>((definition) => ({
    ...definition,
    status: 'pending',
    done: null,
    total: null,
    percent: null,
    metrics: []
  }));

  if (input.status === 'idle') return summarize(stages);

  if (input.profile === 'damage' || input.profile === 'spin') {
    applyForwardProgress(stages, input);
  } else if (input.profile === 'setup') {
    applySetupProgress(stages, input);
  } else {
    applyExactProgress(stages, input);
  }

  if (input.status === 'completed') {
    for (const stage of stages) stage.status = 'complete';
  } else if (
    input.status === 'failed' ||
    input.status === 'cancelled' ||
    input.status === 'terminated'
  ) {
    for (const stage of stages) {
      if (stage.status === 'running') stage.status = 'stopped';
    }
  }
  return summarize(stages);
}

function applyExactProgress(stages: WorkspaceProgressStage[], input: WorkspaceProgressInput) {
  const telemetry = input.telemetry;
  if (!telemetry) {
    applyCoarseProgress(stages, input);
    return;
  }

  const phase = telemetry.phase;
  applyWorkerPreparation(stages[0], telemetry);

  const geometry = stage(stages, 'geometry');
  const verify = stage(stages, 'verify');
  const finalize = stage(stages, 'finalize');
  const producerDone =
    telemetry.producer_complete || isFinalizingPhase(phase) || phase === 'draining';
  const predictedCandidates = predictedCandidateTotal(telemetry);
  const candidateGenerationStarted =
    telemetry.candidates_emitted > 0 || predictedCandidates !== null;
  const verificationStarted =
    verify !== undefined &&
    (telemetry.candidates_verified > 0 ||
      telemetry.coverage_checks > 0 ||
      producerDone);

  if (geometry) {
    geometry.status =
      producerDone || verificationStarted
        ? 'complete'
        : phase === 'searching'
          ? 'running'
          : 'pending';
    if (candidateGenerationStarted) {
      applyMetric(
        geometry,
        telemetry.candidates_emitted,
        predictedCandidates
      );
    } else {
      applyMetric(geometry, telemetry.geometry_nodes, null);
    }
    addMetric(geometry, 'progressMetricNodes', telemetry.geometry_nodes);
    if (candidateGenerationStarted) {
      addMetric(
        geometry,
        'progressMetricCandidates',
        telemetry.candidates_emitted,
        predictedCandidates
      );
    }
    if (telemetry.pass_count > 1) {
      addMetric(
        geometry,
        'progressMetricPass',
        Math.min(telemetry.pass_index + 1, telemetry.pass_count),
        telemetry.pass_count
      );
    }
  }
  if (verify) {
    verify.status =
      isFinalizingPhase(phase)
        ? 'complete'
        : verificationStarted
          ? 'running'
          : 'pending';
    applyMetric(
      verify,
      telemetry.candidates_verified,
      predictedCandidates
    );
    addMetric(verify, 'progressMetricBuildNodes', telemetry.build_nodes);
    addMetric(verify, 'progressMetricChecks', telemetry.coverage_checks);
    if (telemetry.worker_count > 0) {
      addMetric(
        verify,
        'progressMetricWorkers',
        telemetry.active_workers,
        telemetry.worker_count
      );
    }
  }
  if (finalize) finalize.status = isFinalizingPhase(phase) ? 'running' : 'pending';
}

function applySetupProgress(stages: WorkspaceProgressStage[], input: WorkspaceProgressInput) {
  const telemetry = input.telemetry;
  if (!telemetry) {
    applyCoarseProgress(stages, input);
    return;
  }

  applyWorkerPreparation(stages[0], telemetry);
  const geometry = stage(stages, 'geometry');
  const graph = stage(stages, 'graph');
  const tasks = stage(stages, 'tasks');
  const finalize = stage(stages, 'finalize');
  const phase = telemetry.phase;
  const producerDone =
    telemetry.producer_complete || phase === 'draining' || isFinalizingPhase(phase);
  const exactPipeline = telemetry.pass_count >= 4;
  const pipelineStage = exactPipeline
    ? Math.min(telemetry.pass_index, telemetry.pass_count - 1)
    : null;
  const geometryCompiled = exactPipeline
    ? (pipelineStage ?? 0) >= 1 || producerDone
    : telemetry.candidates_emitted > 0 || producerDone;
  const graphStarted =
    (pipelineStage !== null && pipelineStage >= 1) ||
    telemetry.producer_build_nodes > 0 ||
    telemetry.candidates_emitted > 0 ||
    producerDone;
  const graphComplete = exactPipeline
    ? (pipelineStage ?? 0) >= 3 || producerDone
    : telemetry.candidates_emitted > 0 || producerDone;
  const tasksStarted =
    (pipelineStage !== null && pipelineStage >= 3) ||
    telemetry.candidates_emitted > 0 ||
    telemetry.candidates_verified > 0 ||
    producerDone;

  if (geometry) {
    geometry.status =
      geometryCompiled || graphStarted
        ? 'complete'
        : phase === 'searching'
          ? 'running'
          : 'pending';
    applyMetric(geometry, telemetry.geometry_nodes, null);
  }
  if (graph) {
    graph.status =
      graphComplete
        ? 'complete'
        : graphStarted
          ? 'running'
          : 'pending';
    if (graph.status === 'running' && telemetry.layer_total > 0) {
      applyMetric(graph, telemetry.layer_done, telemetry.layer_total);
      addMetric(
        graph,
        'progressMetricLayerWork',
        telemetry.layer_done,
        telemetry.layer_total
      );
    } else {
      applyMetric(graph, telemetry.producer_build_nodes, null);
    }
    addMetric(graph, 'progressMetricBuildNodes', telemetry.producer_build_nodes);
  }
  if (tasks) {
    tasks.status =
      isFinalizingPhase(phase)
        ? 'complete'
        : tasksStarted
          ? 'running'
          : 'pending';
    applyMetric(
      tasks,
      telemetry.candidates_verified,
      telemetry.geometry_family_count
    );
    addMetric(
      tasks,
      'progressMetricDispatched',
      telemetry.candidates_emitted,
      telemetry.geometry_family_count
    );
    addMetric(
      tasks,
      'progressMetricReduced',
      telemetry.producer_coverage_checks,
      telemetry.geometry_family_count
    );
    if (telemetry.worker_count > 0) {
      addMetric(
        tasks,
        'progressMetricWorkers',
        telemetry.active_workers,
        telemetry.worker_count
      );
    }
  }
  if (finalize) finalize.status = isFinalizingPhase(phase) ? 'running' : 'pending';
}

function applyForwardProgress(stages: WorkspaceProgressStage[], input: WorkspaceProgressInput) {
  const prepare = stage(stages, 'prepare');
  const patterns = stage(stages, 'patterns');
  const forward = stage(stages, 'forward');
  const classify = stage(stages, 'classify');
  const telemetry = input.telemetry;
  const searchStarted =
    telemetry?.phase === 'searching' ||
    telemetry?.phase === 'draining' ||
    telemetry?.phase === 'postprocessing' ||
    telemetry?.phase === 'merging' ||
    input.progressLabel === 'forward-search' ||
    input.progressLabel === 'forward-search-patterns' ||
    input.progressDone > 0;

  if (prepare) {
    if (telemetry) applyWorkerPreparation(prepare, telemetry);
    else prepare.status = searchStarted ? 'complete' : 'running';
  }
  if (patterns) {
    const done = telemetry ? telemetry.pass_index : input.forwardPatternDone ?? 0;
    const total = telemetry ? telemetry.pass_count : input.forwardPatternTotal ?? 0;
    const producerDone =
      telemetry?.producer_complete ||
      telemetry?.phase === 'draining' ||
      telemetry?.phase === 'postprocessing' ||
      telemetry?.phase === 'merging';
    patterns.status =
      producerDone || (total > 0 && done >= total)
        ? 'complete'
        : searchStarted
          ? 'running'
          : 'pending';
    applyMetric(patterns, done, total > 0 ? total : null);
    addMetric(patterns, 'progressMetricPatterns', done, total > 0 ? total : null);
  }
  if (forward) {
    const forwardDone = telemetry !== null && (
      telemetry.producer_complete ||
      telemetry.phase === 'draining' ||
      isFinalizingPhase(telemetry.phase)
    );
    forward.status =
      forwardDone
        ? 'complete'
        : searchStarted
          ? 'running'
          : 'pending';
    if (telemetry) {
      if (telemetry.layer_count > 0) {
        const currentLayer = Math.min(
          telemetry.layer_index + 1,
          telemetry.layer_count
        );
        applyMetric(forward, currentLayer, telemetry.layer_count);
        const currentLayerFraction =
          telemetry.layer_total > 0
            ? Math.min(1, telemetry.layer_done / telemetry.layer_total)
            : 0;
        forward.percent = Math.min(
          100,
          ((Math.min(telemetry.layer_index, telemetry.layer_count) +
            currentLayerFraction) /
            telemetry.layer_count) *
            100
        );
        addMetric(
          forward,
          'progressMetricLayerWork',
          telemetry.layer_done,
          telemetry.layer_total > 0 ? telemetry.layer_total : null
        );
      } else {
        applyMetric(forward, telemetry.geometry_nodes, null);
      }
      addMetric(forward, 'progressMetricStates', telemetry.geometry_nodes);
      if (telemetry.worker_count > 0) {
        addMetric(
          forward,
          'progressMetricWorkers',
          telemetry.active_workers,
          telemetry.worker_count
        );
      }
    } else {
      applyMetric(
        forward,
        input.progressLabel === 'forward-search' ? input.progressDone : 0,
        null
      );
    }
  }
  if (classify) {
    const classificationStarted = telemetry
      ? telemetry.producer_complete ||
        telemetry.phase === 'draining' ||
        isFinalizingPhase(telemetry.phase)
      : searchStarted;
    classify.status =
      classificationStarted ? 'running' : 'pending';
    if (telemetry) {
      if (input.profile === 'damage') {
        applyMetric(
          classify,
          telemetry.candidates_verified,
          telemetry.producer_complete ? telemetry.candidates_emitted : null
        );
        addMetric(
          classify,
          'progressMetricDispatched',
          telemetry.candidates_emitted
        );
      } else {
        applyMetric(classify, telemetry.coverage_checks, null);
      }
      addMetric(classify, 'progressMetricLegalLocks', telemetry.coverage_checks);
    }
  }
}

function applyWorkerPreparation(
  prepare: WorkspaceProgressStage,
  telemetry: ClearraSearchProgressTelemetry
) {
  const preparing =
    telemetry.phase === 'preparing' || telemetry.phase === 'initializing';
  prepare.status = preparing ? 'running' : 'complete';
}

function applyCoarseProgress(
  stages: WorkspaceProgressStage[],
  input: WorkspaceProgressInput
) {
  if (input.status === 'validating') {
    stages[0].status = 'running';
    return;
  }

  if (input.progressLabel === 'postprocess') {
    const finalStageIndex = Math.max(
      0,
      stages.findIndex((candidate) =>
        candidate.id === 'finalize' || candidate.id === 'classify'
      )
    );
    for (let index = 0; index < finalStageIndex; index += 1) {
      stages[index].status = 'complete';
    }
    stages[finalStageIndex].status = 'running';
    applyMetric(
      stages[finalStageIndex],
      input.progressDone,
      input.progressTotal > 0 ? input.progressTotal : null
    );
    return;
  }

  const activeIndex = Math.min(
    stages.length - 1,
    Math.max(1, Math.trunc(input.progressDone))
  );
  for (let index = 0; index < activeIndex; index += 1) {
    stages[index].status = 'complete';
  }
  const active = stages[activeIndex];
  active.status = 'running';
  applyMetric(active, input.progressDone, input.progressTotal > 0 ? input.progressTotal : null);
}

function isFinalizingPhase(
  phase: ClearraSearchProgressTelemetry['phase']
): boolean {
  return phase === 'postprocessing' || phase === 'merging';
}

function stage(
  stages: WorkspaceProgressStage[],
  id: WorkspaceProgressStage['id']
): WorkspaceProgressStage | undefined {
  return stages.find((candidate) => candidate.id === id);
}

function applyMetric(
  stageValue: WorkspaceProgressStage,
  done: number | string,
  total: number | string | null
) {
  stageValue.done = normalizeCount(done);
  stageValue.total = total === null ? null : normalizeCount(total);
  stageValue.percent =
    stageValue.total === null ? null : exactPercent(stageValue.done, stageValue.total);
}

function addMetric(
  stageValue: WorkspaceProgressStage,
  labelKey: WorkspaceMessageKey,
  value: number | string,
  total: number | string | null = null
) {
  stageValue.metrics.push({
    labelKey,
    value: normalizeCount(value),
    total: total === null ? null : normalizeCount(total),
    kind: 'count'
  });
}

function predictedCandidateTotal(
  telemetry: ClearraSearchProgressTelemetry
): string | null {
  if (telemetry.geometry_family_count !== null) return telemetry.geometry_family_count;
  return telemetry.producer_complete ? String(telemetry.candidates_emitted) : null;
}

function normalizeCount(value: number | string): string {
  if (typeof value === 'number') return String(Math.max(0, Math.trunc(value)));
  return value;
}

function exactPercent(done: string, total: string): number | null {
  try {
    const denominator = BigInt(total);
    if (denominator <= 0n) return null;
    const numerator = BigInt(done);
    const basisPoints = (numerator * 10_000n) / denominator;
    return Math.min(100, Number(basisPoints) / 100);
  } catch {
    return null;
  }
}

function summarize(stages: WorkspaceProgressStage[]): WorkspaceProgressModel {
  const completedStages = stages.filter((stageValue) => stageValue.status === 'complete').length;
  const partial = stages.reduce((sum, stageValue) => {
    if (stageValue.status === 'complete') return sum + 1;
    if (
      (stageValue.status === 'running' || stageValue.status === 'stopped') &&
      stageValue.percent !== null
    ) {
      return sum + stageValue.percent / 100;
    }
    return sum;
  }, 0);
  return {
    stages,
    completedStages,
    totalStages: stages.length,
    overallPercent: Math.min(100, (partial / Math.max(1, stages.length)) * 100)
  };
}

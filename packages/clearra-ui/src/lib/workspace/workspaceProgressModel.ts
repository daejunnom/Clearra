import type {
  ClearraSearchProgressCountKey,
  ClearraSearchProgressTelemetry
} from '../wasm/wasmCommandClient';
import type { WorkspaceMessageKey } from './workspaceI18n';
import type { WorkspaceRuntimeStatus } from './workspaceRuntime';

export type WorkspaceProgressProfile = 'pc' | 'tiling' | 'setup' | 'build' | 'damage' | 'spin' | 'ren';
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
  | 'spin'
  | 'ren';
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
  value: string | null;
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
  if (profile === 'ren') {
    return [
      COMMON_PREPARE,
      { id: 'forward', labelKey: 'progressStageForwardSearch' },
      { id: 'classify', labelKey: 'progressStageRen' }
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

  if (input.profile === 'damage' || input.profile === 'spin' || input.profile === 'ren') {
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

export function workspaceActiveWorkerCount(
  telemetry: ClearraSearchProgressTelemetry | null,
  status: WorkspaceRuntimeStatus
): number | null {
  if (
    !telemetry ||
    !telemetry.availability.active_workers ||
    !telemetry.exactness.active_workers
  ) {
    return null;
  }
  if (status !== 'running' && status !== 'cancelling') return 0;
  return Number.isSafeInteger(telemetry.active_workers) && telemetry.active_workers >= 0
    ? telemetry.active_workers
    : null;
}

export function workspaceWorkerCapacity(
  telemetry: ClearraSearchProgressTelemetry | null
): number | null {
  if (
    !telemetry?.availability.worker_count ||
    !telemetry.exactness.worker_count
  ) return null;
  return Number.isSafeInteger(telemetry.worker_count) && telemetry.worker_count > 0
    ? telemetry.worker_count
    : null;
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
  // Verification consumes streamed candidates concurrently. Its counters do not
  // prove that the Geometry producer has finished enumerating candidates.
  const producerDone =
    telemetry.producer_complete || isFinalizingPhase(phase) || phase === 'draining';
  const predictedCandidates = predictedCandidateTotal(telemetry);
  const candidateGenerationStarted =
    telemetryPositive(telemetry, 'candidates_emitted', telemetry.candidates_emitted) ||
    (telemetry.availability.geometry_family_count &&
      telemetry.geometry_family_count !== null);
  const verificationStarted =
    verify !== undefined &&
    (telemetryPositive(
      telemetry,
      'candidates_verified',
      telemetry.candidates_verified
    ) ||
      telemetryPositive(telemetry, 'coverage_checks', telemetry.coverage_checks) ||
      producerDone);

  if (geometry) {
    geometry.status =
      producerDone
        ? 'complete'
        : phase === 'searching'
          ? 'running'
          : 'pending';
    if (candidateGenerationStarted) {
      applyMetric(
        geometry,
        telemetryCount(
          telemetry,
          'candidates_emitted',
          telemetry.candidates_emitted
        ),
        predictedCandidates
      );
    } else {
      applyMetric(
        geometry,
        telemetryCount(telemetry, 'geometry_nodes', telemetry.geometry_nodes),
        null
      );
    }
    addMetric(
      geometry,
      'progressMetricNodes',
      telemetryCount(telemetry, 'geometry_nodes', telemetry.geometry_nodes)
    );
    if (candidateGenerationStarted) {
      addMetric(
        geometry,
        'progressMetricCandidates',
        telemetryCount(
          telemetry,
          'candidates_emitted',
          telemetry.candidates_emitted
        ),
        predictedCandidates
      );
    }
    if (telemetry.availability.pass_count && telemetry.pass_count > 1) {
      addMetric(
        geometry,
        'progressMetricPass',
        telemetryCompositeCount(
          telemetry,
          ['pass_index', 'pass_count'],
          Math.min(telemetry.pass_index + 1, telemetry.pass_count)
        ),
        telemetryCount(telemetry, 'pass_count', telemetry.pass_count)
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
      telemetryCount(
        telemetry,
        'candidates_verified',
        telemetry.candidates_verified
      ),
      predictedCandidates
    );
    addMetric(
      verify,
      'progressMetricBuildNodes',
      telemetryCount(telemetry, 'build_nodes', telemetry.build_nodes)
    );
    addMetric(
      verify,
      'progressMetricChecks',
      telemetryCount(telemetry, 'coverage_checks', telemetry.coverage_checks)
    );
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
  const exactPipeline =
    telemetry.availability.pass_index &&
    telemetry.exactness.pass_index &&
    telemetry.availability.pass_count &&
    telemetry.exactness.pass_count &&
    telemetry.pass_count >= 4;
  const pipelineStage = exactPipeline
    ? Math.min(telemetry.pass_index, telemetry.pass_count - 1)
    : null;
  const geometryCompiled = exactPipeline
    ? (pipelineStage ?? 0) >= 1 || producerDone
    : telemetryExactPositive(
          telemetry,
          'candidates_emitted',
          telemetry.candidates_emitted
        ) || producerDone;
  const graphStarted =
    (pipelineStage !== null && pipelineStage >= 1) ||
    telemetryPositive(
      telemetry,
      'producer_build_nodes',
      telemetry.producer_build_nodes
    ) ||
    telemetryPositive(telemetry, 'candidates_emitted', telemetry.candidates_emitted) ||
    producerDone;
  const graphComplete = exactPipeline
    ? (pipelineStage ?? 0) >= 3 || producerDone
    : telemetryExactPositive(
          telemetry,
          'candidates_emitted',
          telemetry.candidates_emitted
        ) || producerDone;
  const tasksStarted =
    (pipelineStage !== null && pipelineStage >= 3) ||
    telemetryPositive(telemetry, 'candidates_emitted', telemetry.candidates_emitted) ||
    telemetryPositive(
      telemetry,
      'candidates_verified',
      telemetry.candidates_verified
    ) ||
    producerDone;

  if (geometry) {
    geometry.status =
      geometryCompiled || graphStarted
        ? 'complete'
        : phase === 'searching'
          ? 'running'
          : 'pending';
    applyMetric(
      geometry,
      telemetryCount(telemetry, 'geometry_nodes', telemetry.geometry_nodes),
      null
    );
  }
  if (graph) {
    graph.status =
      graphComplete
        ? 'complete'
        : graphStarted
          ? 'running'
          : 'pending';
    if (
      graph.status === 'running' &&
      telemetry.availability.layer_total &&
      telemetry.layer_total > 0
    ) {
      applyMetric(
        graph,
        telemetryCount(telemetry, 'layer_done', telemetry.layer_done),
        telemetryCount(telemetry, 'layer_total', telemetry.layer_total)
      );
      addMetric(
        graph,
        'progressMetricLayerWork',
        telemetryCount(telemetry, 'layer_done', telemetry.layer_done),
        telemetryCount(telemetry, 'layer_total', telemetry.layer_total)
      );
    } else {
      applyMetric(
        graph,
        telemetryCount(
          telemetry,
          'producer_build_nodes',
          telemetry.producer_build_nodes
        ),
        null
      );
    }
    addMetric(
      graph,
      'progressMetricBuildNodes',
      telemetryCount(
        telemetry,
        'producer_build_nodes',
        telemetry.producer_build_nodes
      )
    );
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
      telemetryCount(
        telemetry,
        'candidates_verified',
        telemetry.candidates_verified
      ),
      exactTelemetryCount(
        telemetry,
        'geometry_family_count',
        telemetry.geometry_family_count
      )
    );
    addMetric(
      tasks,
      'progressMetricDispatched',
      telemetryCount(
        telemetry,
        'candidates_emitted',
        telemetry.candidates_emitted
      ),
      telemetryCount(
        telemetry,
        'geometry_family_count',
        telemetry.geometry_family_count
      )
    );
    addMetric(
      tasks,
      'progressMetricReduced',
      telemetryCount(
        telemetry,
        'producer_coverage_checks',
        telemetry.producer_coverage_checks
      ),
      telemetryCount(
        telemetry,
        'geometry_family_count',
        telemetry.geometry_family_count
      )
    );
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
    const rawDone = telemetry ? telemetry.pass_index : input.forwardPatternDone ?? 0;
    const rawTotal = telemetry ? telemetry.pass_count : input.forwardPatternTotal ?? 0;
    const done = telemetry
      ? telemetryCount(telemetry, 'pass_index', telemetry.pass_index)
      : rawDone;
    const total = telemetry
      ? telemetryCount(telemetry, 'pass_count', telemetry.pass_count)
      : rawTotal;
    const producerDone =
      telemetry?.producer_complete ||
      telemetry?.phase === 'draining' ||
      telemetry?.phase === 'postprocessing' ||
      telemetry?.phase === 'merging';
    const exactPatternCompletion = telemetry
      ? telemetry.availability.pass_index &&
        telemetry.exactness.pass_index &&
        telemetry.availability.pass_count &&
        telemetry.exactness.pass_count &&
        rawTotal > 0 &&
        rawDone >= rawTotal
      : rawTotal > 0 && rawDone >= rawTotal;
    patterns.status =
      producerDone || exactPatternCompletion
        ? 'complete'
        : searchStarted
          ? 'running'
          : 'pending';
    applyMetric(patterns, done, rawTotal > 0 ? total : null);
    addMetric(patterns, 'progressMetricPatterns', done, rawTotal > 0 ? total : null);
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
      if (telemetry.availability.layer_count && telemetry.layer_count > 0) {
        const currentLayerRaw = Math.min(
          telemetry.layer_index + 1,
          telemetry.layer_count
        );
        applyMetric(
          forward,
          telemetryCompositeCount(
            telemetry,
            ['layer_index', 'layer_count'],
            currentLayerRaw
          ),
          telemetryCount(telemetry, 'layer_count', telemetry.layer_count)
        );
        const exactLayerProgress = [
          'layer_index',
          'layer_count',
          'layer_done',
          'layer_total'
        ].every(
          (key) =>
            telemetry.availability[key as ClearraSearchProgressCountKey] &&
            telemetry.exactness[key as ClearraSearchProgressCountKey]
        );
        if (exactLayerProgress) {
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
        } else {
          forward.percent = null;
        }
        addMetric(
          forward,
          'progressMetricLayerWork',
          telemetryCount(telemetry, 'layer_done', telemetry.layer_done),
          telemetry.layer_total > 0
            ? telemetryCount(telemetry, 'layer_total', telemetry.layer_total)
            : null
        );
      } else {
        applyMetric(
          forward,
          telemetryCount(telemetry, 'geometry_nodes', telemetry.geometry_nodes),
          null
        );
      }
      addMetric(
        forward,
        'progressMetricStates',
        telemetryCount(telemetry, 'geometry_nodes', telemetry.geometry_nodes)
      );
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
      if (input.profile !== 'spin') {
        applyMetric(
          classify,
          telemetryCount(
            telemetry,
            'candidates_verified',
            telemetry.candidates_verified
          ),
          telemetry.producer_complete
            ? exactTelemetryCount(
                telemetry,
                'candidates_emitted',
                telemetry.candidates_emitted
              )
            : null
        );
        addMetric(
          classify,
          'progressMetricDispatched',
          telemetryCount(
            telemetry,
            'candidates_emitted',
            telemetry.candidates_emitted
          )
        );
      } else {
        applyMetric(
          classify,
          telemetryCount(telemetry, 'coverage_checks', telemetry.coverage_checks),
          null
        );
      }
      addMetric(
        classify,
        'progressMetricLegalLocks',
        telemetryCount(telemetry, 'coverage_checks', telemetry.coverage_checks)
      );
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

  if (
    input.progressLabel === 'postprocess' ||
    input.progressLabel === 'pc-minimals-finalize' ||
    input.progressLabel?.startsWith('complete-replay-') === true
  ) {
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
  done: number | string | null,
  total: number | string | null
) {
  stageValue.done = done === null ? null : normalizeCount(done);
  stageValue.total = total === null ? null : normalizeCount(total);
  stageValue.percent =
    stageValue.done === null || stageValue.total === null
      ? null
      : exactPercent(stageValue.done, stageValue.total);
}

function addMetric(
  stageValue: WorkspaceProgressStage,
  labelKey: WorkspaceMessageKey,
  value: number | string | null,
  total: number | string | null = null
) {
  stageValue.metrics.push({
    labelKey,
    value: value === null ? null : normalizeCount(value),
    total: total === null ? null : normalizeCount(total),
    kind: 'count'
  });
}

function predictedCandidateTotal(
  telemetry: ClearraSearchProgressTelemetry
): string | null {
  if (telemetry.geometry_family_count !== null) {
    return exactTelemetryCount(
      telemetry,
      'geometry_family_count',
      telemetry.geometry_family_count
    );
  }
  return telemetry.producer_complete
    ? exactTelemetryCount(
        telemetry,
        'candidates_emitted',
        telemetry.candidates_emitted
      )
    : null;
}

function telemetryCount(
  telemetry: ClearraSearchProgressTelemetry,
  key: ClearraSearchProgressCountKey,
  value: number | string | null
): string | null {
  if (value === null || !telemetry.availability[key]) return null;
  const normalized = normalizeCount(value);
  return telemetry.exactness[key] ? normalized : `≈${normalized}`;
}

function telemetryCompositeCount(
  telemetry: ClearraSearchProgressTelemetry,
  keys: ClearraSearchProgressCountKey[],
  value: number | string
): string | null {
  if (!keys.every((key) => telemetry.availability[key])) return null;
  const normalized = normalizeCount(value);
  return keys.every((key) => telemetry.exactness[key])
    ? normalized
    : `≈${normalized}`;
}

function exactTelemetryCount(
  telemetry: ClearraSearchProgressTelemetry,
  key: ClearraSearchProgressCountKey,
  value: number | string | null
): string | null {
  if (
    value === null ||
    !telemetry.availability[key] ||
    !telemetry.exactness[key]
  ) {
    return null;
  }
  return normalizeCount(value);
}

function telemetryPositive(
  telemetry: ClearraSearchProgressTelemetry,
  key: ClearraSearchProgressCountKey,
  value: number
): boolean {
  return telemetry.availability[key] && Number.isFinite(value) && value > 0;
}

function telemetryExactPositive(
  telemetry: ClearraSearchProgressTelemetry,
  key: ClearraSearchProgressCountKey,
  value: number
): boolean {
  return telemetry.exactness[key] && telemetryPositive(telemetry, key, value);
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

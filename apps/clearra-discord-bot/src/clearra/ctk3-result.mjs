import {
  CTK3_MAX_BUNDLE_PAGES,
  encodeCtk3Bundle,
  encodeCtk3Compact,
  operationCells,
  operationOffsets,
} from "ctk3";

const ARTIFACT_SCHEMA = "clearra.solution-data.v1";
const BOARD_WIDTH = 10;
const FORWARD_HEIGHT = 24;
const SETUP_HEIGHT = 4;
const CTK3_MAX_HEIGHT = 31;
const SEGMENT_PAGE_COUNT = 1024;
const PIECES = new Set(["I", "O", "T", "S", "Z", "J", "L"]);
const COMPACT_KEY_PATTERN =
  /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/;
const EXTENDED_KEY_PATTERN =
  /^ctk2\|height=([0-9]{1,2})\|initial=([0-9a-f]{64})\|placements=(.*)$/;
const COMPACT_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{16})$/;
const EXTENDED_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{64})$/;
const HEX_MASK_PATTERN = /^0x[0-9a-f]+$/;
const BOARD_WORDS_PATTERN = /^0x[0-9a-f]{64}$/;
const INPUT_COUNT_PATTERN = /^(?:0|[1-9][0-9]*)(?:\.[0-9]+)?$/;
const EXACT_INPUT_COUNT_PATTERN = /^(?:0|[1-9][0-9]*)$/;
const FINESSE_UNAVAILABLE_VALUES = new Set(["not-calculated", "unavailable"]);
const FINESSE_POLICIES = new Set(["oracle", "visible-7"]);
const FINESSE_INPUT_ACTIONS = new Set([
  "hold",
  "tap-left",
  "tap-right",
  "das-left",
  "das-right",
  "rotate-clockwise",
  "rotate-counter-clockwise",
  "rotate-180",
  "soft-drop",
  "hard-drop",
]);
const RESULT_COMPLETENESS_FIELDS = new Set([
  "complete",
  "packing_count_complete",
  "solution_keys_complete",
  "count_complete",
  "resource_probability_complete",
  "probability_complete",
  "solution_probability_complete",
  "objective_search_complete",
  "objective_complete",
]);
const SOLUTION_COUNT_FIELDS = Object.freeze([
  "result_count",
  "total_solution_count",
  "unique_solution_count",
  "normalized_unique_solution_count",
  "actual_normalized_unique_solution_count",
]);

const PAGE_FLAGS = Object.freeze({
  lock: true,
  mirror: false,
  colorize: true,
  rise: false,
  quiz: false,
});

const CTK_ROTATIONS = Object.freeze([
  "spawn",
  "right",
  "reverse",
  "left",
]);

const SHAPES = {
  I: [
    [[0, 0], [1, 0], [2, 0], [3, 0]],
    [[0, 0], [0, 1], [0, 2], [0, 3]],
    [[0, 0], [1, 0], [2, 0], [3, 0]],
    [[0, 0], [0, 1], [0, 2], [0, 3]],
  ],
  O: [
    [[0, 0], [1, 0], [0, 1], [1, 1]],
    [[0, 0], [1, 0], [0, 1], [1, 1]],
    [[0, 0], [1, 0], [0, 1], [1, 1]],
    [[0, 0], [1, 0], [0, 1], [1, 1]],
  ],
  T: [
    [[0, 0], [1, 0], [2, 0], [1, 1]],
    [[0, 0], [0, 1], [0, 2], [1, 1]],
    [[0, 1], [1, 1], [2, 1], [1, 0]],
    [[1, 0], [1, 1], [1, 2], [0, 1]],
  ],
  S: [
    [[0, 0], [1, 0], [1, 1], [2, 1]],
    [[1, 0], [0, 1], [1, 1], [0, 2]],
    [[0, 0], [1, 0], [1, 1], [2, 1]],
    [[1, 0], [0, 1], [1, 1], [0, 2]],
  ],
  Z: [
    [[1, 0], [2, 0], [0, 1], [1, 1]],
    [[0, 0], [0, 1], [1, 1], [1, 2]],
    [[1, 0], [2, 0], [0, 1], [1, 1]],
    [[0, 0], [0, 1], [1, 1], [1, 2]],
  ],
  J: [
    [[0, 1], [0, 0], [1, 0], [2, 0]],
    [[0, 0], [0, 1], [0, 2], [1, 2]],
    [[0, 1], [1, 1], [2, 1], [2, 0]],
    [[0, 0], [1, 0], [1, 1], [1, 2]],
  ],
  L: [
    [[2, 1], [0, 0], [1, 0], [2, 0]],
    [[0, 0], [0, 1], [0, 2], [1, 0]],
    [[0, 1], [1, 1], [2, 1], [0, 0]],
    [[0, 2], [1, 0], [1, 1], [1, 2]],
  ],
};

export class Ctk3ResultError extends Error {
  constructor(code, message, path = "artifacts", options) {
    super(`${path}: ${message}`, options);
    this.name = "Ctk3ResultError";
    this.code = code;
    this.path = path;
  }
}

export function buildCtk3Result(jsonOrObject) {
  const located = locateArtifacts(jsonOrObject);
  if (!located) return null;

  const { artifacts, summary } = located;
  validateArtifactSchema(artifacts);
  // A zero count from the authoritative search summary wins over stale or
  // synthetic solution artifacts. In particular, an initial-only key must not
  // turn the input field into a one-page "solution" document.
  if (summaryReportsZeroSolutions(summary)) return null;
  const plan = buildPlan(artifacts);

  const warnings = [];
  const state = { complete: true, warnings };
  collectSummaryCompleteness(summary, state);
  const probabilityComments = solutionProbabilityComments(plan, state);
  const classComments = solutionClassComments(plan);
  const finesseComments = finesseReportComments(plan, state);
  if (finesseComments.searchHasSuccessfulQueue === false) return null;
  const includeFinesseScore = Boolean(
    plan.finesseScore && finesseComments.scoreHasSuccessfulQueue,
  );
  let pageCount = includeFinesseScore
    ? checkedPageCount(plan.pageCount, plan.finesseScore.steps.length)
    : plan.pageCount;
  if (finesseComments.searchWitness?.placements.length > 1) {
    pageCount = checkedPageCount(
      pageCount,
      finesseComments.searchWitness.placements.length - 1,
    );
  }
  checkBundlePageLimit(pageCount);
  if (pageCount === 0) return null;
  const encoder = createSegmentEncoder();

  let searchWitnessWritten = false;
  for (let index = 0; index < plan.solutionKeys.length; index += 1) {
    const solutionKey = plan.solutionKeys[index];
    const path = `artifacts.solution_keys[${index}]`;
    const comment = combinePageComments(
      classComments?.[index],
      probabilityComments?.get(solutionKey),
      finesseComments.solutionComments.get(solutionKey),
    );
    if (!searchWitnessWritten && finesseComments.searchWitness &&
      finesseComments.searchWitness.solutionKey === solutionKey &&
      finesseComments.searchWitness.placements.length > 0) {
      for (const page of finesseSearchPages(
        finesseComments.searchWitness,
        path,
        comment,
      )) {
        encoder.add(page);
      }
      searchWitnessWritten = true;
    } else {
      encoder.add(solutionKeyPage(solutionKey, path, comment));
    }
  }

  for (const entry of plan.setupConditions) {
    collectSetupCompleteness(entry.condition, entry.path, state);
    for (let index = 0; index < entry.candidates.length; index += 1) {
      const candidatePath = `${entry.path}.candidates[${index}]`;
      encoder.add(setupCandidatePage(entry.candidates[index], candidatePath));
    }
  }

  if (plan.forward) {
    const initialMask = parseBoardWordsMask(
      plan.forward.value.initial_board,
      "artifacts.forward.initial_board",
    );
    for (let index = 0; index < plan.forward.outcomes.length; index += 1) {
      encoder.add(
        forwardOutcomePage(
          initialMask,
          plan.forward.outcomes[index],
          `artifacts.forward.outcomes[${index}]`,
        ),
      );
    }
  }

  if (includeFinesseScore) {
    for (const page of finesseScorePages(
      plan.finesseScore,
      finesseComments.scoreComment,
    )) {
      encoder.add(page);
    }
  }

  const source = encoder.finish();
  if (encoder.pageCount !== pageCount) {
    fail(
      "internal-page-count-mismatch",
      "artifacts",
      `expected ${pageCount} pages but encoded ${encoder.pageCount}`,
    );
  }
  return {
    source,
    pageCount: encoder.pageCount,
    complete: state.complete,
    warnings,
  };
}

function summaryReportsZeroSolutions(summary) {
  if (!isRecord(summary)) return false;
  return SOLUTION_COUNT_FIELDS.some((key) =>
    summary[key] === 0 || summary[key] === "0"
  );
}

function locateArtifacts(jsonOrObject) {
  let value = jsonOrObject;
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch (cause) {
      throw new Ctk3ResultError(
        "invalid-json",
        "result is not valid JSON",
        "result",
        { cause },
      );
    }
  }
  if (!isRecord(value)) {
    fail("invalid-result", "result", "expected a JSON object or JSON string");
  }

  if (hasOwn(value, "contract")) {
    const contract = requireRecord(value.contract, "result.contract");
    if (!hasOwn(contract, "artifacts")) return null;
    const artifacts = requireRecord(contract.artifacts, "result.contract.artifacts");
    const summary = hasOwn(value, "summary")
      ? requireRecord(value.summary, "result.summary")
      : null;
    return { artifacts, summary };
  }

  if (hasOwn(value, "artifacts")) {
    return {
      artifacts: requireRecord(value.artifacts, "result.artifacts"),
      summary: hasOwn(value, "summary")
        ? requireRecord(value.summary, "result.summary")
        : null,
    };
  }

  if (
    hasOwn(value, "schema_version") ||
    hasOwn(value, "solution_keys") ||
    hasOwn(value, "solution_classes") ||
    hasOwn(value, "solution_probabilities") ||
    hasOwn(value, "setup_conditions") ||
    hasOwn(value, "forward")
  ) {
    return { artifacts: value, summary: null };
  }
  return null;
}

function validateArtifactSchema(artifacts) {
  if (
    hasOwn(artifacts, "schema_version") &&
    artifacts.schema_version !== ARTIFACT_SCHEMA
  ) {
    fail(
      "unsupported-artifact-schema",
      "artifacts.schema_version",
      `expected ${ARTIFACT_SCHEMA}`,
    );
  }
}

function buildPlan(artifacts) {
  const solutionKeys = optionalArray(
    artifacts,
    "solution_keys",
    "artifacts.solution_keys",
  );
  const hasSolutionClasses = hasOwn(artifacts, "solution_classes");
  const solutionClasses = optionalArray(
    artifacts,
    "solution_classes",
    "artifacts.solution_classes",
  );
  const hasSolutionProbabilities = hasOwn(artifacts, "solution_probabilities");
  const solutionProbabilities = optionalArray(
    artifacts,
    "solution_probabilities",
    "artifacts.solution_probabilities",
  );
  const conditions = optionalArray(
    artifacts,
    "setup_conditions",
    "artifacts.setup_conditions",
  );
  const setupConditions = conditions.map((condition, index) => {
    const path = `artifacts.setup_conditions[${index}]`;
    const value = requireRecord(condition, path);
    return {
      condition: value,
      candidates: requiredArray(value, "candidates", `${path}.candidates`),
      path,
    };
  });

  let forward = null;
  if (hasOwn(artifacts, "forward")) {
    const value = requireRecord(artifacts.forward, "artifacts.forward");
    forward = {
      value,
      outcomes: requiredArray(value, "outcomes", "artifacts.forward.outcomes"),
    };
  }

  const finesseReport = hasOwn(artifacts, "finesse_report")
    ? requireRecord(artifacts.finesse_report, "artifacts.finesse_report")
    : null;
  let finesseScore = null;
  if (hasOwn(artifacts, "finesse_score")) {
    const value = requireRecord(
      artifacts.finesse_score,
      "artifacts.finesse_score",
    );
    finesseScore = {
      value,
      steps: requiredArray(
        value,
        "representative_path",
        "artifacts.finesse_score.representative_path",
      ),
    };
  }

  let pageCount = solutionKeys.length;
  for (const condition of setupConditions) {
    pageCount = checkedPageCount(pageCount, condition.candidates.length);
  }
  if (forward) pageCount = checkedPageCount(pageCount, forward.outcomes.length);
  checkBundlePageLimit(pageCount);

  return {
    solutionKeys,
    solutionClasses,
    hasSolutionClasses,
    solutionProbabilities,
    hasSolutionProbabilities,
    setupConditions,
    forward,
    finesseReport,
    finesseScore,
    pageCount,
  };
}

function finesseReportComments(plan, state) {
  // Only typed user-level costs cross into CTK3 comments. Policy names and
  // execution details deliberately remain outside the document.
  const solutionComments = new Map();
  if (!plan.finesseReport) {
    if (plan.finesseScore) {
      fail(
        "missing-finesse-report",
        "artifacts.finesse_report",
        "finesse_score requires its typed finesse_report",
      );
    }
    return {
      solutionComments,
      scoreComment: undefined,
      scoreHasSuccessfulQueue: false,
      searchWitness: null,
      searchHasSuccessfulQueue: null,
    };
  }

  const report = plan.finesseReport;
  const mode = requiredString(report, "mode", "artifacts.finesse_report.mode");
  if (mode !== "search" && mode !== "score") {
    fail(
      "invalid-finesse-report",
      "artifacts.finesse_report.mode",
      "expected search or score",
    );
  }
  if (plan.finesseScore && mode !== "score") {
    fail(
      "finesse-report-mode-mismatch",
      "artifacts.finesse_report.mode",
      "finesse_score requires a score report",
    );
  }
  if (requiredString(
    report,
    "metric",
    "artifacts.finesse_report.metric",
  ) !== "inputs") {
    fail(
      "invalid-finesse-report",
      "artifacts.finesse_report.metric",
      "only the inputs metric can be written to CTK3",
    );
  }
  requiredString(
    report,
    "pattern_knowledge",
    "artifacts.finesse_report.pattern_knowledge",
  );
  if (!requiredBoolean(
    report,
    "complete",
    "artifacts.finesse_report.complete",
  )) {
    state.complete = false;
    addWarning(state.warnings, "artifacts.finesse_report is incomplete.");
  }

  if (!hasOwn(report, "exact_total_inputs")) {
    fail(
      "invalid-finesse-report",
      "artifacts.finesse_report.exact_total_inputs",
      "field is required",
    );
  }
  const exact = report.exact_total_inputs === null
    ? null
    : inputCount(
      report.exact_total_inputs,
      "artifacts.finesse_report.exact_total_inputs",
      { integer: true },
    );
  const policies = requiredArray(
    report,
    "policy_results",
    "artifacts.finesse_report.policy_results",
  );
  const minimumBySolution = new Map();
  const seenPolicies = new Set();
  const expectedSolutionKeys = new Set(plan.solutionKeys);
  const representativeWitness = validateFinesseRepresentativeWitness(
    report,
    mode,
    exact,
    expectedSolutionKeys,
  );
  let scoreHasSuccessfulQueue = false;
  let searchSuccessCountsObserved = 0;
  let searchHasSuccessfulQueue = false;
  if (plan.finesseScore && policies.length === 0) {
    fail(
      "invalid-finesse-report",
      "artifacts.finesse_report.policy_results",
      "a score report must contain at least one policy",
    );
  }
  for (let policyIndex = 0; policyIndex < policies.length; policyIndex += 1) {
    const policyPath = `artifacts.finesse_report.policy_results[${policyIndex}]`;
    const policy = requireRecord(policies[policyIndex], policyPath);
    const policyName = requiredString(policy, "policy", `${policyPath}.policy`);
    if (!FINESSE_POLICIES.has(policyName)) {
      fail(
        "invalid-finesse-policy",
        `${policyPath}.policy`,
        "expected oracle or visible-7",
      );
    }
    if (seenPolicies.has(policyName)) {
      fail(
        "duplicate-finesse-policy",
        `${policyPath}.policy`,
        "each policy may appear only once",
      );
    }
    seenPolicies.add(policyName);
    if (!requiredBoolean(policy, "complete", `${policyPath}.complete`)) {
      state.complete = false;
      addWarning(state.warnings, `${policyPath} is incomplete.`);
    }
    const averages = requiredArray(
      policy,
      "solution_averages",
      `${policyPath}.solution_averages`,
    );
    const seen = new Set();
    let scoreAverage = undefined;
    for (let averageIndex = 0; averageIndex < averages.length; averageIndex += 1) {
      const averagePath = `${policyPath}.solution_averages[${averageIndex}]`;
      const average = requireRecord(averages[averageIndex], averagePath);
      const solutionKey = requiredString(
        average,
        "solution_key",
        `${averagePath}.solution_key`,
      );
      if (seen.has(solutionKey)) {
        fail(
          "duplicate-finesse-solution-average",
          `${averagePath}.solution_key`,
          "a policy may contain each solution key only once",
        );
      }
      seen.add(solutionKey);
      if (
        mode === "search" &&
        expectedSolutionKeys.size > 0 &&
        !expectedSolutionKeys.has(solutionKey)
      ) {
        fail(
          "finesse-solution-average-key-mismatch",
          `${averagePath}.solution_key`,
          "solution key does not exist in solution_keys",
        );
      }
      if (!requiredBoolean(average, "complete", `${averagePath}.complete`)) {
        state.complete = false;
        addWarning(state.warnings, `${averagePath} is incomplete.`);
      }
      const parsed = inputCount(
        average.average_inputs,
        `${averagePath}.average_inputs`,
        { allowUnavailable: true },
      );
      if (mode === "score" && solutionKey === "given-operation-sequence") {
        scoreAverage = parsed;
      }
      if (!parsed) continue;
      const current = minimumBySolution.get(solutionKey);
      if (!current || parsed.numeric < current.numeric) {
        minimumBySolution.set(solutionKey, parsed);
      }
    }
    if (plan.finesseScore) {
      if (averages.length !== 1 || !seen.has("given-operation-sequence")) {
        fail(
          "finesse-score-average-mismatch",
          `${policyPath}.solution_averages`,
          "a score policy must contain only given-operation-sequence",
        );
      }
      const successfulQueueCount = requiredInteger(
        policy,
        "successful_unique_queue_count",
        `${policyPath}.successful_unique_queue_count`,
        0,
        Number.MAX_SAFE_INTEGER,
      );
      if ((successfulQueueCount === 0) !== (scoreAverage === null)) {
        fail(
          "finesse-score-success-mismatch",
          `${policyPath}.successful_unique_queue_count`,
          "successful queue count and score average disagree",
        );
      }
      if (successfulQueueCount > 0) scoreHasSuccessfulQueue = true;
    }
    if (mode === "search" && hasOwn(policy, "successful_unique_queue_count") &&
      policy.successful_unique_queue_count !== null) {
      const successfulQueueCount = requiredInteger(
        policy,
        "successful_unique_queue_count",
        `${policyPath}.successful_unique_queue_count`,
        0,
        Number.MAX_SAFE_INTEGER,
      );
      searchSuccessCountsObserved += 1;
      if (successfulQueueCount > 0) searchHasSuccessfulQueue = true;
    }
    if (mode === "search") {
      for (const solutionKey of expectedSolutionKeys) {
        if (!seen.has(solutionKey)) {
          fail(
            "finesse-solution-average-key-mismatch",
            `${policyPath}.solution_averages`,
            "averages do not cover every solution key",
          );
        }
      }
    }
  }

  if (plan.finesseScore && exact && !scoreHasSuccessfulQueue) {
    fail(
      "finesse-score-success-mismatch",
      "artifacts.finesse_report.exact_total_inputs",
      "an exact score requires a successful queue",
    );
  }

  for (const key of plan.solutionKeys) {
    const minimum = minimumBySolution.get(key);
    if (minimum) solutionComments.set(key, `F=${minimum.text}`);
  }
  const scoreCost = exact ?? minimumBySolution.get("given-operation-sequence");
  return {
    solutionComments,
    scoreComment: scoreCost ? `F=${scoreCost.text}` : undefined,
    scoreHasSuccessfulQueue,
    searchWitness: mode === "search" ? representativeWitness : null,
    searchHasSuccessfulQueue: mode === "search" &&
      searchSuccessCountsObserved === policies.length && policies.length > 0
      ? searchHasSuccessfulQueue
      : null,
  };
}

function validateFinesseRepresentativeWitness(
  report,
  mode,
  exact,
  expectedSolutionKeys,
) {
  if (!hasOwn(report, "representative_witness") || report.representative_witness === null) {
    return null;
  }
  const path = "artifacts.finesse_report.representative_witness";
  const witness = requireRecord(report.representative_witness, path);
  const policy = requiredString(witness, "policy", `${path}.policy`);
  if (!FINESSE_POLICIES.has(policy)) {
    fail(
      "invalid-finesse-witness",
      `${path}.policy`,
      "expected oracle or visible-7",
    );
  }
  if (!hasOwn(witness, "solution_key")) {
    fail("invalid-finesse-witness", `${path}.solution_key`, "field is required");
  }
  const solutionKey = witness.solution_key;
  if (solutionKey !== null && (typeof solutionKey !== "string" || solutionKey.length === 0)) {
    fail(
      "invalid-finesse-witness",
      `${path}.solution_key`,
      "expected a non-empty string or null",
    );
  }
  if (mode === "search" && solutionKey !== null &&
    !expectedSolutionKeys.has(solutionKey)) {
    fail(
      "invalid-finesse-witness",
      `${path}.solution_key`,
      "solution key does not exist in solution_keys",
    );
  }
  if (mode === "search" && solutionKey === null) {
    fail(
      "invalid-finesse-witness",
      `${path}.solution_key`,
      "a search witness requires its selected solution key",
    );
  }
  if (mode === "score" && solutionKey !== null && solutionKey !== "given-operation-sequence") {
    fail(
      "invalid-finesse-witness",
      `${path}.solution_key`,
      "score witness must use given-operation-sequence",
    );
  }

  const patternIds = requiredArray(witness, "pattern_ids", `${path}.pattern_ids`);
  if (!patternIds.every((id) => Number.isSafeInteger(id) && id >= 0)) {
    fail(
      "invalid-finesse-witness",
      `${path}.pattern_ids`,
      "expected non-negative integer pattern IDs",
    );
  }
  const queue = requiredArray(witness, "queue", `${path}.queue`);
  if (!queue.every((piece) => typeof piece === "string" && PIECES.has(piece))) {
    fail(
      "invalid-finesse-witness",
      `${path}.queue`,
      "expected an array of piece letters",
    );
  }
  const totalInputs = requiredInteger(
    witness,
    "total_inputs",
    `${path}.total_inputs`,
    0,
    Number.MAX_SAFE_INTEGER,
  );
  if (exact && totalInputs !== exact.numeric) {
    fail(
      "invalid-finesse-witness",
      `${path}.total_inputs`,
      "representative cost does not match exact_total_inputs",
    );
  }
  const inputs = requiredArray(witness, "input_sequence", `${path}.input_sequence`);
  if (!inputs.every((input) => FINESSE_INPUT_ACTIONS.has(input))) {
    fail(
      "invalid-finesse-witness",
      `${path}.input_sequence`,
      "contains an unsupported input action",
    );
  }
  if (inputs.length !== totalInputs) {
    fail(
      "invalid-finesse-witness",
      `${path}.input_sequence`,
      "input sequence length does not match total_inputs",
    );
  }
  const placements = requiredArray(witness, "placements", `${path}.placements`);
  if (placements.length > 60) {
    fail(
      "invalid-finesse-witness",
      `${path}.placements`,
      "placement sequence exceeds the supported result limit",
    );
  }
  const normalizedPlacements = placements.map((entry, index) => {
    const placementPath = `${path}.placements[${index}]`;
    const placement = requireRecord(entry, placementPath);
    return {
      piece: requiredPiece(placement, "piece", `${placementPath}.piece`),
      rotation: requiredInteger(
        placement,
        "rotation",
        `${placementPath}.rotation`,
        0,
        3,
      ),
      x: requiredInteger(placement, "x", `${placementPath}.x`, -32, 32),
      y: requiredInteger(placement, "y", `${placementPath}.y`, -32, 32),
    };
  });
  const hardDropCount = inputs.filter((input) => input === "hard-drop").length;
  if (hardDropCount !== normalizedPlacements.length) {
    fail(
      "invalid-finesse-witness",
      `${path}.placements`,
      "each selected placement must have exactly one hard-drop input",
    );
  }
  return {
    solutionKey,
    placements: normalizedPlacements,
  };
}

function inputCount(
  value,
  path,
  { allowUnavailable = false, integer = false } = {},
) {
  if (
    allowUnavailable &&
    typeof value === "string" &&
    FINESSE_UNAVAILABLE_VALUES.has(value)
  ) {
    return null;
  }
  const text = typeof value === "number" ? String(value) : value;
  const pattern = integer ? EXACT_INPUT_COUNT_PATTERN : INPUT_COUNT_PATTERN;
  if (typeof text !== "string" || !pattern.test(text)) {
    fail(
      "invalid-finesse-report",
      path,
      integer
        ? "expected a non-negative integer input count"
        : "expected a non-negative decimal input average",
    );
  }
  const numeric = Number(text);
  if (!Number.isFinite(numeric) || numeric > Number.MAX_SAFE_INTEGER) {
    fail(
      "invalid-finesse-report",
      path,
      "input count is outside the supported range",
    );
  }
  return { numeric, text: normalizeDecimal(text) };
}

function normalizeDecimal(value) {
  if (!value.includes(".")) return value;
  const trimmed = value.replace(/0+$/, "").replace(/\.$/, "");
  return trimmed || "0";
}

function solutionClassComments(plan) {
  if (!plan.hasSolutionClasses) return null;
  if (plan.solutionClasses.length !== plan.solutionKeys.length) {
    fail(
      "solution-class-key-mismatch",
      "artifacts.solution_classes",
      "class entries must match solution_keys one-to-one",
    );
  }
  return plan.solutionClasses.map((value, index) => {
    if (value === "regular") return "Spin: Regular";
    if (value === "mini") return "Spin: Mini";
    fail(
      "invalid-solution-class",
      `artifacts.solution_classes[${index}]`,
      "expected regular or mini",
    );
  });
}

function combinePageComments(...comments) {
  const retained = comments.filter(
    (comment) => typeof comment === "string" && comment.length > 0,
  );
  return retained.length === 0 ? undefined : retained.join(" | ");
}

function checkedPageCount(current, additional) {
  const total = current + additional;
  if (!Number.isSafeInteger(total)) {
    fail("ctk3-page-limit", "artifacts", "page count is not a safe integer");
  }
  return total;
}

function checkBundlePageLimit(pageCount) {
  if (pageCount > CTK3_MAX_BUNDLE_PAGES) {
    fail(
      "ctk3-page-limit",
      "artifacts",
      `page count ${pageCount} exceeds the CTK3 bundle limit ${CTK3_MAX_BUNDLE_PAGES}`,
    );
  }
}

function collectSummaryCompleteness(summary, state) {
  if (!summary) return;
  for (const [key, value] of Object.entries(summary)) {
    if (RESULT_COMPLETENESS_FIELDS.has(key) && value === false) {
      state.complete = false;
      addWarning(state.warnings, `Search summary reports ${key}=false.`);
    }
    if (/(?:^|_)truncated$/.test(key) && value === true) {
      state.complete = false;
      addWarning(state.warnings, `Search summary reports ${key}=true.`);
    }
  }
}

function collectSetupCompleteness(condition, path, state) {
  const complete = requiredBoolean(condition, "complete", `${path}.complete`);
  const truncated = requiredBoolean(
    condition,
    "result_truncated",
    `${path}.result_truncated`,
  );
  if (!complete) {
    state.complete = false;
    addWarning(state.warnings, `${path} is incomplete.`);
  }
  if (truncated) {
    state.complete = false;
    addWarning(state.warnings, `${path} was truncated by the search engine.`);
  }
}

function solutionProbabilityComments(plan, state) {
  if (!plan.hasSolutionProbabilities) return null;
  if (plan.solutionProbabilities.length !== plan.solutionKeys.length) {
    fail(
      "solution-probability-key-mismatch",
      "artifacts.solution_probabilities",
      "probability entries must match solution_keys one-to-one",
    );
  }

  const solutionKeySet = new Set();
  for (let index = 0; index < plan.solutionKeys.length; index += 1) {
    const key = plan.solutionKeys[index];
    if (typeof key !== "string") {
      fail(
        "invalid-solution-key",
        `artifacts.solution_keys[${index}]`,
        "expected a string",
      );
    }
    if (solutionKeySet.has(key)) {
      fail(
        "duplicate-solution-key",
        `artifacts.solution_keys[${index}]`,
        "solution_keys must be unique when probabilities are attached",
      );
    }
    solutionKeySet.add(key);
  }

  const comments = new Map();
  for (let index = 0; index < plan.solutionProbabilities.length; index += 1) {
    const path = `artifacts.solution_probabilities[${index}]`;
    const entry = requireRecord(plan.solutionProbabilities[index], path);
    const key = requiredString(entry, "solution_key", `${path}.solution_key`);
    if (!solutionKeySet.has(key)) {
      fail(
        "solution-probability-key-mismatch",
        `${path}.solution_key`,
        "probability key does not exist in solution_keys",
      );
    }
    if (comments.has(key)) {
      fail(
        "duplicate-solution-probability",
        `${path}.solution_key`,
        "probability key is duplicated",
      );
    }
    const probability = entry.probability;
    const numericProbability =
      typeof probability === "number" || typeof probability === "string"
        ? Number(probability)
        : Number.NaN;
    if (
      !Number.isFinite(numericProbability) ||
      numericProbability < 0 ||
      numericProbability > 1
    ) {
      fail(
        "invalid-solution-probability",
        `${path}.probability`,
        "expected a finite probability in 0..1",
      );
    }
    if (!requiredBoolean(
      entry,
      "probability_complete",
      `${path}.probability_complete`,
    )) {
      state.complete = false;
      addWarning(state.warnings, `${path} reports probability_complete=false.`);
    }
    comments.set(key, `P=${formatPercent(numericProbability)}`);
  }
  if (comments.size !== solutionKeySet.size) {
    fail(
      "solution-probability-key-mismatch",
      "artifacts.solution_probabilities",
      "probability entries do not cover every solution key",
    );
  }
  return comments;
}

function formatPercent(probability) {
  return `${new Intl.NumberFormat("en-US", {
    maximumFractionDigits: 4,
    useGrouping: false,
  }).format(probability * 100)}%`;
}

function createSegmentEncoder() {
  const segments = [];
  let pages = [];
  let pageCount = 0;

  return {
    get pageCount() {
      return pageCount;
    },
    add(page) {
      if (pageCount >= CTK3_MAX_BUNDLE_PAGES) {
        fail(
          "ctk3-page-limit",
          "artifacts",
          `page count exceeds the CTK3 bundle limit ${CTK3_MAX_BUNDLE_PAGES}`,
        );
      }
      pages.push(page);
      pageCount += 1;
      if (pages.length === SEGMENT_PAGE_COUNT) flush();
    },
    finish() {
      flush();
      try {
        return segments.length === 1
          ? segments[0]
          : encodeCtk3Bundle(segments);
      } catch (cause) {
        throw new Ctk3ResultError(
          "ctk3-encode-failed",
          "could not bundle the complete CTK3 result",
          "artifacts",
          { cause },
        );
      }
    },
  };

  function flush() {
    if (pages.length === 0) return;
    try {
      segments.push(encodeCtk3Compact({ width: BOARD_WIDTH, pages }));
    } catch (cause) {
      throw new Ctk3ResultError(
        "ctk3-encode-failed",
        "could not encode the complete CTK3 result",
        "artifacts",
        { cause },
      );
    }
    pages = [];
  }
}

function solutionKeyPage(key, path, comment) {
  const parsed = parseSolutionKey(key, path);
  return coloredPage(parsed.initialMask, parsed.placements, path, comment);
}

function parseSolutionKey(key, path) {
  if (typeof key !== "string") {
    fail("invalid-solution-key", path, "expected a string");
  }
  const compact = COMPACT_KEY_PATTERN.exec(key);
  const extended = compact ? null : EXTENDED_KEY_PATTERN.exec(key);
  if (!compact && !extended) {
    fail("invalid-solution-key", path, "expected a canonical ctk1 or ctk2 key");
  }

  const height = extended ? Number(extended[1]) : 1;
  if (!Number.isInteger(height) || height < 1 || height > 24) {
    fail("invalid-solution-key", path, "declared height is outside 1..24");
  }
  const initialHex = compact ? compact[1] : extended[2];
  const encodedPlacements = compact ? compact[2] : extended[3];
  const bitLimit = compact ? 64 : height * BOARD_WIDTH;
  const placementLimit = compact ? 16 : 60;
  const placementPattern = compact
    ? COMPACT_PLACEMENT_PATTERN
    : EXTENDED_PLACEMENT_PATTERN;
  const initialMask = BigInt(`0x${initialHex}`);
  if (initialMask >> BigInt(bitLimit)) {
    fail("invalid-solution-key", path, "initial mask exceeds the declared board");
  }

  const encoded = encodedPlacements ? encodedPlacements.split(",") : [];
  if (encoded.length > placementLimit) {
    fail("invalid-solution-key", path, "placement count exceeds the key limit");
  }
  const placements = [];
  let occupied = initialMask;
  for (let index = 0; index < encoded.length; index += 1) {
    const match = placementPattern.exec(encoded[index]);
    if (!match) {
      fail(
        "invalid-solution-key",
        `${path}.placements[${index}]`,
        "placement encoding is not canonical",
      );
    }
    const mask = BigInt(`0x${match[2]}`);
    if (
      mask === 0n ||
      mask >> BigInt(bitLimit) ||
      popcount(mask) !== 4 ||
      (occupied & mask) !== 0n
    ) {
      fail(
        "invalid-solution-key",
        `${path}.placements[${index}]`,
        "placement mask is out of bounds, overlapping, or not four cells",
      );
    }
    occupied |= mask;
    placements.push({ piece: match[1], mask });
  }
  return { height, initialMask, placements };
}

function finesseSearchPages(witness, solutionPath, comment) {
  const witnessPath = "artifacts.finesse_report.representative_witness";
  const parsed = parseSolutionKey(witness.solutionKey, solutionPath);
  const canonicalPieces = parsed.placements
    .map((placement) => placement.piece)
    .sort()
    .join("");
  const witnessPieces = witness.placements
    .map((placement) => placement.piece)
    .sort()
    .join("");
  if (canonicalPieces !== witnessPieces) {
    fail(
      "invalid-finesse-search",
      `${witnessPath}.placements`,
      "selected placements do not match the selected solution pieces",
    );
  }

  const canonicalOccupied = parsed.placements.reduce(
    (mask, placement) => mask | placement.mask,
    parsed.initialMask,
  );
  let maximumCellY = highestOccupiedRow(canonicalOccupied);
  for (let index = 0; index < witness.placements.length; index += 1) {
    const placement = witness.placements[index];
    for (const [dx, dy] of SHAPES[placement.piece][placement.rotation]) {
      const cellX = placement.x + dx;
      const cellY = placement.y + dy;
      if (cellX < 0 || cellX >= BOARD_WIDTH || cellY < 0) {
        fail(
          "invalid-finesse-search",
          `${witnessPath}.placements[${index}]`,
          "placement is outside the field",
        );
      }
      maximumCellY = Math.max(maximumCellY, cellY);
    }
  }
  const height = Math.max(parsed.height, maximumCellY + 1, 1);
  if (height > CTK3_MAX_HEIGHT) {
    fail(
      "ctk3-height-limit",
      `${witnessPath}.placements`,
      `representative path requires ${height} rows; CTK3 supports at most ${CTK3_MAX_HEIGHT}`,
    );
  }
  if (parsed.initialMask >> BigInt(height * BOARD_WIDTH)) {
    fail(
      "invalid-finesse-search",
      solutionPath,
      "initial field exceeds the representative path height",
    );
  }

  const rows = emptyRows(height);
  forEachSetBit(parsed.initialMask, (bit) => {
    rows[Math.floor(bit / BOARD_WIDTH)][bit % BOARD_WIDTH] = "G";
  });
  if (fullRowIndexes(rows).length !== 0) {
    fail(
      "invalid-finesse-search",
      solutionPath,
      "selected solution initial field must already have its complete rows cleared",
    );
  }
  clearCompletedRowsInPlace(rows);

  const pages = [];
  for (let index = 0; index < witness.placements.length; index += 1) {
    const placement = witness.placements[index];
    const stepPath = `${witnessPath}.placements[${index}]`;
    const targetCells = SHAPES[placement.piece][placement.rotation].map(
      ([dx, dy]) => ({ x: placement.x + dx, y: placement.y + dy }),
    );
    for (const cell of targetCells) {
      if (cell.x < 0 || cell.x >= BOARD_WIDTH || cell.y < 0 || cell.y >= height) {
        fail("invalid-finesse-search", stepPath, "placement is outside the field");
      }
      if (rows[cell.y][cell.x] !== null) {
        fail(
          "invalid-finesse-search",
          stepPath,
          "placement overlaps an occupied cell",
        );
      }
    }
    pages.push({
      height,
      cells: rows.flat(),
      ...(index === 0 && comment ? { comment } : {}),
      operation: ctkOperationForCells(
        placement.piece,
        placement.rotation,
        targetCells,
        stepPath,
        "invalid-finesse-search",
      ),
      flags: PAGE_FLAGS,
    });
    for (const cell of targetCells) rows[cell.y][cell.x] = placement.piece;
    clearCompletedRowsInPlace(rows);
  }

  const expectedRows = emptyRows(height);
  forEachSetBit(parsed.initialMask, (bit) => {
    expectedRows[Math.floor(bit / BOARD_WIDTH)][bit % BOARD_WIDTH] = "G";
  });
  for (const placement of parsed.placements) {
    forEachSetBit(placement.mask, (bit) => {
      expectedRows[Math.floor(bit / BOARD_WIDTH)][bit % BOARD_WIDTH] = placement.piece;
    });
  }
  clearCompletedRowsInPlace(expectedRows);
  if (!sameColoredRows(rows, expectedRows)) {
    fail(
      "invalid-finesse-search-final-field",
      `${witnessPath}.placements`,
      "replayed placements do not match the selected solution field",
    );
  }
  return pages;
}

function setupCandidatePage(candidate, path) {
  const value = requireRecord(candidate, path);
  const finalMask = parseHexMask(
    requiredString(value, "board_mask", `${path}.board_mask`),
    SETUP_HEIGHT * BOARD_WIDTH,
    `${path}.board_mask`,
  );
  const steps = requiredArray(
    value,
    "representative_path",
    `${path}.representative_path`,
  );
  const logicalRows = emptyRows(SETUP_HEIGHT);
  const displayRows = emptyRows(SETUP_HEIGHT);
  const logicalToDisplay = Array.from(
    { length: SETUP_HEIGHT },
    (_, row) => row,
  );
  const placements = [];

  for (let index = 0; index < steps.length; index += 1) {
    const stepPath = `${path}.representative_path[${index}]`;
    const step = requireRecord(steps[index], stepPath);
    const piece = requiredPiece(step, "piece", `${stepPath}.piece`);
    const rotation = requiredInteger(
      step,
      "rotation",
      `${stepPath}.rotation`,
      0,
      3,
    );
    const x = requiredInteger(step, "x", `${stepPath}.x`, -32, 32);
    const y = requiredInteger(step, "y", `${stepPath}.y`, -32, 32);
    const clearedLines = requiredInteger(
      step,
      "cleared_lines",
      `${stepPath}.cleared_lines`,
      0,
      4,
    );
    let placementMask = 0n;
    for (const [dx, dy] of SHAPES[piece][rotation]) {
      const cellX = x + dx;
      const cellY = y + dy;
      if (
        cellX < 0 ||
        cellX >= BOARD_WIDTH ||
        cellY < 0 ||
        cellY >= logicalRows.length
      ) {
        fail("invalid-setup", stepPath, "placement is outside the setup board");
      }
      const displayY = logicalToDisplay[cellY];
      if (
        logicalRows[cellY][cellX] !== null ||
        displayRows[displayY][cellX] !== null
      ) {
        fail("invalid-setup", stepPath, "placement overlaps an occupied cell");
      }
      logicalRows[cellY][cellX] = piece;
      displayRows[displayY][cellX] = piece;
      placementMask |= 1n << BigInt(displayY * BOARD_WIDTH + cellX);
    }
    placements.push({ piece, mask: placementMask });

    const fullRows = fullRowIndexes(logicalRows);
    if (fullRows.length !== clearedLines) {
      fail(
        "invalid-setup",
        `${stepPath}.cleared_lines`,
        `expected ${fullRows.length} cleared lines`,
      );
    }
    clearRows(logicalRows, logicalToDisplay, displayRows, fullRows);
  }

  if (rowsMask(logicalRows) !== finalMask) {
    fail(
      "invalid-setup-final-mask",
      `${path}.board_mask`,
      "replayed representative_path does not match board_mask",
    );
  }
  return coloredPage(0n, placements, path);
}

function forwardOutcomePage(initialMask, outcome, path) {
  const value = requireRecord(outcome, path);
  const finalMask = parseBoardWordsMask(
    requiredString(value, "final_board", `${path}.final_board`),
    `${path}.final_board`,
  );
  const steps = requiredArray(value, "path", `${path}.path`);
  const logicalRows = emptyRows(FORWARD_HEIGHT);
  const displayRows = emptyRows(FORWARD_HEIGHT);
  const logicalToDisplay = Array.from(
    { length: FORWARD_HEIGHT },
    (_, row) => row,
  );
  const placements = [];
  writeInitialRows(logicalRows, displayRows, initialMask, path);

  for (let index = 0; index < steps.length; index += 1) {
    const stepPath = `${path}.path[${index}]`;
    const step = requireRecord(steps[index], stepPath);
    const piece = requiredPiece(step, "piece", `${stepPath}.piece`);
    const placement = parseBoardWordsMask(
      requiredString(step, "placement_mask", `${stepPath}.placement_mask`),
      `${stepPath}.placement_mask`,
    );
    if (popcount(placement) !== 4) {
      fail(
        "invalid-forward",
        `${stepPath}.placement_mask`,
        "placement mask must contain exactly four cells",
      );
    }
    const displayPlacement = writeForwardPlacement(
      logicalRows,
      displayRows,
      logicalToDisplay,
      placement,
      piece,
      stepPath,
    );
    placements.push({ piece, mask: displayPlacement });

    const fullRows = fullRowIndexes(logicalRows);
    const actualClearedMask = fullRows.reduce(
      (mask, row) => mask | (1n << BigInt(row)),
      0n,
    );
    const clearedRowMask = requiredInteger(
      step,
      "cleared_row_mask",
      `${stepPath}.cleared_row_mask`,
      0,
      2 ** FORWARD_HEIGHT - 1,
    );
    if (actualClearedMask !== BigInt(clearedRowMask)) {
      fail(
        "invalid-forward",
        `${stepPath}.cleared_row_mask`,
        `expected 0x${actualClearedMask.toString(16)}`,
      );
    }
    if (hasOwn(step, "cleared_lines")) {
      const clearedLines = requiredInteger(
        step,
        "cleared_lines",
        `${stepPath}.cleared_lines`,
        0,
        4,
      );
      if (clearedLines !== fullRows.length) {
        fail(
          "invalid-forward",
          `${stepPath}.cleared_lines`,
          `expected ${fullRows.length}`,
        );
      }
    }
    clearRows(logicalRows, logicalToDisplay, displayRows, fullRows);

    const expectedAfter = parseBoardWordsMask(
      requiredString(step, "board_after", `${stepPath}.board_after`),
      `${stepPath}.board_after`,
    );
    if (rowsMask(logicalRows) !== expectedAfter) {
      fail(
        "invalid-forward-board-after",
        `${stepPath}.board_after`,
        "replayed board does not match board_after",
      );
    }
  }

  if (rowsMask(logicalRows) !== finalMask) {
    fail(
      "invalid-forward-final-mask",
      `${path}.final_board`,
      "replayed path does not match final_board",
    );
  }
  return coloredPage(initialMask, placements, path, forwardOutcomeComment(value, path));
}

function forwardOutcomeComment(outcome, path) {
  const parts = [];
  if (hasOwn(outcome, "id")) {
    parts.push(`#${requiredInteger(outcome, "id", `${path}.id`, 0, Number.MAX_SAFE_INTEGER)}`);
  }
  if (hasOwn(outcome, "source_queue")) {
    const queue = requiredString(outcome, "source_queue", `${path}.source_queue`);
    if (!/^[IOTSZJL]*$/.test(queue)) {
      fail("invalid-artifacts", `${path}.source_queue`, "queue contains an invalid piece");
    }
    parts.push(`Q=${queue || "-"}`);
  }
  if (hasOwn(outcome, "total_damage")) {
    parts.push(
      `D=${requiredInteger(
        outcome,
        "total_damage",
        `${path}.total_damage`,
        0,
        Number.MAX_SAFE_INTEGER,
      )}`,
    );
  }
  if (hasOwn(outcome, "spin_piece")) {
    const spinPiece = outcome.spin_piece;
    if (spinPiece !== null && !PIECES.has(spinPiece)) {
      fail("invalid-artifacts", `${path}.spin_piece`, "expected a piece or null");
    }
    if (spinPiece !== null) {
      const mini = hasOwn(outcome, "spin_mini")
        ? requiredBoolean(outcome, "spin_mini", `${path}.spin_mini`)
        : false;
      const lines = hasOwn(outcome, "spin_lines")
        ? requiredInteger(outcome, "spin_lines", `${path}.spin_lines`, 0, 4)
        : 0;
      parts.push(`${mini ? "Mini " : ""}${spinPiece}-spin ${lines}L`);
    }
  }
  return parts.length ? parts.join(" | ") : undefined;
}

function finesseScorePages(entry, comment) {
  const value = entry.value;
  const path = "artifacts.finesse_score";
  const height = requiredInteger(value, "height", `${path}.height`, 1, 24);
  const initialMask = parseFinesseBoardMask(
    requiredString(value, "initial_board", `${path}.initial_board`),
    height,
    `${path}.initial_board`,
  );
  const rows = emptyRows(height);
  forEachSetBit(initialMask, (bit) => {
    rows[Math.floor(bit / BOARD_WIDTH)][bit % BOARD_WIDTH] = "G";
  });
  if (fullRowIndexes(rows).length !== 0) {
    fail(
      "invalid-finesse-score",
      `${path}.initial_board`,
      "initial_board must already have its complete rows cleared",
    );
  }

  const pages = [];
  for (let index = 0; index < entry.steps.length; index += 1) {
    const stepPath = `${path}.representative_path[${index}]`;
    const step = requireRecord(entry.steps[index], stepPath);
    const piece = requiredPiece(step, "piece", `${stepPath}.piece`);
    const rotation = requiredInteger(
      step,
      "rotation",
      `${stepPath}.rotation`,
      0,
      3,
    );
    const x = requiredInteger(step, "x", `${stepPath}.x`, -32, 32);
    const y = requiredInteger(step, "y", `${stepPath}.y`, -32, 32);
    const clearedLines = requiredInteger(
      step,
      "cleared_lines",
      `${stepPath}.cleared_lines`,
      0,
      4,
    );
    const targetCells = SHAPES[piece][rotation].map(([dx, dy]) => ({
      x: x + dx,
      y: y + dy,
    }));
    for (const cell of targetCells) {
      if (
        cell.x < 0 ||
        cell.x >= BOARD_WIDTH ||
        cell.y < 0 ||
        cell.y >= height
      ) {
        fail(
          "invalid-finesse-score",
          stepPath,
          "placement is outside the declared field",
        );
      }
      if (rows[cell.y][cell.x] !== null) {
        fail(
          "invalid-finesse-score",
          stepPath,
          "placement overlaps an occupied cell",
        );
      }
    }
    const operation = ctkOperationForCells(
      piece,
      rotation,
      targetCells,
      stepPath,
    );
    pages.push({
      height,
      cells: rows.flat(),
      ...(index === 0 && comment ? { comment } : {}),
      operation,
      flags: PAGE_FLAGS,
    });

    for (const cell of targetCells) rows[cell.y][cell.x] = piece;
    const fullRows = fullRowIndexes(rows);
    if (fullRows.length !== clearedLines) {
      fail(
        "invalid-finesse-score",
        `${stepPath}.cleared_lines`,
        `expected ${fullRows.length} cleared lines`,
      );
    }
    for (let row = fullRows.length - 1; row >= 0; row -= 1) {
      rows.splice(fullRows[row], 1);
    }
    while (rows.length < height) rows.push(Array(BOARD_WIDTH).fill(null));
  }
  return pages;
}

function ctkOperationForCells(
  piece,
  declaredRotation,
  targetCells,
  path,
  errorCode = "invalid-finesse-score",
) {
  const target = new Set(targetCells.map(({ x, y }) => `${x},${y}`));
  const rotation = canonicalCtkRotation(piece, declaredRotation);
  const offsets = operationOffsets(piece, rotation);
  for (const targetCell of targetCells) {
    for (const [offsetX, offsetY] of offsets) {
      const candidate = {
        piece,
        rotation,
        x: targetCell.x - offsetX,
        y: targetCell.y - offsetY,
      };
      const cells = operationCells(candidate);
      if (
        cells.length === target.size &&
        cells.every(({ x, y }) => target.has(`${x},${y}`))
      ) {
        return candidate;
      }
    }
  }
  fail(
    errorCode,
    path,
    "placement cannot be represented as a CTK3 page operation",
  );
}

function canonicalCtkRotation(piece, declaredRotation) {
  const rotation = CTK_ROTATIONS[declaredRotation];
  if (piece === "O") return "spawn";
  if (piece === "I" || piece === "S" || piece === "Z") {
    return declaredRotation % 2 === 0 ? "spawn" : "right";
  }
  return rotation;
}

function writeInitialRows(logicalRows, displayRows, mask, path) {
  const bitLimit = logicalRows.length * BOARD_WIDTH;
  if (mask >> BigInt(bitLimit)) {
    fail("invalid-forward", path, "initial board exceeds 24 rows");
  }
  forEachSetBit(mask, (bit) => {
    const y = Math.floor(bit / BOARD_WIDTH);
    const x = bit % BOARD_WIDTH;
    logicalRows[y][x] = "G";
    displayRows[y][x] = "G";
  });
}

function writeForwardPlacement(
  logicalRows,
  displayRows,
  logicalToDisplay,
  mask,
  piece,
  path,
) {
  if (mask >> BigInt(logicalRows.length * BOARD_WIDTH)) {
    fail("invalid-forward", path, "placement exceeds 24 rows");
  }
  let displayMask = 0n;
  forEachSetBit(mask, (bit) => {
    const y = Math.floor(bit / BOARD_WIDTH);
    const x = bit % BOARD_WIDTH;
    const displayY = logicalToDisplay[y];
    if (logicalRows[y][x] !== null || displayRows[displayY][x] !== null) {
      fail("invalid-forward", path, "placement overlaps an occupied cell");
    }
    logicalRows[y][x] = piece;
    displayRows[displayY][x] = piece;
    displayMask |= 1n << BigInt(displayY * BOARD_WIDTH + x);
  });
  return displayMask;
}

function clearRows(logicalRows, logicalToDisplay, displayRows, fullRows) {
  for (let index = fullRows.length - 1; index >= 0; index -= 1) {
    logicalRows.splice(fullRows[index], 1);
    logicalToDisplay.splice(fullRows[index], 1);
  }
  for (let index = 0; index < fullRows.length; index += 1) {
    logicalRows.push(Array(BOARD_WIDTH).fill(null));
    logicalToDisplay.push(displayRows.length);
    displayRows.push(Array(BOARD_WIDTH).fill(null));
  }
}

function coloredPage(initialMask, placements, path, comment) {
  let occupied = initialMask;
  for (let index = 0; index < placements.length; index += 1) {
    const placement = placements[index];
    if (
      placement.mask <= 0n ||
      popcount(placement.mask) !== 4 ||
      (occupied & placement.mask) !== 0n
    ) {
      fail(
        "invalid-colored-page",
        `${path}.placements[${index}]`,
        "placement is empty, overlapping, or not four cells",
      );
    }
    occupied |= placement.mask;
  }
  const height = Math.max(1, highestOccupiedRow(occupied) + 1);
  if (height > CTK3_MAX_HEIGHT) {
    fail(
      "ctk3-height-limit",
      path,
      `colored placement history requires ${height} rows; CTK3 supports at most ${CTK3_MAX_HEIGHT}`,
    );
  }
  const cells = Array(height * BOARD_WIDTH).fill(null);
  paintMask(cells, initialMask, "G");
  for (const placement of placements) {
    paintMask(cells, placement.mask, placement.piece);
  }
  return {
    height,
    cells,
    ...(comment ? { comment } : {}),
    flags: PAGE_FLAGS,
  };
}

function parseHexMask(value, bitLimit, path) {
  if (typeof value !== "string" || !HEX_MASK_PATTERN.test(value)) {
    fail("invalid-mask", path, "expected a lowercase 0x-prefixed hexadecimal mask");
  }
  const mask = BigInt(value);
  if (mask >> BigInt(bitLimit)) {
    fail("invalid-mask", path, `mask exceeds ${bitLimit} bits`);
  }
  return mask;
}

function parseBoardWordsMask(value, path) {
  if (typeof value !== "string" || !BOARD_WORDS_PATTERN.test(value)) {
    fail("invalid-mask", path, "expected 0x followed by exactly 64 lowercase hex digits");
  }
  return parseHexMask(value, FORWARD_HEIGHT * BOARD_WIDTH, path);
}

function parseFinesseBoardMask(value, height, path) {
  if (typeof value !== "string" || !BOARD_WORDS_PATTERN.test(value)) {
    fail(
      "invalid-mask",
      path,
      "expected 0x followed by exactly 64 lowercase hex digits",
    );
  }
  return parseHexMask(value, height * BOARD_WIDTH, path);
}

function emptyRows(height) {
  return Array.from({ length: height }, () => Array(BOARD_WIDTH).fill(null));
}

function fullRowIndexes(rows) {
  const result = [];
  for (let index = 0; index < rows.length; index += 1) {
    if (rows[index].every((cell) => cell !== null)) result.push(index);
  }
  return result;
}

function clearCompletedRowsInPlace(rows) {
  const declaredHeight = rows.length;
  const fullRows = fullRowIndexes(rows);
  for (let row = fullRows.length - 1; row >= 0; row -= 1) {
    rows.splice(fullRows[row], 1);
  }
  while (rows.length < declaredHeight) rows.push(Array(BOARD_WIDTH).fill(null));
}

function rowsMask(rows) {
  let mask = 0n;
  for (let y = 0; y < rows.length; y += 1) {
    for (let x = 0; x < BOARD_WIDTH; x += 1) {
      if (rows[y][x] !== null) {
        mask |= 1n << BigInt(y * BOARD_WIDTH + x);
      }
    }
  }
  return mask;
}

function sameColoredRows(left, right) {
  return left.length === right.length && left.every((row, y) =>
    row.length === right[y]?.length &&
    row.every((cell, x) => cell === right[y][x])
  );
}

function paintMask(cells, mask, color) {
  forEachSetBit(mask, (bit) => {
    cells[bit] = color;
  });
}

function forEachSetBit(source, visit) {
  let mask = source;
  while (mask !== 0n) {
    const bit = trailingZeroes(mask);
    visit(bit);
    mask &= mask - 1n;
  }
}

function highestOccupiedRow(mask) {
  if (mask === 0n) return 0;
  let highestBit = -1;
  while (mask !== 0n) {
    mask >>= 1n;
    highestBit += 1;
  }
  return Math.floor(highestBit / BOARD_WIDTH);
}

function trailingZeroes(value) {
  let count = 0;
  while ((value & 1n) === 0n) {
    value >>= 1n;
    count += 1;
  }
  return count;
}

function popcount(value) {
  let count = 0;
  while (value !== 0n) {
    value &= value - 1n;
    count += 1;
  }
  return count;
}

function optionalArray(owner, key, path) {
  return hasOwn(owner, key) ? requireArray(owner[key], path) : [];
}

function requiredArray(owner, key, path) {
  if (!hasOwn(owner, key)) fail("invalid-artifacts", path, "field is required");
  return requireArray(owner[key], path);
}

function requireArray(value, path) {
  if (!Array.isArray(value)) fail("invalid-artifacts", path, "expected an array");
  return value;
}

function requireRecord(value, path) {
  if (!isRecord(value)) fail("invalid-artifacts", path, "expected an object");
  return value;
}

function requiredString(owner, key, path) {
  if (!hasOwn(owner, key) || typeof owner[key] !== "string") {
    fail("invalid-artifacts", path, "expected a string");
  }
  return owner[key];
}

function requiredBoolean(owner, key, path) {
  if (!hasOwn(owner, key) || typeof owner[key] !== "boolean") {
    fail("invalid-artifacts", path, "expected a boolean");
  }
  return owner[key];
}

function requiredInteger(owner, key, path, minimum, maximum) {
  const value = owner[key];
  if (
    !hasOwn(owner, key) ||
    !Number.isInteger(value) ||
    value < minimum ||
    value > maximum
  ) {
    fail(
      "invalid-artifacts",
      path,
      `expected an integer in ${minimum}..${maximum}`,
    );
  }
  return value;
}

function requiredPiece(owner, key, path) {
  const value = requiredString(owner, key, path);
  if (!PIECES.has(value)) {
    fail("invalid-artifacts", path, "expected one of I, O, T, S, Z, J, L");
  }
  return value;
}

function addWarning(warnings, message) {
  if (!warnings.includes(message)) warnings.push(message);
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hasOwn(owner, key) {
  return Object.prototype.hasOwnProperty.call(owner, key);
}

function fail(code, path, message) {
  throw new Ctk3ResultError(code, message, path);
}

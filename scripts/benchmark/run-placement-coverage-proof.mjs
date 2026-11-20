// A local-only exact mapping qualification, not ABBA timing or release authority.
// Each capture and comparison acquires a native benchmark-search lease. The
// JS orchestrator owns no solver directly and never nests a Linux supervisor.
import { spawn, spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
  createReadStream, existsSync, lstatSync, mkdirSync, readFileSync,
  realpathSync, statfsSync, unlinkSync, writeFileSync,
} from "node:fs";
import { availableParallelism, cpus, hostname, release as osRelease, totalmem, version } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { assertManagedBuildTransaction } from "../tools/clearra-build-policy.mjs";
import {
  DEFAULT_PLACEMENT_EXPERIMENT, placementExperiment, unprofiledEmptyMinimumRoute,
} from "./placement-experiment-contract.mjs";

const SHA = /^[a-f0-9]{64}$/u;
const COMMIT = /^[a-f0-9]{40}$/u;
const RUN_ID = /^\d{16,21}-[1-9]\d*$/u;
const MAX_JSON_BYTES = 4 * 1024 * 1024;
const INDEPENDENT_UNION_STORAGE = "CLEARRA_STANDARD_BAG_UNION_MEMO";
const OPTIMIZATION_AXES = new Set([INDEPENDENT_UNION_STORAGE,
  "CLEARRA_STRICT_CATALOG_GUARDS", "CLEARRA_STANDARD_BAG_ABSORB",
  "CLEARRA_STRICT_LANGUAGE_FRONTIER", "CLEARRA_STRICT_SUBSUMPTION",
  "CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES",
  "CLEARRA_STRICT_STAGE_PROFILE", "CLEARRA_STRICT_STAGE_LEGACY_UPPER"]);
const FIXED_MEMO_PROCESS = Object.freeze({
  CLEARRA_STANDARD_BAG_MEMO: "reference",
  CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: "adaptive",
});
const GIB = 1024 ** 3;
const P7P4_FREE_RESERVE = 33 * GIB;
const STRICT_SPOOL_WRITE_LIMIT = 5 * GIB;
const STRICT_OUTPUT_LIMIT = 3 * GIB;
const STRICT_HEADROOM = 8 * GIB;
const STRICT_CAPTURE_LIMIT = STRICT_SPOOL_WRITE_LIMIT + STRICT_OUTPUT_LIMIT;
const STRICT_DISK_CODE = "CLEARRA_STRICT_COVERAGE_DISK_LIMIT";
const WSL_VOLUME = /^\/mnt\/[a-z]$/u;
const DEFAULT_EXPERIMENT = placementExperiment(DEFAULT_PLACEMENT_EXPERIMENT);
const MINIMUM_EXPERIMENT = placementExperiment("minimum-domain-cutoff");
// These entries belong only to the exact-coverage proof driver. The legacy
// adoption launcher must reject them before scheduling any timing work.
const OPERATION_MASKS_EXPERIMENT = Object.freeze({
  name: "operation-masks",
  axis: "operation_mask_mode",
  environmentKey: "CLEARRA_OPERATION_MASKS",
  dependencyMode: "0",
  structuralMode: "0",
});
const STRICT_PREFIX_EXPERIMENT = Object.freeze({
  name: "strict-prefix",
  axis: "strict_prefix_mode",
  environmentKey: "CLEARRA_STRICT_PREFIX",
  dependencyMode: "0",
  structuralMode: "0",
});
const STRICT_PREFIX_UNION_EXPERIMENT = Object.freeze({
  ...STRICT_PREFIX_EXPERIMENT,
  name: "strict-prefix-union-memo",
  axis: "standard_bag_union_memo_storage",
});
const AXIS = DEFAULT_EXPERIMENT.axis;
const ARTIFACT = "solution-coverage.clrcov";
const COVERAGE_ROOT = ["_local", "artifacts", "benchmarks", "placement-graph-coverage"];
const MODES = Object.freeze({
  CLEARRA_PLACEMENT_DEPENDENCY: "1",
  CLEARRA_STRUCTURAL_ORDER: "1",
  CLEARRA_STATIC_INFLUENCE: "0",
  CLEARRA_TRANSLATION_INTERVAL: "0",
  CLEARRA_INCREMENTAL_DOMAIN: "0",
  CLEARRA_MOVEMENT_CLEAR_CERTIFICATES: "0",
  CLEARRA_MINIMUM_DOMAIN_CUTOFF: "0",
  CLEARRA_MINIMUM_DOMAIN_WORK: "0",
  CLEARRA_FIRST_KICK_INTERVAL: "0",
  CLEARRA_CONDITIONAL_ORDER: "0",
  CLEARRA_TERMINAL_IDENTITY_MOVE: "0",
  CLEARRA_ORDERED_KICK_INTERVAL: "0",
  CLEARRA_ORDERED_KICK_GUARD_REUSE: "0",
  CLEARRA_GEOMETRIC_EDGE_REUSE: "0",
  CLEARRA_ILC_LAZY_FRAME: "0",
  CLEARRA_NO_ENTRY_COMPONENTS: "0",
  CLEARRA_SAME_TILE_PARENT_REUSE: "0",
});
const NEUTRAL_AXES = [
  "static_influence_mode", "translation_interval_mode", "incremental_domain_mode",
  "first_kick_interval_mode", "conditional_order_mode", "terminal_identity_move_mode",
];
// Some legacy movement-clear reports were generated before these axes
// existed. Only that legacy lane accepts their corresponding absent fields.
const LEGACY_OPTIONAL_AXES = [
  "minimum_domain_cutoff_mode", "minimum_domain_work_mode",
  "ordered_kick_guard_reuse_mode", "geometric_edge_reuse_mode",
];
const LEGACY_NULLABLE_AXES = [
  "ordered_kick_interval_mode", "ilc_lazy_frame_mode", "no_entry_components_mode", "same_tile_parent_reuse_mode",
];
const KNOWN_MODE_KEYS = new Set([
  "output_mode", "structural_mode", AXIS, OPERATION_MASKS_EXPERIMENT.axis, STRICT_PREFIX_EXPERIMENT.axis,
  ...NEUTRAL_AXES, ...LEGACY_OPTIONAL_AXES, ...LEGACY_NULLABLE_AXES,
]);
const CASES = Object.freeze({
  small: Object.freeze({
    name: "one-I-preflight", universe: "small-i", solutions: 1, queues: 1,
    board: "0x3f0", height: 1, placed: 1, supplied: 1, supply: "I",
    hold: "disabled", defaultTimeout: 120, maxTimeout: 300, comparisonTimeout: 60,
  }),
  p7: Object.freeze({
    name: "nonempty-4L-P7-srs-plus", universe: "p7", solutions: 246, queues: 5_040,
    board: "0x3c0f03c0f", height: 4, placed: 6, supplied: 7, supply: "P7", rule: "srs-plus",
    hold: "enabled-empty", candidates: 2_260, pairs: 79_288,
    defaultTimeout: 180, maxTimeout: 300, comparisonTimeout: 60,
  }),
  p7p4: Object.freeze({
    name: "empty-4L-P7P4-jstris180", universe: "p7p4", solutions: 456_459, queues: 4_233_600,
    board: "0", height: 4, placed: 10, supplied: 11, supply: "P7P4",
    hold: "enabled-empty", defaultTimeout: 1_800, maxTimeout: 2_400, comparisonTimeout: 900,
  }),
});
const REFERENCE_HASH = "cts1:2fcbc950acdffa31";

function requireThat(value, message) {
  if (!value) throw new Error(message);
}

function coverageExperiment(name) {
  if (name === OPERATION_MASKS_EXPERIMENT.name) return OPERATION_MASKS_EXPERIMENT;
  if (name === STRICT_PREFIX_EXPERIMENT.name) return STRICT_PREFIX_EXPERIMENT;
  if (name === STRICT_PREFIX_UNION_EXPERIMENT.name) return STRICT_PREFIX_UNION_EXPERIMENT;
  return placementExperiment(name);
}

function experimentFor(options) {
  return options.experiment === undefined ? DEFAULT_EXPERIMENT : coverageExperiment(options.experiment?.name ?? null);
}

function isStrictPrefixProof(experiment) {
  return experiment === STRICT_PREFIX_EXPERIMENT || experiment === STRICT_PREFIX_UNION_EXPERIMENT;
}

function unionMemoStorage(experiment, step) {
  if (experiment !== STRICT_PREFIX_UNION_EXPERIMENT || step.name === "comparison") return undefined;
  return step.value === "0" ? "reference" : "compact";
}

export function parseProofArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index], value = argv[index + 1];
    requireThat(["--case", "--id", "--source-sha", "--timeout-seconds", "--build-profile", "--experiment",
      "--mode", "--wsl-host-volume"].includes(key) &&
      typeof value === "string" && !Object.hasOwn(options, key), "Distinct known option/value pairs required");
    options[key] = value;
  }
  const key = options["--case"];
  const id = options["--id"];
  const sourceCommit = options["--source-sha"];
  requireThat(Object.hasOwn(CASES, key) &&
    /^[a-z0-9][a-z0-9-]{0,47}$/u.test(id ?? "") &&
    COMMIT.test(sourceCommit ?? ""),
  "Usage: run-placement-coverage-proof --case small|p7|p7p4 --id UNIQUE-LEAF --source-sha FULL-HEAD [--timeout-seconds N] [--experiment movement-clear|minimum-domain-cutoff|operation-masks|strict-prefix|strict-prefix-union-memo] [--wsl-host-volume /mnt/c]");
  const mode = options["--mode"];
  requireThat(mode === undefined || (mode === STRICT_PREFIX_EXPERIMENT.name && options["--experiment"] === undefined),
    "Use --mode strict-prefix or --experiment, not both");
  const experiment = coverageExperiment(options["--experiment"] ?? mode);
  if (isStrictPrefixProof(experiment)) {
    requireThat(key === "p7" || key === "p7p4", "The strict-prefix proof requires P7 or empty-origin P7P4");
  } else {
    requireThat(key !== "p7", "The P7 preflight is only for strict-prefix");
    requireThat(experiment === DEFAULT_EXPERIMENT || key === "p7p4",
      `The ${experiment.name} proof requires the empty-origin P7P4 fixture`);
  }
  const fixture = CASES[key];
  const wslHostVolume = options["--wsl-host-volume"];
  requireThat(wslHostVolume === undefined || (isStrictPrefixProof(experiment) && key === "p7p4" &&
    WSL_VOLUME.test(wslHostVolume)), "Specify the WSL VHD's host volume as /mnt/<drive> only for the full strict proof");
  const buildProfile = options["--build-profile"] ?? "release";
  requireThat(buildProfile === "release" || (key !== "p7p4" && buildProfile === "debug"),
  "Debug binaries can be used only for preflights, not P7P4");
  const raw = options["--timeout-seconds"] ?? String(fixture.defaultTimeout);
  const timeout = Number(raw);
  requireThat(/^[1-9]\d*$/u.test(raw) && Number.isSafeInteger(timeout) && timeout <= fixture.maxTimeout,
    "Invalid finite timeout for the selected fixture");
  return { id, sourceCommit, key, fixture, timeout, buildProfile, experiment, wslHostVolume };
}

export function proofSchedule(options, binary, artifactRoot) {
  const experiment = experimentFor(options);
  const arm = (name, value, suffix) => {
    const slot = options.id + suffix;
    return {
      name, slot, value, timeout: options.timeout,
      args: [...(options.key === "small" ? ["--small"] : options.key === "p7" ? ["--p7"] : []),
        "--coverage-slot", slot],
      artifact: join(artifactRoot, slot, ARTIFACT),
    };
  };
  const reference = arm("reference", "0", "-a");
  const candidate = arm("candidate", "1", "-b");
  return [
    reference, candidate, {
      name: "comparison", value: "0", timeout: options.fixture.comparisonTimeout,
      args: ["--compare-coverage", reference.artifact, candidate.artifact],
    },
  ].map((step) => ({ ...step, experiment, command: [binary, ...step.args] }));
}

function strictDiskLimitObserved(receipt, execution, step) {
  const outcome = receipt.outcome;
  return isStrictPrefixProof(experimentFor(step)) && step.name !== "comparison" &&
    !execution.overflow && execution.signal == null && Number.isInteger(execution.code) && execution.code > 0 &&
    outcome.reason === "nonzero" && outcome.return_code === execution.code &&
    outcome.process_tree_stopped === true && outcome.error_code === null &&
    outcome.supervisor_termination_reason === null && outcome.child_termination?.kind === "exit" &&
    outcome.child_termination.code === execution.code && receipt.stderr_tail?.includes(STRICT_DISK_CODE) &&
    execution.stderr?.includes(STRICT_DISK_CODE);
}

// Require all receipts to bind to this sequence's concrete invocation. A
// successful process with a timeout/cancellation receipt is not complete.
export function classifyProofRuntime(receipt, execution, step) {
  const outcome = receipt?.outcome;
  requireThat(receipt?.schema_id === "clearra.runtime-receipt.v2" && RUN_ID.test(outcome?.run_id ?? "") &&
    outcome.profile === "benchmark-search" && outcome.producer === "benchmark" &&
    outcome.timeout_seconds === step.timeout && outcome.automatic_retry === false &&
    isDeepStrictEqual(outcome.command, step.command), step.name + ": receipt does not bind this invocation");
  const signal = outcome.child_termination?.kind === "unix-signal" ? outcome.child_termination.signal : null;
  if (["timeout", "memory-limit", "memory-pressure", "output-limit", "process-limit", "cancelled"].includes(outcome.reason) ||
      (outcome.reason === "signal" && [2, 9, 15].includes(signal)) ||
      strictDiskLimitObserved(receipt, execution, step)) return "censored";
  if (execution.overflow || execution.code !== 0 || execution.signal != null ||
      outcome.return_code !== 0 || outcome.reason !== "normal" || outcome.process_tree_stopped !== true ||
      outcome.error_code !== null || outcome.supervisor_termination_reason !== null ||
      outcome.child_termination?.kind !== "exit" || outcome.child_termination.code !== 0) return "failed";
  requireThat(typeof outcome.memory_pressure_observed === "boolean" &&
    Number.isSafeInteger(outcome.memory_pressure_events) && outcome.memory_pressure_events >= 0,
  step.name + ": missing supervisor pressure evidence");
  return outcome.memory_pressure_observed || outcome.memory_pressure_events > 0 ? "confounded" : "completed";
}

function summaryEquals(value, expected) {
  return value === expected || value === String(expected);
}

function operationMaskParity(report, step, fixture) {
  const proof = report.operation_masks;
  const count = (key) => {
    const value = proof?.[key];
    requireThat(Number.isSafeInteger(value) && value >= 0, step.name + ": invalid operation-mask evidence " + key);
    return value;
  };
  const workers = count("workers");
  const requested = count("requested_workers");
  const executing = count("executing_workers");
  const graphPasses = count("graph_passes");
  const maskPasses = count("mask_passes");
  const live = count("mask_live_passes");
  const subsets = count("visited_subsets");
  const edges = count("selected_edges");
  const maskBytes = count("mask_bytes");
  const concreteBytes = ["graph_node_bytes", "graph_edge_bytes", "graph_piece_edge_bytes"]
    .reduce((total, key) => total + count(key), 0);
  const projection = count("graph_projection_bytes");
  const passes = graphPasses + maskPasses;
  requireThat(workers === report.cpu.hard_limit && executing >= 2 && executing <= workers &&
    subsets > 0 && edges > 0 && Number.isSafeInteger(passes) &&
    Number.isSafeInteger(concreteBytes + maskBytes) && projection >= concreteBytes + maskBytes &&
    summaryEquals(report.summary.total_build_order_nodes, subsets) &&
    summaryEquals(report.summary.worker_graph_projection_retained_bytes, projection),
  step.name + ": operation-mask worker/work evidence does not match the core result");
  if (step.value === "1") {
    requireThat(requested === workers && graphPasses === 0 && concreteBytes === 0 &&
      live > 0 && maskPasses >= live && maskBytes > 0,
    step.name + ": operation-mask treatment did not use a live bounded mask traversal exclusively");
  } else {
    requireThat(requested === 0 && maskPasses === 0 && live === 0 && maskBytes === 0 &&
      graphPasses > 0 && concreteBytes > 0,
    step.name + ": operation-mask control was not purely concrete");
  }
  const replay = report.operation_mask_representative;
  const ordinal = report.summary.representative_pattern_id;
  requireThat((typeof ordinal === "string" && /^(?:0|[1-9]\d*)$/u.test(ordinal) ||
    Number.isSafeInteger(ordinal) && ordinal >= 0) && Number(ordinal) < fixture.queues &&
    Array.isArray(replay) && replay.length === fixture.placed && replay.every((lock) =>
      Array.isArray(lock) && lock.length === 6 && typeof lock[0] === "string" &&
      /^[IOTSZJL]$/u.test(lock[0]) &&
      Number.isSafeInteger(lock[1]) && lock[1] >= 0 && lock[1] < 4 &&
      Number.isSafeInteger(lock[2]) && Number.isSafeInteger(lock[3]) &&
      typeof lock[4] === "string" && lock[4].length > 0 &&
      Number.isSafeInteger(lock[5]) && lock[5] >= 0 && lock[5] <= 4) &&
    replay.reduce((sum, lock) => sum + lock[5], 0) === fixture.height,
  step.name + ": operation-mask evidence is missing concrete representative replay");
  // Compare the work being represented, not the byte capacity of the two
  // different representations. The latter is neither elapsed time nor RSS.
  return { passes, subsets, edges, ordinal: Number(ordinal), replay };
}

function strictPrefixParity(report, step, fixture, outputMode, strictPrefixMode) {
  const summary = report.summary;
  function count(key) {
    const value = summary[key], parsed = Number(value);
    requireThat(Number.isSafeInteger(parsed) && parsed >= 0 && summaryEquals(value, parsed),
      step.name + ": invalid strict-prefix summary " + key);
    return parsed;
  }
  const candidates = count("packing_candidate_count");
  const workers = count("workers_used");
  const active = count("parallel_active_workers");
  const buildNodes = count("total_build_order_nodes");
  const scalarPoses = count("total_reachability_states");
  requireThat(report.coverage_presentation === "coverage-summary-omit" &&
    report.coverage_capture_sink === (outputMode === "solution-coverage" ? "exact-spool" : "discard") &&
    report.materialized_path_step_count === 0 && report.materialized_solution_coverage_count === 0 &&
    report.materialized_solution_probability_count === 0 &&
    summary.representative_pattern_id === "" && summary.representative_candidate_id === "" &&
    candidates > 0 && (fixture.candidates === undefined || candidates === fixture.candidates),
  step.name + ": strict-prefix proof requires a complete no-replay coverage summary");
  if (strictPrefixMode === "0") {
    requireThat(report.strict_prefix === null && buildNodes > 0 && scalarPoses > 0,
      step.name + ": strict-prefix reference did not use the concrete BuildUp route");
  } else {
    const proof = report.strict_prefix;
    const positive = ["reverse_relations", "unresolved_lanes", "pose_work", "peak_relation_logical_bytes"];
    const nonnegative = ["pruned_lanes", "worklist_pops", "pose_domains", "ordered_kick_terms"];
    requireThat(proof?.sealed === true && proof.candidates === candidates && proof.workers === workers &&
      proof.executing_workers === active && positive.every((key) => Number.isSafeInteger(proof[key]) && proof[key] > 0) &&
      nonnegative.every((key) => Number.isSafeInteger(proof[key]) && proof[key] >= 0) &&
      proof.reverse_relations <= proof.unresolved_lanes && buildNodes === 0 && scalarPoses === 0,
    step.name + ": incomplete strict-prefix worker coverage or concrete fallback recorded");
  }
  // The algorithms deliberately visit different states. Candidate count and
  // topology are common work; sealed custody and no fallback are arm-specific.
  return { candidates, workers };
}

function validStrictSpoolBudget(mapping) {
  const within = (value, limit) => Number.isSafeInteger(value) && value >= 0 && value <= limit;
  const spool = mapping?.spool;
  return mapping?.output_limit_bytes === STRICT_OUTPUT_LIMIT &&
    within(mapping.output_bytes, STRICT_OUTPUT_LIMIT) && mapping.output_bytes > 0 &&
    spool?.spill_write_limit_bytes === STRICT_SPOOL_WRITE_LIMIT &&
    within(spool.spill_bytes_written, STRICT_SPOOL_WRITE_LIMIT) &&
    within(spool.peak_live_spill_bytes, spool.spill_bytes_written);
}

export function validatedProofArm(report, receipt, step, options, binarySha, outputMode = "solution-coverage") {
  const fixture = options.fixture;
  const experiment = experimentFor(options);
  const strictSource = experiment !== DEFAULT_EXPERIMENT;
  const minimumCutoff = experiment === MINIMUM_EXPERIMENT;
  const operationMasks = experiment === OPERATION_MASKS_EXPERIMENT;
  const strictPrefix = isStrictPrefixProof(experiment);
  const unionStorage = unionMemoStorage(experiment, step);
  const strictPrefixMode = unionStorage ? "1" : step.value;
  const selectedRoute = unionStorage
    ? report?.[STRICT_PREFIX_EXPERIMENT.axis] === strictPrefixMode &&
      report?.summary?.[experiment.axis] === unionStorage
    : report?.[experiment.axis] === step.value;
  const neutralFollowOn = (name) => name === experiment.axis || report[name] === "0" ||
    (!strictSource && !Object.hasOwn(report, name));
  requireThat(experimentFor(step) === experiment && (step.value === "0" || step.value === "1"),
    step.name + ": wrong scheduled proof experiment or arm");
  requireThat(report?.schema === "clearra.placement-graph-benchmark.v1" &&
    report.case === fixture.name && report.valid === true &&
    Array.isArray(report.validation_errors) && report.validation_errors.length === 0 &&
    report.binary_sha256 === binarySha && report.supervisor_run_id === receipt.outcome.run_id &&
    report.output_mode === outputMode && selectedRoute &&
    report.mode === experiment.dependencyMode && report.structural_mode === experiment.structuralMode &&
    (experiment.axis === AXIS || report[AXIS] === "0") &&
    NEUTRAL_AXES.every((name) => report[name] === "0") && LEGACY_OPTIONAL_AXES.every(neutralFollowOn) &&
    LEGACY_NULLABLE_AXES.every((name) => report[name] === "0" || (!strictSource && report[name] == null)) &&
    (!strictSource || Object.keys(report).every((key) => !key.endsWith("_mode") || KNOWN_MODE_KEYS.has(key))) &&
    (operationMasks ? outputMode === "solution-coverage" :
      [OPERATION_MASKS_EXPERIMENT.axis, "operation_masks", "operation_mask_representative"]
        .every((name) => !Object.hasOwn(report, name))) &&
    (unionStorage ? outputMode === "solution-coverage" :
      strictPrefix ? ["solution-coverage", "global-only"].includes(outputMode) :
      [STRICT_PREFIX_EXPERIMENT.axis, "strict_prefix", "coverage_presentation", "coverage_capture_sink"]
        .every((name) => !Object.hasOwn(report, name))),
  step.name + ": unexpected source, mode, or coverage output");
  if (strictSource) {
    requireThat((options.key === "p7p4" || (strictPrefix && options.key === "p7")) &&
      report.compiled_source_commit === options.sourceCommit,
    step.name + ": " + experiment.name + " proof requires the exact fixture compile-time source");
  }
  if (minimumCutoff) {
    requireThat(unprofiledEmptyMinimumRoute(report),
      step.name + ": minimum-domain proof requires the active unprofiled empty-origin route");
  }
  const input = report.input;
  requireThat(input?.board === fixture.board && input.height === fixture.height &&
    input.placed_pieces === fixture.placed && input.supplied_pieces === fixture.supplied &&
    input.supply === fixture.supply && input.original_supply_count === fixture.queues &&
    input.hold === fixture.hold && input.rule === (fixture.rule ?? "jstris-180") && input.count === "unique" &&
    input.queue_knowledge === "full-future-oracle", step.name + ": wrong exact fixture");
  const cpu = report.cpu;
  const summary = report.summary;
  const reportedWorkers = Number(summary?.workers_used);
  const activeWorkers = Number(summary?.parallel_active_workers);
  const isStrictPreflight = strictPrefix && options.key === "p7";
  const workerLimitSatisfied = isStrictPreflight
    ? reportedWorkers <= cpu?.hard_limit && activeWorkers <= reportedWorkers
    : reportedWorkers === cpu?.hard_limit && activeWorkers === reportedWorkers;
  requireThat(cpu?.worker_policy === "auto" && cpu.use_all_logical_processors === true &&
    cpu.expected_vcpus === null && Number.isSafeInteger(cpu.hard_limit) && cpu.hard_limit > 0 &&
    (options.key === "small" || (workerLimitSatisfied && Number.isSafeInteger(reportedWorkers) && reportedWorkers >= 2 &&
      Number.isSafeInteger(activeWorkers) && activeWorkers >= 2 &&
      summaryEquals(summary?.workers_used, reportedWorkers) && summaryEquals(summary?.parallel_active_workers, activeWorkers) &&
      summaryEquals(summary?.cpu_parallel_execution, true))), step.name + ": automatic parallel placement contract changed");
  const ids = report.materialized_identities;
  const coverage = report.coverage;
  const mapping = report.solution_coverage;
  const process = report.other_process_configuration;
  requireThat(typeof report.backend_contract === "string" && report.backend_contract.length > 0 &&
    process && typeof process === "object" && !Array.isArray(process) &&
    !Object.keys(process).some((key) => OPTIMIZATION_AXES.has(key.toUpperCase()) &&
      !(unionStorage && key === INDEPENDENT_UNION_STORAGE)) &&
    (!unionStorage || (process[INDEPENDENT_UNION_STORAGE] === unionStorage &&
      Object.entries(FIXED_MEMO_PROCESS).every(([key, value]) => process[key] === value))),
  step.name + ": missing or non-isolated benchmark runtime configuration");
  requireThat(summaryEquals(summary?.count_complete, true) && summaryEquals(summary?.probability_complete, true) &&
    summaryEquals(summary?.resource_truncated, false) &&
    (options.key !== "p7p4" || summary.normalized_solution_set_hash === REFERENCE_HASH) &&
    ids?.count === fixture.solutions && ids.reported_total_count === fixture.solutions &&
    ids.covers_reported_family === true && SHA.test(ids.sha256_ordered_ctk1_v1 ?? "") &&
    coverage?.set_bit_count === fixture.queues && SHA.test(coverage.sha256_le_v1 ?? "") &&
    (!strictSource || coverage.word_count === Math.ceil(fixture.queues / 64)) &&
    (outputMode === "global-only" ? mapping == null : (
      outputMode === "solution-coverage" && mapping?.complete === true && SHA.test(mapping.sha256 ?? "") &&
      mapping.solution_count === fixture.solutions && mapping.total_weight === fixture.queues &&
      mapping.covered_weight === fixture.queues && Number.isSafeInteger(mapping.covered_pairs) &&
      (fixture.pairs === undefined || mapping.covered_pairs === fixture.pairs) &&
      (!strictPrefix || validStrictSpoolBudget(mapping)) &&
      mapping.covered_pairs >= Math.max(fixture.solutions, fixture.queues) &&
      mapping.covered_pairs <= fixture.solutions * fixture.queues
    )),
  step.name + ": incomplete or incorrect full per-solution output");
  return {
    input, cpu, ids: ids.sha256_ordered_ctk1_v1, globalCoverage: coverage.sha256_le_v1,
    // The scheduled union storage is validated above, and is the only
    // permitted process difference in the two capture contracts.
    process: unionStorage ? Object.fromEntries(Object.entries(process).filter(([key]) =>
      key !== INDEPENDENT_UNION_STORAGE)) : process,
    backend: report.backend_contract,
    ...(operationMasks ? { operationMaskParity: operationMaskParity(report, step, fixture) } : {}),
    ...(strictPrefix ? { strictPrefixParity: strictPrefixParity(report, step, fixture, outputMode, strictPrefixMode) } : {}),
  };
}

export function assertExactComparison(comparison, reference, candidate, fixture) {
  requireThat(comparison?.schema === "clearra.placement-graph-solution-coverage-comparison.v1" &&
    comparison.equivalent === true && comparison.complete === true && comparison.universe === fixture.universe &&
    comparison.solution_count === fixture.solutions && comparison.total_weight === fixture.queues &&
    comparison.covered_weight === fixture.queues &&
    (fixture.pairs === undefined || comparison.covered_pairs === fixture.pairs) &&
    comparison.covered_pairs === reference.solution_coverage.covered_pairs &&
    comparison.covered_pairs === candidate.solution_coverage.covered_pairs &&
    comparison.left_sha256 === reference.solution_coverage.sha256 &&
    comparison.right_sha256 === candidate.solution_coverage.sha256,
  "Per-solution comparator did not prove the exact complete mappings of both captured artifacts");
}

// Hook injection lets tests cover sequence and cutoff without invoking a bench
// or needing an actual managed worktree or large coverage files.
export async function collectPlacementCoverageProof(options, binarySha, steps, io) {
  const experiment = experimentFor(options);
  const observations = [];
  const reports = {};
  const evidence = {
    schema: "clearra.placement-graph-coverage-proof.v1",
    release_authority: false,
    timing_adoption_authority: false,
    source_commit: options.sourceCommit,
    binary_sha256: binarySha,
    manager_sha256: io.custody?.managerSha,
    host_fingerprint: io.custody?.hostFingerprint,
    ...(io.custody?.diskBudget ? { disk_budget: io.custody.diskBudget } : {}),
    case: options.fixture.name,
    axis: experiment.axis,
    ...(experiment === DEFAULT_EXPERIMENT ? {} : { experiment: experiment.name }),
    ...(experiment === STRICT_PREFIX_UNION_EXPERIMENT ? {
      fixed_strict_prefix_mode: "1",
      union_memo_storage: { reference: "reference", candidate: "compact" },
    } : {}),
    observations,
    mapping: null,
  };
  let referenceContract, active;
  try {
    requireThat(isDeepStrictEqual(steps.map(({ name }) => name), ["reference", "candidate", "comparison"]),
      "Exact proof needs both captures followed by their comparison");
    for (const step of steps) {
      active = { name: step.name, state: "invalid", command: step.command, supervisor: null };
      observations.push(active);
      requireThat(experimentFor(step) === experiment, step.name + ": wrong scheduled proof experiment");
      await io.checkInputs();
      const execution = await io.execute(step);
      const receipt = io.receipt(execution);
      active.state = classifyProofRuntime(receipt, execution, step);
      active.reason = receipt.outcome.reason;
      if (strictDiskLimitObserved(receipt, execution, step)) active.reason = "strict-coverage-disk-limit";
      active.run_id = receipt.outcome.run_id;
      active.child_termination = receipt.outcome.child_termination;
      requireThat(!observations.slice(0, -1).some((earlier) => earlier.run_id === active.run_id),
        step.name + ": reused a prior supervisor run ID");
      active.supervisor = step.name + "-runtime.json";
      io.write(active.supervisor, receipt);
      const reportPath = step.name + ".json";
      if (active.state !== "completed") {
        // A semantic mismatch can print a useful negative report before exit.
        // Preserve it as diagnostic only, not as the verified comparison.
        if (step.name === "comparison") {
          const diagnostic = parseBoundBenchmarkOutput(receipt, execution);
          if (diagnostic?.equivalent === false) io.write("comparison-diagnostic.json", diagnostic);
        }
        throw new Error(step.name + ": " + active.state + " (" + active.reason + ")");
      }
      active.state = "invalid";
      const report = parseBoundBenchmarkOutput(receipt, execution);
      requireThat(report, step.name + ": missing or unbound JSON output");
      if (step.name === "comparison") {
        assertExactComparison(report, reports.reference, reports.candidate, options.fixture);
      } else {
        const contract = validatedProofArm(report, receipt, step, options, binarySha);
        referenceContract ??= contract;
        requireThat(isDeepStrictEqual(referenceContract, contract),
          "Reference and candidate must agree on exact fixture, auto workers, and aggregate contracts");
        io.assertArtifact(step.artifact, isStrictPrefixProof(experiment)
          ? report.solution_coverage.output_bytes : undefined);
      }
      io.write(reportPath, report);
      reports[step.name] = report;
      active.state = "completed";
    }
    await io.checkInputs();
    evidence.state = options.key === "p7p4" ? "completed-exact-per-solution" : "completed-small-preflight";
    evidence.source_binding = [reports.reference, reports.candidate].every(
      (report) => report.compiled_source_commit === options.sourceCommit)
      ? "compile-time-and-checkout" : "checkout-and-binary-custody-only";
    evidence.mapping = {
      comparison: "comparison.json",
      reference: { benchmark: "reference.json", supervisor: "reference-runtime.json" },
      candidate: { benchmark: "candidate.json", supervisor: "candidate-runtime.json" },
    };
  } catch (error) {
    evidence.state = active?.state === "censored" || active?.state === "confounded" ? active.state : "failed";
    evidence.error = error.message;
  }
  io.write("summary.json", evidence);
  return evidence;
}

export function parseBoundBenchmarkOutput(receipt, execution) {
  try {
    const reported = JSON.parse(receipt.stdout_tail);
    return isDeepStrictEqual(reported, JSON.parse(execution.stdout)) ? reported : null;
  } catch {
    return null;
  }
}

export function benchmarkEnvironment(environment, step) {
  const experiment = experimentFor(step);
  requireThat(step.value === "0" || step.value === "1", "Invalid placement experiment arm");
  const unionStorage = unionMemoStorage(experiment, step);
  const expected = {
    ...MODES, CLEARRA_PLACEMENT_DEPENDENCY: experiment.dependencyMode,
    CLEARRA_STRUCTURAL_ORDER: experiment.structuralMode, [experiment.environmentKey]: unionStorage ? "1" : step.value,
    ...(unionStorage ? { ...FIXED_MEMO_PROCESS, [INDEPENDENT_UNION_STORAGE]: unionStorage } : {}),
  };
  // Explicit control ("0") is not the legacy unset behavior for these axes.
  // Strip inherited variants, and opt in only for the selected experiment.
  const allowed = Object.fromEntries(Object.entries(environment).filter(([key]) => {
    const normalized = key.toUpperCase();
    return normalized !== OPERATION_MASKS_EXPERIMENT.environmentKey &&
      !OPTIMIZATION_AXES.has(normalized) &&
      normalized !== STRICT_PREFIX_EXPERIMENT.environmentKey && !Object.hasOwn(expected, normalized);
  }));
  return { ...allowed, ...expected };
}

export function composeRuntimeArgs(root, step) {
  return ["--root", root, "runtime", "run", "--producer", "benchmark", "--profile", "benchmark-search",
    "--timeout", String(step.timeout), "--", ...step.command];
}

export function captureChild(manager, args, cwd, env) {
  return new Promise((resolvePromise, reject) => {
    // POSIX terminal cancellation belongs to the JS launcher, not the
    // manager. The manager alone owns and enforces the active solver lease.
    const child = spawn(manager, args, { cwd, env, windowsHide: true,
      detached: process.platform !== "win32", stdio: ["ignore", "pipe", "pipe"] });
    const stdout = [], stderr = [];
    let size = 0, overflow = false;
    const collect = (parts) => (chunk) => {
      size += chunk.length;
      if (size <= MAX_JSON_BYTES) parts.push(chunk);
      else overflow = true;
    };
    child.stdout.on("data", collect(stdout));
    child.stderr.on("data", collect(stderr));
    child.once("error", reject);
    child.once("close", (code, signal) => resolvePromise({
      code, signal, overflow, stdout: Buffer.concat(stdout).toString("utf8"),
      stderr: Buffer.concat(stderr).toString("utf8"),
    }));
  });
}

export function stateReceiptDirectory(environment) {
  const root = process.platform === "win32"
    ? join(environment.LOCALAPPDATA ?? "", "Clearra", "state")
    : join(environment.XDG_STATE_HOME ?? join(environment.HOME ?? "", ".local", "state"), "Clearra");
  requireThat(isAbsolute(root), "The supervisor's native receipt directory must resolve to an absolute path");
  return join(root, "receipts", "rust-v2");
}

export function readSupervisorReceipt(execution, receiptDirectory) {
  const slots = [...execution.stderr.matchAll(/^clearra_runtime_receipt=([^\r\n]+)\r?$/gmu)];
  if (slots.length === 0 && execution.stderr.includes("E_CLEARRA_RUNTIME_SLOT_BUSY")) {
    throw new Error("benchmark-search ownership already held; did not retry");
  }
  requireThat(!execution.overflow && slots.length === 1, "Missing unique bounded supervisor receipt");
  const actual = slots[0][1];
  const match = /^(\d{16,21}-[1-9]\d*)-runtime\.json$/u.exec(basename(actual));
  const canonicalDirectory = realpathSync(receiptDirectory);
  requireThat(canonicalDirectory === resolve(receiptDirectory) && isAbsolute(actual) && match &&
    realpathSync(dirname(actual)) === canonicalDirectory,
    "Supervisor receipt must belong to the native Clearra state directory");
  const file = requireOwnedFile(join(canonicalDirectory, basename(actual)));
  requireThat(lstatSync(file).size <= MAX_JSON_BYTES, "Oversized supervisor receipt");
  const receipt = JSON.parse(readFileSync(file, "utf8"));
  requireThat(receipt.outcome?.run_id === match[1], "Receipt filename and run ID do not agree");
  return receipt;
}

export function requireOwnedFile(path, allowCargoHardlink = false) {
  const stat = lstatSync(path);
  requireThat(stat.isFile() && (allowCargoHardlink || stat.nlink === 1) && realpathSync(path) === path,
    "Expected an owned regular file with no link traversal: " + basename(path));
  return path;
}

export function placementBenchmarkBinaries(root, buildProfile, environment = process.env) {
  requireThat(buildProfile === "debug" || buildProfile === "release", "Unexpected Cargo benchmark profile");
  const target = environment.CLEARRA_BUILD_SESSION_ID
    ? assertManagedBuildTransaction({ environment, sourceRoot: root }).cargoTarget
    : join(root, "build", "cargo", "default");
  const rustRoot = join(target, buildProfile);
  const suffix = process.platform === "win32" ? ".exe" : "";
  return {
    binary: requireOwnedFile(join(rustRoot, "examples", "placement_graph_bench" + suffix), true),
    manager: requireOwnedFile(join(rustRoot, "clearra-manage" + suffix), true),
  };
}

export function checkedCommand(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, windowsHide: true, encoding: "utf8",
    timeout: 30_000, maxBuffer: MAX_JSON_BYTES });
  requireThat(result.status === 0 && !result.error, "Bounded preflight command failed: " + basename(command));
  return result.stdout;
}

export async function fileSha(path, allowCargoHardlink = true) {
  const sha = createHash("sha256");
  // Cargo can hardlink the canonical output to its own fingerprinted file.
  for await (const chunk of createReadStream(requireOwnedFile(path, allowCargoHardlink))) sha.update(chunk);
  return sha.digest("hex");
}

export function acquireProofOwnership(path, id) {
  const owner = JSON.stringify({ run: id, pid: process.pid, token: randomUUID() }) + "\n";
  try {
    writeFileSync(path, owner, { flag: "wx" });
  } catch (error) {
    if (error.code === "EEXIST") {
      throw new Error("Placement coverage suite already owns this root; did not retry or reclaim its lock");
    }
    throw error;
  }
  const original = lstatSync(path, { bigint: true });
  return () => {
    const current = lstatSync(requireOwnedFile(path), { bigint: true });
    requireThat(original.dev === current.dev && original.ino === current.ino &&
      original.size === current.size && readFileSync(path, "utf8") === owner,
    "Suite lock ownership changed; refusing cleanup");
    unlinkSync(path);
  };
}

export function localPlacementHostFingerprint() {
  // Bind to the same host, OS and CPU presentation without emitting the hostname.
  return createHash("sha256").update(JSON.stringify({
    host: hostname(), version: version(), platform: process.platform, arch: process.arch,
    available: availableParallelism(), cpu: cpus().map(({ model }) => model), totalMemory: totalmem(),
  })).digest("hex");
}

export function requirePlacementEvidenceSlot(manager, root, path) {
  requireThat(!existsSync(path), "Coverage evidence slot is already owned: " + basename(path));
  const verification = JSON.parse(checkedCommand(manager, ["--root", root, "storage", "verify", "--path", path], root));
  requireThat(verification?.schema_id === "clearra.storage-verification.v2" &&
    verification.root_id === "local-artifacts" && resolve(verification.path) === path,
  "Coverage slot is not inside the declared local artifact root");
}

export function readOwnedPlacementJson(path) {
  const file = requireOwnedFile(path);
  requireThat(lstatSync(file).size <= MAX_JSON_BYTES, "Oversized placement suite evidence");
  return JSON.parse(readFileSync(file, "utf8"));
}

export { AXIS, COVERAGE_ROOT, coverageExperiment };

let interrupted = null;

export function placementProofDiskPlan(options, isWsl) {
  if (options.key !== "p7p4") return null;
  if (!isStrictPrefixProof(experimentFor(options))) return { minimumFreeBytes: P7P4_FREE_RESERVE };
  requireThat(!isWsl || options.wslHostVolume,
    "The full strict proof on WSL needs --wsl-host-volume /mnt/<drive> for the VHD's actual backing volume");
  requireThat(isWsl || !options.wslHostVolume, "The WSL host-volume verification requires a Linux WSL runtime");
  return {
    minimumFreeBytes: 2 * STRICT_CAPTURE_LIMIT + STRICT_HEADROOM,
    minimumActiveArmFreeBytes: STRICT_CAPTURE_LIMIT + STRICT_HEADROOM,
    minimumComparisonFreeBytes: STRICT_HEADROOM,
  };
}

function checkPlacementProofDisk(options, plan, artifactRoot, minimumFreeBytes) {
  if (!plan) return undefined;
  const volumes = [["artifact", artifactRoot]];
  if (options.wslHostVolume) {
    const mount = options.wslHostVolume;
    const volume = lstatSync(mount, { bigint: true });
    requireThat(realpathSync(mount) === mount && volume.isDirectory() &&
      volume.dev !== lstatSync(dirname(mount), { bigint: true }).dev,
    "The specified WSL host volume must be a mounted Windows drive, not a guest directory");
    volumes.push(["wsl_host", mount]);
  }
  const observed = {};
  for (const [name, path] of volumes) {
    const volume = statfsSync(path, { bigint: true });
    const freeBytes = volume.bavail * volume.bsize;
    requireThat(freeBytes >= BigInt(minimumFreeBytes),
      `P7P4 storage-preflight refused: ${name} volume has ${freeBytes} free bytes, requires ${minimumFreeBytes}`);
    observed[name] = { path, free_bytes: freeBytes.toString() };
  }
  return { minimum_free_bytes: minimumFreeBytes, volumes: observed };
}

async function main(argv) {
  const options = parseProofArguments(argv);
  requireThat(process.env.CLEARRA_RUNTIME_SUPERVISED !== "1",
    "Run the orchestrator directly; it starts a separate finite native supervisor for each phase");
  requireThat(!Object.keys(process.env).some((key) => key.toUpperCase() === "CLEARRA_EXPECTED_VCPUS"),
    "Remove CLEARRA_EXPECTED_VCPUS to keep native automatic all-logical worker selection");
  const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), "../.."));
  const { binary, manager } = placementBenchmarkBinaries(root, options.buildProfile);
  const artifactRoot = join(root, ...COVERAGE_ROOT);
  const proofRoot = join(artifactRoot, options.id + "-proof");
  const lockPath = join(artifactRoot, "proof-transaction.lock");
  const steps = proofSchedule(options, binary, artifactRoot);
  const binarySha = await fileSha(binary);
  const managerSha = await fileSha(manager);
  const hostFingerprint = localPlacementHostFingerprint();
  const checkInputs = async () => {
    requireThat(!interrupted, "Interrupted; stopped before starting another phase");
    requireThat(checkedCommand("git", ["rev-parse", "HEAD"], root).trim() === options.sourceCommit &&
      checkedCommand("git", ["status", "--porcelain=v1", "--untracked-files=normal"], root).trim() === "",
    "The tracked and untracked source must match the exact clean checkout for this proof");
    requireThat(await fileSha(binary) === binarySha && await fileSha(manager) === managerSha,
      "Benchmark or supervisor binary changed during the proof");
    requireThat(localPlacementHostFingerprint() === hostFingerprint, "The measured host changed during the proof");
  };
  await checkInputs();
  const owned = [lockPath, proofRoot, ...steps.slice(0, 2).map((step) => dirname(step.artifact))];
  for (const path of owned) {
    requirePlacementEvidenceSlot(manager, root, path);
  }
  const supervisorDirectory = stateReceiptDirectory(process.env);
  mkdirSync(artifactRoot, { recursive: true });
  requireThat(realpathSync(artifactRoot) === artifactRoot, "Coverage artifact parent is a link or junction");
  const diskPlan = placementProofDiskPlan(options, process.platform === "linux" && /microsoft|wsl/iu.test(osRelease()));
  const diskBudget = checkPlacementProofDisk(options, diskPlan, artifactRoot, diskPlan?.minimumFreeBytes);
  const release = acquireProofOwnership(lockPath, options.id);
  let evidence;
  try {
    mkdirSync(proofRoot); // Exclusive ownership; no reopen/resume of a failed proof.
    evidence = await collectPlacementCoverageProof(options, binarySha, steps, {
      custody: { hostFingerprint, managerSha, diskBudget },
      checkInputs,
      execute: (step) => {
        if (diskPlan?.minimumActiveArmFreeBytes) {
          checkPlacementProofDisk(options, diskPlan, artifactRoot, step.name === "comparison"
            ? diskPlan.minimumComparisonFreeBytes : diskPlan.minimumActiveArmFreeBytes);
        }
        return captureChild(manager, composeRuntimeArgs(root, step), root, benchmarkEnvironment(process.env, step));
      },
      receipt: (execution) => readSupervisorReceipt(execution, supervisorDirectory),
      assertArtifact: (path, expectedBytes) => {
        const owned = requireOwnedFile(path);
        if (expectedBytes !== undefined) {
          requireThat(lstatSync(owned).size === expectedBytes, "Captured file size does not match its sealed receipt");
        }
      },
      write: (name, value) => writeFileSync(join(proofRoot, name), JSON.stringify(value, null, 2) + "\n", { flag: "wx" }),
    });
  } finally {
    release();
  }
  process.stdout.write(JSON.stringify({
    proof: join(proofRoot, "summary.json"), state: evidence.state, error: evidence.error,
  }) + "\n");
  if (!evidence.mapping) process.exitCode = interrupted === "SIGINT" ? 130 : interrupted === "SIGTERM" ? 143 : 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, () => {
      if (interrupted) return;
      interrupted = signal;
      process.stderr.write("[placement-coverage-proof] cancelled: no further phase will start; " +
        "the current native supervisor, if any, remains responsible for its finite lease\n");
    });
  }
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write("[placement-coverage-proof] " + error.message + "\n");
    process.exitCode = 1;
  });
}

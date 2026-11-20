// Non-publishing evaluation of already captured, same-binary benchmark evidence.
// Input is a manifest with eight { benchmark, supervisor } file references and
// mapping: { comparison, reference: { benchmark, supervisor },
// candidate: { benchmark, supervisor } }. Paths are relative to the manifest
// directory. A censored/failed run can omit its benchmark field; it cannot
// contribute a completed sample. The evaluator does not run workloads.
import { readFileSync, realpathSync, statSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { pathToFileURL } from "node:url";

const INPUT_SCHEMA = "clearra.placement-graph-adoption-input.v1";
const BENCHMARK_SCHEMA = "clearra.placement-graph-benchmark.v1";
const MAPPING_SCHEMA = "clearra.placement-graph-solution-coverage-comparison.v1";
const RUNTIME_SCHEMA = "clearra.runtime-receipt.v2";
const ORDER = "ABBABAAB";
const AXES = [
  "mode", "structural_mode", "static_influence_mode", "translation_interval_mode",
  "movement_clear_certificates_mode",
];
const FIXED_P7P4_AXES = ["incremental_domain_mode"];
const SOLUTIONS = 456_459;
const QUEUES = 4_233_600;
const SOLUTION_SET_HASH = "cts1:2fcbc950acdffa31";
const SHA256 = /^[a-f0-9]{64}$/u;
const RUN_ID = /^(\d{16,21})-[1-9]\d*$/u;
const MAX_JSON_BYTES = 4 * 1024 * 1024;

function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}

function asFinitePositive(value) {
  return Number.isFinite(value) && value > 0;
}

function asPositiveInt(value) {
  return Number.isSafeInteger(value) && value > 0;
}

function summaryBool(value, expected) {
  // The Rust benchmark exposes typed booleans and the string fields of its
  // flat CoreExecutionResult summary separately.
  return value === expected || value === String(expected);
}

function summaryIntEquals(value, expected) {
  return value === expected || value === String(expected);
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

function mad(values) {
  const center = median(values);
  return median(values.map((value) => Math.abs(value - center)));
}

function runtimeOutcome(runtime, label) {
  const outcome = runtime?.outcome;
  const stamp = RUN_ID.exec(outcome?.run_id ?? "");
  requireThat(runtime?.schema_id === RUNTIME_SCHEMA && stamp && BigInt(stamp[1]) > 0n &&
    outcome.profile === "benchmark-search" && asPositiveInt(outcome.duration_ms) &&
    asPositiveInt(outcome.timeout_seconds) && Number.isSafeInteger(outcome.return_code) &&
    outcome.automatic_retry === false, `${label}: invalid supervised benchmark receipt`);
  return { outcome, stampNs: BigInt(stamp[1]) };
}

function outcomeState(outcome) {
  const signal = outcome.child_termination?.kind === "unix-signal" ? outcome.child_termination.signal : null;
  if (["timeout", "memory-limit", "memory-pressure", "output-limit", "process-limit", "cancelled"].includes(outcome.reason) ||
      (outcome.reason === "signal" && [2, 9, 15].includes(signal))) return "censored";
  if (outcome.return_code !== 0 || outcome.reason !== "normal" || outcome.error_code !== null ||
      outcome.supervisor_termination_reason !== null || outcome.process_tree_stopped !== true ||
      outcome.child_termination?.kind !== "exit" || outcome.child_termination.code !== 0) return "failed";
  requireThat(typeof outcome.memory_pressure_observed === "boolean" &&
    Number.isSafeInteger(outcome.memory_pressure_events) && outcome.memory_pressure_events >= 0,
  "missing supervisor pressure information");
  if (outcome.memory_pressure_observed || outcome.memory_pressure_events > 0) return "confounded";
  return "completed";
}

function peakOf(outcome, label) {
  const kernel = outcome.kernel_memory_peak;
  const sample = outcome.sampled_memory_peak;
  requireThat(kernel === null || (kernel?.source === "windows-job-object" && asPositiveInt(kernel.bytes)),
    `${label}: missing or invalid kernel peak provenance`);
  requireThat(sample === null || (["windows-sampled-tree-private-usage", "linux-sampled-process-group-rss"].includes(sample?.source) &&
    asPositiveInt(sample.bytes)), `${label}: missing or invalid sampled peak provenance`);
  requireThat(!(kernel && sample?.source === "linux-sampled-process-group-rss"),
    `${label}: conflicting kernel and sampled peak sources`);
  const peak = kernel ?? sample;
  requireThat(peak && peak.bytes === outcome.peak_memory_bytes, `${label}: no consistent measured peak`);
  return peak;
}

function readCompletedRun(run, label, axis, arm, outputMode) {
  const { outcome, stampNs } = runtimeOutcome(run?.supervisor, label);
  requireThat(outcomeState(outcome) === "completed", `${label}: not an uncontended completed run`);
  let captured;
  try {
    captured = JSON.parse(run.supervisor.stdout_tail);
  } catch {
    throw new Error(`${label}: supervisor did not capture a full benchmark JSON report`);
  }
  const report = run.benchmark;
  requireThat(isDeepStrictEqual(captured, report) && report?.schema === BENCHMARK_SCHEMA &&
    report.supervisor_run_id === outcome.run_id && SHA256.test(report.binary_sha256 ?? "") &&
    report.output_mode === outputMode && report.valid === true &&
    Array.isArray(report.validation_errors) && report.validation_errors.length === 0,
  `${label}: invalid or unbound benchmark report`);
  const input = report.input;
  requireThat(report.case === "empty-4L-P7P4-jstris180" && input?.board === "0" && input.height === 4 &&
    input.placed_pieces === 10 && input.supplied_pieces === 11 && input.supply === "P7P4" &&
    input.original_supply_count === QUEUES && input.hold === "enabled-empty" &&
    input.rule === "jstris-180" && input.count === "unique" && input.queue_knowledge === "full-future-oracle",
  `${label}: wrong P7P4 qualification input`);
  const cpu = report.cpu;
  requireThat(cpu?.worker_policy === "auto" && cpu.use_all_logical_processors === true &&
    asPositiveInt(cpu.hard_limit) && cpu.hard_limit > 1 &&
    summaryIntEquals(report.summary?.workers_used, cpu.hard_limit) &&
    summaryIntEquals(report.summary?.parallel_active_workers, cpu.hard_limit) &&
    summaryBool(report.summary?.cpu_parallel_execution, true),
  `${label}: actual workers do not match automatic all-logical parallel policy`);
  const result = report.summary;
  const ids = report.materialized_identities;
  const coverage = report.coverage;
  requireThat(summaryBool(result?.count_complete, true) &&
    summaryBool(result?.probability_complete, true) && summaryBool(result?.resource_truncated, false) &&
    result.normalized_solution_set_hash === SOLUTION_SET_HASH &&
    ids?.count === SOLUTIONS && ids.reported_total_count === SOLUTIONS && ids.covers_reported_family === true &&
    SHA256.test(ids.sha256_ordered_ctk1_v1 ?? "") && coverage?.set_bit_count === QUEUES &&
    coverage.word_count === QUEUES / 64 && SHA256.test(coverage.sha256_le_v1 ?? ""),
  `${label}: incomplete or incorrect P7P4 aggregate`);
  if (outputMode === "solution-coverage") {
    const mapping = report.solution_coverage;
    requireThat(mapping?.complete === true && SHA256.test(mapping.sha256 ?? "") &&
      mapping.solution_count === SOLUTIONS && mapping.total_weight === QUEUES && mapping.covered_weight === QUEUES,
    `${label}: missing complete per-solution evidence`);
  } else {
    requireThat(report.solution_coverage == null, `${label}: mixed benchmark output contract`);
  }
  requireThat(typeof report.backend_contract === "string" && report.backend_contract.length > 0 &&
    report.other_process_configuration && typeof report.other_process_configuration === "object" &&
    !Array.isArray(report.other_process_configuration), `${label}: missing runtime or process configuration`);
  const fixedModes = {};
  // Incremental APDP stays fixed: the empty P7P4 fixture cannot qualify it.
  requireThat(Object.keys(report).every((key) => !key.endsWith("_mode") || key === "output_mode" ||
    AXES.includes(key) || FIXED_P7P4_AXES.includes(key)), `${label}: unknown experimental axis`);
  for (const key of [...AXES, ...FIXED_P7P4_AXES]) {
    requireThat(["0", "1"].includes(report[key]), `${label}: missing or invalid axis ${key}`);
    if (key === axis) {
      requireThat(report[key] === (arm === "A" ? "0" : "1"), `${label}: wrong A/B mode order`);
    } else {
      fixedModes[key] = report[key];
    }
  }
  const searchMs = report.timing_ms?.search;
  requireThat(asFinitePositive(searchMs) && searchMs < outcome.duration_ms + 1,
    `${label}: missing or invalid search duration`);
  requireThat(asPositiveInt(outcome.admission?.hard_limit_bytes) &&
    typeof outcome.admission.capacity_basis === "string" &&
    asPositiveInt(outcome.admission.snapshot?.physical_total_bytes),
  `${label}: missing supervisor admission contract`);
  return {
    outcome, stampNs, searchMs, peak: peakOf(outcome, label), mapping: report.solution_coverage,
    contract: {
      binarySha256: report.binary_sha256,
      case: report.case,
      input, cpu, fixedModes, backend: report.backend_contract, process: report.other_process_configuration,
      ids: ids.sha256_ordered_ctk1_v1, globalCoverage: coverage.sha256_le_v1,
      supervisor: {
        profile: outcome.profile,
        timeoutSeconds: outcome.timeout_seconds,
        capacityBasis: outcome.admission.capacity_basis,
        hardLimitBytes: outcome.admission.hard_limit_bytes,
        physicalTotalBytes: outcome.admission.snapshot.physical_total_bytes,
      },
    },
  };
}

function verifyMapping(mapping, axis, contract) {
  const reference = readCompletedRun(mapping?.reference, "mapping reference", axis, "A", "solution-coverage");
  const candidate = readCompletedRun(mapping?.candidate, "mapping candidate", axis, "B", "solution-coverage");
  const comparison = mapping?.comparison;
  requireThat(reference.outcome.run_id !== candidate.outcome.run_id &&
    isDeepStrictEqual(reference.contract, contract) && isDeepStrictEqual(candidate.contract, contract),
  "per-solution representatives must use the same binary/input/CPU and differ only in the tested axis");
  requireThat(comparison?.schema === MAPPING_SCHEMA && comparison.universe === "p7p4" &&
    comparison.complete === true && comparison.equivalent === true &&
    comparison.solution_count === SOLUTIONS && comparison.total_weight === QUEUES &&
    comparison.covered_weight === QUEUES && comparison.left_sha256 === reference.mapping.sha256 &&
    comparison.right_sha256 === candidate.mapping.sha256,
  "complete per-solution comparator evidence is missing or does not bind both artifacts");
  return { A: reference.mapping.sha256, B: candidate.mapping.sha256 };
}

export function evaluatePlacementGraphAdoption(input) {
  const { axis, output_mode: outputMode, time_resolution_ms: resolution, runs } = input ?? {};
  requireThat(input?.schema === INPUT_SCHEMA && AXES.includes(axis) && asFinitePositive(resolution) &&
    ["global-only", "solution-coverage"].includes(outputMode) && Array.isArray(runs) && runs.length === ORDER.length,
  "invalid adoption manifest: exactly one known axis and ordered ABBA + BAAB are required");
  const evidence = {
    schema: "clearra.placement-graph-adoption.v1", release_authority: false, axis,
    order: "ABBA+BAAB", output_mode: outputMode, time_resolution_ms: resolution,
  };
  const observations = runs.map((run, index) => {
    const { outcome, stampNs } = runtimeOutcome(run?.supervisor, `run ${index + 1}`);
    return {
      slot: index + 1, arm: ORDER[index], stampNs, run_id: outcome.run_id, state: outcomeState(outcome),
      reason: outcome.reason, child_termination: outcome.child_termination,
      duration_ms: outcome.duration_ms,
    };
  });
  for (let index = 1; index < observations.length; index += 1) {
    const prior = observations[index - 1];
    requireThat(observations[index].stampNs > prior.stampNs + BigInt(prior.duration_ms) * 1_000_000n,
      "supervisor run IDs must be in order and must not overlap");
  }
  evidence.observations = observations.map(({ stampNs, ...observation }) => observation);
  if (observations.some(({ state }) => state === "failed")) return { ...evidence, status: "failed" };
  if (observations.some(({ state }) => state !== "completed")) return { ...evidence, status: "inconclusive" };

  const completed = runs.map((run, index) => readCompletedRun(run, `run ${index + 1}`, axis, ORDER[index], outputMode));
  const { contract, peak } = completed[0];
  requireThat(completed.every((run) => isDeepStrictEqual(run.contract, contract)),
    "binary, CPU/request/result identity, fixed axes or supervisor contracts differ");
  requireThat(completed.every((run) => run.peak.source === peak.source),
    "cannot compare different sampled/kernel peak provenance");
  const maps = verifyMapping(input.mapping, axis, contract);
  if (outputMode === "solution-coverage") {
    requireThat(completed.every((run, index) => run.mapping.sha256 === maps[ORDER[index]]),
      "measured solution-coverage artifact is not the verified canonical mapping");
  }

  const select = (series, arm, start, end) => series
    .filter((_, i) => i >= start && i < end && ORDER[i] === arm);
  const times = completed.map(({ searchMs }) => searchMs);
  const peaks = completed.map((run) => run.peak.bytes);
  const baselineTimes = select(times, "A", 0, ORDER.length);
  const candidateTimes = select(times, "B", 0, ORDER.length);
  const baselinePeaks = select(peaks, "A", 0, ORDER.length);
  const candidatePeaks = select(peaks, "B", 0, ORDER.length);
  const blocks = [0, 4].map((start) => ({
    baseline_ms: median(select(times, "A", start, start + 4)),
    candidate_ms: median(select(times, "B", start, start + 4)),
  }));
  const timeMedian = { baseline: median(baselineTimes), candidate: median(candidateTimes) };
  const timeNoise = Math.max(mad(baselineTimes), resolution);
  const peakMedian = { baseline: median(baselinePeaks), candidate: median(candidatePeaks) };
  const peakMax = { baseline: Math.max(...baselinePeaks), candidate: Math.max(...candidatePeaks) };
  const peakNoise = mad(baselinePeaks);
  const timeImproved = blocks.every(({ baseline_ms, candidate_ms }) => candidate_ms < baseline_ms) &&
    timeMedian.baseline - timeMedian.candidate > timeNoise;
  const memoryNonregressing = peakMedian.candidate - peakMedian.baseline <= peakNoise &&
    peakMax.candidate - peakMax.baseline <= peakNoise;
  return {
    ...evidence, status: timeImproved && memoryNonregressing ? "adoptable" : "not-adoptable",
    scope: outputMode === "solution-coverage"
      ? "measured-artifacts-bound-to-exact-per-solution-comparison"
      : "global-only-timings-with-separate-exact-per-solution-qualification",
    binary_sha256: contract.binarySha256, mapping_sha256: maps, blocks,
    time: { measured_ms: times, median_ms: timeMedian, baseline_noise_floor_ms: timeNoise, improved: timeImproved },
    memory: {
      source: peak.source, measured_bytes: peaks, median_bytes: peakMedian, maximum_bytes: peakMax,
      baseline_mad_bytes: peakNoise, nonregressing: memoryNonregressing,
    },
  };
}

function readJson(path) {
  const stat = statSync(path);
  requireThat(stat.isFile() && stat.size <= MAX_JSON_BYTES, "evidence must be a bounded JSON file");
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch {
    throw new Error("invalid evidence JSON");
  }
}

export function evaluateManifest(path) {
  const safeJsonPath = (value) => typeof value === "string" && value.length > 0 && /\.json$/iu.test(value) &&
    !value.split(/[/\\]/u).some((part) => /(?:^|[_. -])(?:env|secret|credential|private[_ -]?key|api[_ -]?key|service[_ -]?account)(?:$|[_. -])/iu.test(part));
  requireThat(safeJsonPath(path), "expected a non-credential JSON evidence manifest");
  const manifestPath = realpathSync(path);
  requireThat(safeJsonPath(manifestPath), "manifest must not resolve to credential material");
  const root = dirname(manifestPath);
  const manifest = readJson(manifestPath);
  requireThat(manifest?.schema === INPUT_SCHEMA && Array.isArray(manifest.runs) &&
    manifest.runs.length === ORDER.length, "expected exactly eight manifest run references");
  const readReference = (name) => {
    requireThat(safeJsonPath(name) && !isAbsolute(name) && !name.split(/[/\\]/u).includes(".."),
      "evidence path must be an explicit JSON path relative to the manifest directory");
    const actual = realpathSync(resolve(root, name));
    const local = relative(root, actual);
    requireThat(local && local !== ".." && !local.startsWith(`..${sep}`) && !isAbsolute(local) && safeJsonPath(actual),
      "evidence path leaves the manifest directory");
    return readJson(actual);
  };
  const loadRun = (run, allowMissingBenchmark = false) => ({
    supervisor: readReference(run?.supervisor),
    benchmark: allowMissingBenchmark && run?.benchmark == null ? null : readReference(run?.benchmark),
  });
  const runs = manifest.runs.map((run) => loadRun(run, true));
  // Incomplete measurements are useful without mapping files. Delay mandatory
  // proof reads until there is a complete, uncontended sequence to evaluate.
  const complete = runs.every((run, index) =>
    outcomeState(runtimeOutcome(run.supervisor, `run ${index + 1}`).outcome) === "completed");
  const mapping = complete ? {
    comparison: readReference(manifest.mapping?.comparison),
    reference: loadRun(manifest.mapping?.reference),
    candidate: loadRun(manifest.mapping?.candidate),
  } : null;
  return evaluatePlacementGraphAdoption({ ...manifest, runs, mapping });
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    requireThat(process.argv.length === 3, "usage: node scripts/tools/placement-graph-adoption.mjs MANIFEST.json");
    const result = evaluateManifest(process.argv[2]);
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    if (result.status !== "adoptable") process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`placement-graph-adoption: ${error.message}\n`);
    process.exitCode = 2;
  }
}

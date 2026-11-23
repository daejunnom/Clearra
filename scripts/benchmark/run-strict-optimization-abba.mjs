// Local-only mechanism A/B. Complete timings are not per-solution proof or
// release authority. Compare one strict-prefix axis against the pinned,
// exact-qualified frame-owner algorithm and approved compact union storage.
import { existsSync, mkdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { createManagedSearchOwner } from "../tools/managed-search-owner.mjs";
import {
  acquireProofOwnership, benchmarkEnvironment, captureChild, checkedCommand,
  classifyProofRuntime, composeRuntimeArgs, COVERAGE_ROOT, coverageExperiment,
  fileSha, localPlacementHostFingerprint, parseBoundBenchmarkOutput,
  placementBenchmarkBinaries, readSupervisorReceipt, requirePlacementEvidenceSlot,
  stateReceiptDirectory,
} from "./run-placement-coverage-proof.mjs";

const BASELINE = "strict-frame-owner-v1-compact";
const HISTORICAL_BASELINE = "strict-frame-owner-v1";
const UNION_MEMO = "CLEARRA_STANDARD_BAG_UNION_MEMO";
const AXES = Object.freeze({
  catalog: ["CLEARRA_STRICT_CATALOG_GUARDS", "1"],
  absorption: ["CLEARRA_STANDARD_BAG_ABSORB", "1"],
  frontier: ["CLEARRA_STRICT_LANGUAGE_FRONTIER", "1"],
  subsumption: ["CLEARRA_STRICT_SUBSUMPTION", "1"],
  "union-memo": [UNION_MEMO, "compact"],
});
const NEUTRAL = new Set([...Object.values(AXES).map(([key]) => key),
  "CLEARRA_STRICT_STAGE_PROFILE", "CLEARRA_STRICT_STAGE_LEGACY_UPPER",
  "CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES", "CLEARRA_EXPECTED_VCPUS"]);
const CASES = Object.freeze({
  p7: { name: "nonempty-4L-P7-srs-plus", solutions: 246, queues: 5_040, candidates: 2_260 },
  p7p4: { name: "empty-4L-P7P4-jstris180", solutions: 456_459, queues: 4_233_600, candidates: 29_856_840 },
});
const DIGEST = /^[a-f0-9]{64}$/u;
const FORCE = {
  CLEARRA_STANDARD_BAG_MEMO: "reference",
  CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: "adaptive",
};
// Read back from the completed e023 frame-owner proof and the independent
// d936 reference trials for this exact fixture. These checks identify the
// workload and physical route; they do not prove the per-solution mapping.
const P7P4_BASELINE = Object.freeze({
  orderedIdentities: "08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062",
  queueUnion: "0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912",
  strictWork: Object.freeze({
    unresolved_lanes: 124_505_112,
    reverse_relations: 8_528_265,
    ordered_kick_terms: 2_879_722_152,
  }),
});
let interrupted = false;

function requireThat(value, message) {
  if (!value) throw new Error(message);
}

export function parseStrictOptimizationArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index], value = argv[index + 1];
    requireThat(["--axis", "--baseline", "--case", "--id", "--source-sha", "--timeout-seconds", "--workers", "--workspace"].includes(key) &&
      !Object.hasOwn(options, key) && typeof value === "string", "Expected distinct named option/value pairs");
    options[key] = value;
  }
  const axis = options["--axis"];
  const key = options["--case"] ?? "p7p4";
  const id = options["--id"];
  const source = options["--source-sha"];
  const timeout = Number(options["--timeout-seconds"] ?? "0");
  const workers = Number(options["--workers"] ?? "8");
  const baseline = options["--baseline"] ?? BASELINE;
  const workspace = options["--workspace"];
  requireThat(Object.hasOwn(AXES, axis) && Object.hasOwn(CASES, key) &&
    (baseline === HISTORICAL_BASELINE || (baseline === BASELINE && axis !== "union-memo")) &&
    /^[a-z0-9][a-z0-9-]{0,47}$/u.test(id ?? "") && /^[a-f0-9]{40}$/u.test(source ?? "") &&
    Number.isSafeInteger(timeout) && timeout >= 0 && timeout <= 1800 &&
    Number.isSafeInteger(workers) && workers >= 2 && workers <= 128,
  "Invalid strict optimization ABBA options");
  requireThat(key === "p7p4" || (axis !== "frontier" && axis !== "subsumption"),
    "Repeated-kind axes require the P7P4 fixture");
  return { axis, baseline, key, id, source, timeout, workers, workspace };
}

export function strictOptimizationSchedule(options, binary) {
  return [..."ABBA"].map((arm, index) => ({
    name: `trial-${index + 1}-${arm.toLowerCase()}`, arm, value: "1", baseline: options.baseline,
    experiment: coverageExperiment("strict-prefix"),
    command: [binary, ...(options.key === "p7" ? ["--p7"] : [])], timeout: options.timeout,
  }));
}

export function strictOptimizationEnvironment(inherited, axis, step) {
  const clean = Object.fromEntries(Object.entries(inherited).filter(([key]) =>
    !NEUTRAL.has(key.toUpperCase()) && !Object.hasOwn(FORCE, key.toUpperCase())));
  const [key, value] = AXES[axis];
  return {
    ...benchmarkEnvironment(clean, step), ...FORCE,
    [UNION_MEMO]: step.baseline === HISTORICAL_BASELINE ? "reference" : "compact",
    ...(step.arm === "B" ? { [key]: value } : {}),
  };
}

export function validateStrictOptimizationReport(report, options, step, receipt, binarySha) {
  const fixture = CASES[options.key];
  const summary = report?.summary;
  const proof = report?.strict_prefix;
  const ids = report?.materialized_identities;
  const memory = report?.core_memory_lifetimes;
  const configuration = report?.other_process_configuration;
  const [axisKey, axisValue] = AXES[options.axis];
  const unionMemo = axisKey === UNION_MEMO && step.arm === "B" ? axisValue :
    options.baseline === HISTORICAL_BASELINE ? "reference" : "compact";
  const activeAxes = [...Object.values(AXES)].filter(([key]) =>
    key !== UNION_MEMO && Object.hasOwn(configuration ?? {}, key));
  const expectedAxis = step.arm === "B" && axisKey !== UNION_MEMO ? axisKey : null;
  const buildNodes = Number(summary?.total_build_order_nodes);
  const scalarPoses = Number(summary?.total_reachability_states);
  const validAlgorithm = step.value === "1" &&
    proof?.sealed === true && proof.workers === options.workers && proof.executing_workers === options.workers &&
    proof.candidates === fixture.candidates && buildNodes === 0 && scalarPoses === 0;
  requireThat(report?.schema === "clearra.placement-graph-benchmark.v1" && report.valid === true &&
    report.compiled_source_commit === options.source && report.binary_sha256 === binarySha &&
    report.supervisor_run_id === receipt.outcome.run_id && report.case === fixture.name &&
    report.strict_prefix_mode === step.value && validAlgorithm && report.output_mode === "global-only" &&
    report.coverage_presentation === "coverage-summary-omit" && report.coverage_capture_sink === "discard" &&
    report.solution_coverage === null && report.materialized_path_step_count === 0 &&
    report.materialized_solution_probability_count === 0 && report.materialized_solution_coverage_count === 0 &&
    Number(summary?.packing_candidate_count) === fixture.candidates &&
    Number(summary.workers_used) === options.workers && Number(summary.parallel_active_workers) === options.workers &&
    report.cpu?.hard_limit === options.workers && report.cpu?.expected_vcpus === null &&
    report.cpu?.worker_policy === "auto" && report.cpu?.use_all_logical_processors === true &&
    String(summary.count_complete) === "true" && String(summary.probability_complete) === "true" &&
    String(summary.resource_truncated) === "false" &&
    ids?.count === fixture.solutions && ids.reported_total_count === fixture.solutions &&
    ids.covers_reported_family === true && DIGEST.test(ids.sha256_ordered_ctk1_v1 ?? "") &&
    report.coverage?.set_bit_count === fixture.queues && DIGEST.test(report.coverage.sha256_le_v1 ?? "") &&
    (options.key !== "p7p4" || (summary.normalized_solution_set_hash === "cts1:2fcbc950acdffa31" &&
      ids.sha256_ordered_ctk1_v1 === P7P4_BASELINE.orderedIdentities &&
      report.coverage.sha256_le_v1 === P7P4_BASELINE.queueUnion &&
      (step.arm !== "A" || Object.entries(P7P4_BASELINE.strictWork).every(([key, value]) => proof[key] === value)))) &&
    Object.entries(FORCE).every(([key, value]) => configuration?.[key] === value) &&
    (expectedAxis === null ? activeAxes.length === 0 : activeAxes.length === 1 &&
      configuration[expectedAxis] === axisValue) &&
    configuration?.[UNION_MEMO] === unionMemo && summary.standard_bag_union_memo_storage === unionMemo,
    `${step.name}: unexpected or incomplete fixture, worker, or optimization evidence`);
  const search = report.timing_ms?.search;
  const peak = receipt.outcome.sampled_memory_peak;
  requireThat(Number.isFinite(search) && search > 0 && search <= receipt.outcome.duration_ms &&
    peak?.source === "linux-sampled-process-group-rss" && Number.isSafeInteger(peak.bytes) && peak.bytes > 0,
  `${step.name}: missing comparable search or sampled RSS evidence`);
  return {
    outcome: { input: report.input, cpu: report.cpu, ids, coverage: report.coverage },
    sample: {
      arm: step.arm, algorithm: step.arm === "A" ? options.baseline : `${options.baseline}+${options.axis}`,
      search_ms: search, peak_rss_bytes: peak.bytes, workers: Number(summary.workers_used),
      build_order_nodes: buildNodes, scalar_pose_states: scalarPoses,
      union_payload_bytes: Number(summary.standard_bag_union_memo_retained_payload_bytes),
      union_entries: Number(summary.standard_bag_union_memo_entries),
      union_capacity: Number(summary.standard_bag_union_memo_capacity),
      product_payload_bytes: Number(summary.standard_bag_product_memo_retained_payload_bytes),
      coverage_product_states: Number(summary.coverage_product_states),
      coverage_edge_checks: Number(summary.coverage_product_edge_checks),
      worker_exit_scratch_bytes: memory?.worker_exit?.scratch_bytes_sum,
      proof,
    },
  };
}

const halfMedian = ([left, right]) => (left + right) / 2;
export function summarizeStrictOptimization(samples) {
  requireThat(samples.map((sample) => sample.arm).join("") === "ABBA", "ABBA requires four completed ordered samples");
  const pair = (key, arm) => samples.filter((sample) => sample.arm === arm).map((sample) => sample[key]);
  const a = halfMedian(pair("search_ms", "A")), b = halfMedian(pair("search_ms", "B"));
  const peakA = halfMedian(pair("peak_rss_bytes", "A")), peakB = halfMedian(pair("peak_rss_bytes", "B"));
  return {
    search_median_ms: { A: a, B: b },
    search_change_percent: 100 * (b - a) / a,
    both_pairs_faster: samples[1].search_ms < samples[0].search_ms && samples[2].search_ms < samples[3].search_ms,
    sampled_peak_rss_median_bytes: { A: peakA, B: peakB },
    sampled_peak_rss_change_bytes: peakB - peakA,
  };
}

async function main(argv, searchOwner) {
  const options = parseStrictOptimizationArguments(argv);
  requireThat(process.env.CLEARRA_RUNTIME_SUPERVISED !== "1",
    "Each ABBA sample owns a separate native supervisor; do not nest the orchestrator");
  const launcher = fileURLToPath(import.meta.url);
  const root = realpathSync(options.workspace ?? resolve(dirname(launcher), "../.."));
  const { manager, binary } = placementBenchmarkBinaries(root, "release");
  const binarySha = await fileSha(binary), managerSha = await fileSha(manager);
  const runnerSha = await fileSha(launcher);
  const fingerprint = localPlacementHostFingerprint();
  const verify = async () => {
    requireThat(!interrupted, "Interrupted before the next sample");
    requireThat(checkedCommand("git", ["rev-parse", "HEAD"], root).trim() === options.source &&
      checkedCommand("git", ["status", "--porcelain=v1", "--untracked-files=normal"], root).trim() === "" &&
      localPlacementHostFingerprint() === fingerprint && await fileSha(binary) === binarySha &&
      await fileSha(manager) === managerSha && await fileSha(launcher) === runnerSha,
    "Source, binaries, runner, or host changed during ABBA");
  };
  await verify();
  const evidenceRoot = join(root, ...COVERAGE_ROOT);
  const reportRoot = join(evidenceRoot, options.id + "-optimization-abba");
  const lock = join(evidenceRoot, "proof-transaction.lock");
  requirePlacementEvidenceSlot(manager, root, reportRoot);
  requirePlacementEvidenceSlot(manager, root, lock);
  mkdirSync(evidenceRoot, { recursive: true });
  requireThat(realpathSync(evidenceRoot) === evidenceRoot, "Invalid evidence root");
  const release = acquireProofOwnership(lock, options.id);
  const state = { schema: "clearra.strict-optimization-abba.v1", axis: options.axis, baseline: options.baseline, order: "ABBA",
    case: options.key, source_commit: options.source, binary_sha256: binarySha, manager_sha256: managerSha,
    host_fingerprint: fingerprint, runner_sha256: runnerSha, release_authority: false, timing_adoption_authority: false,
    accuracy_boundary: "global-only fingerprints; not exact per-solution comparison",
    status: "incomplete", samples: [] };
  const write = (name, value) => writeFileSync(join(reportRoot, name + ".json"),
    JSON.stringify(value, null, 2) + "\n", { flag: "wx" });
  let contract, previousEnd;
  try {
    mkdirSync(reportRoot);
    for (const step of strictOptimizationSchedule(options, binary)) {
      await verify();
      console.log(`${options.axis}: starting ${step.name}`);
      const result = await captureChild(manager, composeRuntimeArgs(root, step), root,
        strictOptimizationEnvironment(process.env, options.axis, step), searchOwner);
      const receipt = readSupervisorReceipt(result, stateReceiptDirectory(process.env));
      write(step.name + "-runtime", receipt);
      const runtime = classifyProofRuntime(receipt, result, step);
      requireThat(runtime === "completed", `${step.name}: ${runtime} (${receipt.outcome.reason})`);
      const start = BigInt(receipt.outcome.run_id.split("-")[0]);
      requireThat(previousEnd === undefined || start > previousEnd, "Supervisor intervals overlap");
      previousEnd = start + BigInt(receipt.outcome.duration_ms) * 1_000_000n;
      const report = parseBoundBenchmarkOutput(receipt, result);
      const { outcome, sample } = validateStrictOptimizationReport(report, options, step, receipt, binarySha);
      requireThat(!contract || isDeepStrictEqual(contract, outcome), `${step.name}: product output differs`);
      contract ??= outcome;
      write(step.name, report);
      const row = { name: step.name, ...sample };
      state.samples.push(row);
      console.log(JSON.stringify({ axis: options.axis, trial: step.name, search_ms: row.search_ms,
        peak_rss_bytes: row.peak_rss_bytes, workers: row.workers }));
    }
    await verify();
    state.status = "completed-diagnostic";
    state.comparison = summarizeStrictOptimization(state.samples);
  } catch (error) {
    state.error = error.message;
    process.exitCode = 1;
  } finally {
    try {
      if (existsSync(reportRoot)) write("summary", state);
    } finally {
      release();
    }
  }
  console.log(JSON.stringify({ axis: options.axis, status: state.status, comparison: state.comparison,
    error: state.error, receipt: join(reportRoot, "summary.json") }));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const searchOwner = createManagedSearchOwner({ onCancel: () => { interrupted = true; } });
  main(process.argv.slice(2), searchOwner).catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }).finally(() => searchOwner.dispose());
}

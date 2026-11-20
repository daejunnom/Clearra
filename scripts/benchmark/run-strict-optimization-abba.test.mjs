import assert from "node:assert/strict";
import test from "node:test";
import {
  parseStrictOptimizationArguments, strictOptimizationEnvironment, strictOptimizationSchedule,
  summarizeStrictOptimization, validateStrictOptimizationReport,
} from "./run-strict-optimization-abba.mjs";

const source = "a".repeat(40);
const binSha = "b".repeat(64);
const orderedIdentities = "08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062";
const queueUnion = "0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912";
const cli = (axis, ...rest) => ["--axis", axis, "--source-sha", source, "--id", "case-1", ...rest];

test("strict mechanism comparisons are ABBA only and repeated-kind axes require P7P4", () => {
  const opts = parseStrictOptimizationArguments(cli("subsumption"));
  const schedule = strictOptimizationSchedule(opts, "/abs/example");
  assert.equal(schedule.map(({ arm }) => arm).join(""), "ABBA");
  assert.equal(new Set(schedule.map(({ timeout }) => timeout)).size, 1);
  assert.throws(() => parseStrictOptimizationArguments(cli("frontier", "--case", "p7")), /Repeated-kind/);
  assert.throws(() => parseStrictOptimizationArguments(cli("union-memo", "--timeout-seconds", "1801")), /Invalid/);
});

test("exactly one new mechanism changes in treatment, even with hostile inherited options", () => {
  const opts = parseStrictOptimizationArguments(cli("union-memo", "--baseline", "strict-frame-owner-v1"));
  const [a, b] = strictOptimizationSchedule(opts, "/abs/example");
  const ambient = { KEEP: "yes", CLEARRA_STANDARD_BAG_MEMO: "compact",
    clearra_strict_language_frontier: "1", CLEARRA_STRICT_CATALOG_GUARDS: "1",
    CLEARRA_STANDARD_BAG_UNION_MEMO: "compact", CLEARRA_EXPECTED_VCPUS: "12" };
  const baseline = strictOptimizationEnvironment(ambient, opts.axis, a);
  const treatment = strictOptimizationEnvironment(ambient, opts.axis, b);
  assert.deepEqual(treatment, { ...baseline, CLEARRA_STANDARD_BAG_UNION_MEMO: "compact" });
  assert.equal(baseline.KEEP, "yes");
  assert.equal(baseline.CLEARRA_STANDARD_BAG_MEMO, "reference");
  assert.equal(baseline.CLEARRA_STANDARD_BAG_UNION_MEMO, "reference");
  assert.equal(baseline.CLEARRA_STRICT_PREFIX, "1");
  for (const key of ["CLEARRA_EXPECTED_VCPUS", "clearra_strict_language_frontier", "CLEARRA_STRICT_CATALOG_GUARDS"]) {
    assert.equal(Object.hasOwn(baseline, key), false);
  }
});

test("the fixed frame-owner reference never changes with the candidate axis or product default", () => {
  const reference = (axis) => {
    const opts = parseStrictOptimizationArguments(cli(axis, "--baseline", "strict-frame-owner-v1"));
    const schedule = strictOptimizationSchedule(opts, "/abs/example");
    assert.deepEqual(schedule.map(({ value }) => value), ["1", "1", "1", "1"]);
    const env = (step) => strictOptimizationEnvironment({ CLEARRA_STRICT_PREFIX: "0",
      CLEARRA_STANDARD_BAG_UNION_MEMO: "compact", CLEARRA_STRICT_SUBSUMPTION: "1",
      clearra_strict_stage_legacy_upper: "1", CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES: "12" }, opts.axis, step);
    assert.equal(env(schedule[0]).CLEARRA_STRICT_PREFIX, "1");
    assert.equal(env(schedule[1]).CLEARRA_STRICT_PREFIX, "1");
    assert.equal(env(schedule[0]).CLEARRA_STANDARD_BAG_UNION_MEMO, "reference");
    assert.equal(env(schedule[0]).CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT, "adaptive");
    assert.equal(Object.hasOwn(env(schedule[0]), "clearra_strict_stage_legacy_upper"), false);
    assert.equal(Object.hasOwn(env(schedule[0]), "CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES"), false);
    return env(schedule[0]);
  };
  assert.deepEqual(reference("union-memo"), reference("subsumption"));
  const adopted = parseStrictOptimizationArguments(cli("subsumption"));
  assert.equal(adopted.baseline, "strict-frame-owner-v1-compact");
  for (const step of strictOptimizationSchedule(adopted, "/abs/example")) {
    const env = strictOptimizationEnvironment({ CLEARRA_STANDARD_BAG_UNION_MEMO: "reference" }, adopted.axis, step);
    assert.equal(env.CLEARRA_STANDARD_BAG_UNION_MEMO, "compact");
  }
  assert.throws(() => parseStrictOptimizationArguments(cli("union-memo")), /Invalid/);
  assert.throws(() => parseStrictOptimizationArguments(cli("union-memo", "--baseline", "rust-buildup")), /Invalid/);
});

test("summary refuses extra BAAB or incomplete samples and gives correct pair results", () => {
  const samples = [..."ABBA"].map((arm, i) => ({ arm, search_ms: [100, 87, 85, 102][i], peak_rss_bytes: [90, 80, 84, 100][i] }));
  const result = summarizeStrictOptimization(samples);
  assert.deepEqual(result.search_median_ms, { A: 101, B: 86 });
  assert.equal(result.both_pairs_faster, true);
  assert.equal(result.sampled_peak_rss_change_bytes, -13);
  assert.throws(() => summarizeStrictOptimization([...samples, ...samples]), /four completed/);
});

test("validation binds real worker activity, exact-scope aggregate results and actual union selection", () => {
  const opts = parseStrictOptimizationArguments(cli("union-memo", "--baseline", "strict-frame-owner-v1"));
  const [, step] = strictOptimizationSchedule(opts, "/abs/example");
  const receipt = { outcome: { run_id: "run", duration_ms: 101,
    sampled_memory_peak: { source: "linux-sampled-process-group-rss", bytes: 1000 } } };
  const report = {
    schema: "clearra.placement-graph-benchmark.v1", valid: true, case: "empty-4L-P7P4-jstris180",
    strict_prefix_mode: "1", compiled_source_commit: source, binary_sha256: binSha, supervisor_run_id: "run",
    output_mode: "global-only", coverage_presentation: "coverage-summary-omit", coverage_capture_sink: "discard",
    solution_coverage: null, materialized_path_step_count: 0, materialized_solution_probability_count: 0,
    materialized_solution_coverage_count: 0, timing_ms: { search: 100 },
    summary: { workers_used: "8", parallel_active_workers: "8", packing_candidate_count: "29856840",
      total_build_order_nodes: "0", total_reachability_states: "0", count_complete: "true",
      probability_complete: "true", resource_truncated: "false",
      normalized_solution_set_hash: "cts1:2fcbc950acdffa31", standard_bag_union_memo_storage: "compact" },
    materialized_identities: { count: 456459, reported_total_count: 456459,
      covers_reported_family: true, sha256_ordered_ctk1_v1: orderedIdentities },
    coverage: { set_bit_count: 4233600, sha256_le_v1: queueUnion },
    cpu: { hard_limit: 8, expected_vcpus: null, use_all_logical_processors: true, worker_policy: "auto" },
    strict_prefix: { sealed: true, workers: 8, executing_workers: 8, candidates: 29856840 },
    other_process_configuration: { CLEARRA_STANDARD_BAG_MEMO: "reference",
      CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: "adaptive", CLEARRA_STANDARD_BAG_UNION_MEMO: "compact" },
  };
  assert.doesNotThrow(() => validateStrictOptimizationReport(report, opts, step, receipt, binSha));
  for (const patch of [{ parallel_active_workers: "2" }, { standard_bag_union_memo_storage: "reference" },
    { total_build_order_nodes: "1" }]) {
    assert.throws(() => validateStrictOptimizationReport({ ...report, summary: { ...report.summary, ...patch } },
      opts, step, receipt, binSha), /unexpected or incomplete/);
  }
  const fixed = parseStrictOptimizationArguments(cli("union-memo", "--baseline", "strict-frame-owner-v1"));
  const [ref] = strictOptimizationSchedule(fixed, "/abs/example");
  const baseline = { ...report,
    strict_prefix: { ...report.strict_prefix, unresolved_lanes: 124_505_112,
      reverse_relations: 8_528_265, ordered_kick_terms: 2_879_722_152 },
    summary: { ...report.summary, standard_bag_union_memo_storage: "reference" },
    other_process_configuration: { CLEARRA_STANDARD_BAG_MEMO: "reference",
      CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: "adaptive", CLEARRA_STANDARD_BAG_UNION_MEMO: "reference" } };
  assert.doesNotThrow(() => validateStrictOptimizationReport(baseline, fixed, ref, receipt, binSha));
  assert.throws(() => validateStrictOptimizationReport({ ...baseline, strict_prefix_mode: "0", strict_prefix: null },
    fixed, ref, receipt, binSha), /unexpected or incomplete/);
  assert.throws(() => validateStrictOptimizationReport(report, fixed, ref, receipt, binSha), /unexpected or incomplete/);
  assert.throws(() => validateStrictOptimizationReport({ ...baseline, strict_prefix: { ...baseline.strict_prefix,
    reverse_relations: 1 } }, fixed, ref, receipt, binSha), /unexpected or incomplete/);
  assert.throws(() => validateStrictOptimizationReport({ ...baseline, coverage: { ...baseline.coverage,
    sha256_le_v1: binSha } }, fixed, ref, receipt, binSha), /unexpected or incomplete/);
  assert.throws(() => validateStrictOptimizationReport({ ...baseline, other_process_configuration: { ...baseline.other_process_configuration,
    CLEARRA_STANDARD_BAG_UNION_MEMO: "compact" } }, fixed, ref, receipt, binSha), /unexpected or incomplete/);
});

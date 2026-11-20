import test from "node:test";
import assert from "node:assert/strict";
import { evaluatePlacementGraphAdoption } from "./placement-graph-adoption.mjs";

const AXIS = "movement_clear_certificates_mode";
const MINIMUM_AXIS = "minimum_domain_cutoff_mode";
const ORDER = "ABBABAAB";
const PEAK = "linux-sampled-process-group-rss";
const SOLUTIONS = 456_459;
const QUEUES = 4_233_600;
const BASE_NS = 1_795_134_317_314_219_000n;
const digest = (letter) => letter.repeat(64);

function run(index, arm, ms, bytes, outputMode = "global-only") {
  const run_id = `${BASE_NS + BigInt(index) * 2_000_000_000n}-5`;
  const benchmark = {
    schema: "clearra.placement-graph-benchmark.v1",
    case: "empty-4L-P7P4-jstris180", binary_sha256: digest("a"), supervisor_run_id: run_id,
    backend_contract: "native Rust",
    input: {
      board: "0", height: 4, placed_pieces: 10, supplied_pieces: 11,
      supply: "P7P4", original_supply_count: QUEUES, hold: "enabled-empty",
      rule: "jstris-180", count: "unique", queue_knowledge: "full-future-oracle",
    },
    cpu: { worker_policy: "auto", hard_limit: 9, use_all_logical_processors: true },
    summary: {
      count_complete: "true", probability_complete: "true", resource_truncated: "false",
      workers_used: "9", parallel_active_workers: "9", cpu_parallel_execution: "true",
      normalized_solution_set_hash: "cts1:2fcbc950acdffa31",
    },
    coverage: { set_bit_count: QUEUES, word_count: QUEUES / 64, sha256_le_v1: digest("b") },
    materialized_identities: {
      count: SOLUTIONS, covers_reported_family: true, reported_total_count: SOLUTIONS,
      sha256_ordered_ctk1_v1: digest("c"),
    },
    mode: "1", structural_mode: "1", static_influence_mode: "0", translation_interval_mode: "0",
    [AXIS]: arm === "A" ? "0" : "1", incremental_domain_mode: "0", first_kick_interval_mode: "0",
    conditional_order_mode: "0",
    terminal_identity_move_mode: "0",
    ordered_kick_interval_mode: "0",
    ilc_lazy_frame_mode: "0",
    no_entry_components_mode: "0",
    same_tile_parent_reuse_mode: "0",
    other_process_configuration: {}, output_mode: outputMode,
    timing_ms: { search: ms }, valid: true, validation_errors: [],
    solution_coverage: outputMode === "global-only" ? null : {
      sha256: digest("e"), solution_count: SOLUTIONS, total_weight: QUEUES, covered_weight: QUEUES, complete: true,
    },
  };
  const supervisor = {
    schema_id: "clearra.runtime-receipt.v2",
    stdout_tail: JSON.stringify(benchmark),
    outcome: {
      run_id, profile: "benchmark-search", return_code: 0, reason: "normal", error_code: null,
      supervisor_termination_reason: null, child_termination: { kind: "exit", code: 0 },
      duration_ms: 1_000, timeout_seconds: 1_800, automatic_retry: false, process_tree_stopped: true,
      peak_memory_bytes: bytes, kernel_memory_peak: null, sampled_memory_peak: { source: PEAK, bytes },
      admission: { capacity_basis: "physical", hard_limit_bytes: 16_384, snapshot: { physical_total_bytes: 16_384 } },
      memory_pressure_observed: false, memory_pressure_events: 0,
    },
  };
  return { benchmark, supervisor };
}

function rewriteReport(run, fn) {
  fn(run.benchmark);
  run.supervisor.stdout_tail = JSON.stringify(run.benchmark);
}

function setup({
  times = [100, 80, 79, 102, 81, 99, 101, 80],
  sizes = [1000, 997, 998, 1002, 998, 999, 1001, 1001],
  outputMode = "global-only",
} = {}) {
  return {
    schema: "clearra.placement-graph-adoption-input.v1",
    axis: AXIS,
    output_mode: outputMode,
    time_resolution_ms: 0.1,
    runs: Array.from(ORDER, (arm, index) => run(index, arm, times[index], sizes[index], outputMode)),
    mapping: {
      reference: run(8, "A", 120, 1000, "solution-coverage"),
      candidate: run(9, "B", 95, 998, "solution-coverage"),
      comparison: {
        schema: "clearra.placement-graph-solution-coverage-comparison.v1",
        universe: "p7p4", equivalent: true, complete: true, solution_count: SOLUTIONS,
        left_sha256: digest("e"), right_sha256: digest("e"), total_weight: QUEUES, covered_weight: QUEUES,
      },
    },
  };
}

test("time improvement and non-regressing sampled peak qualify with separate exact mapping", () => {
  const result = evaluatePlacementGraphAdoption(setup());
  assert.equal(result.status, "adoptable");
  assert.equal(result.release_authority, false);
  assert.equal(result.scope, "global-only-timings-with-separate-exact-per-solution-qualification");
  assert.deepEqual(result.time.median_ms, { baseline: 100.5, candidate: 80 });
  assert.equal(result.memory.source, PEAK);
  assert.equal(result.memory.baseline_mad_bytes, 1);
});

test("pooled median cannot override a lost reverse block or a sub-resolution improvement", () => {
  const reverseLoss = evaluatePlacementGraphAdoption(setup({
    times: [101, 10, 11, 100, 102, 99, 100, 103],
  }));
  assert.ok(reverseLoss.time.median_ms.candidate < reverseLoss.time.median_ms.baseline);
  assert.equal(reverseLoss.status, "not-adoptable");
  const noise = setup();
  noise.time_resolution_ms = 21;
  assert.equal(evaluatePlacementGraphAdoption(noise).status, "not-adoptable");
});

test("one treatment peak tail beyond baseline MAD blocks adoption despite lower median", () => {
  const result = evaluatePlacementGraphAdoption(setup({
    sizes: [1000, 950, 951, 1002, 950, 999, 1001, 1004],
  }));
  assert.ok(result.memory.median_bytes.candidate < result.memory.median_bytes.baseline);
  assert.equal(result.memory.nonregressing, false);
  assert.equal(result.status, "not-adoptable");
});

test("kernel/sampled measurement families cannot be mixed", () => {
  const evidence = setup();
  const outcome = evidence.runs[4].supervisor.outcome;
  outcome.kernel_memory_peak = { bytes: outcome.peak_memory_bytes, source: "windows-job-object" };
  outcome.sampled_memory_peak = null;
  assert.throws(() => evaluatePlacementGraphAdoption(evidence), /different sampled\/kernel peak/u);
});

test("censored SIGKILL is not attributed to OOM or used as performance evidence", () => {
  const evidence = setup();
  const run = evidence.runs[7];
  run.benchmark = null;
  run.supervisor.stdout_tail = "";
  Object.assign(run.supervisor.outcome, {
    reason: "signal", return_code: 137, child_termination: { kind: "unix-signal", signal: 9, core_dumped: false },
  });
  const result = evaluatePlacementGraphAdoption(evidence);
  assert.equal(result.status, "inconclusive");
  assert.equal(result.observations[7].state, "censored");
  assert.equal(result.observations[7].reason, "signal");
  assert.equal(result.time, undefined);

  // An explicit exit(137) is not an OS signal receipt.
  Object.assign(run.supervisor.outcome, { reason: "nonzero", child_termination: { kind: "exit", code: 137 } });
  const failed = evaluatePlacementGraphAdoption(evidence);
  assert.equal(failed.status, "failed");
  assert.equal(failed.observations[7].state, "failed");
});

test("completed runs observed under memory pressure are confounded, not successes", () => {
  const evidence = setup();
  evidence.runs[2].supervisor.outcome.memory_pressure_observed = true;
  const result = evaluatePlacementGraphAdoption(evidence);
  assert.equal(result.status, "inconclusive");
  assert.equal(result.observations[2].state, "confounded");
});

test("run order, output identity and reported operation are enforced", () => {
  const overlap = setup();
  overlap.runs[0].supervisor.outcome.duration_ms = 3000;
  assert.throws(() => evaluatePlacementGraphAdoption(overlap), /must not overlap/u);
  const flag = setup();
  rewriteReport(flag.runs[3], (report) => { report[AXIS] = "1"; });
  assert.throws(() => evaluatePlacementGraphAdoption(flag), /wrong A\/B mode order/u);
  const extraAxis = setup();
  rewriteReport(extraAxis.runs[1], (report) => { report.static_influence_mode = "1"; });
  assert.throws(() => evaluatePlacementGraphAdoption(extraAxis), /fixed axes/u);
  const nonemptyOnly = setup();
  nonemptyOnly.axis = "incremental_domain_mode";
  assert.throws(() => evaluatePlacementGraphAdoption(nonemptyOnly), /one known axis/u);
  const apdp = setup();
  rewriteReport(apdp.runs[1], (report) => { report.minimum_domain_cutoff_mode = "1"; });
  assert.throws(() => evaluatePlacementGraphAdoption(apdp), /non-neutral axis minimum_domain_cutoff_mode/u);
  const changedMode = setup();
  rewriteReport(changedMode.runs[1], (report) => { report.output_mode = "solution-coverage"; });
  assert.throws(() => evaluatePlacementGraphAdoption(changedMode), /unbound benchmark/u);
  const tamper = setup();
  tamper.runs[1].benchmark.timing_ms.search = 66;
  assert.throws(() => evaluatePlacementGraphAdoption(tamper), /unbound benchmark/u);
});

test("same aggregates do not substitute for exact per-solution comparison", () => {
  const evidence = setup();
  evidence.mapping.comparison.equivalent = false;
  assert.throws(() => evaluatePlacementGraphAdoption(evidence), /comparator evidence/u);
  evidence.mapping.comparison.equivalent = true;
  evidence.mapping.comparison.right_sha256 = digest("d");
  assert.throws(() => evaluatePlacementGraphAdoption(evidence), /bind both artifacts/u);
});

test("timed exact-mapping files must be bound to the compared canonical artifacts", () => {
  const evidence = setup({ outputMode: "solution-coverage" });
  const result = evaluatePlacementGraphAdoption(evidence);
  assert.equal(result.status, "adoptable");
  assert.equal(result.scope, "measured-artifacts-bound-to-exact-per-solution-comparison");
  rewriteReport(evidence.runs[2], (report) => { report.solution_coverage.sha256 = digest("f"); });
  assert.throws(() => evaluatePlacementGraphAdoption(evidence), /not the verified canonical mapping/u);
});

function setupMinimum() {
  const evidence = setup();
  evidence.axis = MINIMUM_AXIS;
  evidence.experiment = "minimum-domain-cutoff";
  evidence.custody = {
    source_commit: "a".repeat(40), binary_sha256: digest("a"),
    manager_sha256: digest("b"), host_fingerprint: digest("c"),
  };
  evidence.mapping.reference = run(-3, "A", 120, 1000, "solution-coverage");
  evidence.mapping.candidate = run(-2, "B", 95, 998, "solution-coverage");
  evidence.mapping.comparison.covered_pairs = QUEUES;
  const bind = (tracked, arm, command) => {
    tracked.supervisor.outcome.producer = "benchmark";
    tracked.supervisor.outcome.command = command;
    rewriteReport(tracked, (report) => {
      report.compiled_source_commit = evidence.custody.source_commit;
      report.cpu.expected_vcpus = null;
      for (const name of [
        "mode", "structural_mode", AXIS, "minimum_domain_work_mode",
        "ordered_kick_guard_reuse_mode", "geometric_edge_reuse_mode",
      ]) report[name] = "0";
      report[MINIMUM_AXIS] = arm === "A" ? "0" : "1";
      report.geometry_domain = {
        completed_families: 2, minimum_compiles: 2, minimum_empty_origin: 2,
        minimum_resource_authoritative: 0, minimum_below_seven_pieces: 0,
        advanced_full_compiles: 0, advanced_incremental_compiles: 0, minimum_profiled_compiles: 0,
        minimum_incidence_rows_scanned: 0, minimum_incidence_rows_skipped: 0, minimum_cells_cut_off: 0,
      };
      if (report.solution_coverage) report.solution_coverage.covered_pairs = QUEUES;
    });
  };
  evidence.runs.forEach((tracked, index) => bind(tracked, ORDER[index], ["placement_graph_bench"]));
  bind(evidence.mapping.reference, "A", ["placement_graph_bench", "--coverage-slot", "proof-a"]);
  bind(evidence.mapping.candidate, "B", ["placement_graph_bench", "--coverage-slot", "proof-b"]);
  const comparator = run(-1, "A", 100, 1000);
  comparator.supervisor.stdout_tail = JSON.stringify(evidence.mapping.comparison);
  comparator.supervisor.outcome.producer = "benchmark";
  comparator.supervisor.outcome.command = ["placement_graph_bench", "--compare-coverage", "proof-a.bin", "proof-b.bin"];
  evidence.mapping.comparison_supervisor = comparator.supervisor;
  evidence.mapping.proof = {
    schema: "clearra.placement-graph-coverage-proof.v1", state: "completed-exact-per-solution",
    source_binding: "compile-time-and-checkout", case: "empty-4L-P7P4-jstris180",
    axis: MINIMUM_AXIS, experiment: evidence.experiment, ...evidence.custody,
    release_authority: false, timing_adoption_authority: false,
    observations: [evidence.mapping.reference, evidence.mapping.candidate, comparator].map(({ supervisor }, index) => {
      const name = ["reference", "candidate", "comparison"][index];
      return { name, state: "completed", run_id: supervisor.outcome.run_id,
        supervisor: name + "-runtime.json", command: supervisor.outcome.command };
    }),
  };
  return evidence;
}

test("minimum-domain evaluation demands an unprofiled empty route, neutral other axes, and exact custody", () => {
  const complete = setupMinimum();
  const result = evaluatePlacementGraphAdoption(complete);
  assert.equal(result.status, "adoptable");
  assert.equal(result.experiment, "minimum-domain-cutoff");
  assert.equal(result.custody.host_fingerprint, complete.custody.host_fingerprint);
  for (const corrupt of [
    (evidence) => { evidence.mapping.proof.host_fingerprint = digest("d"); },
    (evidence) => { evidence.mapping.comparison_supervisor.stdout_tail = "{}"; },
    (evidence) => { evidence.mapping.comparison_supervisor.outcome.run_id = evidence.runs[0].supervisor.outcome.run_id; },
    (evidence) => { evidence.runs[3].supervisor.outcome.command = ["other-binary"]; },
    (evidence) => rewriteReport(evidence.runs[0], (report) => { report.compiled_source_commit = null; }),
    (evidence) => rewriteReport(evidence.runs[1], (report) => { report.geometry_domain.minimum_profiled_compiles = 1; }),
    (evidence) => rewriteReport(evidence.runs[2], (report) => { report.minimum_domain_work_mode = "1"; }),
    (evidence) => rewriteReport(evidence.runs[3], (report) => { report[AXIS] = "1"; }),
    (evidence) => rewriteReport(evidence.runs[4], (report) => { report.cpu.expected_vcpus = 9; }),
  ]) {
    const tampered = setupMinimum();
    corrupt(tampered);
    assert.throws(() => evaluatePlacementGraphAdoption(tampered));
  }
});

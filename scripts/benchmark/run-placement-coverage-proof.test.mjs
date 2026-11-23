import assert from "node:assert/strict";
import { join } from "node:path";
import test from "node:test";
import { parseAdoptionArguments } from "./run-placement-graph-adoption.mjs";
import {
  assertExactComparison, benchmarkEnvironment, classifyProofRuntime, collectPlacementCoverageProof, composeRuntimeArgs,
  parseProofArguments, placementProofDiskPlan, proofSchedule, validatedProofArm,
} from "./run-placement-coverage-proof.mjs";

const source = "a".repeat(40);
const binarySha = "b".repeat(64);
const artifacts = join("test-artifacts", "coverage");
const binary = join("test-build", "placement_graph_bench");
const cli = (key, rest = []) => ["--case", key, "--id", "proof-42", "--source-sha", source, ...rest];
const minimumArgs = ["--experiment", "minimum-domain-cutoff"];
const operationMaskArgs = ["--experiment", "operation-masks"];
const strictPrefixArgs = ["--experiment", "strict-prefix"];
const strictPrefixUnionArgs = ["--experiment", "strict-prefix-union-memo"];
const [reference, candidate, comparison] = proofSchedule(parseProofArguments(cli("p7p4")), binary, artifacts);

function runtime(step, index) {
  return {
    schema_id: "clearra.runtime-receipt.v2",
    outcome: {
      run_id: String(1_800_000_000_000_000_000n + BigInt(index) * 1_000_000_000n) + "-100",
      profile: "benchmark-search", producer: "benchmark",
      timeout_seconds: step.timeout, automatic_retry: false, command: step.command,
      reason: "normal", return_code: 0, process_tree_stopped: true,
      error_code: null, supervisor_termination_reason: null, child_termination: { kind: "exit", code: 0 },
      memory_pressure_observed: false, memory_pressure_events: 0,
    },
  };
}

function armReport(step, receipt, fixture) {
  const minimum = step.experiment?.name === "minimum-domain-cutoff";
  const operationMask = step.experiment?.name === "operation-masks";
  const strictPrefixUnion = step.experiment?.name === "strict-prefix-union-memo";
  const strictPrefix = step.experiment?.name === "strict-prefix" || strictPrefixUnion;
  const strictPrefixMode = strictPrefixUnion ? "1" : step.value;
  const unionStorage = step.value === "0" ? "reference" : "compact";
  const isolatedAxis = minimum || operationMask || strictPrefix;
  const candidates = fixture.candidates ?? 29_856_840;
  return {
    schema: "clearra.placement-graph-benchmark.v1", case: fixture.name, valid: true, validation_errors: [],
    binary_sha256: binarySha, supervisor_run_id: receipt.outcome.run_id, output_mode: "solution-coverage",
    movement_clear_certificates_mode: isolatedAxis ? "0" : step.value,
    mode: isolatedAxis ? "0" : "1", structural_mode: isolatedAxis ? "0" : "1",
    static_influence_mode: "0", translation_interval_mode: "0", incremental_domain_mode: "0",
    first_kick_interval_mode: "0",
    conditional_order_mode: "0",
    terminal_identity_move_mode: "0",
    ordered_kick_interval_mode: "0",
    no_entry_components_mode: "0",
    same_tile_parent_reuse_mode: "0",
    ...(minimum ? {
      compiled_source_commit: source,
      minimum_domain_cutoff_mode: step.value, minimum_domain_work_mode: "0",
      ordered_kick_guard_reuse_mode: "0", geometric_edge_reuse_mode: "0", ilc_lazy_frame_mode: "0",
      geometry_domain: {
        completed_families: 100, minimum_compiles: 42, minimum_empty_origin: 42,
        minimum_resource_authoritative: 0, minimum_below_seven_pieces: 0,
        advanced_full_compiles: 0, advanced_incremental_compiles: 0, minimum_profiled_compiles: 0,
        minimum_incidence_rows_scanned: 0, minimum_incidence_rows_skipped: 0, minimum_cells_cut_off: 0,
      },
    } : {}),
    ...(operationMask ? {
      compiled_source_commit: source, operation_mask_mode: step.value,
      minimum_domain_cutoff_mode: "0", minimum_domain_work_mode: "0",
      ordered_kick_guard_reuse_mode: "0", geometric_edge_reuse_mode: "0", ilc_lazy_frame_mode: "0",
      operation_masks: {
        workers: 12, executing_workers: 12, requested_workers: step.value === "1" ? 12 : 0,
        graph_passes: step.value === "0" ? 456_459 : 0,
        mask_passes: step.value === "1" ? 456_459 : 0, mask_live_passes: step.value === "1" ? 456_459 : 0,
        visited_subsets: 400_000_000, selected_edges: 2_147_483_648,
        graph_node_bytes: step.value === "0" ? 123_000 : 0,
        graph_edge_bytes: step.value === "0" ? 321_000 : 0,
        graph_piece_edge_bytes: step.value === "0" ? 34_000 : 0,
        mask_bytes: step.value === "1" ? 51_000 : 0,
        graph_projection_bytes: step.value === "0" ? 500_000 : 60_000,
      },
      operation_mask_representative: Array.from("IOTSZJLIOT", (kind, index) =>
        [kind, index % 4, index, 0, "use-current", index === 9 ? 4 : 0]),
    } : {}),
    ...(strictPrefix ? {
      compiled_source_commit: source, strict_prefix_mode: strictPrefixMode,
      minimum_domain_cutoff_mode: "0", minimum_domain_work_mode: "0",
      ordered_kick_guard_reuse_mode: "0", geometric_edge_reuse_mode: "0", ilc_lazy_frame_mode: "0",
      coverage_presentation: "coverage-summary-omit",
      coverage_capture_sink: "exact-spool",
      materialized_path_step_count: 0,
      materialized_solution_coverage_count: 0,
      materialized_solution_probability_count: 0,
      strict_prefix: strictPrefixMode === "0" ? null : {
        workers: 12, executing_workers: 12, candidates,
        reverse_relations: 2, unresolved_lanes: 100, pruned_lanes: 40,
        pose_work: 200, worklist_pops: 10, pose_domains: 70,
        ordered_kick_terms: 60, peak_relation_logical_bytes: 1_000, sealed: true,
      },
    } : {}),
    input: {
      board: fixture.board, height: fixture.height, placed_pieces: fixture.placed, supplied_pieces: fixture.supplied,
      supply: fixture.supply, original_supply_count: fixture.queues, hold: fixture.hold,
      rule: fixture.rule ?? "jstris-180", count: "unique", queue_knowledge: "full-future-oracle",
    },
    cpu: { worker_policy: "auto", use_all_logical_processors: true, expected_vcpus: null, hard_limit: 12 },
    summary: {
      count_complete: "true", probability_complete: "true", resource_truncated: "false",
      normalized_solution_set_hash: "cts1:2fcbc950acdffa31", workers_used: "12",
      parallel_active_workers: "12", cpu_parallel_execution: "true",
      ...(operationMask ? { total_build_order_nodes: "400000000", representative_pattern_id: "0",
        worker_graph_projection_retained_bytes: step.value === "0" ? "500000" : "60000" } : {}),
      ...(strictPrefix ? {
        packing_candidate_count: String(candidates),
        representative_pattern_id: "", representative_candidate_id: "",
        total_build_order_nodes: strictPrefixMode === "0" ? "400000000" : "0",
        total_reachability_states: strictPrefixMode === "0" ? "321321321" : "0",
        ...(strictPrefixUnion ? { standard_bag_union_memo_storage: unionStorage } : {}),
      } : {}),
    },
    materialized_identities: {
      count: fixture.solutions, reported_total_count: fixture.solutions, covers_reported_family: true,
      sha256_ordered_ctk1_v1: "0".repeat(64),
    },
    coverage: { set_bit_count: fixture.queues, word_count: Math.ceil(fixture.queues / 64), sha256_le_v1: "1".repeat(64) },
    solution_coverage: {
      complete: true, sha256: step.name === "reference" ? "c".repeat(64) : "d".repeat(64),
      solution_count: fixture.solutions, total_weight: fixture.queues, covered_weight: fixture.queues,
      covered_pairs: fixture.pairs ?? fixture.queues,
      ...(strictPrefix ? {
        output_bytes: 80_000, output_limit_bytes: 3 * 1024 ** 3,
        spool: { spill_write_limit_bytes: 5 * 1024 ** 3, spill_bytes_written: 80_000, peak_live_spill_bytes: 50_000 },
      } : {}),
    },
    backend_contract: "native-host Rust",
    other_process_configuration: strictPrefixUnion ? {
      CLEARRA_STANDARD_BAG_MEMO: "reference", CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: "adaptive",
      CLEARRA_STANDARD_BAG_UNION_MEMO: unionStorage,
    } : {},
  };
}

function comparisonReport(fixture) {
  return {
    schema: "clearra.placement-graph-solution-coverage-comparison.v1",
    equivalent: true, complete: true, universe: fixture.universe, solution_count: fixture.solutions,
    total_weight: fixture.queues, covered_weight: fixture.queues, covered_pairs: fixture.pairs ?? fixture.queues,
    left_sha256: "c".repeat(64), right_sha256: "d".repeat(64),
  };
}

function mockIo(options, steps, change = (_, input) => input) {
  const invoked = [];
  const writes = new Map();
  let active = false;
  return {
    invoked, writes,
    checkInputs() {},
    async execute(step) {
      assert.equal(active, false, "phases must not overlap");
      active = true;
      invoked.push(step.name);
      const receipt = runtime(step, steps.indexOf(step));
      const report = step.name === "comparison" ? comparisonReport(options.fixture) : armReport(step, receipt, options.fixture);
      const execution = change(step, { code: 0, signal: null, receipt, report });
      execution.stdout = JSON.stringify(execution.report);
      execution.receipt.stdout_tail = execution.stdout;
      await Promise.resolve();
      active = false;
      return execution;
    },
    receipt: (e) => e.receipt,
    assertArtifact: (path) => assert.ok(path.endsWith("solution-coverage.clrcov")),
    write: (name, value) => {
      assert.equal(writes.has(name), false, "evidence may not be overwritten");
      writes.set(name, structuredClone(value));
    },
  };
}

test("unlimited parsing preserves explicit finite budgets and rejects malformed requests", () => {
  assert.throws(() => parseProofArguments([]), /Usage/u);
  assert.throws(() => parseProofArguments(["--case", "large", "--id", "a", "--source-sha", source]), /Usage/u);
  assert.throws(() => parseProofArguments(["--case", "small", "--id", "..", "--source-sha", source]), /Usage/u);
  assert.throws(() => parseProofArguments(cli("small", ["--timeout-seconds", "301"])), /finite timeout/u);
  assert.throws(() => parseProofArguments(cli("p7p4", ["--case", "p7p4"])), /Distinct/u);
  assert.equal(parseProofArguments(cli("p7p4")).timeout, 0);
  assert.equal(parseProofArguments(cli("p7p4", ["--timeout-seconds", "0"])).timeout, 0);
  assert.equal(parseProofArguments(cli("small", ["--timeout-seconds", "12"])).timeout, 12);
});

test("capture order holds all other axes fixed and uses separate new slots before comparison", () => {
  assert.deepEqual([reference.value, candidate.value], ["0", "1"]);
  assert.deepEqual(reference.args, ["--coverage-slot", "proof-42-a"]);
  assert.deepEqual(candidate.args, ["--coverage-slot", "proof-42-b"]);
  assert.deepEqual(comparison.args, ["--compare-coverage", reference.artifact, candidate.artifact]);
  assert.deepEqual([reference.timeout, candidate.timeout, comparison.timeout], [0, 0, 0]);
  assert(composeRuntimeArgs("/workspace", reference).includes("--cancel-on-stdin-eof"));
  assert.deepEqual(proofSchedule(parseProofArguments(cli("small")), binary, artifacts)[0].args,
    ["--small", "--coverage-slot", "proof-42-a"]);
});

test("supervisor result binds the actual command and distinguishes SIGKILL from a controlled memory reason", () => {
  const receipt = runtime(reference, 0);
  assert.equal(classifyProofRuntime(receipt, { code: 0 }, reference), "completed");
  assert.throws(() => classifyProofRuntime(receipt, { code: 0 }, candidate), /does not bind/u);
  receipt.outcome.child_termination = { kind: "unix-signal", signal: 9 };
  receipt.outcome.reason = "signal";
  receipt.outcome.return_code = 137;
  assert.equal(classifyProofRuntime(receipt, { code: 137 }, reference), "censored");
  receipt.outcome.reason = "nonzero";
  receipt.outcome.child_termination = { kind: "exit", code: 137 };
  assert.equal(classifyProofRuntime(receipt, { code: 137 }, reference), "failed");
});

test("capture qualification catches worker substitutions even when counts match", () => {
  const options = parseProofArguments(cli("p7p4"));
  const receipt = runtime(reference, 0);
  const report = armReport(reference, receipt, options.fixture);
  report.summary.workers_used = "11";
  assert.throws(() => validatedProofArm(report, receipt, reference, options, binarySha), /automatic parallel/u);
});

test("movement-clear proof neutralizes APDP, kick and geometric edge follow-on modes", () => {
  const input = {
    clearra_minimum_domain_cutoff: "1", CLEARRA_MINIMUM_DOMAIN_WORK: "1", CLEARRA_ORDERED_KICK_GUARD_REUSE: "1", CLEARRA_GEOMETRIC_EDGE_REUSE: "1",
  };
  const environment = benchmarkEnvironment(input, reference);
  assert.equal(environment.CLEARRA_MINIMUM_DOMAIN_CUTOFF, "0");
  assert.equal(environment.CLEARRA_MINIMUM_DOMAIN_WORK, "0");
  assert.equal(environment.CLEARRA_ORDERED_KICK_GUARD_REUSE, "0");
  assert.equal(environment.CLEARRA_GEOMETRIC_EDGE_REUSE, "0");
  assert.equal(environment.clearra_minimum_domain_cutoff, undefined);
  const options = parseProofArguments(cli("p7p4"));
  const receipt = runtime(reference, 0);
  for (const axis of ["minimum_domain_cutoff_mode", "minimum_domain_work_mode", "ordered_kick_guard_reuse_mode", "geometric_edge_reuse_mode"]) {
    const report = armReport(reference, receipt, options.fixture);
    report[axis] = "1";
    assert.throws(() => validatedProofArm(report, receipt, reference, options, binarySha),
      /unexpected source, mode, or coverage output/u);
  }
});

test("explicit minimum-domain proof is P7P4-only and neutralizes every other experiment", () => {
  assert.throws(() => parseProofArguments(cli("small", minimumArgs)), /requires the empty-origin P7P4/u);
  assert.throws(() => parseProofArguments(cli("p7p4", ["--experiment", "unregistered"])), /Unknown placement experiment/u);
  const options = parseProofArguments(cli("p7p4", minimumArgs));
  const steps = proofSchedule(options, binary, artifacts);
  assert.deepEqual(steps.map((step) => step.experiment), [options.experiment, options.experiment, options.experiment]);
  assert.equal(options.experiment.axis, "minimum_domain_cutoff_mode");
  const inherited = {
    CLEARRA_PLACEMENT_DEPENDENCY: "1", CLEARRA_STRUCTURAL_ORDER: "1",
    clearra_minimum_domain_cutoff: "1", clearra_movement_clear_certificates: "1", CLEARRA_MINIMUM_DOMAIN_WORK: "1",
    CLEARRA_INCREMENTAL_DOMAIN: "1", CLEARRA_GEOMETRIC_EDGE_REUSE: "1", VALID_UNRELATED_SETTING: "1",
  };
  for (const [step, cutoff] of steps.map((step, index) => [step, index === 1 ? "1" : "0"])) {
    const environment = benchmarkEnvironment(inherited, step);
    assert.equal(environment.CLEARRA_MINIMUM_DOMAIN_CUTOFF, cutoff);
    for (const key of [
      "CLEARRA_PLACEMENT_DEPENDENCY", "CLEARRA_STRUCTURAL_ORDER", "CLEARRA_MOVEMENT_CLEAR_CERTIFICATES",
      "CLEARRA_MINIMUM_DOMAIN_WORK", "CLEARRA_INCREMENTAL_DOMAIN", "CLEARRA_GEOMETRIC_EDGE_REUSE",
    ]) assert.equal(environment[key], "0");
    assert.equal(environment.clearra_minimum_domain_cutoff, undefined);
    assert.equal(environment.clearra_movement_clear_certificates, undefined);
    assert.equal(environment.VALID_UNRELATED_SETTING, "1");
  }
});

test("minimum proof rejects mixed axes, unknown modes, older binding and false route receipts", () => {
  const options = parseProofArguments(cli("p7p4", minimumArgs));
  const [, cutoff] = proofSchedule(options, binary, artifacts);
  const receipt = runtime(cutoff, 1);
  const report = armReport(cutoff, receipt, options.fixture);
  const validate = (changes = {}, step = cutoff) =>
    validatedProofArm({ ...report, ...changes }, receipt, step, options, binarySha);
  assert.doesNotThrow(() => validate());
  assert.throws(() => validate({}, candidate), /wrong scheduled proof experiment/u);
  for (const bad of [
    { minimum_domain_cutoff_mode: undefined }, { minimum_domain_cutoff_mode: "0" },
    { minimum_domain_work_mode: "1" }, { mode: "1" }, { structural_mode: "1" },
    { movement_clear_certificates_mode: "1" }, { geometric_edge_reuse_mode: "1" },
    { hypothetical_future_mode: "0" },
  ]) assert.throws(() => validate(bad), /unexpected source, mode, or coverage output/u);
  assert.throws(() => validate({ compiled_source_commit: null }), /compile-time source/u);
  assert.throws(() => validate({ coverage: { ...report.coverage, word_count: 12 } }), /incomplete or incorrect/u);
  for (const value of [
    null, { minimum_empty_origin: 0 }, { minimum_empty_origin: 41 }, { minimum_empty_origin: 43 },
    { minimum_resource_authoritative: 1 }, { minimum_below_seven_pieces: 1 }, { advanced_full_compiles: 1 },
    { minimum_profiled_compiles: 1 }, { minimum_incidence_rows_skipped: 1 },
  ]) {
    assert.throws(() => validate({ geometry_domain: value && { ...report.geometry_domain, ...value } }),
      /active unprofiled empty-origin route/u);
  }
});

test("minimum comparison binds both captured arms without acquiring adoption authority", async () => {
  const options = parseProofArguments(cli("p7p4", minimumArgs));
  const steps = proofSchedule(options, binary, artifacts);
  const io = mockIo(options, steps);
  const proof = await collectPlacementCoverageProof(options, binarySha, steps, io);
  assert.deepEqual(io.invoked, ["reference", "candidate", "comparison"]);
  assert.equal(proof.experiment, "minimum-domain-cutoff");
  assert.equal(proof.axis, "minimum_domain_cutoff_mode");
  assert.equal(proof.source_binding, "compile-time-and-checkout");
  assert.equal(proof.state, "completed-exact-per-solution");
  assert.equal(proof.release_authority, false);
  assert.equal(proof.timing_adoption_authority, false);
  assert.ok(io.writes.has("comparison-runtime.json"));

  const bad = mockIo(options, steps, (step, execution) => {
    if (step.name === "candidate") execution.report.minimum_domain_cutoff_mode = "0";
    return execution;
  });
  const rejected = await collectPlacementCoverageProof(options, binarySha, steps, bad);
  assert.deepEqual(bad.invoked, ["reference", "candidate"]);
  assert.equal(rejected.state, "failed");
  assert.equal(rejected.mapping, null);
});

test("operation-mask proof opts in both roles but leaves the flag unset in historical lanes", () => {
  assert.throws(() => parseProofArguments(cli("small", operationMaskArgs)), /requires the empty-origin P7P4/u);
  assert.throws(() => parseAdoptionArguments(["--id", "adopt", "--proof-id", "proof-incompatible",
    "--source-sha", source, ...operationMaskArgs]), /Unknown placement experiment/u);
  const options = parseProofArguments(cli("p7p4", operationMaskArgs));
  const steps = proofSchedule(options, binary, artifacts);
  assert.deepEqual(steps.map(({ value }) => value), ["0", "1", "0"]);
  assert.equal(options.experiment.axis, "operation_mask_mode");
  const hostile = {
    clearra_operation_masks: "1", CLEARRA_OPERATION_MASKS: "1",
    CLEARRA_PLACEMENT_DEPENDENCY: "1", CLEARRA_STRUCTURAL_ORDER: "1",
    CLEARRA_MINIMUM_DOMAIN_CUTOFF: "1", CLEARRA_MINIMUM_DOMAIN_WORK: "1",
    CLEARRA_MOVEMENT_CLEAR_CERTIFICATES: "1", KEEP_SETTING: "1",
  };
  for (const step of steps.slice(0, 2)) {
    const environment = benchmarkEnvironment(hostile, step);
    assert.equal(environment.CLEARRA_OPERATION_MASKS, step.value);
    assert.equal(environment.clearra_operation_masks, undefined);
    assert.equal(environment.KEEP_SETTING, "1");
    assert.ok(Object.entries(environment).every(([name, value]) => !name.startsWith("CLEARRA_") ||
      name === "CLEARRA_OPERATION_MASKS" || value === "0"));
  }
  for (const other of [reference, proofSchedule(parseProofArguments(cli("p7p4", minimumArgs)), binary, artifacts)[0]]) {
    const environment = benchmarkEnvironment(hostile, other);
    assert.equal(Object.hasOwn(environment, "CLEARRA_OPERATION_MASKS"), false);
    assert.equal(Object.hasOwn(environment, "clearra_operation_masks"), false);
  }
});

test("operation-mask report enforces real concrete/mask activity, isolated source, and replay", () => {
  const options = parseProofArguments(cli("p7p4", operationMaskArgs));
  const [ref, mask] = proofSchedule(options, binary, artifacts);
  const receipt = runtime(mask, 1);
  const result = armReport(mask, receipt, options.fixture);
  const validate = (report = result) => validatedProofArm(report, receipt, mask, options, binarySha);
  assert.doesNotThrow(() => validate());
  assert.throws(() => validate({ ...result, compiled_source_commit: null }), /compile-time source/u);
  for (const patch of [
    { operation_mask_mode: undefined }, { operation_mask_mode: "0" }, { mode: "1" }, { structural_mode: "1" },
    { movement_clear_certificates_mode: "1" }, { minimum_domain_cutoff_mode: "1" }, { minimum_domain_work_mode: "1" },
    { unrecognized_operation_shortcut_mode: "0" },
  ]) assert.throws(() => validate({ ...result, ...patch }), /unexpected source, mode, or coverage output/u);
  for (const patch of [
    { executing_workers: 1 }, { workers: 10 }, { requested_workers: 0 }, { mask_live_passes: 0 },
    { graph_passes: 1 }, { graph_node_bytes: 1 },
    { mask_bytes: 0 }, { selected_edges: 0 }, { visited_subsets: 1 }, { graph_projection_bytes: 59_999 },
  ]) assert.throws(() => validate({ ...result, operation_masks: { ...result.operation_masks, ...patch } }), /operation-mask/u);
  const referenceReceipt = runtime(ref, 0);
  const control = armReport(ref, referenceReceipt, options.fixture);
  assert.doesNotThrow(() => validatedProofArm(control, referenceReceipt, ref, options, binarySha));
  assert.throws(() => validatedProofArm({
    ...control, operation_masks: { ...control.operation_masks, mask_passes: 1 },
  }, referenceReceipt, ref, options, binarySha), /control was not purely concrete/u);
  assert.throws(() => validate({ ...result, operation_mask_representative: [] }), /concrete representative replay/u);
  assert.throws(() => validate({ ...result, summary: { ...result.summary, representative_pattern_id: "" } }),
    /concrete representative replay/u);
  assert.throws(() => validatedProofArm({ ...result, solution_coverage: null, output_mode: "global-only" },
    receipt, mask, options, binarySha, "global-only"), /unexpected source, mode, or coverage output/u);
});

test("historical movement-clear and minimum proofs reject explicit operation-mask control as well as treatment", () => {
  for (const args of [[], minimumArgs]) {
    const options = parseProofArguments(cli("p7p4", args));
    const [step] = proofSchedule(options, binary, artifacts);
    const receipt = runtime(step, 0);
    const historical = armReport(step, receipt, options.fixture);
    assert.doesNotThrow(() => validatedProofArm(historical, receipt, step, options, binarySha));
    for (const mode of ["0", "1"]) {
      assert.throws(() => validatedProofArm({ ...historical, operation_mask_mode: mode }, receipt, step, options, binarySha),
        /unexpected source, mode, or coverage output/u);
    }
  }
});

test("operation-mask proof compares equal traversal work and concrete replay, then exact queue streams", async () => {
  const options = parseProofArguments(cli("p7p4", operationMaskArgs));
  const steps = proofSchedule(options, binary, artifacts);
  const io = mockIo(options, steps);
  const proof = await collectPlacementCoverageProof(options, binarySha, steps, io);
  assert.deepEqual(io.invoked, ["reference", "candidate", "comparison"]);
  assert.equal(proof.experiment, "operation-masks");
  assert.equal(proof.axis, "operation_mask_mode");
  assert.equal(proof.state, "completed-exact-per-solution");
  assert.equal(proof.source_binding, "compile-time-and-checkout");
  assert.equal(proof.release_authority, false);
  assert.equal(proof.timing_adoption_authority, false);
  assert.deepEqual(proof.mapping, {
    comparison: "comparison.json",
    reference: { benchmark: "reference.json", supervisor: "reference-runtime.json" },
    candidate: { benchmark: "candidate.json", supervisor: "candidate-runtime.json" },
  });
  assert.ok(io.writes.has("comparison-runtime.json"));
  for (const mutate of [
    (report) => { report.operation_masks.selected_edges += 1; },
    (report) => { report.operation_masks.mask_passes += 1; },
    (report) => { report.operation_mask_representative[0][2] += 1; },
    (report) => { report.summary.representative_pattern_id = "1"; },
  ]) {
    const bad = mockIo(options, steps, (step, execution) => {
      if (step.name === "candidate") mutate(execution.report);
      return execution;
    });
    const rejected = await collectPlacementCoverageProof(options, binarySha, steps, bad);
    assert.deepEqual(bad.invoked, ["reference", "candidate"]);
    assert.equal(rejected.state, "failed");
    assert.equal(rejected.mapping, null);
    assert.match(rejected.error, /agree on exact fixture/u);
  }
  const badStream = mockIo(options, steps, (step, execution) => {
    if (step.name === "comparison") execution.report.right_sha256 = execution.report.left_sha256;
    return execution;
  });
  const rejectedStream = await collectPlacementCoverageProof(options, binarySha, steps, badStream);
  assert.deepEqual(badStream.invoked, ["reference", "candidate", "comparison"]);
  assert.equal(rejectedStream.state, "failed");
  assert.equal(rejectedStream.mapping, null);
});

test("strict-prefix uses explicit isolated capture arms for P7 and P7P4", () => {
  assert.throws(() => parseProofArguments(cli("small", strictPrefixArgs)), /requires P7/u);
  assert.throws(() => parseProofArguments(cli("p7")), /only for strict-prefix/u);
  assert.throws(() => parseProofArguments(cli("p7", [...strictPrefixArgs, "--mode", "strict-prefix"])), /not both/u);
  const alias = parseProofArguments(cli("p7", ["--mode", "strict-prefix"]));
  assert.equal(alias.experiment.name, "strict-prefix");
  for (const key of ["p7", "p7p4"]) {
    const options = parseProofArguments(cli(key, strictPrefixArgs));
    const [, strict, compare] = proofSchedule(options, binary, artifacts);
    assert.deepEqual(strict.args, [...(key === "p7" ? ["--p7"] : []), "--coverage-slot", "proof-42-b"]);
    assert.deepEqual(compare.args, ["--compare-coverage", join(artifacts, "proof-42-a", "solution-coverage.clrcov"),
      strict.artifact]);
    const inherited = { clearra_strict_prefix: "1", CLEARRA_OPERATION_MASKS: "1",
      CLEARRA_PLACEMENT_DEPENDENCY: "1", clearra_standard_bag_union_memo: "compact" };
    assert.equal(benchmarkEnvironment(inherited, strict).CLEARRA_STRICT_PREFIX, "1");
    assert.equal(benchmarkEnvironment(inherited, strict).clearra_strict_prefix, undefined);
    assert.equal(benchmarkEnvironment(inherited, strict).CLEARRA_OPERATION_MASKS, undefined);
    assert.equal(benchmarkEnvironment(inherited, strict).clearra_standard_bag_union_memo, undefined);
    assert.equal(benchmarkEnvironment(inherited, strict).CLEARRA_PLACEMENT_DEPENDENCY, "0");
    assert.equal(benchmarkEnvironment(inherited, reference).CLEARRA_STRICT_PREFIX, undefined);
  }
});

test("strict-prefix receipt binds fixture, no replay and sealed parallel work", () => {
  const options = parseProofArguments(cli("p7", strictPrefixArgs));
  const [ref, strict] = proofSchedule(options, binary, artifacts);
  const receipt = runtime(strict, 1);
  const result = armReport(strict, receipt, options.fixture);
  const validate = (report = result) => validatedProofArm(report, receipt, strict, options, binarySha);
  assert.deepEqual(validate().strictPrefixParity, { candidates: 2_260, workers: 12 });
  assert.doesNotThrow(() => validate({
    ...result,
    summary: { ...result.summary, workers_used: "2", parallel_active_workers: "2" },
    strict_prefix: { ...result.strict_prefix, workers: 2, executing_workers: 2 },
  }), "P7 may be admitted to a smaller auto topology");
  assert.throws(() => validate({ ...result, compiled_source_commit: null }), /compile-time source/u);
  assert.throws(() => validate({ ...result, input: { ...result.input, rule: "jstris-180" } }), /wrong exact fixture/u);
  assert.throws(() => validate({ ...result, materialized_path_step_count: 1 }), /no-replay/u);
  assert.throws(() => validate({ ...result, coverage_capture_sink: "discard" }), /no-replay/u);
  assert.throws(() => validate({ ...result, other_process_configuration: { CLEARRA_STANDARD_BAG_UNION_MEMO: "compact" } }),
    /non-isolated benchmark runtime configuration/u);
  assert.throws(() => validate({ ...result, solution_coverage: { ...result.solution_coverage, covered_pairs: 79_287 } }),
    /incomplete or incorrect/u);
  for (const patch of [
    { output_limit_bytes: result.solution_coverage.output_limit_bytes + 1 },
    { output_bytes: result.solution_coverage.output_limit_bytes + 1 },
    { spool: { ...result.solution_coverage.spool, spill_bytes_written: 5 * 1024 ** 3 + 1 } },
    { spool: { ...result.solution_coverage.spool, peak_live_spill_bytes: 80_001 } },
  ]) {
    assert.throws(() => validate({ ...result, solution_coverage: { ...result.solution_coverage, ...patch } }),
      /incomplete or incorrect/u);
  }
  assert.throws(() => validate({ ...result, summary: { ...result.summary, total_reachability_states: "1" } }),
    /concrete fallback/u);
  for (const change of [{ sealed: false }, { candidates: 2_259 }, { workers: 11 }, { executing_workers: 1 }]) {
    assert.throws(() => validate({ ...result, strict_prefix: { ...result.strict_prefix, ...change } }),
      /incomplete strict-prefix worker coverage/u);
  }
  const timing = { ...result, output_mode: "global-only", coverage_capture_sink: "discard", solution_coverage: null };
  assert.deepEqual(validatedProofArm(timing, receipt, strict, options, binarySha, "global-only"), validate());
  assert.throws(() => validatedProofArm({ ...timing, coverage_capture_sink: "exact-spool" },
    receipt, strict, options, binarySha, "global-only"), /no-replay/u);
  const controlReceipt = runtime(ref, 0);
  const control = armReport(ref, controlReceipt, options.fixture);
  assert.doesNotThrow(() => validatedProofArm(control, controlReceipt, ref, options, binarySha));
  assert.throws(() => validatedProofArm({ ...control, strict_prefix: {} }, controlReceipt, ref, options, binarySha),
    /did not use the concrete/u);
});

test("strict-prefix proof cannot qualify from matching totals and fingerprints alone", async () => {
  for (const key of ["p7", "p7p4"]) {
    const options = parseProofArguments(cli(key, strictPrefixArgs));
    const steps = proofSchedule(options, binary, artifacts);
    const io = mockIo(options, steps);
    const proof = await collectPlacementCoverageProof(options, binarySha, steps, io);
    assert.deepEqual(io.invoked, ["reference", "candidate", "comparison"]);
    assert.equal(proof.experiment, "strict-prefix");
    assert.equal(proof.source_binding, "compile-time-and-checkout");
    assert.equal(proof.state, key === "p7" ? "completed-small-preflight" : "completed-exact-per-solution");
    assert.equal(proof.timing_adoption_authority, false);
    assert.equal(proof.release_authority, false);

    const mismatch = mockIo(options, steps, (step, execution) => {
      if (step.name === "comparison") execution.report.equivalent = false;
      return execution;
    });
    const failed = await collectPlacementCoverageProof(options, binarySha, steps, mismatch);
    assert.deepEqual(mismatch.invoked, ["reference", "candidate", "comparison"]);
    assert.equal(failed.state, "failed");
    assert.equal(failed.mapping, null);
  }
});

test("union proof holds strict B1 fixed and selects only the union storage in the two captures", () => {
  assert.throws(() => parseProofArguments(cli("small", strictPrefixUnionArgs)), /requires P7/u);
  assert.throws(() => parseAdoptionArguments([
    "--id", "trial-42", "--proof-id", "proof-42", "--source-sha", source, ...strictPrefixUnionArgs,
  ]), /Unknown placement experiment/u);
  const options = parseProofArguments(cli("p7", strictPrefixUnionArgs));
  const [ref, compact, compare] = proofSchedule(options, binary, artifacts);
  const inherited = {
    clearra_strict_prefix: "0", CLEARRA_STANDARD_BAG_ABSORB: "1", CLEARRA_STRICT_CATALOG_GUARDS: "1",
    CLEARRA_STRICT_LANGUAGE_FRONTIER: "1", CLEARRA_STRICT_SUBSUMPTION: "1",
    clearra_standard_bag_memo: "compact", CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: "hash",
    clearra_standard_bag_union_memo: "compact", CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES: "0",
    CLEARRA_STRICT_STAGE_PROFILE: "1", CLEARRA_STRICT_STAGE_LEGACY_UPPER: "1",
  };
  const baseline = benchmarkEnvironment(inherited, ref);
  const treatment = benchmarkEnvironment(inherited, compact);
  assert.equal(baseline.CLEARRA_STRICT_PREFIX, "1");
  assert.equal(baseline.CLEARRA_STANDARD_BAG_MEMO, "reference");
  assert.equal(baseline.CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT, "adaptive");
  assert.equal(baseline.CLEARRA_STANDARD_BAG_UNION_MEMO, "reference");
  assert.deepEqual(treatment, { ...baseline, CLEARRA_STANDARD_BAG_UNION_MEMO: "compact" });
  for (const key of Object.keys(inherited)) {
    if (key !== "CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT") assert.equal(baseline[key], undefined);
  }
  assert.equal(benchmarkEnvironment(inherited, compare).CLEARRA_STANDARD_BAG_UNION_MEMO, undefined);
  assert.deepEqual(compare.args, ["--compare-coverage", ref.artifact, compact.artifact]);
});

test("union proof checks effective storage and excludes all other modes and timing-only output", () => {
  const options = parseProofArguments(cli("p7", strictPrefixUnionArgs));
  const [ref, compact] = proofSchedule(options, binary, artifacts);
  const receipt = runtime(compact, 1);
  const result = armReport(compact, receipt, options.fixture);
  const configuration = result.other_process_configuration;
  const validate = (report) => validatedProofArm(report, receipt, compact, options, binarySha);
  const refReceipt = runtime(ref, 0);
  const refContract = validatedProofArm(armReport(ref, refReceipt, options.fixture), refReceipt, ref, options, binarySha);
  assert.deepEqual(validate(result), refContract);
  assert.throws(() => validate({ ...result, summary: { ...result.summary,
    standard_bag_union_memo_storage: "reference" } }), /unexpected source, mode, or coverage output/u);
  assert.throws(() => validate({ ...result, strict_prefix_mode: "0" }), /unexpected source, mode, or coverage output/u);
  assert.throws(() => validate({ ...result, strict_prefix: null }), /incomplete strict-prefix worker coverage/u);
  for (const other of [
    { ...configuration, CLEARRA_STANDARD_BAG_UNION_MEMO: "reference" },
    { ...configuration, CLEARRA_STANDARD_BAG_UNION_MEMO: undefined },
    { ...configuration, CLEARRA_STANDARD_BAG_MEMO: "compact" },
    { ...configuration, CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: "hash" },
    { ...configuration, clearra_standard_bag_union_memo: "compact" },
    { ...configuration, CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES: "0" },
    { ...configuration, CLEARRA_STRICT_SUBSUMPTION: "1" },
  ]) {
    assert.throws(() => validate({ ...result, other_process_configuration: other }),
      /non-isolated benchmark runtime configuration/u);
  }
  const timing = { ...result, output_mode: "global-only", coverage_capture_sink: "discard", solution_coverage: null };
  assert.throws(() => validatedProofArm(timing, receipt, compact, options, binarySha, "global-only"),
    /unexpected source, mode, or coverage output/u);
});

test("union P7P4 mapping proof uses the bounded strict spool and mandatory exact comparison", async () => {
  const options = parseProofArguments(cli("p7p4", [...strictPrefixUnionArgs, "--wsl-host-volume", "/mnt/c"]));
  assert.deepEqual(placementProofDiskPlan(options, true), {
    minimumFreeBytes: 24 * 1024 ** 3,
    minimumActiveArmFreeBytes: 16 * 1024 ** 3,
    minimumComparisonFreeBytes: 8 * 1024 ** 3,
  });
  const steps = proofSchedule(options, binary, artifacts);
  const io = mockIo(options, steps);
  const proof = await collectPlacementCoverageProof(options, binarySha, steps, io);
  assert.deepEqual(io.invoked, ["reference", "candidate", "comparison"]);
  assert.equal(proof.state, "completed-exact-per-solution");
  assert.equal(proof.experiment, "strict-prefix-union-memo");
  assert.equal(proof.axis, "standard_bag_union_memo_storage");
  assert.equal(proof.fixed_strict_prefix_mode, "1");
  assert.deepEqual(proof.union_memo_storage, { reference: "reference", candidate: "compact" });
  assert.equal(proof.release_authority, false);
  assert.equal(proof.timing_adoption_authority, false);

  const incomplete = mockIo(options, steps);
  const incompleteProof = await collectPlacementCoverageProof(options, binarySha, steps.slice(0, 2), incomplete);
  assert.equal(incompleteProof.state, "failed");
  assert.equal(incompleteProof.mapping, null);
  assert.deepEqual(incomplete.invoked, []);

  const mismatch = mockIo(options, steps, (step, execution) => {
    if (step.name === "comparison") execution.report.equivalent = false;
    return execution;
  });
  const rejected = await collectPlacementCoverageProof(options, binarySha, steps, mismatch);
  assert.equal(rejected.state, "failed");
  assert.equal(rejected.mapping, null);
});

test("the tighter proof retains both artifact and WSL host budgets", () => {
  const strict = parseProofArguments(cli("p7p4", strictPrefixArgs));
  const legacy = parseProofArguments(cli("p7p4", operationMaskArgs));
  assert.equal(placementProofDiskPlan(legacy, true).minimumFreeBytes, 33 * 1024 ** 3);
  assert.equal(placementProofDiskPlan(strict, false).minimumFreeBytes, 24 * 1024 ** 3);
  assert.throws(() => placementProofDiskPlan(strict, true), /actual backing volume/u);
  assert.throws(() => parseProofArguments(cli("p7p4", [...strictPrefixArgs, "--wsl-host-volume", "/mnt/c/source"])),
    /host volume/u);
  const withHost = parseProofArguments(cli("p7p4", [...strictPrefixArgs, "--wsl-host-volume", "/mnt/c"]));
  assert.deepEqual(placementProofDiskPlan(withHost, true), {
    minimumFreeBytes: 24 * 1024 ** 3,
    minimumActiveArmFreeBytes: 16 * 1024 ** 3,
    minimumComparisonFreeBytes: 8 * 1024 ** 3,
  });
  assert.throws(() => placementProofDiskPlan(withHost, false), /requires a Linux WSL runtime/u);
});

test("strict storage quota stops the sequence without upgrading a partial proof", async () => {
  const options = parseProofArguments(cli("p7", strictPrefixArgs));
  const steps = proofSchedule(options, binary, artifacts);
  const diagnostic = "placement_graph_bench: CLEARRA_STRICT_COVERAGE_DISK_LIMIT\n";
  const refuse = (bound) => {
    const io = mockIo(options, steps, (_, execution) => {
      execution.code = 1;
      execution.stderr = diagnostic;
      execution.receipt.stderr_tail = bound ? diagnostic : "";
      Object.assign(execution.receipt.outcome, {
        reason: "nonzero", return_code: 1, child_termination: { kind: "exit", code: 1 },
      });
      return execution;
    });
    return io;
  };
  for (const [bound, state, reason] of [
    [true, "censored", "strict-coverage-disk-limit"], [false, "failed", "nonzero"],
  ]) {
    const io = refuse(bound);
    const proof = await collectPlacementCoverageProof(options, binarySha, steps, io);
    assert.deepEqual(io.invoked, ["reference"]);
    assert.equal(proof.state, state);
    assert.equal(proof.observations[0].reason, reason);
    assert.equal(proof.mapping, null);
  }
});

test("successful exact sequence writes bound mapping and no release/performance authority", async () => {
  for (const key of ["small", "p7p4"]) {
    const options = parseProofArguments(cli(key));
    const steps = proofSchedule(options, binary, artifacts);
    const io = mockIo(options, steps);
    const proof = await collectPlacementCoverageProof(options, binarySha, steps, io);
    assert.deepEqual(io.invoked, ["reference", "candidate", "comparison"]);
    assert.equal(proof.state, key === "small" ? "completed-small-preflight" : "completed-exact-per-solution");
    assert.equal(proof.axis, "movement_clear_certificates_mode");
    assert.equal(Object.hasOwn(proof, "experiment"), false, "legacy receipt shape stays unchanged");
    assert.equal(proof.release_authority, false);
    assert.equal(proof.timing_adoption_authority, false);
    assert.equal(proof.mapping.reference.benchmark, "reference.json");
    assert.ok(io.writes.has("comparison-runtime.json"));
  }
});

test("censorship or pressure preserves first receipt and never starts another phase", async () => {
  const options = parseProofArguments(cli("p7p4"));
  const steps = proofSchedule(options, binary, artifacts);
  for (const state of ["censored", "confounded"]) {
    const io = mockIo(options, steps, (_, execution) => {
      if (state === "confounded") execution.receipt.outcome.memory_pressure_events = 1;
      else {
        execution.code = 124;
        Object.assign(execution.receipt.outcome, { reason: "timeout", return_code: 124 });
      }
      return execution;
    });
    const proof = await collectPlacementCoverageProof(options, binarySha, steps, io);
    assert.deepEqual(io.invoked, ["reference"]);
    assert.equal(proof.state, state);
    assert.equal(proof.mapping, null);
    assert.ok(io.writes.has("reference-runtime.json"));
    assert.equal(io.writes.has("candidate-runtime.json"), false);
  }
});

test("separate aggregate equality or a swapped artifact is not exact proof", () => {
  const fixture = parseProofArguments(cli("p7p4")).fixture;
  const referenceReport = armReport(reference, runtime(reference, 0), fixture);
  const candidateReport = armReport(candidate, runtime(candidate, 1), fixture);
  for (const change of [
    { equivalent: false }, { complete: false }, { left_sha256: "d".repeat(64) },
    { covered_pairs: fixture.queues + 1 },
  ]) {
    assert.throws(() => assertExactComparison({ ...comparisonReport(fixture), ...change },
      referenceReport, candidateReport, fixture), /did not prove/u);
  }
});

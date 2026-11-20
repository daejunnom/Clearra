import assert from "node:assert/strict";
import { join } from "node:path";
import test from "node:test";

import { evaluatePlacementGraphAdoption } from "../tools/placement-graph-adoption.mjs";
import {
  adoptionSchedule, collectPlacementGraphAdoption, parseAdoptionArguments, validateAdoptionProof,
} from "./run-placement-graph-adoption.mjs";
import { parseProofArguments, proofSchedule } from "./run-placement-coverage-proof.mjs";
import { DEFAULT_PLACEMENT_EXPERIMENT, placementExperiment } from "./placement-experiment-contract.mjs";

const sourceCommit = "a".repeat(40);
const binary = join("test-build", "placement_graph_bench");
const artifactRoot = join("test-artifacts", "coverage");
const custody = { binarySha: "a".repeat(64), managerSha: "b".repeat(64), hostFingerprint: "c".repeat(64) };
const cli = ["--id", "timings-42", "--proof-id", "proof-42", "--source-sha", sourceCommit];
const options = parseAdoptionArguments(cli);
const minimumOptions = parseAdoptionArguments([...cli, "--experiment", "minimum-domain-cutoff"]);
const strictOptions = parseAdoptionArguments([...cli, "--experiment", "strict-prefix"]);
const planFor = (configured) => parseProofArguments(["--case", "p7p4", "--id", configured.proofId,
  "--source-sha", sourceCommit, "--experiment", configured.experiment.name]);
const plan = planFor(options);
const schedule = adoptionSchedule(binary, plan.timeout);
const digest = (byte) => byte.repeat(64);

function runtime(step, index) {
  return {
    schema_id: "clearra.runtime-receipt.v2",
    outcome: {
      run_id: (1_800_000_000_000_000_000n + BigInt(index) * 2_000_000_000n).toString() + "-100",
      profile: "benchmark-search", producer: "benchmark",
      timeout_seconds: step.timeout, automatic_retry: false, command: step.command,
      reason: "normal", return_code: 0, process_tree_stopped: true, duration_ms: 1_000,
      error_code: null, supervisor_termination_reason: null,
      child_termination: { kind: "exit", code: 0 },
      memory_pressure_observed: false, memory_pressure_events: 0,
      admission: { hard_limit_bytes: 8_000_000_000, snapshot: { physical_total_bytes: 8_000_000_000 },
        capacity_basis: "runtime-pressure" },
      kernel_memory_peak: null, sampled_memory_peak: { source: "linux-sampled-process-group-rss", bytes: 1000 },
      peak_memory_bytes: 1000,
    },
  };
}

function bind(phase) {
  phase.runtime.stdout_tail = JSON.stringify(phase.report);
  return phase;
}

function report(step, receipt, output, sha) {
  const { fixture } = plan;
  const experiment = step.experiment ?? placementExperiment(DEFAULT_PLACEMENT_EXPERIMENT);
  const strict = experiment.name === "strict-prefix";
  return {
    schema: "clearra.placement-graph-benchmark.v1", case: fixture.name, valid: true, validation_errors: [],
    binary_sha256: custody.binarySha, compiled_source_commit: sourceCommit,
    supervisor_run_id: receipt.outcome.run_id, output_mode: output,
    movement_clear_certificates_mode: "0", minimum_domain_cutoff_mode: "0", minimum_domain_work_mode: "0",
    [experiment.axis]: step.value, mode: experiment.dependencyMode, structural_mode: experiment.structuralMode,
    static_influence_mode: "0", translation_interval_mode: "0", incremental_domain_mode: "0",
    first_kick_interval_mode: "0", ordered_kick_interval_mode: "0", ilc_lazy_frame_mode: "0",
    no_entry_components_mode: "0", same_tile_parent_reuse_mode: "0",
    conditional_order_mode: "0", terminal_identity_move_mode: "0",
    ordered_kick_guard_reuse_mode: "0", geometric_edge_reuse_mode: "0",
    geometry_domain: {
      completed_families: 2, minimum_compiles: 2, minimum_empty_origin: 2,
      minimum_resource_authoritative: 0, minimum_below_seven_pieces: 0,
      advanced_full_compiles: 0, advanced_incremental_compiles: 0, minimum_profiled_compiles: 0,
      minimum_incidence_rows_scanned: 0, minimum_incidence_rows_skipped: 0, minimum_cells_cut_off: 0,
    },
    input: {
      board: fixture.board, height: fixture.height, placed_pieces: fixture.placed, supplied_pieces: fixture.supplied,
      supply: fixture.supply, original_supply_count: fixture.queues, hold: fixture.hold,
      rule: "jstris-180", count: "unique", queue_knowledge: "full-future-oracle",
    },
    cpu: { worker_policy: "auto", use_all_logical_processors: true, expected_vcpus: null, hard_limit: 12 },
    summary: {
      count_complete: "true", probability_complete: "true", resource_truncated: "false",
      normalized_solution_set_hash: "cts1:2fcbc950acdffa31", workers_used: "12",
      parallel_active_workers: "12", cpu_parallel_execution: "true",
      ...(strict ? {
        representative_pattern_id: "", representative_candidate_id: "",
        packing_candidate_count: "30000000",
        total_build_order_nodes: step.value === "1" ? "0" : "1000000",
        total_reachability_states: step.value === "1" ? "0" : "2000000",
      } : {}),
    },
    materialized_identities: {
      count: fixture.solutions, reported_total_count: fixture.solutions, covers_reported_family: true,
      sha256_ordered_ctk1_v1: digest("0"),
    },
    coverage: { set_bit_count: fixture.queues, word_count: fixture.queues / 64, sha256_le_v1: digest("1") },
    solution_coverage: output === "solution-coverage" ? {
      complete: true, sha256: sha, solution_count: fixture.solutions, total_weight: fixture.queues,
      covered_weight: fixture.queues, covered_pairs: fixture.queues,
      ...(strict ? {
        output_bytes: step.value === "0" ? 1_860_000_000 : 1_860_000_032,
        output_limit_bytes: 3 * 1024 ** 3,
        spool: {
          spill_write_limit_bytes: 5 * 1024 ** 3,
          spill_bytes_written: step.value === "0" ? 3_500_000_000 : 3_600_000_000,
          peak_live_spill_bytes: step.value === "0" ? 1_900_000_000 : 2_000_000_000,
        },
      } : {}),
    } : null,
    timing_ms: { search: step.value === "0" ? 120 : 80 },
    backend_contract: "native-host Rust", other_process_configuration: {},
    ...(strict ? {
      coverage_presentation: "coverage-summary-omit",
      coverage_capture_sink: output === "global-only" ? "discard" : "exact-spool",
      materialized_path_step_count: 0,
      materialized_solution_coverage_count: 0,
      materialized_solution_probability_count: 0,
      strict_prefix: step.value === "0" ? null : {
        sealed: true, candidates: 30000000, workers: 12, executing_workers: 12,
        reverse_relations: 100, unresolved_lanes: 200, pruned_lanes: 42,
        pose_work: 100, worklist_pops: 100, pose_domains: 100, ordered_kick_terms: 100,
        peak_relation_logical_bytes: 16,
      },
    } : {}),
  };
}

function proofBundle(configured = options) {
  const proofPlan = planFor(configured);
  const steps = proofSchedule(proofPlan, binary, artifactRoot);
  const [reference, candidate, comparison] = steps.map((step, index) => {
    const receipt = runtime(step, index);
    return index < 2 ? bind({
      runtime: receipt, report: report(step, receipt, "solution-coverage", digest(index === 0 ? "e" : "f")),
    }) : bind({
      runtime: receipt,
      report: {
        schema: "clearra.placement-graph-solution-coverage-comparison.v1",
        equivalent: true, complete: true, universe: plan.fixture.universe, solution_count: plan.fixture.solutions,
        total_weight: plan.fixture.queues, covered_weight: plan.fixture.queues, covered_pairs: plan.fixture.queues,
        left_sha256: digest("e"), right_sha256: digest("f"),
      },
    });
  });
  return {
    summary: {
      schema: "clearra.placement-graph-coverage-proof.v1", state: "completed-exact-per-solution",
      release_authority: false, timing_adoption_authority: false,
      source_commit: sourceCommit, source_binding: "compile-time-and-checkout",
      binary_sha256: custody.binarySha, manager_sha256: custody.managerSha, host_fingerprint: custody.hostFingerprint,
      case: plan.fixture.name, axis: configured.experiment.axis,
      ...(configured === options ? {} : { experiment: configured.experiment.name }),
      observations: steps.map((step, index) => ({
        name: step.name, state: "completed", command: step.command, supervisor: step.name + "-runtime.json",
        run_id: [reference, candidate, comparison][index].runtime.outcome.run_id,
      })),
      mapping: {
        comparison: "comparison.json",
        reference: { benchmark: "reference.json", supervisor: "reference-runtime.json" },
        candidate: { benchmark: "candidate.json", supervisor: "candidate-runtime.json" },
      },
    },
    reference, candidate, comparison,
  };
}

function recordingIo(change = () => {}) {
  const started = [], writes = new Map();
  let active = false;
  return {
    started, writes,
    checkInputs() {},
    async execute(step) {
      assert.equal(active, false, "timing workloads must not overlap");
      active = true;
      started.push(step.name);
      const receipt = runtime(step, started.length + 2);
      const measurement = { code: 0, signal: null, receipt, report: report(step, receipt, "global-only") };
      change(step, measurement, started.length);
      measurement.receipt.stdout_tail = JSON.stringify(measurement.report);
      measurement.stdout = measurement.receipt.stdout_tail;
      await Promise.resolve();
      active = false;
      return measurement;
    },
    receipt: (execution) => execution.receipt,
    write(name, value) {
      assert.equal(writes.has(name), false, "evidence must not be overwritten");
      writes.set(name, structuredClone(value));
    },
  };
}

test("explicit proof and finite timing resolution; exactly eight global-only ABBA+BAAB invocations", () => {
  assert.throws(() => parseAdoptionArguments([]), /Usage/u);
  assert.throws(() => parseAdoptionArguments([...cli, "--proof-id", "../other"]), /Distinct/u);
  assert.throws(() => parseAdoptionArguments([...cli, "--time-resolution-ms", "0"]), /Usage/u);
  assert.equal(parseAdoptionArguments([...cli, "--time-resolution-ms", "0.1"]).timeResolutionMs, 0.1);
  assert.equal(schedule.map((step) => step.arm).join(""), "ABBABAAB");
  assert.ok(schedule.every((step) => step.command.length === 1 && step.timeout === plan.timeout));
});

test("validates previous mapping proof against exact compiled source, host, binary, and supervised arms", () => {
  const validated = validateAdoptionProof(options, binary, artifactRoot, custody, proofBundle());
  assert.deepEqual(validated.artifacts.map(({ sha256 }) => sha256), [digest("e"), digest("f")]);
  for (const tamper of [
    (bundle) => { bundle.summary.host_fingerprint = digest("d"); },
    (bundle) => { bundle.summary.source_binding = "checkout-and-binary-custody-only"; },
    (bundle) => { bundle.summary.axis = "ordered_kick_interval_mode"; },
    (bundle) => { bundle.candidate.report.compiled_source_commit = null; bind(bundle.candidate); },
    (bundle) => { bundle.candidate.report.ilc_lazy_frame_mode = "1"; bind(bundle.candidate); },
    (bundle) => { bundle.candidate.report.no_entry_components_mode = "1"; bind(bundle.candidate); },
    (bundle) => { bundle.candidate.report.compiled_source_commit = digest("b"); },
    (bundle) => { bundle.comparison.report.right_sha256 = digest("e"); bind(bundle.comparison); },
  ]) {
    const malformed = proofBundle();
    tamper(malformed);
    assert.throws(() => validateAdoptionProof(options, binary, artifactRoot, custody, malformed));
  }
});

test("completed timing produces independently evaluable manifest without release authority", async () => {
  const proof = validateAdoptionProof(options, binary, artifactRoot, custody, proofBundle());
  const io = recordingIo();
  const evidence = await collectPlacementGraphAdoption(options, custody, proof, schedule, io);
  assert.equal(evidence.state, "completed-adoptable");
  assert.equal(evidence.release_authority, false);
  assert.equal(evidence.decision, "decision.json");
  assert.deepEqual(io.started, schedule.map((step) => step.name));
  assert.equal(io.writes.get("manifest.json").runs.length, 8);
  assert.deepEqual(io.writes.get("mapping-comparison.json"), proof.mapping.comparison);
  assert.equal(io.writes.get("decision.json").release_authority, false);
});

test("stops after a censored or confounded phase, preserving the receipt as non-acceptance", async () => {
  const proof = validateAdoptionProof(options, binary, artifactRoot, custody, proofBundle());
  for (const condition of ["censored", "confounded"]) {
    const io = recordingIo((_, execution, ordinal) => {
      if (ordinal !== 2) return;
      if (condition === "censored") {
        execution.code = 124;
        Object.assign(execution.receipt.outcome, { reason: "timeout", return_code: 124 });
      } else {
        execution.receipt.outcome.memory_pressure_events = 1;
      }
    });
    const evidence = await collectPlacementGraphAdoption(options, custody, proof, schedule, io);
    assert.equal(evidence.state, condition);
    assert.equal(evidence.decision, null);
    assert.equal(io.started.length, 2);
    assert.ok(io.writes.has("trial-2-b-runtime.json"));
    assert.equal(io.writes.has("manifest.json"), false);
    assert.equal(io.writes.has("decision.json"), false);
  }
});

test("different compiled source or actual worker contract stops instead of taking more samples", async () => {
  const proof = validateAdoptionProof(options, binary, artifactRoot, custody, proofBundle());
  for (const mutate of [
    (measured) => { measured.compiled_source_commit = null; },
    (measured) => { measured.cpu.hard_limit = 11; },
    (measured) => { measured.ordered_kick_interval_mode = "1"; },
    (measured) => { measured.same_tile_parent_reuse_mode = "1"; },
  ]) {
    const io = recordingIo((_, execution) => mutate(execution.report));
    const evidence = await collectPlacementGraphAdoption(options, custody, proof, schedule, io);
    assert.equal(evidence.state, "failed");
    assert.equal(evidence.decision, null);
    assert.equal(io.started.length, 1);
  }
});

test("minimum-domain scheduling is opt-in and cannot borrow a movement-clear mapping", () => {
  assert.throws(() => parseAdoptionArguments([...cli, "--experiment", "minimum-domain-work"]), /Unknown placement/u);
  assert.equal(options.experiment.name, DEFAULT_PLACEMENT_EXPERIMENT);
  assert.equal(minimumOptions.experiment.name, "minimum-domain-cutoff");
  const minimumSchedule = adoptionSchedule(binary, plan.timeout, minimumOptions.experiment);
  assert.equal(minimumSchedule.map((step) => step.arm).join(""), "ABBABAAB");
  assert.ok(minimumSchedule.every((step) => step.experiment.axis === "minimum_domain_cutoff_mode"));
  assert.throws(() => validateAdoptionProof(minimumOptions, binary, artifactRoot, custody, proofBundle()),
    /proof for minimum-domain-cutoff/u);
});

test("minimum-domain timing binds a complete same-host setup only on the unprofiled empty route", async () => {
  const configured = minimumOptions;
  const proof = validateAdoptionProof(configured, binary, artifactRoot, custody, proofBundle(configured));
  const minimumSchedule = adoptionSchedule(binary, proof.proofOptions.timeout, configured.experiment);
  const io = recordingIo();
  const timing = await collectPlacementGraphAdoption(configured, custody, proof, minimumSchedule, io);
  assert.equal(timing.state, "completed-adoptable", timing.error);
  assert.equal(timing.axis, "minimum_domain_cutoff_mode");
  assert.equal(io.writes.get("manifest.json").custody.host_fingerprint, custody.hostFingerprint);
  assert.equal(io.writes.get("mapping-proof.json").experiment, "minimum-domain-cutoff");
  assert.equal(io.writes.get("mapping-comparison-runtime.json").outcome.command[1], "--compare-coverage");
  for (const tamper of [
    (result) => { result.geometry_domain.minimum_profiled_compiles = 1; },
    (result) => { result.geometry_domain.minimum_empty_origin = 0; },
    (result) => { result.minimum_domain_work_mode = "1"; },
    (result) => { result.mode = "1"; },
  ]) {
    const failedIo = recordingIo((_, execution) => tamper(execution.report));
    const denied = await collectPlacementGraphAdoption(configured, custody, proof, minimumSchedule, failedIo);
    assert.equal(denied.state, "failed");
    assert.equal(denied.decision, null);
    assert.equal(failedIo.started.length, 1);
  }
});

test("strict-prefix runs only ABBA with full exact mapping and no timed spool", async () => {
  const proofBundleForStrict = proofBundle(strictOptions);
  const proof = validateAdoptionProof(strictOptions, binary, artifactRoot, custody, proofBundleForStrict);
  const timingSchedule = adoptionSchedule(binary, proof.proofOptions.timeout, strictOptions.experiment);
  assert.equal(timingSchedule.map((step) => step.arm).join(""), "ABBA");
  const io = recordingIo();
  const timing = await collectPlacementGraphAdoption(strictOptions, custody, proof, timingSchedule, io);
  assert.equal(timing.state, "completed-adoptable", timing.error);
  assert.equal(timing.release_authority, false);
  assert.equal(timing.order, "ABBA");
  assert.equal(io.started.length, 4);
  const manifest = io.writes.get("manifest.json");
  assert.equal(manifest.order, "ABBA");
  assert.equal(manifest.runs.length, 4);
  const decision = io.writes.get("decision.json");
  assert.equal(decision.experiment, "strict-prefix");
  assert.deepEqual(decision.blocks, [
    { order: "AB", baseline_ms: 120, candidate_ms: 80 },
    { order: "BA", baseline_ms: 120, candidate_ms: 80 },
  ]);
  assert.equal(io.writes.get("trial-2-b.json").coverage_capture_sink, "discard");
  assert.equal(io.writes.get("mapping-candidate.json").coverage_capture_sink, "exact-spool");
  assert.throws(() => validateAdoptionProof(strictOptions, binary, artifactRoot, custody, proofBundle()));
  for (const tamper of [
    (bundle) => { bundle.summary.case = "ctk3-profile-4L-P7-srsplus"; },
    (bundle) => { bundle.comparison.report.equivalent = false; bind(bundle.comparison); },
    (bundle) => { bundle.candidate.report.strict_prefix.sealed = false; bind(bundle.candidate); },
    (bundle) => {
      bundle.candidate.report.solution_coverage.spool.spill_write_limit_bytes += 1;
      bind(bundle.candidate);
    },
    (bundle) => {
      bundle.candidate.report.solution_coverage.output_bytes =
        bundle.candidate.report.solution_coverage.output_limit_bytes + 1;
      bind(bundle.candidate);
    },
  ]) {
    const denied = proofBundle(strictOptions);
    tamper(denied);
    assert.throws(() => validateAdoptionProof(strictOptions, binary, artifactRoot, custody, denied));
  }
});

test("strict ABBA requires both directions and distinguishes incomplete and extra timing samples", async () => {
  const proof = validateAdoptionProof(strictOptions, binary, artifactRoot, custody, proofBundle(strictOptions));
  const timingSchedule = adoptionSchedule(binary, proof.proofOptions.timeout, strictOptions.experiment);
  const io = recordingIo((_, execution, ordinal) => {
    // Pooled B is faster, but the B-before-A pair loses.
    if (ordinal === 3) execution.report.timing_ms.search = 130;
  });
  const timing = await collectPlacementGraphAdoption(strictOptions, custody, proof, timingSchedule, io);
  assert.equal(timing.state, "completed-not-adoptable", timing.error);
  const decision = io.writes.get("decision.json");
  assert.equal(decision.status, "not-adoptable");
  assert.ok(decision.time.median_ms.candidate < decision.time.median_ms.baseline);
  assert.equal(decision.time.improved, false);
  assert.deepEqual(decision.blocks[1], { order: "BA", baseline_ms: 120, candidate_ms: 130 });

  const manifest = io.writes.get("manifest.json");
  const runs = manifest.runs.map(({ benchmark, supervisor }) => ({
    benchmark: io.writes.get(benchmark), supervisor: io.writes.get(supervisor),
  }));
  const incomplete = evaluatePlacementGraphAdoption({ ...manifest, runs: runs.slice(0, 3) });
  assert.equal(incomplete.status, "inconclusive");
  assert.equal(incomplete.time, undefined);
  assert.throws(() => evaluatePlacementGraphAdoption({ ...manifest, runs: [...runs, runs[1]] }), /ordered ABBA/u);

  const extraIo = recordingIo();
  const extra = await collectPlacementGraphAdoption(strictOptions, custody, proof,
    [...timingSchedule, timingSchedule[1]], extraIo);
  assert.equal(extra.state, "failed");
  assert.match(extra.error, /fixed global-only ABBA sequence/u);
  assert.equal(extraIo.started.length, 0);
});

test("strict-prefix timing rejects mismatched candidate custody, output mode, and fallback", async () => {
  const proof = validateAdoptionProof(strictOptions, binary, artifactRoot, custody, proofBundle(strictOptions));
  const timingSchedule = adoptionSchedule(binary, proof.proofOptions.timeout, strictOptions.experiment);
  for (const tamper of [
    (measured) => { measured.summary.total_build_order_nodes = "1"; },
    (measured) => { measured.strict_prefix.executing_workers = 1; },
    (measured) => { measured.coverage_capture_sink = "exact-spool"; },
  ]) {
    const io = recordingIo((_, execution, ordinal) => {
      if (ordinal === 2) tamper(execution.report);
    });
    const timing = await collectPlacementGraphAdoption(strictOptions, custody, proof, timingSchedule, io);
    assert.equal(timing.state, "failed");
    assert.equal(timing.decision, null);
    assert.equal(io.started.length, 2);
  }
});

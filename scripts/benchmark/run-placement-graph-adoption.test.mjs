import assert from "node:assert/strict";
import { join } from "node:path";
import test from "node:test";

import {
  adoptionSchedule, collectPlacementGraphAdoption, parseAdoptionArguments, validateAdoptionProof,
} from "./run-placement-graph-adoption.mjs";
import { AXIS, parseProofArguments, proofSchedule } from "./run-placement-coverage-proof.mjs";

const sourceCommit = "a".repeat(40);
const binary = join("test-build", "placement_graph_bench");
const artifactRoot = join("test-artifacts", "coverage");
const custody = { binarySha: "a".repeat(64), managerSha: "b".repeat(64), hostFingerprint: "c".repeat(64) };
const cli = ["--id", "timings-42", "--proof-id", "proof-42", "--source-sha", sourceCommit];
const options = parseAdoptionArguments(cli);
const plan = parseProofArguments(["--case", "p7p4", "--id", options.proofId, "--source-sha", sourceCommit]);
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
  return {
    schema: "clearra.placement-graph-benchmark.v1", case: fixture.name, valid: true, validation_errors: [],
    binary_sha256: custody.binarySha, compiled_source_commit: sourceCommit,
    supervisor_run_id: receipt.outcome.run_id, output_mode: output,
    [AXIS]: step.value, mode: "1", structural_mode: "1",
    static_influence_mode: "0", translation_interval_mode: "0", incremental_domain_mode: "0",
    first_kick_interval_mode: "0", ordered_kick_interval_mode: "0", ilc_lazy_frame_mode: "0",
    no_entry_components_mode: "0", same_tile_parent_reuse_mode: "0",
    conditional_order_mode: "0", terminal_identity_move_mode: "0",
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
    },
    materialized_identities: {
      count: fixture.solutions, reported_total_count: fixture.solutions, covers_reported_family: true,
      sha256_ordered_ctk1_v1: digest("0"),
    },
    coverage: { set_bit_count: fixture.queues, word_count: fixture.queues / 64, sha256_le_v1: digest("1") },
    solution_coverage: output === "solution-coverage" ? {
      complete: true, sha256: sha, solution_count: fixture.solutions, total_weight: fixture.queues,
      covered_weight: fixture.queues, covered_pairs: fixture.queues,
    } : null,
    timing_ms: { search: step.value === "0" ? 120 : 80 },
    backend_contract: "native-host Rust", other_process_configuration: {},
  };
}

function proofBundle() {
  const steps = proofSchedule(plan, binary, artifactRoot);
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
      case: plan.fixture.name, axis: AXIS,
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

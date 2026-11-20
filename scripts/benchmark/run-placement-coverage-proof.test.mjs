import assert from "node:assert/strict";
import { join } from "node:path";
import test from "node:test";
import {
  assertExactComparison, benchmarkEnvironment, classifyProofRuntime, collectPlacementCoverageProof,
  parseProofArguments, proofSchedule, validatedProofArm,
} from "./run-placement-coverage-proof.mjs";

const source = "a".repeat(40);
const binarySha = "b".repeat(64);
const artifacts = join("test-artifacts", "coverage");
const binary = join("test-build", "placement_graph_bench");
const cli = (key, rest = []) => ["--case", key, "--id", "proof-42", "--source-sha", source, ...rest];
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
  return {
    schema: "clearra.placement-graph-benchmark.v1", case: fixture.name, valid: true, validation_errors: [],
    binary_sha256: binarySha, supervisor_run_id: receipt.outcome.run_id, output_mode: "solution-coverage",
    movement_clear_certificates_mode: step.value, mode: "1", structural_mode: "1",
    static_influence_mode: "0", translation_interval_mode: "0", incremental_domain_mode: "0",
    first_kick_interval_mode: "0",
    conditional_order_mode: "0",
    terminal_identity_move_mode: "0",
    ordered_kick_interval_mode: "0",
    no_entry_components_mode: "0",
    same_tile_parent_reuse_mode: "0",
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
      sha256_ordered_ctk1_v1: "0".repeat(64),
    },
    coverage: { set_bit_count: fixture.queues, sha256_le_v1: "1".repeat(64) },
    solution_coverage: {
      complete: true, sha256: step.name === "reference" ? "c".repeat(64) : "d".repeat(64),
      solution_count: fixture.solutions, total_weight: fixture.queues, covered_weight: fixture.queues,
      covered_pairs: fixture.queues,
    },
    backend_contract: "native-host Rust", other_process_configuration: {},
  };
}

function comparisonReport(fixture) {
  return {
    schema: "clearra.placement-graph-solution-coverage-comparison.v1",
    equivalent: true, complete: true, universe: fixture.universe, solution_count: fixture.solutions,
    total_weight: fixture.queues, covered_weight: fixture.queues, covered_pairs: fixture.queues,
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

test("finite parsing rejects implicit workload, traversal, repeats, and excessive timeout", () => {
  assert.throws(() => parseProofArguments([]), /Usage/u);
  assert.throws(() => parseProofArguments(["--case", "large", "--id", "a", "--source-sha", source]), /Usage/u);
  assert.throws(() => parseProofArguments(["--case", "small", "--id", "..", "--source-sha", source]), /Usage/u);
  assert.throws(() => parseProofArguments(cli("small", ["--timeout-seconds", "301"])), /finite timeout/u);
  assert.throws(() => parseProofArguments(cli("p7p4", ["--case", "p7p4"])), /Distinct/u);
  assert.equal(parseProofArguments(cli("p7p4")).timeout, 1_800);
  assert.equal(parseProofArguments(cli("small", ["--timeout-seconds", "12"])).timeout, 12);
});

test("capture order holds all other axes fixed and uses separate new slots before comparison", () => {
  assert.deepEqual([reference.value, candidate.value], ["0", "1"]);
  assert.deepEqual(reference.args, ["--coverage-slot", "proof-42-a"]);
  assert.deepEqual(candidate.args, ["--coverage-slot", "proof-42-b"]);
  assert.deepEqual(comparison.args, ["--compare-coverage", reference.artifact, candidate.artifact]);
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

test("movement-clear proof neutralizes APDP and kick follow-on modes", () => {
  const input = {
    clearra_minimum_domain_cutoff: "1", CLEARRA_MINIMUM_DOMAIN_WORK: "1", CLEARRA_ORDERED_KICK_GUARD_REUSE: "1",
  };
  const environment = benchmarkEnvironment(input, reference);
  assert.equal(environment.CLEARRA_MINIMUM_DOMAIN_CUTOFF, "0");
  assert.equal(environment.CLEARRA_MINIMUM_DOMAIN_WORK, "0");
  assert.equal(environment.CLEARRA_ORDERED_KICK_GUARD_REUSE, "0");
  assert.equal(environment.clearra_minimum_domain_cutoff, undefined);
  const options = parseProofArguments(cli("p7p4"));
  const receipt = runtime(reference, 0);
  for (const axis of ["minimum_domain_cutoff_mode", "minimum_domain_work_mode", "ordered_kick_guard_reuse_mode"]) {
    const report = armReport(reference, receipt, options.fixture);
    report[axis] = "1";
    assert.throws(() => validatedProofArm(report, receipt, reference, options, binarySha),
      /unexpected source, mode, or coverage output/u);
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

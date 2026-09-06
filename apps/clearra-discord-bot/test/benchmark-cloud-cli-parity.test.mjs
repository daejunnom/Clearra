import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  benchmarkCloudCliParity, diagnosticFailureReport, PARITY_FIXTURES, PARITY_SCHEMA, validateParityResult,
} from "../scripts/benchmark-cloud-cli-parity.mjs";
import { prepareClearraArguments } from "../src/clearra/command.mjs";
import {
  currentRuntimeIdentityForCommit, productBuildIdentityFromRuntime,
} from "../src/job-service/runtime-identity.mjs";

const sourceCommit = "a".repeat(40);
const identity = currentRuntimeIdentityForCommit(sourceCommit);
const options = { executable: "/synthetic/clearra", sourceCommit, cpus: 8, workers: 8 };

function payloadFor(fixture) {
  if (fixture.kind === "minimum-pc") {
    const members = Array.from({ length: 25 }, (_, i) => ({
      candidate_id: String(i + 1), normalized_solution_key: `canonical-key-${String(i).padStart(2, "0")}`,
    }));
    return {
      schema_version: 2, kind: "pc-minimum-cover.v2",
      runtime_identity: productBuildIdentityFromRuntime(identity),
      contract: { command: { kind: "pc-minimum-cover.v2" } },
      summary: {
        capability_id: "pc.minimals", result_contract: "pc-minimum-cover.v2",
        payload_kind: "coverage-portfolio", set_contract: "portfolio-alternative-set.v1",
        page_contract: "portfolio-alternative-page.v1", member_page_contract: "portfolio-member-page.v1",
        set_identity_sha256: "b".repeat(64), candidate_map_sha256: "c".repeat(64),
        alternative_index: "1", optimal_cardinality: "25", known_alternative_count: "1",
        total_alternative_count: null, enumeration_complete: false,
        member_page_number: "1", total_member_pages: "1", members,
        page_handle_available: true, canonical_selection: "smallest-canonical-candidate-id",
        canonical_witness: structuredClone(members[0]),
      },
    };
  }
  return {
    schema_version: 2, kind: fixture.kind === "all-pc" ? "pc-scenario" : "build-probability",
    runtime_identity: productBuildIdentityFromRuntime(identity),
    summary: {
      count_complete: true, probability_complete: true, count_truncated_reason: "none",
      normalized_unique_solution_count: 2, normalized_solution_set_hash: "cts1:1234567890abcdef",
      materialized_pattern_count: 5040,
    },
    contract: {
      solution_data: { status: "complete", requested: true, reason: null },
      artifacts: { schema_version: "clearra.solution-data.v1", solution_keys: ["field-a", "field-b"] },
    },
  };
}
function result(payload, extra = {}) {
  return { exitCode: 0, signal: null, stdout: JSON.stringify(payload), stderr: "", ...extra };
}
function mocks(mutate = () => {}) {
  const spawns = [];
  const capabilities = [];
  let running = 0;
  let peak = 0;
  let hashCalls = 0;
  const dependencies = {
    availableParallelism: () => 8,
    hashExecutable: async () => { hashCalls += 1; return "d".repeat(64); },
    execFile: (file, args, settings, callback) => {
      capabilities.push({ file, args, settings });
      callback(null, JSON.stringify({
        runtime_identity: productBuildIdentityFromRuntime(identity),
        finesse_report: { mode: args[1], metric: "inputs" },
      }), "");
    },
    spawn: (file, args, settings) => {
      assert.equal(capabilities.length, 2, "capabilities must finish before every timed spawn");
      const fixture = args[0] === "build-probability" ? PARITY_FIXTURES[2]
        : args[1] === "minimals" ? PARITY_FIXTURES[1] : PARITY_FIXTURES[0];
      const payload = payloadFor(fixture);
      const output = result(payload);
      mutate(output, payload, spawns.length, fixture);
      const child = new EventEmitter();
      child.stdout = new PassThrough();
      child.stderr = new PassThrough();
      child.exitCode = null;
      child.signalCode = null;
      let closed = false;
      function close(code, signal) {
        if (closed) return;
        closed = true;
        running -= 1;
        child.exitCode = code;
        child.signalCode = signal;
        child.emit("close", code, signal);
      }
      child.kill = (signal) => { close(null, signal); return true; };
      spawns.push({ file, args: [...args], expectedVcpus: settings.env.CLEARRA_EXPECTED_VCPUS });
      running += 1;
      peak = Math.max(peak, running);
      setImmediate(() => {
        if (closed) return;
        child.stdout.end(output.stdout);
        child.stderr.end(output.stderr);
        close(output.exitCode, output.signal);
      });
      return child;
    },
  };
  return { dependencies, spawns, capabilities, get peak() { return peak; }, get hashCalls() { return hashCalls; } };
}

test("real loopback service and direct mocked CLI use identical source/argv/CPU, one warm plus three serial pairs", async () => {
  const mock = mocks();
  const report = await benchmarkCloudCliParity(options, mock.dependencies);
  assert.equal(report.schema_id, PARITY_SCHEMA);
  assert.equal(report.status, "passed");
  assert.equal(report.release_authority, false);
  assert.equal(report.performance_threshold_applied, false);
  assert.equal(report.pure_solver_timing, false);
  assert.equal(report.cpus, 8);
  assert.equal(report.workers, 8);
  assert.deepEqual(report.runtime_identity, identity);
  assert.equal(report.cli_binary_sha256, "d".repeat(64));
  assert.equal(mock.hashCalls, 2, "binary authority bracketed before/after all samples");
  assert.equal(mock.peak, 1, "no competing child processes");
  assert.equal(mock.spawns.length, 3 * (1 + 3) * 2);
  assert.equal(mock.capabilities.length, 2);
  for (let i = 0; i < PARITY_FIXTURES.length; i += 1) {
    const expected = prepareClearraArguments([...PARITY_FIXTURES[i].arguments], {
      workers: 8, useAllLogicalProcessors: true, logicalProcessors: 8,
      outputFormat: "json", includeSolutionData: true,
    });
    for (const spawn of mock.spawns.slice(i * 8, i * 8 + 8)) {
      assert.equal(spawn.file, options.executable);
      assert.deepEqual(spawn.args, expected);
      assert.equal(spawn.expectedVcpus, "8");
      assert.equal(spawn.args.includes("25"), false, "known minimum is never a solver hint");
    }
    const row = report.fixtures[i];
    assert.equal(row.fixture_id, PARITY_FIXTURES[i].id);
    assert.equal(row.warm_pairs, 1);
    assert.equal(row.measured_pairs, 3);
    assert.equal(row.timings.length, 3);
    for (const sample of row.timings) {
      for (const duration of Object.values(sample)) assert.ok(duration >= 0 && Number.isFinite(duration));
    }
  }
  assert.equal(report.fixtures[1].solution_count, "25");
  assert.equal(report.fixtures[1].normalized_solution_set_hash, null,
    "typed minimum is not falsely labeled as exporting a cts1 digest");
  assert.match(report.fixtures[1].canonical_members_sha256, /^[a-f0-9]{64}$/u);
  const encoded = JSON.stringify(report);
  for (const privateText of ["canonical-key-", "field-a", "/synthetic", "authorization", "environment", "--patterns", "0x3c0f"])
    assert.equal(encoded.includes(privateText), false, privateText);
});

test("fixture argv freezes left CTK3/P7/Jstris180 and target-only Build geometry", () => {
  assert.deepEqual(PARITY_FIXTURES.map((fixture) => fixture.kind), ["all-pc", "minimum-pc", "all-build"]);
  for (const fixture of PARITY_FIXTURES) {
    assert.ok(fixture.arguments.includes("jstris-180"));
    assert.ok(fixture.arguments.includes("P7"));
    assert.ok(fixture.arguments.includes("0x3c0f03c0f"));
    assert.equal(fixture.arguments.includes("0xf03c0f03c0"), false);
  }
  assert.ok(PARITY_FIXTURES[2].arguments.includes("0xfc3f0fc3f0"));
  assert.ok(PARITY_FIXTURES[2].arguments.includes("--no-mirror"));
});

test("canonical minimum allows unenumerated later ties but rejects partial members, changed canonical order, or stale identity", () => {
  const fixture = PARITY_FIXTURES[1];
  assert.equal(validateParityResult(result(payloadFor(fixture)), fixture, identity).evidence.solution_count, "25");
  const cases = [
    (payload) => { payload.summary.members.pop(); },
    (payload) => { payload.summary.optimal_cardinality = "24"; },
    (payload) => { payload.summary.total_member_pages = "2"; },
    (payload) => { payload.summary.members.reverse(); },
    (payload) => { payload.summary.canonical_witness.candidate_id = "2"; },
    (payload) => { payload.runtime_identity = { ...payload.runtime_identity, engine_build_id: "e".repeat(40) }; },
    (payload) => { payload.summary.set_identity_sha256 = "partial"; },
  ];
  for (const mutate of cases) {
    const payload = payloadFor(fixture);
    mutate(payload);
    assert.throws(() => validateParityResult(result(payload), fixture, identity));
  }
});

test("all-result validation refuses truncation, missing complete keys, wrong universe and abnormal termination", () => {
  const fixture = PARITY_FIXTURES[0];
  for (const mutate of [
    (payload) => { payload.summary.count_complete = false; },
    (payload) => { payload.summary.probability_complete = false; },
    (payload) => { payload.summary.count_truncated_reason = "memory-budget-exceeded"; },
    (payload) => { payload.summary.count_truncated_reason = "None"; },
    (payload) => { payload.contract.solution_data.status = "partial"; },
    (payload) => { payload.contract.artifacts.solution_keys.pop(); },
    (payload) => { payload.contract.artifacts.solution_keys[1] = "field-a"; },
    (payload) => { payload.summary.materialized_pattern_count = 7; },
    (payload) => { payload.resource_report = { truncated: true }; },
  ]) {
    const payload = payloadFor(fixture);
    mutate(payload);
    assert.throws(() => validateParityResult(result(payload), fixture, identity));
  }
  for (const extra of [{ exitCode: 2 }, { signal: "SIGTERM" }, { stdout: "not JSON" }])
    assert.throws(() => validateParityResult(result(payloadFor(fixture), extra), fixture, identity));
});

test("different raw results fail despite successful CLI exits; no successful report is returned", async () => {
  const mock = mocks((output, payload, index) => {
    if (index === 1) {
      payload.contract.artifacts.solution_keys[1] = "different-field";
      output.stdout = JSON.stringify(payload);
    }
  });
  await assert.rejects(benchmarkCloudCliParity(options, mock.dependencies), /route_result_mismatch/u);
  assert.equal(mock.peak, 1);
  assert.equal(mock.spawns.length, 2);
});

test("nonzero service/direct exits and output overflow never become elapsed success samples", async () => {
  for (const mode of ["direct-failure", "service-failure", "overflow", "cancelled"]) {
    const mock = mocks((output, _payload, index) => {
      if (mode === "direct-failure" && index === 0 || mode === "service-failure" && index === 1) output.exitCode = 2;
      if (mode === "cancelled" && index === 0) output.signal = "SIGTERM";
      if (mode === "overflow" && index === 0) output.stdout = "x".repeat(4 * 1024 * 1024 + 1);
    });
    await assert.rejects(benchmarkCloudCliParity(options, mock.dependencies));
  }
});

test("CPU oversubscription and post-run binary replacement are rejected, timing has no P2 speed gate", async () => {
  const invalid = mocks();
  await assert.rejects(benchmarkCloudCliParity({ ...options, cpus: 9 }, invalid.dependencies), /invalid_cpu_authority/u);
  assert.equal(invalid.spawns.length, 0);
  const replacement = mocks();
  let hashes = 0;
  replacement.dependencies.hashExecutable = async () => (++hashes === 1 ? "d" : "e").repeat(64);
  await assert.rejects(benchmarkCloudCliParity(options, replacement.dependencies), /executable_changed/u);
  const slow = mocks();
  let fakeTime = 0;
  slow.dependencies.clock = () => { fakeTime += 40_000; return fakeTime; };
  const report = await benchmarkCloudCliParity(options, slow.dependencies);
  assert.equal(report.status, "passed");
  assert.ok(report.fixtures.every((fixture) => fixture.medians_ms.direct_process_ms >= 40_000));
  assert.equal(report.performance_threshold_applied, false);
});

test("a late CLI failure retains completed evidence and only allow-listed failure metadata", async () => {
  const mock = mocks((output, _payload, index) => {
    if (index !== 8) return;
    output.exitCode = 2;
    output.stdout = JSON.stringify({ error: { code: "E_PC_SEARCH_INTERNAL", message: "PRIVATE MESSAGE" } });
    output.stderr = "PRIVATE INPUT OR TOKEN";
  });
  let failure;
  try { await benchmarkCloudCliParity(options, mock.dependencies); } catch (error) { failure = error; }
  const report = diagnosticFailureReport(failure);
  assert.equal(report.status, "failed");
  assert.equal(report.code, "cli_not_successful");
  assert.equal(report.completed_fixtures.length, 1);
  assert.equal(report.context.fixture_id, PARITY_FIXTURES[1].id);
  assert.equal(report.context.route, "direct");
  assert.equal(report.samples.at(-1).cli_error_code, "E_PC_SEARCH_INTERNAL");
  assert.equal(report.samples.at(-1).exit_code, 2);
  assert.equal(mock.spawns.length, 9, "do not run a second arm after the first has failed");
  assert.doesNotMatch(JSON.stringify(report), /PRIVATE|synthetic|canonical-key|--patterns/u);
  assert.equal(diagnosticFailureReport(new Error("PRIVATE")).code, "diagnostic_failed");
});

test("unrecognized error codes and error messages are never copied into diagnostic artifacts", async () => {
  const mock = mocks((output) => {
    output.exitCode = 2;
    output.stdout = JSON.stringify({ error: { code: "E_PRIVATE_DATA", message: "PRIVATE" } });
    output.stderr = "E_PRIVATE_DATA: PRIVATE";
  });
  await assert.rejects(benchmarkCloudCliParity(options, mock.dependencies), (error) => {
    const report = diagnosticFailureReport(error);
    assert.equal(report.samples[0].cli_error_code, null);
    assert.doesNotMatch(JSON.stringify(report), /PRIVATE/u);
    return true;
  });
});

test("script emits only bounded reports, rejects native Windows invocation and has no remote/deploy hook", async () => {
  const source = await readFile(new URL("../scripts/benchmark-cloud-cli-parity.mjs", import.meta.url), "utf8");
  assert.match(source, /native_windows_execution_forbidden/u);
  assert.match(source, /http:\/\/127\.0\.0\.1:/u);
  assert.doesNotMatch(source, /gcloud|execSync|console\.log|console\.error|writeFile|process\.env\.[A-Z_]+\s*=/u);
  assert.match(source, /performance_threshold_applied: false/u);
  assert.match(source, /15 \* 60_000/u);
  assert.match(source, /FIXTURE_TIMEOUT_MS = 120_000/u);
});

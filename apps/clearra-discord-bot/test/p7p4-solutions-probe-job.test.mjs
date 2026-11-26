import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import test from "node:test";
import { encodeCtk3 } from "ctk3";

import {
  P7P4_PROBE_TIMEOUT_MS,
  P7P4_SOLUTIONS_PROBE_SCHEMA,
  publicP7p4SolutionsPlan,
  runP7p4SolutionsProbe,
} from "../scripts/run-p7p4-solutions-probe-job.mjs";
import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArgumentPlan } from "../src/discord/slash-command-input.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityFromRuntime,
} from "../src/job-service/runtime-identity.mjs";

const sourceCommit = "6".repeat(40);
const cliSha256 = "7".repeat(64);
const runtimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);
const options = Object.freeze({
  sourceCommit,
  cliSha256,
  jobName: "clearra-parity-6666666-42",
  revision: "clearra-current-job-v080-6666666",
  serviceUrl: "https://candidate---service-test-an.a.run.app",
  authorizationToken: "synthetic-managed-probe-token",
});
const expectedPlan = () => buildSlashCommandArgumentPlan(findSlashCommand("pc").subcommands.solutions, [
  { name: "field", value: "grid:__________" },
  { name: "next", value: "P7P4" },
  { name: "hold", value: "empty" },
  { name: "kicktable", value: "srs-plus" },
]);
const value = (args, key) => args[args.indexOf(key) + 1];

function runtimeEnvironment() {
  return {
    NODE_ENV: "production",
    CLEARRA_SOURCE_COMMIT: sourceCommit,
    CLEARRA_ENGINE_BUILD_ID: sourceCommit,
    CLEARRA_SEARCH_CONTRACT_REVISION: runtimeIdentity.contractSchemaVersion,
    CLEARRA_SUPPLY_SEMANTICS_ID: runtimeIdentity.supplySemanticsId,
    CLEARRA_ARTIFACT_SCHEMA_VERSION: runtimeIdentity.artifactSchemaVersion,
    CLOUD_RUN_JOB: options.jobName,
    CLOUD_RUN_EXECUTION: `${options.jobName}-execution`,
    CLOUD_RUN_TASK_INDEX: "0",
    CLOUD_RUN_TASK_ATTEMPT: "0",
    CLOUD_RUN_TASK_COUNT: "1",
    CLEARRA_P7P4_OWNER_NONCE: "1234567890abcdef",
  };
}

function validResult(lines = 2) {
  const solutionCount = lines === 2 ? 1 : 2;
  const source = encodeCtk3({ width: 10, pages: Array.from({ length: solutionCount }, (_, index) => ({
    height: 1,
    cells: index === 0
      ? ["I", "I", "I", "I", null, null, null, null, null, null]
      : [null, "I", "I", "I", "I", null, null, null, null, null],
  })) });
  return {
    exitCode: 0,
    signal: null,
    stderr: "",
    stdout: JSON.stringify({
      kind: "pc-scenario",
      contract: { command: { kind: "pc-scenario" } },
      runtime_identity: productBuildIdentityFromRuntime(runtimeIdentity),
      summary: {
        objective: "unique", search_output_policy: "trace",
        rule_profile: "srs-plus", queue_knowledge: "oracle",
        postprocess_pc_path_requested: "false", packing_candidate_is_solution: "false",
        buildability_verified: true, count_complete: true, solution_count_calculated: true,
        solution_set_materialized: true, solution_keys_complete: true, solution_found: true,
        materialized_pattern_count: lines === 2 ? 5_040 : 4_233_600,
        normalized_unique_solution_count: solutionCount,
        normalized_solution_set_hash: `cts1:${String(lines).padStart(16, "0")}`,
        workers_requested: "auto", workers_used: lines === 4 ? 8 : 1,
      },
    }),
    artifact: {
      contract: "clearra.discord-solution-artifact.v1", mediaType: "text/plain; charset=utf-8",
      filename: "clearra-result.ctk3", source, byteLength: Buffer.byteLength(source),
      sha256: createHash("sha256").update(source).digest("hex"), solutionCount, complete: true,
    },
  };
}

function mutateSummary(result, changes) {
  const payload = JSON.parse(result.stdout);
  Object.assign(payload.summary, changes);
  result.stdout = JSON.stringify(payload);
}

function makeEmptyResult(result, workersUsed = 0) {
  mutateSummary(result, { solution_found: false, normalized_unique_solution_count: 0,
    normalized_solution_set_hash: "cts1:0000000000000000", workers_used: workersUsed });
  result.artifact = null;
}

function harness({ environment = runtimeEnvironment(), runtime = { platform: "linux", nodeVersion: "22.23.2" },
  onExecute, onHash } = {}) {
  let time = 1_000;
  const calls = [];
  const constructors = [];
  const hashes = [];
  const state = { calls, constructors, hashes, advance: (milliseconds) => { time += milliseconds; } };
  class FakeExecutor {
    constructor(configuration) { constructors.push(configuration); }
    async execute(arguments_, execution) {
      const lines = Number(value(arguments_, "--lines"));
      const result = validResult(lines);
      calls.push({ arguments_: [...arguments_], execution: { ...execution }, result });
      if (onExecute) return onExecute({ lines, result, state, call: calls.at(-1) });
      state.advance(100);
      return result;
    }
  }
  state.dependencies = { environment, runtime, Executor: FakeExecutor, now: () => time,
    hashExecutable: async () => {
      hashes.push(true);
      return onHash ? onHash(state) : cliSha256;
    } };
  return state;
}

test("P7P4 proof uses the actual public solutions form and retains both automatic targets", () => {
  const plan = publicP7p4SolutionsPlan();
  assert.deepEqual(plan, expectedPlan());
  assert.equal(plan.automaticPcTargets, true);
  assert.deepEqual(plan.argumentSets.map((args) => value(args, "--lines")), ["2", "4"]);
  assert.deepEqual(plan.argumentSets.map((args) => value(args, "--pieces")), ["5", "10"]);
  for (const args of plan.argumentSets) {
    assert.equal(value(args, "--patterns"), "P7P4");
    assert.equal(value(args, "--hold"), "empty");
    assert.equal(value(args, "--rule"), "srs-plus");
    assert.equal(value(args, "--count"), "unique");
    assert.equal(value(args, "--objective"), "unique");
    assert.ok(!args.includes("path"));
    assert.ok(!args.includes("--tiling-only"));
    assert.ok(!args.includes("--solution-probabilities"));
  }
});

test("one public request executes 2L then 4L with one exact-runtime executor and one shared deadline", async () => {
  const state = harness();
  const report = await runP7p4SolutionsProbe(options, state.dependencies);
  assert.equal(P7P4_PROBE_TIMEOUT_MS, 840_000);
  assert.equal(state.hashes.length, 1);
  assert.equal(state.constructors.length, 1);
  const configuration = state.constructors[0];
  assert.equal(configuration.endpoint.href, `${options.serviceUrl}/jobs`);
  assert.equal(configuration.authorizationToken, options.authorizationToken);
  assert.deepEqual(configuration.expectedRuntimeIdentity, runtimeIdentity);
  assert.equal(configuration.maxOutputBytes, 4 * 1024 ** 2);
  assert.equal(configuration.maxArtifactBytes, 24 * 1024 ** 2);
  for (const key of ["searchTimeoutMs", "pcSearchTimeoutMs", "reverseSearchTimeoutMs", "forwardSearchTimeoutMs"]) {
    assert.equal(configuration[key], 840_000);
  }
  assert.deepEqual(state.calls.map(({ arguments_ }) => arguments_), expectedPlan().argumentSets);
  assert.deepEqual(state.calls.map(({ execution }) => execution), [2, 4].map((lines) => ({
    jobId: `${options.jobName}-${lines}l`, deadlineUnixMs: 841_000, timeoutClass: "pc_reverse",
  })));
  assert.equal(report.schema, P7P4_SOLUTIONS_PROBE_SCHEMA);
  assert.equal(report.status, "passed");
  assert.equal(report.diagnosticOnly, true);
  assert.equal(report.sourceCommit, sourceCommit);
  assert.equal(report.cliSha256, cliSha256);
  assert.equal(report.executionName, runtimeEnvironment().CLOUD_RUN_EXECUTION);
  assert.equal(report.revision, options.revision);
  assert.equal(report.capabilityId, "pc.solutions");
  assert.equal(report.countPolicy, "CountUnique");
  assert.equal(report.sourcePattern, "P7P4");
  assert.equal(report.originalPatternCount, 4_233_600);
  assert.equal(report.originalSequenceLength, 11);
  assert.equal(report.automaticPcTargets, true);
  assert.equal(report.sharedDeadlineMs, 840_000);
  assert.equal(report.expectedVcpus, 8);
  assert.equal(report.durationMs, 200);
  assert.deepEqual(report.targets.map(({ lines, workersUsed, solutionCount }) => ({ lines, workersUsed, solutionCount })), [
    { lines: 2, workersUsed: 1, solutionCount: 1 },
    { lines: 4, workersUsed: 8, solutionCount: 2 },
  ]);
  assert.deepEqual(report.targets.map(({ materializedPatternCount }) => materializedPatternCount), [5_040, 4_233_600]);
  for (const [index, target] of report.targets.entries()) {
    const native = state.calls[index].result;
    const payload = JSON.parse(native.stdout);
    assert.equal(target.status, "passed");
    assert.equal(target.complete, true);
    assert.equal(target.solutionSetHash, payload.summary.normalized_solution_set_hash);
    assert.equal(target.resultSha256, createHash("sha256").update(native.stdout).digest("hex"));
    assert.deepEqual(target.artifact, { byteLength: native.artifact.byteLength,
      sha256: native.artifact.sha256, solutionCount: native.artifact.solutionCount, complete: true });
  }
});

test("authority, task ownership and packaged binary identity fail before any request", async () => {
  for (const change of [
    { sourceCommit: "not-a-commit" }, { cliSha256: "not-a-hash" },
    { revision: "clearra-current-job-v080-7777777" }, { jobName: "clearra-parity-7777777-42" },
    { serviceUrl: "http://candidate.run.app" }, { serviceUrl: "https://candidate.run.app/jobs" },
    { serviceUrl: "https://candidate.run.app?token=synthetic" },
    { serviceUrl: "https://user:synthetic@candidate.run.app" }, { authorizationToken: "" },
  ]) {
    const state = harness();
    await assert.rejects(runP7p4SolutionsProbe({ ...options, ...change }, state.dependencies));
    assert.equal(state.hashes.length, 0);
    assert.equal(state.constructors.length, 0);
    assert.equal(state.calls.length, 0);
  }
  for (const change of [
    { CLEARRA_SOURCE_COMMIT: "7".repeat(40) }, { CLEARRA_ENGINE_BUILD_ID: "7".repeat(40) },
    { CLEARRA_SEARCH_CONTRACT_REVISION: "clearra.search.contract.legacy-v1" },
    { CLEARRA_SUPPLY_SEMANTICS_ID: "clearra.supply.legacy-v1" },
    { CLEARRA_ARTIFACT_SCHEMA_VERSION: "clearra.solution-data.legacy-v1" },
    { CLOUD_RUN_JOB: "foreign-job" }, { CLOUD_RUN_EXECUTION: "foreign-job-execution" },
    { CLOUD_RUN_TASK_INDEX: "1" }, { CLOUD_RUN_TASK_ATTEMPT: "1" },
    { CLOUD_RUN_TASK_COUNT: "2" },
  ]) {
    const state = harness({ environment: { ...runtimeEnvironment(), ...change } });
    await assert.rejects(runP7p4SolutionsProbe(options, state.dependencies), undefined, JSON.stringify(change));
    assert.equal(state.hashes.length, 0);
    assert.equal(state.calls.length, 0);
  }
  for (const runtime of [{ platform: "win32", nodeVersion: "22.23.2" },
    { platform: "linux", nodeVersion: "22.23.1" }]) {
    const state = harness({ runtime });
    await assert.rejects(runP7p4SolutionsProbe(options, state.dependencies), /pinned Linux Node/u);
    assert.equal(state.hashes.length, 0);
  }
  const wrongBinary = harness({ onHash: () => "8".repeat(64) });
  await assert.rejects(runP7p4SolutionsProbe(options, wrongBinary.dependencies), /CLI hash differs/u);
  assert.equal(wrongBinary.hashes.length, 1);
  assert.equal(wrongBinary.constructors.length, 0);
  assert.equal(wrongBinary.calls.length, 0);
});

test("failures never retry either target and retain only bounded diagnostic fields", async () => {
  for (const failingLines of [2, 4]) {
    const state = harness({ onExecute: ({ lines, result }) => {
      if (lines === failingLines) throw Object.assign(new Error("private execution error"), {
        failureCode: "engine-oom", failureStage: "execute",
        stageDurationsMs: { queue: 12, execute: 42, prepare: -1, artifact: 900_001,
          projection: "private", foreign: 9 },
        authorizationToken: options.authorizationToken, stderr: "private child stderr",
      });
      return result;
    } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "failed");
    assert.equal(state.calls.length, failingLines === 2 ? 1 : 2);
    assert.equal(report.targets.length, state.calls.length);
    assert.equal(report.targets.at(-1).lines, failingLines);
    assert.equal(report.targets.at(-1).failureCode, "engine-oom");
    assert.equal(report.targets.at(-1).failureStage, "execute");
    assert.deepEqual(report.targets.at(-1).stageDurationsMs, { queue: 12, execute: 42 });
    const serialized = JSON.stringify(report);
    assert.ok(Buffer.byteLength(serialized) < 32 * 1024);
    assert.ok(!serialized.includes(options.authorizationToken));
    assert.ok(!serialized.includes("private"));
  }
  const state = harness({ onExecute: () => { throw { failureCode: "private-code", failureStage: "private-stage" }; } });
  const report = await runP7p4SolutionsProbe(options, state.dependencies);
  assert.equal(report.targets[0].failureCode, "result-validation");
  assert.equal(report.targets[0].failureStage, "execute");
  for (const failureCode of ["transport-timeout", "deadline"]) {
    const rejected = harness({ onExecute: () => { throw Object.assign(new Error("private transport error"), { failureCode }); } });
    const report = await runP7p4SolutionsProbe(options, rejected.dependencies);
    assert.equal(report.status, "failed");
    assert.equal(report.targets[0].failureCode, failureCode);
    assert.equal(report.targets[0].failureStage, "execute");
    assert.equal(rejected.calls.length, 1);
  }
});

test("expired shared budget prevents requests and never grants 4L a fresh timeout", async () => {
  const expired = harness({ onHash: (state) => { state.advance(840_000); return cliSha256; } });
  const beforeRequest = await runP7p4SolutionsProbe(options, expired.dependencies);
  assert.equal(beforeRequest.status, "failed");
  assert.equal(beforeRequest.targets[0].failureCode, "deadline");
  assert.equal(expired.calls.length, 0);

  const firstExpired = harness({ onExecute: ({ result, state }) => { state.advance(840_000); return result; } });
  const firstReport = await runP7p4SolutionsProbe(options, firstExpired.dependencies);
  assert.equal(firstReport.status, "failed");
  assert.equal(firstReport.targets[0].failureCode, "deadline");
  assert.equal(firstExpired.calls.length, 1);

  const finalExpired = harness({ onExecute: ({ lines, result, state }) => {
    state.advance(lines === 2 ? 839_999 : 2);
    return result;
  } });
  const finalReport = await runP7p4SolutionsProbe(options, finalExpired.dependencies);
  assert.equal(finalReport.status, "failed");
  assert.equal(finalReport.targets[0].status, "passed");
  assert.equal(finalReport.targets[1].failureCode, "deadline");
  assert.equal(finalExpired.calls.length, 2);
  assert.deepEqual(finalExpired.calls.map(({ execution }) => execution.deadlineUnixMs), [841_000, 841_000]);
});

test("a shorter controller ownership deadline bounds both targets and prevents late execution", async () => {
  const expired = harness();
  const expiredReport = await runP7p4SolutionsProbe({ ...options, ownerDeadlineUnixMs: 999 }, expired.dependencies);
  assert.equal(expiredReport.status, "failed");
  assert.equal(expiredReport.deadlineUnixMs, 999);
  assert.equal(expiredReport.targets[0].failureCode, "deadline");
  assert.equal(expired.calls.length, 0);

  const state = harness();
  const report = await runP7p4SolutionsProbe({ ...options, ownerDeadlineUnixMs: 1_150 }, state.dependencies);
  assert.equal(report.status, "failed");
  assert.equal(report.targets[0].status, "passed");
  assert.equal(report.targets[1].failureCode, "deadline");
  assert.equal(report.deadlineUnixMs, 1_150);
  assert.deepEqual(state.calls.map(({ execution }) => execution.deadlineUnixMs), [1_150, 1_150]);

  const laterOwner = harness();
  const laterReport = await runP7p4SolutionsProbe({ ...options, ownerDeadlineUnixMs: 900_000 }, laterOwner.dependencies);
  assert.equal(laterReport.status, "passed");
  assert.equal(laterReport.deadlineUnixMs, 841_000);
  assert.deepEqual(laterOwner.calls.map(({ execution }) => execution.deadlineUnixMs), [841_000, 841_000]);
});

test("the shared deadline includes native artifact validation and result hashing", async () => {
  const state = harness({ onExecute: ({ result, state }) => {
    const source = result.artifact.source;
    let consumed = false;
    Object.defineProperty(result.artifact, "source", { enumerable: true, get() {
      if (!consumed) { state.advance(840_000); consumed = true; }
      return source;
    } });
    return result;
  } });
  const report = await runP7p4SolutionsProbe(options, state.dependencies);
  assert.equal(report.status, "failed");
  assert.equal(report.targets[0].status, "failed");
  assert.equal(report.targets[0].failureCode, "deadline");
  assert.equal(state.calls.length, 1);
});

test("each successful target requires original-source complete Unique output, exact runtime and native artifact", async () => {
  for (const changes of [
    { objective: "all" }, { search_output_policy: "coverage-summary" },
    { rule_profile: "srs" }, { queue_knowledge: "visible-7" },
    { postprocess_pc_path_requested: "true" }, { packing_candidate_is_solution: "true" },
    { buildability_verified: false }, { solution_found: false },
    { count_complete: false }, { solution_count_calculated: false },
    { solution_set_materialized: false }, { solution_keys_complete: false },
    { materialized_pattern_count: 840 }, { materialized_pattern_count: 4_233_600 },
    { materialized_pattern_count: "not-calculated" },
    { normalized_unique_solution_count: 0 }, { normalized_unique_solution_count: "1" },
    { normalized_solution_set_hash: "not-calculated" }, { workers_requested: 8 },
    { workers_used: 0 }, { workers_used: 9 },
  ]) {
    const state = harness({ onExecute: ({ result }) => { mutateSummary(result, changes); return result; } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "failed", JSON.stringify(changes));
    assert.equal(report.targets[0].status, "failed");
    assert.equal(state.calls.length, 1);
  }
  for (const mutate of [
    (payload) => { payload.kind = "pc-path-family.v2"; },
    (payload) => { payload.contract.command.kind = "pc-probability.v2"; },
    (payload) => { payload.runtime_identity.source_commit = "7".repeat(40); },
    (payload) => { payload.runtime_identity.engine_build_id = "7".repeat(40); },
    (payload) => { payload.runtime_identity.supply_semantics_id = "clearra.supply.legacy-v1"; },
  ]) {
    const state = harness({ onExecute: ({ result }) => {
      const payload = JSON.parse(result.stdout);
      mutate(payload);
      result.stdout = JSON.stringify(payload);
      return result;
    } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "failed");
    assert.equal(state.calls.length, 1);
  }
  for (const mutate of [
    (result) => { delete result.artifact; },
    (result) => { result.artifact.complete = false; },
    (result) => { result.artifact.sha256 = "9".repeat(64); },
    (result) => { result.artifact.byteLength += 1; },
    (result) => { result.artifact.solutionCount += 1; },
    (result) => { mutateSummary(result, { normalized_unique_solution_count: 2 }); },
  ]) {
    const state = harness({ onExecute: ({ result }) => { mutate(result); return result; } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "failed");
    assert.equal(state.calls.length, 1);
  }
});

test("4L proves eight active workers while the small 2L target may use fewer", async () => {
  const fewerAtFour = harness({ onExecute: ({ lines, result }) => {
    if (lines === 4) mutateSummary(result, { workers_used: 7 });
    return result;
  } });
  const failed = await runP7p4SolutionsProbe(options, fewerAtFour.dependencies);
  assert.equal(failed.status, "failed");
  assert.equal(failed.targets[0].status, "passed");
  assert.equal(failed.targets[1].status, "failed");
  assert.equal(fewerAtFour.calls.length, 2);
  for (const workers of [1, 4, 8]) {
    const state = harness({ onExecute: ({ lines, result }) => {
      if (lines === 2) mutateSummary(result, { workers_used: workers });
      return result;
    } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "passed");
    assert.equal(report.targets[0].workersUsed, workers);
  }
});

test("complete zero-solution 2L remains successful and proceeds to the same 4L request", async () => {
  for (const workers of [0, 1, 8]) {
    const state = harness({ onExecute: ({ lines, result }) => {
      if (lines === 2) makeEmptyResult(result, workers);
      return result;
    } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "passed");
    assert.equal(state.calls.length, 2);
    assert.equal(report.targets[0].status, "passed");
    assert.equal(report.targets[0].complete, true);
    assert.equal(report.targets[0].solutionCount, 0);
    assert.equal(report.targets[0].solutionSetHash, "cts1:0000000000000000");
    assert.equal(report.targets[0].workersUsed, workers);
    assert.equal(report.targets[0].artifact, null);
    assert.equal(report.targets[1].workersUsed, 8);
    assert.equal(report.targets[1].solutionCount, 2);
    assert.deepEqual(state.calls.map(({ execution }) => execution.deadlineUnixMs), [841_000, 841_000]);
  }
  for (const mutate of [
    (result) => { mutateSummary(result, { solution_found: true }); },
    (result) => { result.artifact = validResult().artifact; },
  ]) {
    const state = harness({ onExecute: ({ result }) => {
      makeEmptyResult(result);
      mutate(result);
      return result;
    } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "failed");
    assert.equal(state.calls.length, 1);
  }
  const emptyFour = harness({ onExecute: ({ lines, result }) => {
    if (lines === 4) makeEmptyResult(result, 8);
    return result;
  } });
  assert.equal((await runP7p4SolutionsProbe(options, emptyFour.dependencies)).status, "failed");
  assert.equal(emptyFour.calls.length, 2);
});

test("execution output and artifact limits reject overflow without retaining raw data", async () => {
  for (const mutate of [
    (result) => { result.stdout = "x".repeat(4 * 1024 ** 2 + 1); },
    (result) => { result.stderr = "x".repeat(4 * 1024 ** 2); },
    (result) => { result.artifact.byteLength = 24 * 1024 ** 2 + 1; },
    (result) => { result.stdout = "invalid JSON"; },
    (result) => { result.exitCode = 137; },
    (result) => { result.signal = "SIGKILL"; },
  ]) {
    const state = harness({ onExecute: ({ result }) => { mutate(result); return result; } });
    const report = await runP7p4SolutionsProbe(options, state.dependencies);
    assert.equal(report.status, "failed");
    assert.equal(state.calls.length, 1);
    assert.ok(Buffer.byteLength(JSON.stringify(report)) < 32 * 1024);
  }
  const state = harness({ onExecute: ({ result }) => {
    result.stderr = "private successful child diagnostics";
    result.untrusted = options.authorizationToken;
    const payload = JSON.parse(result.stdout);
    payload.untrusted = "private payload field";
    result.stdout = JSON.stringify(payload);
    return result;
  } });
  const report = await runP7p4SolutionsProbe(options, state.dependencies);
  assert.equal(report.status, "passed");
  const serialized = JSON.stringify(report);
  assert.ok(!serialized.includes(options.authorizationToken));
  assert.ok(!serialized.includes("private"));
  for (const { result } of state.calls) assert.ok(!serialized.includes(result.artifact.source));
});

test("P7P4 proof imports in a fresh production process with the app's declared codecs", () => {
  const script = new URL("../scripts/run-p7p4-solutions-probe-job.mjs", import.meta.url);
  const result = spawnSync(process.execPath, ["--input-type=module", "-e",
    "const module = await import(process.argv[1]); if (typeof module.runP7p4SolutionsProbe !== 'function' || module.publicP7p4SolutionsPlan().argumentSets.length !== 2) process.exit(1);",
    script.href], {
    env: { ...process.env, ...runtimeEnvironment() },
    encoding: "utf8", timeout: 5_000, maxBuffer: 64 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "");
});

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { encodeCtk3 } from "ctk3";

import { runCloudCandidateSmokeJob } from "../scripts/run-cloud-candidate-smoke-job.mjs";
import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArguments } from "../src/discord/slash-command-input.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityFromRuntime,
} from "../src/job-service/runtime-identity.mjs";

const sourceCommit = "6".repeat(40);
const runtimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);
const options = {
  candidateUrl: "https://candidate---service-test-an.a.run.app",
  sourceCommit,
  authorizationToken: "synthetic-managed-token",
};

function validResult() {
  const source = encodeCtk3({ width: 10, pages: [{ height: 1,
    cells: ["I", "I", "I", "I", null, null, null, null, null, null] }] });
  return {
    exitCode: 0,
    signal: null,
    stdout: JSON.stringify({
      kind: "pc-scenario",
      contract: { command: { kind: "pc-scenario" } },
      runtime_identity: productBuildIdentityFromRuntime(runtimeIdentity),
      summary: {
        objective: "unique", search_output_policy: "trace",
        postprocess_pc_path_requested: "false", packing_candidate_is_solution: "false",
        buildability_verified: true, count_complete: true, solution_count_calculated: true,
        solution_set_materialized: true, solution_found: true,
        normalized_unique_solution_count: 1, normalized_solution_set_hash: "cts1:0000000000000000",
        workers_requested: "auto", workers_used: 1,
      },
    }),
    artifact: {
      contract: "clearra.discord-solution-artifact.v1", mediaType: "text/plain; charset=utf-8",
      filename: "clearra-result.ctk3", source, byteLength: Buffer.byteLength(source),
      sha256: createHash("sha256").update(source).digest("hex"), solutionCount: 1, complete: true,
    },
  };
}

test("managed-secret smoke Job submits one bounded exact-runtime /jobs request", async () => {
  const calls = [];
  class FakeExecutor {
    constructor(options) {
      calls.push({ type: "constructor", options });
    }

    async execute(arguments_, options) {
      calls.push({ type: "execute", arguments_, options });
      return validResult();
    }
  }

  const result = await runCloudCandidateSmokeJob(options, { Executor: FakeExecutor, now: () => 1_000 });
  assert.equal(result.sourceCommit, sourceCommit);
  assert.equal(result.jobId, "candidate-smoke-666666666666-rs");
  assert.equal(result.solutionSetHash, "cts1:0000000000000000");
  assert.equal(calls[0].options.endpoint.href, "https://candidate---service-test-an.a.run.app/jobs");
  assert.equal(calls[0].options.authorizationToken, "synthetic-managed-token");
  assert.equal(calls[0].options.maxOutputBytes, 1024 * 1024);
  assert.equal(calls[0].options.maxArtifactBytes, 1024 * 1024);
  assert.deepEqual(calls[1].arguments_, buildSlashCommandArguments(findSlashCommand("pc").subcommands.solutions, [
    { name: "field", value: "grid:__________" }, { name: "next", value: "IJLOO" },
    { name: "lines", value: 2 }, { name: "hold", value: "disabled" },
  ]));
  assert.equal(calls[1].options.deadlineUnixMs, 61_000);
});

test("candidate smoke requires complete executable Unique output and its matching native solution artifact", async () => {
  for (const change of [
    { objective: "all" }, { search_output_policy: "coverage-summary" },
    { postprocess_pc_path_requested: "true" }, { packing_candidate_is_solution: "true" },
    { buildability_verified: false }, { buildability_verified: undefined },
    { count_complete: false }, { solution_count_calculated: false }, { solution_set_materialized: false },
    { normalized_unique_solution_count: 0 }, { normalized_unique_solution_count: "not-calculated" },
    { normalized_solution_set_hash: "not-calculated" }, { workers_used: 0 }, { workers_used: 9 },
  ]) {
    const result = validResult();
    const payload = JSON.parse(result.stdout);
    Object.assign(payload.summary, change);
    result.stdout = JSON.stringify(payload);
    class InvalidExecutor { async execute() { return result; } }
    await assert.rejects(runCloudCandidateSmokeJob(options,
      { Executor: InvalidExecutor, now: () => 1_000 }), /invalid PC result contract/u, JSON.stringify(change));
  }
  for (const mutate of [
    (result) => { delete result.artifact; },
    (result) => { result.artifact.complete = false; },
    (result) => { result.artifact.source += "invalid"; },
    (result) => {
      const payload = JSON.parse(result.stdout);
      payload.summary.normalized_unique_solution_count = 2;
      result.stdout = JSON.stringify(payload);
    },
  ]) {
    const result = validResult();
    mutate(result);
    class InvalidExecutor { async execute() { return result; } }
    await assert.rejects(runCloudCandidateSmokeJob(options,
      { Executor: InvalidExecutor, now: () => 1_000 }), /solution artifact/u);
  }
});

test("managed-secret smoke Job fails closed without authority or an exact PC result", async () => {
  await assert.rejects(
    runCloudCandidateSmokeJob({
      candidateUrl: "http://candidate.example.test",
      sourceCommit,
      authorizationToken: "token",
    }),
    /credential-free HTTPS run\.app origin/u,
  );
  await assert.rejects(
    runCloudCandidateSmokeJob({
      candidateUrl: "https://candidate---service-test-an.a.run.app",
      sourceCommit,
      authorizationToken: "",
    }),
    /managed Secret binding/u,
  );
  class InvalidExecutor {
    async execute() {
      return { exitCode: 0, signal: null, stdout: "{}" };
    }
  }
  await assert.rejects(
    runCloudCandidateSmokeJob({
      candidateUrl: "https://candidate---service-test-an.a.run.app",
      sourceCommit,
      authorizationToken: "token",
    }, { Executor: InvalidExecutor, now: () => 2_000 }),
    /invalid PC result contract/u,
  );
});

test("managed-secret smoke Job source never prints or serializes its bearer", async () => {
  const source = await readFile(
    new URL("../scripts/run-cloud-candidate-smoke-job.mjs", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(source, /stdout\.write\([^\n]*authorizationToken/u);
  assert.doesNotMatch(source, /JSON\.stringify\([^\n]*authorizationToken/u);
  assert.match(source, /candidate_smoke_job=passed source_commit=.*solution_set_hash=/u);
});

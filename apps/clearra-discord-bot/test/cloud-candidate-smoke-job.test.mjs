import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { runCloudCandidateSmokeJob } from "../scripts/run-cloud-candidate-smoke-job.mjs";
import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityFromRuntime,
} from "../src/job-service/runtime-identity.mjs";

const sourceCommit = "6".repeat(40);
const runtimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);
const productIdentity = productBuildIdentityFromRuntime(runtimeIdentity);
const smokeOptions = Object.freeze({
  candidateUrl: "https://candidate---service-test-an.a.run.app",
  sourceCommit,
  authorizationToken: "synthetic-managed-token",
});

test("managed-secret smoke Job submits both exact-runtime probes under one deadline and seals the original digest", async () => {
  const calls = [];
  class FixtureExecutor extends ClearraJobExecutor {
    constructor(options) {
      super({ ...options, fetch: async (url, request) => {
        assert.equal(url.href, smokeOptions.candidateUrl + "/jobs");
        assert.equal(request.method, "POST");
        assert.equal(request.headers.authorization, "Bearer synthetic-managed-token");
        const job = JSON.parse(request.body);
        assert.equal(request.headers["idempotency-key"], job.id);
        calls.push(job);
        const payload = calls.length === 1 ? genericPcResult() : typedPathResult();
        return Response.json({
          protocol: job.protocol,
          id: job.id,
          state: "completed",
          runtime: runtimeIdentity,
          result: clearraResult(payload),
        });
      } });
    }
  }

  const result = await runCloudCandidateSmokeJob(smokeOptions, {
    Executor: FixtureExecutor,
    now: () => 1_000 + calls.length * 500,
  });
  assert.deepEqual(result, {
    sourceCommit,
    jobId: "candidate-smoke-666666666666-rs",
    solutionSetHash: "cts1:0000000000000000",
  });
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0].arguments, [
    "pc", "--lines", "2", "--queue", "IJLOO", "--fixed", "--no-hold",
  ]);
  assert.deepEqual(calls[1].arguments, [
    "pc", "path", "--lines", "1", "--board-mask", "0x3f0",
    "--height", "1", "--pieces", "1", "--queue", "I", "--no-hold", "--rule", "srs-plus",
    "--backend", "cpu",
  ]);
  assert.deepEqual(calls.map(job => job.id), [result.jobId, result.jobId + "-path"]);
  for (const job of calls) {
    assert.equal(job.timeoutClass, "pc_reverse");
    assert.equal(job.deadlineUnixMs, 61_000);
    assert.deepEqual(job.expectedRuntime, runtimeIdentity);
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

test("managed-secret smoke Job refuses a receipt when the typed probe is wrong or from another runtime", async () => {
  const wrongWitness = typedPathResult();
  wrongWitness.summary.canonical_witness.steps[0].board_before_mask = "0x000000000000003f";
  const wrongRuntime = typedPathResult(
    productBuildIdentityFromRuntime(currentRuntimeIdentityForCommit("7".repeat(40))),
  );
  for (const payload of [genericPcResult(), wrongWitness, wrongRuntime]) {
    class FixtureExecutor {
      async execute(arguments_) {
        return clearraResult(arguments_[1] === "path" ? payload : genericPcResult());
      }
    }
    await assert.rejects(
      runCloudCandidateSmokeJob(smokeOptions, { Executor: FixtureExecutor, now: () => 1_000 }),
      /invalid typed PC path result/u,
    );
  }
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

function clearraResult(payload) {
  return { exitCode: 0, signal: null, stdout: JSON.stringify(payload), stderr: "" };
}

function genericPcResult() {
  return {
    kind: "pc",
    runtime_identity: productIdentity,
    summary: { solution_found: true, normalized_solution_set_hash: "cts1:0000000000000000" },
  };
}

function typedPathResult(runtime = productIdentity) {
  return {
    kind: "pc-path-family.v2",
    contract: { command: { kind: "pc-path-family.v2" } },
    runtime_identity: runtime,
    summary: {
      capability_id: "pc.path",
      result_contract: "pc-path-family.v2",
      payload_kind: "canonical-pc-path-witness",
      witness_contract: "pc-path-witness.v2",
      ordering: "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending",
      canonical_selection: "smallest-canonical-candidate-id",
      problem_id: "smoke",
      complete: true,
      canonical_witness: {
        candidate_id: "1",
        producer_candidate_id: "1",
        pattern_id: "0",
        trace_identity: "trace",
        normalized_trace_key: "trk1:trace",
        consumed_piece_count: "1",
        terminal_hold_piece: null,
        steps: [{
          step_index: "0",
          operation_id: "0",
          active_piece: "I",
          input_cursor: "0",
          output_cursor: "1",
          input_hold_piece: null,
          output_hold_piece: null,
          hold_decision: "none",
          rotation: "0",
          x: "1",
          y: "0",
          placement_mask: "0x000000000000000f",
          board_before_mask: "0x00000000000003f0",
          board_after_placement_mask: "0x00000000000003ff",
          board_after_line_clear_mask: "0x0000000000000000",
          cleared_row_mask: "0x0000000000000001",
          cleared_lines: "1",
          line_clear_identity: "rows:0000000000000001:count:1",
        }],
      },
    },
  };
}

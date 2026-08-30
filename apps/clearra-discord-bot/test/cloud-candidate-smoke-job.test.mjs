import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { runCloudCandidateSmokeJob } from "../scripts/run-cloud-candidate-smoke-job.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityFromRuntime,
} from "../src/job-service/runtime-identity.mjs";

const sourceCommit = "6".repeat(40);
const runtimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);

test("managed-secret smoke Job submits one bounded exact-runtime /jobs request", async () => {
  const calls = [];
  class FakeExecutor {
    constructor(options) {
      calls.push({ type: "constructor", options });
    }

    async execute(arguments_, options) {
      calls.push({ type: "execute", arguments_, options });
      return {
        exitCode: 0,
        signal: null,
        stdout: JSON.stringify({
          kind: "pc",
          runtime_identity: productBuildIdentityFromRuntime(runtimeIdentity),
          summary: {
            solution_found: true,
            normalized_solution_set_hash: "cts1:0000000000000000",
          },
        }),
      };
    }
  }

  const result = await runCloudCandidateSmokeJob({
    candidateUrl: "https://candidate---service-test-an.a.run.app",
    sourceCommit,
    authorizationToken: "synthetic-managed-token",
  }, { Executor: FakeExecutor, now: () => 1_000 });
  assert.equal(result.sourceCommit, sourceCommit);
  assert.equal(result.jobId, "candidate-smoke-666666666666-rs");
  assert.equal(result.solutionSetHash, "cts1:0000000000000000");
  assert.equal(calls[0].options.endpoint.href, "https://candidate---service-test-an.a.run.app/jobs");
  assert.equal(calls[0].options.authorizationToken, "synthetic-managed-token");
  assert.deepEqual(calls[1].arguments_, [
    "pc", "--lines", "2", "--queue", "IJLOO", "--fixed", "--no-hold",
  ]);
  assert.equal(calls[1].options.deadlineUnixMs, 61_000);
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

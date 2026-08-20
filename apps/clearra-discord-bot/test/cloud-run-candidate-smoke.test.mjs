import assert from "node:assert/strict";
import test from "node:test";

import { verifyCloudRunCandidate } from "../scripts/verify-cloud-run-candidate.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityFromRuntime,
} from "../src/job-service/runtime-identity.mjs";

const SOURCE_COMMIT = "7".repeat(40);
const TEST_RUNTIME_IDENTITY = currentRuntimeIdentityForCommit(SOURCE_COMMIT);

test("candidate smoke submits one bounded exact-runtime job without exposing its bearer", async () => {
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
          runtime_identity: productBuildIdentityFromRuntime(TEST_RUNTIME_IDENTITY),
          summary: {
            solution_found: true,
            normalized_solution_set_hash: "cts1:0000000000000000",
          },
        }),
        stderr: "",
      };
    }
  }

  const result = await verifyCloudRunCandidate(
    {
      baseUrl: "https://candidate---clearra.example.run.app",
      sourceCommit: SOURCE_COMMIT,
      authorizationToken: "private-test-token",
    },
    { Executor: FakeExecutor, now: () => 1_000 },
  );

  assert.equal(result.sourceCommit, SOURCE_COMMIT);
  assert.equal(calls.length, 2);
  assert.equal(calls[0].options.endpoint.href, "https://candidate---clearra.example.run.app/jobs");
  assert.equal(calls[0].options.authorizationToken, "private-test-token");
  assert.deepEqual(calls[0].options.expectedRuntimeIdentity, TEST_RUNTIME_IDENTITY);
  assert.deepEqual(calls[1].arguments_, [
    "pc",
    "--lines",
    "2",
    "--queue",
    "IJLOO",
    "--fixed",
    "--no-hold",
  ]);
  assert.equal(calls[1].options.deadlineUnixMs, 61_000);
  assert.match(calls[1].options.jobId, /^candidate-smoke-777777777777-/);
});

test("candidate smoke rejects malformed identity, endpoint, and terminal results", async () => {
  class InvalidResultExecutor {
    async execute() {
      return {
        exitCode: 0,
        signal: null,
        stdout: JSON.stringify({ kind: "pc", summary: { solution_found: false } }),
        stderr: "",
      };
    }
  }

  await assert.rejects(
    verifyCloudRunCandidate({
      baseUrl: "http://candidate.example.test",
      sourceCommit: SOURCE_COMMIT,
      authorizationToken: "token",
    }),
    /credential-free HTTPS/,
  );
  await assert.rejects(
    verifyCloudRunCandidate({
      baseUrl: "https://candidate.example.test",
      sourceCommit: "short",
      authorizationToken: "token",
    }),
    /full lowercase Git SHA/,
  );
  await assert.rejects(
    verifyCloudRunCandidate(
      {
        baseUrl: "https://candidate.example.test",
        sourceCommit: SOURCE_COMMIT,
        authorizationToken: "token",
      },
      { Executor: InvalidResultExecutor, now: () => 2_000 },
    ),
    /invalid PC result contract/,
  );
});

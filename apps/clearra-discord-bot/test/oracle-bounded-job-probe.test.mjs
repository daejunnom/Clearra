import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  ORACLE_BOUNDED_JOB_PROBE_CONTRACT,
  runOracleBoundedJobProbe,
} from "../scripts/run-oracle-bounded-job-probe.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityFromRuntime,
} from "../src/job-service/runtime-identity.mjs";

const sourceCommit = "6".repeat(40);
const deploymentNonce = "9".repeat(64);
const jobUrl = "https://candidate---service-test-an.a.run.app/jobs";
const runtimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);

test("Oracle probe uses one fixed bounded product Job and exact runtime identity", async () => {
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
  const times = [1_000, 1_125];
  const result = await runOracleBoundedJobProbe({
    phase: "candidate",
    jobUrl,
    deploymentNonce,
    expectedSourceCommit: sourceCommit,
    authorizationToken: "synthetic-vault-token",
  }, {
    Executor: FakeExecutor,
    now: () => times.shift(),
  });

  assert.equal(result.contract, ORACLE_BOUNDED_JOB_PROBE_CONTRACT);
  assert.equal(result.phase, "candidate");
  assert.equal(result.jobUrl, jobUrl);
  assert.equal(result.expectedSourceCommit, sourceCommit);
  assert.equal(result.solutionSetHash, "cts1:0000000000000000");
  assert.equal(result.completedAt, "1970-01-01T00:00:01.125Z");
  assert.equal(calls[0].options.endpoint.href, jobUrl);
  assert.equal(calls[0].options.authorizationToken, "synthetic-vault-token");
  assert.deepEqual(calls[1].arguments_, [
    "pc", "--lines", "2", "--queue", "IJLOO", "--fixed", "--no-hold",
  ]);
  assert.equal(calls[1].options.deadlineUnixMs, 61_000);
});

test("rollback probe supports captured legacy runtime while retaining result checks", async () => {
  class LegacyExecutor {
    constructor(options) {
      assert.equal(options.expectedRuntimeIdentity, null);
    }

    async execute() {
      return {
        exitCode: 0,
        signal: null,
        stdout: JSON.stringify({
          kind: "pc",
          summary: {
            solution_found: true,
            normalized_solution_set_hash: "cts1:1111111111111111",
          },
        }),
      };
    }
  }
  const times = [2_000, 2_001];
  const result = await runOracleBoundedJobProbe({
    phase: "rollback",
    jobUrl,
    deploymentNonce,
    expectedSourceCommit: null,
    authorizationToken: "synthetic-vault-token",
  }, { Executor: LegacyExecutor, now: () => times.shift() });
  assert.equal(result.expectedSourceCommit, null);
  assert.equal(result.solutionSetHash, "cts1:1111111111111111");
});

test("Oracle probe and Vault launcher fail closed without exposing credentials", async () => {
  await assert.rejects(
    runOracleBoundedJobProbe({
      phase: "candidate",
      jobUrl,
      deploymentNonce,
      expectedSourceCommit: null,
      authorizationToken: "token",
    }),
    /requires an exact source commit/u,
  );
  await assert.rejects(
    runOracleBoundedJobProbe({
      phase: "candidate",
      jobUrl: "https://example.com/jobs",
      deploymentNonce,
      expectedSourceCommit: sourceCommit,
      authorizationToken: "token",
    }),
    /run\.app/u,
  );
  await assert.rejects(
    runOracleBoundedJobProbe({
      phase: "candidate",
      jobUrl,
      deploymentNonce,
      expectedSourceCommit: sourceCommit,
      authorizationToken: "",
    }),
    /Vault token/u,
  );

  const nodeSource = await readFile(
    new URL("../scripts/run-oracle-bounded-job-probe.mjs", import.meta.url),
    "utf8",
  );
  const launcherSource = await readFile(
    new URL("../scripts/run-oracle-bounded-job-probe", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(nodeSource, /console\.(?:log|error).*authorizationToken/u);
  assert.doesNotMatch(nodeSource, /JSON\.stringify\([^\n]*authorizationToken/u);
  assert.match(launcherSource, /--auth instance_principal/u);
  assert.match(launcherSource, /--user ubuntu/u);
  assert.match(launcherSource, /--kill-after=5s 75s/u);
  assert.doesNotMatch(launcherSource, /eval|sh -c|bash -c/u);
});

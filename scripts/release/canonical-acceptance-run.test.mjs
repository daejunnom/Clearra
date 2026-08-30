import assert from "node:assert/strict";
import test from "node:test";

import {
  resolveCanonicalAcceptanceHistory,
  resolveCanonicalAcceptanceRun,
  validateCanonicalAcceptanceLookup,
} from "./canonical-acceptance-run.mjs";

const SOURCE_COMMIT = "7".repeat(40);
const REPOSITORY = "daejunnom/Clearra";

function canonicalRun(overrides = {}) {
  return {
    id: 123,
    run_attempt: 1,
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
    head_branch: "main",
    head_sha: SOURCE_COMMIT,
    path: ".github/workflows/release-cli.yml",
    ...overrides,
  };
}

function lookup(runs) {
  return { total_count: runs.length, workflow_runs: runs };
}

test("canonical acceptance requires exact zero before a dispatch", () => {
  assert.equal(validateCanonicalAcceptanceLookup(lookup([]), {
    sourceCommit: SOURCE_COMMIT,
    expectedCount: 0,
  }), null);
  assert.throws(
    () => validateCanonicalAcceptanceLookup(lookup([canonicalRun()]), {
      sourceCommit: SOURCE_COMMIT,
      expectedCount: 0,
    }),
    /exactly 0/u,
  );
});

test("canonical acceptance binds exactly one complete run identity", () => {
  assert.deepEqual(validateCanonicalAcceptanceLookup(lookup([canonicalRun()]), {
    sourceCommit: SOURCE_COMMIT,
    expectedCount: 1,
    expectedRunId: "123",
    expectedRunAttempt: "1",
  }), {
    id: "123",
    attempt: "1",
    sourceCommit: SOURCE_COMMIT,
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
    branch: "main",
    path: ".github/workflows/release-cli.yml",
  });

  for (const mutate of [
    (run) => { run.event = "push"; },
    (run) => { run.status = "in_progress"; },
    (run) => { run.conclusion = "failure"; },
    (run) => { run.head_branch = "feature"; },
    (run) => { run.head_sha = "8".repeat(40); },
    (run) => { run.path = ".github/workflows/other.yml"; },
    (run) => { run.id = 0; },
    (run) => { run.run_attempt = 0; },
  ]) {
    const run = canonicalRun();
    mutate(run);
    assert.throws(
      () => validateCanonicalAcceptanceLookup(lookup([run]), {
        sourceCommit: SOURCE_COMMIT,
        expectedCount: 1,
      }),
    );
  }
});

test("canonical acceptance rejects duplicate success and malformed counts", () => {
  assert.throws(
    () => validateCanonicalAcceptanceLookup(
      lookup([canonicalRun(), canonicalRun({ id: 124 })]),
      { sourceCommit: SOURCE_COMMIT, expectedCount: 1 },
    ),
    /exactly 1/u,
  );
  assert.throws(
    () => validateCanonicalAcceptanceLookup(
      { total_count: 1, workflow_runs: [] },
      { sourceCommit: SOURCE_COMMIT, expectedCount: 1 },
    ),
    /exactly 1/u,
  );
  assert.throws(
    () => validateCanonicalAcceptanceLookup(
      { workflow_runs: [canonicalRun()] },
      { sourceCommit: SOURCE_COMMIT, expectedCount: 1 },
    ),
    /total_count/u,
  );
});

test("canonical acceptance rejects a different bound run or attempt", () => {
  assert.throws(
    () => validateCanonicalAcceptanceLookup(lookup([canonicalRun()]), {
      sourceCommit: SOURCE_COMMIT,
      expectedCount: 1,
      expectedRunId: "124",
    }),
    /run ID differs/u,
  );
  assert.throws(
    () => validateCanonicalAcceptanceLookup(lookup([canonicalRun()]), {
      sourceCommit: SOURCE_COMMIT,
      expectedCount: 1,
      expectedRunAttempt: "3",
    }),
    /run attempt differs/u,
  );
});

test("canonical acceptance rejects every workflow rerun attempt", () => {
  assert.throws(
    () => validateCanonicalAcceptanceLookup(
      lookup([canonicalRun({ run_attempt: 2 })]),
      { sourceCommit: SOURCE_COMMIT, expectedCount: 1 },
    ),
    /reruns are forbidden/u,
  );
});

test("resolver preserves a hidden first-attempt success after a failed rerun", async () => {
  const firstAttempt = canonicalRun();
  const latestAttempt = canonicalRun({
    run_attempt: 2,
    status: "completed",
    conclusion: "failure",
  });
  const result = await resolveCanonicalAcceptanceHistory({
    sourceCommit: SOURCE_COMMIT,
    expectedCount: 1,
  }, {
    async listRuns() {
      return lookup([latestAttempt]);
    },
    async getAttempt(runId, runAttempt) {
      assert.equal(runId, "123");
      assert.equal(runAttempt, "1");
      return firstAttempt;
    },
  });
  assert.equal(result.id, "123");
  assert.equal(result.attempt, "1");

  await assert.rejects(
    resolveCanonicalAcceptanceHistory({
      sourceCommit: SOURCE_COMMIT,
      expectedCount: 0,
    }, {
      async listRuns() {
        return lookup([latestAttempt]);
      },
      async getAttempt() {
        return firstAttempt;
      },
    }),
    /exactly 0/u,
  );
});

test("resolver rejects duplicate successes across workflow attempt history", async () => {
  await assert.rejects(
    resolveCanonicalAcceptanceHistory({
      sourceCommit: SOURCE_COMMIT,
      expectedCount: 1,
    }, {
      async listRuns() {
        return lookup([canonicalRun({ run_attempt: 2 })]);
      },
      async getAttempt() {
        return canonicalRun();
      },
    }),
    /exactly 1/u,
  );
});

test("resolver rejects truncated or cross-owned workflow attempt history", async () => {
  await assert.rejects(
    resolveCanonicalAcceptanceHistory({
      sourceCommit: SOURCE_COMMIT,
      expectedCount: 0,
    }, {
      async listRuns() {
        return { total_count: 2, workflow_runs: [canonicalRun()] };
      },
    }),
    /complete and non-truncated/u,
  );

  await assert.rejects(
    resolveCanonicalAcceptanceHistory({
      sourceCommit: SOURCE_COMMIT,
      expectedCount: 1,
    }, {
      async listRuns() {
        return lookup([canonicalRun({
          run_attempt: 2,
          status: "completed",
          conclusion: "failure",
        })]);
      },
      async getAttempt() {
        return canonicalRun({ id: 999 });
      },
    }),
    /history owner/u,
  );
});

test("resolver performs the full exact-SHA lookup with a non-truncating page", async () => {
  const calls = [];
  const result = await resolveCanonicalAcceptanceRun({
    repository: REPOSITORY,
    sourceCommit: SOURCE_COMMIT,
    expectedCount: 1,
  }, {
    run(command, arguments_) {
      calls.push([command, arguments_]);
      return JSON.stringify(lookup([canonicalRun()]));
    },
  });
  assert.equal(result.id, "123");
  assert.deepEqual(calls, [[
    "gh",
    [
      "api", "--method", "GET",
      `repos/${REPOSITORY}/actions/workflows/release-cli.yml/runs`,
      "-f", "event=workflow_dispatch",
      "-f", "branch=main",
      "-f", `head_sha=${SOURCE_COMMIT}`,
      "-f", "per_page=100",
    ],
  ]]);
});

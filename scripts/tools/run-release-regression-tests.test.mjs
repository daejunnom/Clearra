import assert from "node:assert/strict";
import test from "node:test";

import {
  ACTIONS_TEST_WORKER_CAP,
  RELEASE_REGRESSION_TEST_FILES,
  buildReleaseRegressionTestCommand,
  resolveActionsTestWorkers,
  runReleaseRegressionTests,
} from "./run-release-regression-tests.mjs";

test("derives a positive Actions worker budget capped at four logical processors", () => {
  assert.equal(ACTIONS_TEST_WORKER_CAP, 4);
  assert.equal(resolveActionsTestWorkers(1), 1);
  assert.equal(resolveActionsTestWorkers(2), 2);
  assert.equal(resolveActionsTestWorkers(4), 4);
  assert.equal(resolveActionsTestWorkers(64), 4);
  for (const invalid of [0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(
      () => resolveActionsTestWorkers(invalid),
      /positive safe integer/u,
    );
  }
});

test("keeps a duplicate-free list of runnable release regressions", () => {
  assert.equal(
    new Set(RELEASE_REGRESSION_TEST_FILES).size,
    RELEASE_REGRESSION_TEST_FILES.length,
  );
  assert.ok(
    RELEASE_REGRESSION_TEST_FILES.every((path) =>
      /^(?:scripts\/release|scripts\/tools|scripts\/windows)\/[a-z0-9./-]+\.test\.mjs$/u.test(path)),
  );
});

test("builds one shell-free Node test pool with explicit bounded file concurrency", () => {
  const invocation = buildReleaseRegressionTestCommand(12);
  assert.equal(invocation.command, process.execPath);
  assert.deepEqual(invocation.args, [
    "--test",
    "--test-concurrency=4",
    "--",
    ...RELEASE_REGRESSION_TEST_FILES,
  ]);
});

test("runs the complete pool exactly once and propagates its failure", () => {
  const calls = [];
  const passed = runReleaseRegressionTests({
    logicalProcessors: 3,
    repositoryRoot: "C:/fixture/repository",
    spawnImplementation(command, args, options) {
      calls.push({ command, args, options });
      return { status: 0, signal: null };
    },
  });
  assert.equal(passed.workers, 3);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].options.shell, false);
  assert.equal(calls[0].options.stdio, "inherit");
  assert.throws(
    () =>
      runReleaseRegressionTests({
        logicalProcessors: 2,
        spawnImplementation: () => ({ status: 9, signal: null }),
      }),
    /exit code 9/u,
  );
});

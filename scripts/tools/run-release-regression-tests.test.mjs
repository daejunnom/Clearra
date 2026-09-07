import assert from "node:assert/strict";
import test from "node:test";

import {
  ACTIONS_TEST_WORKER_CAP,
  RELEASE_REGRESSION_TEST_FILES,
  buildReleaseRegressionTestCommand,
  resolveActionsTestWorkers,
  runReleaseRegressionTests,
} from "./run-release-regression-tests.mjs";

const EXPECTED_RELEASE_REGRESSIONS = Object.freeze([
  "scripts/release/accepted-wasm-build.test.mjs",
  "scripts/release/accepted-pages-build.test.mjs",
  "scripts/release/canonical-acceptance-evidence.test.mjs",
  "scripts/release/canonical-acceptance-run.test.mjs",
  "scripts/release/candidate-preflight-workflow.test.mjs",
  "scripts/release/candidate-preflight-artifacts.test.mjs",
  "scripts/release/candidate-preflight-regressions.test.mjs",
  "scripts/release/cloud/benchmark-cli-parity-v080.test.mjs",
  "scripts/release/create-exact-source-archive.test.mjs",
  "scripts/release/deployment-impact.test.mjs",
  "scripts/release/fast-fix-qualification-evidence.test.mjs",
  "scripts/release/fast-fix-qualification-workflow.test.mjs",
  "scripts/release/discord-catalog-recovery-authority.test.mjs",
  "scripts/release/discord-deploy-workflow.test.mjs",
  "scripts/release/discord-deployment-recovery.test.mjs",
  "scripts/release/discord-deployment-state.test.mjs",
  "scripts/release/discord-production-checkpoint-receipt.test.mjs",
  "scripts/release/discord-recovery-debt.test.mjs",
  "scripts/release/final-source-attempt-journal.test.mjs",
  "scripts/release/final-source-event-contract.test.mjs",
  "scripts/release/final-source-stage-evidence.test.mjs",
  "scripts/release/finalize-discord-production-checkpoint.test.mjs",
  "scripts/release/observe-production-surfaces.test.mjs",
  "scripts/release/oracle-inactive-stage-v080.test.mjs",
  "scripts/release/oracle/create-prestage-helper-bundle.test.mjs",
  "scripts/release/oracle/invoke-release-deploy-v080.test.mjs",
  "scripts/release/pages-deployment-authority.test.mjs",
  "scripts/release/pages-legacy-contract.test.mjs",
  "scripts/release/pages-rollback-authority.test.mjs",
  "scripts/release/pages-rollback-package.test.mjs",
  "scripts/release/queue-pages-publication.test.mjs",
  "scripts/release/release-publication-evidence.test.mjs",
  "scripts/release/release-failure-summary.test.mjs",
  "scripts/release/validate-final-source-revalidation.test.mjs",
  "scripts/release/validate-release-metadata.test.mjs",
  "scripts/release/verify-remote-annotated-tag.test.mjs",
  "scripts/tools/run-focused-js-tests.test.mjs",
  "scripts/tools/run-gui-experiment.test.mjs",
  "scripts/tools/import-verified-clearra-wasm.test.mjs",
  "scripts/tools/retain-clearra-debug-builds.test.mjs",
  "scripts/tools/run-release-regression-tests.test.mjs",
  "scripts/tools/validate-release-cli-smokes.test.mjs",
  "scripts/windows/clearra-local-services-watchdog.test.mjs",
]);

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

test("keeps one closed duplicate-free manifest for every independent release regression", () => {
  assert.deepEqual(RELEASE_REGRESSION_TEST_FILES, EXPECTED_RELEASE_REGRESSIONS);
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
    ...EXPECTED_RELEASE_REGRESSIONS,
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

import { spawnSync } from "node:child_process";
import { availableParallelism } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const ACTIONS_TEST_WORKER_CAP = 4;

export const RELEASE_REGRESSION_TEST_FILES = Object.freeze([
  "scripts/release/accepted-wasm-build.test.mjs",
  "scripts/release/accepted-pages-build.test.mjs",
  "scripts/release/canonical-acceptance-evidence.test.mjs",
  "scripts/release/canonical-acceptance-run.test.mjs",
  "scripts/release/candidate-preflight-workflow.test.mjs",
  "scripts/release/candidate-preflight-artifacts.test.mjs",
  "scripts/release/candidate-preflight-regressions.test.mjs",
  "scripts/release/cloud/benchmark-cli-parity-v080.test.mjs",
  "scripts/tools/benchmark-qnia-cpsat.test.mjs",
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
  "scripts/release/release-publication-evidence.test.mjs",
  "scripts/release/validate-final-source-revalidation.test.mjs",
  "scripts/release/validate-release-metadata.test.mjs",
  "scripts/release/verify-remote-annotated-tag.test.mjs",
  "scripts/tools/run-focused-js-tests.test.mjs",
  "scripts/tools/import-verified-clearra-wasm.test.mjs",
  "scripts/tools/retain-clearra-debug-builds.test.mjs",
  "scripts/tools/run-release-regression-tests.test.mjs",
  "scripts/tools/validate-release-cli-smokes.test.mjs",
  "scripts/windows/clearra-local-services-watchdog.test.mjs",
]);

const REPOSITORY_ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)));

export function resolveActionsTestWorkers(logicalProcessors) {
  if (!Number.isSafeInteger(logicalProcessors) || logicalProcessors < 1) {
    throw new Error("logical processor count must be a positive safe integer");
  }
  return Math.min(ACTIONS_TEST_WORKER_CAP, logicalProcessors);
}

export function buildReleaseRegressionTestCommand(logicalProcessors) {
  const workers = resolveActionsTestWorkers(logicalProcessors);
  return Object.freeze({
    command: process.execPath,
    args: Object.freeze([
      "--test",
      `--test-concurrency=${workers}`,
      "--",
      ...RELEASE_REGRESSION_TEST_FILES,
    ]),
    workers,
  });
}

export function runReleaseRegressionTests({
  logicalProcessors = availableParallelism(),
  repositoryRoot = REPOSITORY_ROOT,
  spawnImplementation = spawnSync,
} = {}) {
  const invocation = buildReleaseRegressionTestCommand(logicalProcessors);
  process.stdout.write(
    `release_regressions=start worker_count=${invocation.workers} ` +
      `worker_cap=${ACTIONS_TEST_WORKER_CAP} file_count=${RELEASE_REGRESSION_TEST_FILES.length}\n`,
  );
  const result = spawnImplementation(invocation.command, invocation.args, {
    cwd: repositoryRoot,
    shell: false,
    stdio: "inherit",
    windowsHide: true,
  });
  if (result?.error) {
    throw result.error;
  }
  if (result?.status !== 0) {
    const outcome = result?.signal === null || result?.signal === undefined
      ? `exit code ${String(result?.status)}`
      : `signal ${result.signal}`;
    throw new Error(`release regression test pool failed with ${outcome}`);
  }
  process.stdout.write(
    `release_regressions=passed worker_count=${invocation.workers} ` +
      `file_count=${RELEASE_REGRESSION_TEST_FILES.length}\n`,
  );
  return invocation;
}

const isMain =
  process.argv[1] !== undefined &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url;

if (isMain) {
  try {
    if (process.argv.length !== 2) {
      throw new Error("release regression runner does not accept arguments");
    }
    runReleaseRegressionTests();
  } catch (error) {
    process.stderr.write(`release_regressions=failed reason=${error.message}\n`);
    process.exitCode = 1;
  }
}

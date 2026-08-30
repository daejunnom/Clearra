import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const SOURCE_COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const REPOSITORY_PATTERN = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const DECIMAL_ID_PATTERN = /^[1-9][0-9]*$/u;
const WORKFLOW_PATH = ".github/workflows/release-cli.yml";
const MAX_HISTORY_ATTEMPTS = 100;

export function validateCanonicalAcceptanceLookup(value, options) {
  const sourceCommit = canonicalSourceCommit(options?.sourceCommit);
  const expectedCount = canonicalExpectedCount(options?.expectedCount);
  const expectedRunId = optionalDecimalId(
    options?.expectedRunId,
    "expected canonical run ID",
  );
  const expectedRunAttempt = optionalDecimalId(
    options?.expectedRunAttempt,
    "expected canonical run attempt",
  );
  const label = nonEmptyString(options?.label ?? "canonical acceptance lookup", "label");

  if (expectedCount === 0 && (expectedRunId || expectedRunAttempt)) {
    throw new Error(`${label} cannot bind a run while requiring zero successes`);
  }
  requirePlainObject(value, label);
  if (!Number.isSafeInteger(value.total_count) || value.total_count < 0) {
    throw new Error(`${label}.total_count must be a nonnegative safe integer`);
  }
  if (!Array.isArray(value.workflow_runs)) {
    throw new Error(`${label}.workflow_runs must be an array`);
  }
  if (value.total_count !== expectedCount || value.workflow_runs.length !== expectedCount) {
    throw new Error(
      `${label} must contain exactly ${expectedCount} successful exact-SHA canonical run(s)`,
    );
  }
  if (expectedCount === 0) return null;

  const run = value.workflow_runs[0];
  requirePlainObject(run, `${label}.workflow_runs[0]`);
  if (
    run.event !== "workflow_dispatch" ||
    run.status !== "completed" ||
    run.conclusion !== "success" ||
    run.head_branch !== "main" ||
    run.head_sha !== sourceCommit ||
    run.path !== WORKFLOW_PATH
  ) {
    throw new Error(`${label} does not contain the exact canonical run identity`);
  }
  const id = decimalId(run.id, `${label} run ID`);
  const attempt = decimalId(run.run_attempt, `${label} run attempt`);
  if (attempt !== "1") {
    throw new Error(`${label} must use the first workflow attempt; reruns are forbidden`);
  }
  if (expectedRunId && id !== expectedRunId) {
    throw new Error(`${label} run ID differs from the bound acceptance`);
  }
  if (expectedRunAttempt && attempt !== expectedRunAttempt) {
    throw new Error(`${label} run attempt differs from the bound acceptance`);
  }
  return Object.freeze({
    id,
    attempt,
    sourceCommit,
    event: run.event,
    status: run.status,
    conclusion: run.conclusion,
    branch: run.head_branch,
    path: run.path,
  });
}

export async function resolveCanonicalAcceptanceHistory(options, dependencies) {
  const sourceCommit = canonicalSourceCommit(options?.sourceCommit);
  const expectedCount = canonicalExpectedCount(options?.expectedCount);
  const label = nonEmptyString(
    options?.label ?? "canonical acceptance history",
    "label",
  );
  if (typeof dependencies?.listRuns !== "function") {
    throw new Error(`${label} requires a workflow run list provider`);
  }
  const list = await dependencies.listRuns();
  requirePlainObject(list, `${label} run list`);
  if (!Number.isSafeInteger(list.total_count) || list.total_count < 0) {
    throw new Error(`${label} run list total_count is invalid`);
  }
  if (
    !Array.isArray(list.workflow_runs) ||
    list.workflow_runs.length !== list.total_count
  ) {
    throw new Error(`${label} run list must be complete and non-truncated`);
  }

  const successfulAttempts = [];
  let historyAttemptCount = 0;
  for (const [runIndex, latestAttempt] of list.workflow_runs.entries()) {
    const latest = validateHistoryAttempt(latestAttempt, {
      sourceCommit,
      label: `${label} run ${runIndex}`,
    });
    const maximumAttempt = positiveSafeInteger(
      latestAttempt.run_attempt,
      `${label} run ${runIndex} latest attempt`,
    );
    historyAttemptCount += maximumAttempt;
    if (historyAttemptCount > MAX_HISTORY_ATTEMPTS) {
      throw new Error(`${label} exceeds the closed attempt history limit`);
    }
    for (let attemptNumber = 1; attemptNumber <= maximumAttempt; attemptNumber += 1) {
      let attempt = latestAttempt;
      if (attemptNumber !== maximumAttempt) {
        if (typeof dependencies.getAttempt !== "function") {
          throw new Error(`${label} requires a historical attempt provider`);
        }
        attempt = await dependencies.getAttempt(latest.id, String(attemptNumber));
      }
      const identity = validateHistoryAttempt(attempt, {
        sourceCommit,
        expectedRunId: latest.id,
        expectedRunAttempt: String(attemptNumber),
        label: `${label} run ${latest.id} attempt ${attemptNumber}`,
      });
      if (identity.status === "completed" && identity.conclusion === "success") {
        successfulAttempts.push(attempt);
      }
    }
  }

  return validateCanonicalAcceptanceLookup({
    total_count: successfulAttempts.length,
    workflow_runs: successfulAttempts,
  }, {
    sourceCommit,
    expectedCount,
    expectedRunId: options?.expectedRunId,
    expectedRunAttempt: options?.expectedRunAttempt,
    label,
  });
}

export async function resolveCanonicalAcceptanceRun(options, dependencies = {}) {
  const repository = canonicalRepository(options?.repository);
  const sourceCommit = canonicalSourceCommit(options?.sourceCommit);
  const expectedCount = canonicalExpectedCount(options?.expectedCount);
  const runCommand = dependencies.run ?? run;
  return resolveCanonicalAcceptanceHistory({
    sourceCommit,
    expectedCount,
    expectedRunId: options?.expectedRunId,
    expectedRunAttempt: options?.expectedRunAttempt,
  }, {
    async listRuns() {
      return parseCommandJson(await runCommand("gh", [
        "api",
        "--method",
        "GET",
        `repos/${repository}/actions/workflows/release-cli.yml/runs`,
        "-f",
        "event=workflow_dispatch",
        "-f",
        "branch=main",
        "-f",
        `head_sha=${sourceCommit}`,
        "-f",
        "per_page=100",
      ]));
    },
    async getAttempt(runId, runAttempt) {
      return parseCommandJson(await runCommand("gh", [
        "api",
        "--method",
        "GET",
        `repos/${repository}/actions/runs/${runId}/attempts/${runAttempt}`,
      ]));
    },
  });
}

function validateHistoryAttempt(value, options) {
  const label = options.label;
  requirePlainObject(value, label);
  const id = decimalId(value.id, `${label} run ID`);
  const attempt = decimalId(value.run_attempt, `${label} run attempt`);
  if (options.expectedRunId !== undefined && id !== options.expectedRunId) {
    throw new Error(`${label} run ID differs from its history owner`);
  }
  if (
    options.expectedRunAttempt !== undefined &&
    attempt !== options.expectedRunAttempt
  ) {
    throw new Error(`${label} number differs from its history position`);
  }
  if (
    value.event !== "workflow_dispatch" ||
    value.head_branch !== "main" ||
    value.head_sha !== options.sourceCommit ||
    value.path !== WORKFLOW_PATH ||
    typeof value.status !== "string" ||
    value.status.length === 0 ||
    (value.conclusion !== null && typeof value.conclusion !== "string")
  ) {
    throw new Error(`${label} differs from the exact canonical run authority`);
  }
  return Object.freeze({
    id,
    attempt,
    status: value.status,
    conclusion: value.conclusion,
  });
}

function positiveSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${label} must be a positive safe integer`);
  }
  return value;
}

function parseCommandJson(value) {
  try {
    return JSON.parse(value);
  } catch {
    throw new Error("canonical acceptance command returned invalid JSON");
  }
}

function canonicalRepository(value) {
  const repository = typeof value === "string" ? value.trim() : "";
  if (!REPOSITORY_PATTERN.test(repository)) {
    throw new Error("repository must use the owner/name form");
  }
  return repository;
}

function canonicalSourceCommit(value) {
  const sourceCommit = typeof value === "string" ? value.trim() : "";
  if (!SOURCE_COMMIT_PATTERN.test(sourceCommit)) {
    throw new Error("source commit must be a full lowercase Git SHA");
  }
  return sourceCommit;
}

function canonicalExpectedCount(value) {
  if (value !== 0 && value !== 1) {
    throw new Error("canonical acceptance expected count must be zero or one");
  }
  return value;
}

function decimalId(value, label) {
  const text = typeof value === "number" && Number.isSafeInteger(value)
    ? String(value)
    : typeof value === "string" ? value : "";
  if (!DECIMAL_ID_PATTERN.test(text)) {
    throw new Error(`${label} must be a positive decimal integer`);
  }
  return text;
}

function optionalDecimalId(value, label) {
  return value === undefined ? undefined : decimalId(value, label);
}

function nonEmptyString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be non-empty`);
  }
  return value;
}

function requirePlainObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
}

function run(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error(`canonical acceptance command failed: ${command}`);
  }
  return result.stdout;
}

function formatResult(result, expectedCount, format) {
  if (format === "github-output") {
    if (expectedCount === 0) return "canonical_acceptance_count=0";
    return [
      "canonical_acceptance_count=1",
      `accepted_run_id=${result.id}`,
      `accepted_run_attempt=${result.attempt}`,
    ].join("\n");
  }
  if (format !== "summary") {
    throw new Error("canonical acceptance format must be summary or github-output");
  }
  return expectedCount === 0
    ? "canonical_acceptance=passed count=0"
    : `canonical_acceptance=passed count=1 run_id=${result.id} run_attempt=${result.attempt}`;
}

async function main() {
  const { values } = parseArgs({
    options: {
      repository: { type: "string" },
      "source-commit": { type: "string" },
      require: { type: "string" },
      "expected-run-id": { type: "string" },
      "expected-run-attempt": { type: "string" },
      format: { type: "string", default: "summary" },
    },
    strict: true,
  });
  try {
    const expectedCount = values.require === "zero"
      ? 0
      : values.require === "one" ? 1 : undefined;
    const result = await resolveCanonicalAcceptanceRun({
      repository: values.repository,
      sourceCommit: values["source-commit"],
      expectedCount,
      expectedRunId: values["expected-run-id"],
      expectedRunAttempt: values["expected-run-attempt"],
    });
    console.log(formatResult(result, expectedCount, values.format));
  } catch {
    console.error("canonical_acceptance=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

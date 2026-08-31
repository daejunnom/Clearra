#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { lstat, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { canonicalJson } from "./canonical-release-evidence.mjs";
import { validatePagesDeploymentAuthorityReport } from "./pages-deployment-authority.mjs";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const WORKFLOW_PATH = ".github/workflows/pages.yml";

export async function resolvePagesDeploymentRun(options, dependencies = {}) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(
    options?.sourceCommit,
    SOURCE_COMMIT,
    "Pages deployment source commit",
  );
  const runCommand = dependencies.run ?? run;
  const response = parseCommandJson(await runCommand("gh", [
    "api",
    "--method",
    "GET",
    `repos/${repository}/actions/workflows/pages.yml/runs`,
    "-f",
    "event=workflow_dispatch",
    "-f",
    "branch=main",
    "-f",
    `head_sha=${sourceCommit}`,
    "-f",
    "per_page=100",
  ]));
  return validatePagesDeploymentRunList(response, { repository, sourceCommit });
}

export function validatePagesDeploymentRunList(value, options) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(
    options?.sourceCommit,
    SOURCE_COMMIT,
    "Pages deployment source commit",
  );
  requirePlainObject(value, "Pages deployment run list");
  if (
    !Number.isSafeInteger(value.total_count) ||
    value.total_count < 0 ||
    !Array.isArray(value.workflow_runs) ||
    value.workflow_runs.length !== value.total_count
  ) {
    throw new Error("Pages deployment run list must be complete and non-truncated");
  }
  const successful = [];
  for (const [index, run_] of value.workflow_runs.entries()) {
    requirePlainObject(run_, `Pages deployment run ${index}`);
    const id = requireDecimalId(run_.id, `Pages deployment run ${index} ID`);
    const attempt = requireDecimalId(
      run_.run_attempt,
      `Pages deployment run ${index} attempt`,
    );
    if (
      run_.event !== "workflow_dispatch" ||
      run_.head_branch !== "main" ||
      run_.head_sha !== sourceCommit ||
      run_.path !== WORKFLOW_PATH ||
      run_.head_repository?.full_name !== repository ||
      typeof run_.status !== "string" ||
      run_.status.length === 0 ||
      (run_.conclusion !== null && typeof run_.conclusion !== "string")
    ) {
      throw new Error("Pages deployment run differs from the exact same-repository authority");
    }
    if (attempt !== "1") {
      throw new Error("Pages deployment rerun attempts are forbidden");
    }
    if (run_.status === "completed" && run_.conclusion === "success") {
      successful.push({ id, attempt });
    }
  }
  if (successful.length !== 1) {
    throw new Error("Pages deployment authority requires exactly one successful exact-SHA run");
  }
  const result = successful[0];
  return Object.freeze({
    id: result.id,
    attempt: result.attempt,
    artifactName:
      `clearra-pages-deployment-authority-${sourceCommit}` +
      `-run-${result.id}-attempt-${result.attempt}`,
  });
}

export async function verifyPagesDeploymentReport(path, options) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(
    options?.sourceCommit,
    SOURCE_COMMIT,
    "Pages deployment source commit",
  );
  const workflowRunId = requireDecimalId(
    options?.workflowRunId,
    "Pages workflow run ID",
  );
  const workflowRunAttempt = requireDecimalId(
    options?.workflowRunAttempt,
    "Pages workflow run attempt",
  );
  const acceptedRunId = requireDecimalId(
    options?.acceptedRunId,
    "Pages accepted run ID",
  );
  const acceptedRunAttempt = requireDecimalId(
    options?.acceptedRunAttempt,
    "Pages accepted run attempt",
  );
  if (workflowRunAttempt !== "1") {
    throw new Error("Pages deployment rerun attempts are forbidden");
  }
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error("Pages deployment authority must be a regular non-link file");
  }
  const raw = await readFile(target, "utf8");
  let report;
  try {
    report = JSON.parse(raw);
  } catch {
    throw new Error("Pages deployment authority is not valid JSON");
  }
  if (raw !== `${canonicalJson(report)}\n`) {
    throw new Error("Pages deployment authority bytes are not canonical JSON");
  }
  validatePagesDeploymentAuthorityReport(report, { expectedSourceCommit: sourceCommit });
  if (
    report.mode !== "forward" ||
    report.repository !== repository ||
    report.workflow_source_commit !== sourceCommit ||
    report.workflow_run_id !== workflowRunId ||
    report.workflow_run_attempt !== workflowRunAttempt ||
    report.accepted_run_id !== acceptedRunId ||
    report.accepted_run_attempt !== acceptedRunAttempt
  ) {
    throw new Error("Pages deployment authority differs from the exact accepted run binding");
  }
  return Object.freeze(report);
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Pages deployment authority path uses a link or non-directory");
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function parseCommandJson(value) {
  try {
    return JSON.parse(value);
  } catch {
    throw new Error("Pages deployment run lookup returned invalid JSON");
  }
}

function requirePattern(value, pattern, label) {
  const text = typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function requireDecimalId(value, label) {
  const text = typeof value === "number" && Number.isSafeInteger(value)
    ? String(value)
    : typeof value === "string" ? value : "";
  if (!DECIMAL_ID.test(text)) throw new Error(`${label} is invalid`);
  return text;
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
    throw new Error(`Pages deployment run lookup failed: ${command}`);
  }
  return result.stdout;
}

function formatResult(result, format) {
  if (format === "github-output") {
    return [
      `pages_run_id=${result.id}`,
      `pages_run_attempt=${result.attempt}`,
      `pages_artifact_name=${result.artifactName}`,
    ].join("\n");
  }
  if (format !== "summary") throw new Error("Pages run format is invalid");
  return `pages_deployment=resolved run_id=${result.id} run_attempt=${result.attempt}`;
}

async function main() {
  const { values, positionals } = parseArgs({
    options: {
      repository: { type: "string" },
      "source-commit": { type: "string" },
      "workflow-run-id": { type: "string" },
      "workflow-run-attempt": { type: "string" },
      "accepted-run-id": { type: "string" },
      "accepted-run-attempt": { type: "string" },
      report: { type: "string" },
      format: { type: "string", default: "summary" },
    },
    strict: true,
    allowPositionals: true,
  });
  try {
    if (positionals.length !== 1) throw new Error("one Pages operation is required");
    if (positionals[0] === "resolve") {
      const result = await resolvePagesDeploymentRun({
        repository: values.repository,
        sourceCommit: values["source-commit"],
      });
      process.stdout.write(`${formatResult(result, values.format)}\n`);
    } else if (positionals[0] === "verify-report") {
      await verifyPagesDeploymentReport(values.report, {
        repository: values.repository,
        sourceCommit: values["source-commit"],
        workflowRunId: values["workflow-run-id"],
        workflowRunAttempt: values["workflow-run-attempt"],
        acceptedRunId: values["accepted-run-id"],
        acceptedRunAttempt: values["accepted-run-attempt"],
      });
      process.stdout.write("pages_deployment_authority=verified\n");
    } else {
      throw new Error("Pages operation must be resolve or verify-report");
    }
  } catch (error) {
    process.stderr.write(
      `pages_deployment_authority=failed reason=${
        error instanceof Error ? error.message : String(error)
      }\n`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

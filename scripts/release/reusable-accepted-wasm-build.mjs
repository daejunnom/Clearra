import { spawnSync } from "node:child_process";
import { appendFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const DECIMAL_ID = /^[1-9][0-9]{0,19}$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const ARTIFACT_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const WORKFLOW_PATH = ".github/workflows/release-cli.yml";
const WASM_JOB = "release-acceptance-wasm-build";
const WASM_UPLOAD_STEP = "Upload accepted WASM build";
const MAX_ITEMS = 100;
const MAX_ARTIFACT_BYTES = 128 * 1024 * 1024;
const REUSABLE_CONCLUSIONS = new Set(["failure", "cancelled", "timed_out"]);

export async function resolveReusableAcceptedWasmBuild(options, dependencies) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(
    options?.sourceCommit,
    SOURCE_COMMIT,
    "WASM reuse source commit",
  );
  const currentRunId = decimalId(options?.currentRunId, "current run ID");
  const currentRunAttempt = decimalId(
    options?.currentRunAttempt,
    "current run attempt",
  );
  if (currentRunAttempt !== "1") {
    throw new Error("WASM reuse is available only to a first canonical run attempt");
  }
  if (
    typeof dependencies?.listRuns !== "function" ||
    typeof dependencies?.listJobs !== "function" ||
    typeof dependencies?.listArtifacts !== "function"
  ) {
    throw new Error("WASM reuse resolver dependencies are incomplete");
  }

  const runList = validateCollection(
    await dependencies.listRuns(),
    "workflow_runs",
    "WASM reuse run history",
  );
  const runs = runList.map((run, index) => validateRun(run, {
    repository,
    sourceCommit,
    label: `WASM reuse run ${index}`,
  }));
  const current = runs.filter((run) => run.id === currentRunId);
  if (
    current.length !== 1 ||
    current[0].attempt !== currentRunAttempt ||
    current[0].status === "completed"
  ) {
    throw new Error("WASM reuse history does not contain the one active current run");
  }
  if (runs.some((run) => run.id !== currentRunId && run.conclusion === "success")) {
    throw new Error("WASM reuse cannot bypass an existing successful canonical run");
  }

  const candidates = runs
    .filter((run) =>
      run.id !== currentRunId &&
      run.attempt === "1" &&
      run.status === "completed" &&
      REUSABLE_CONCLUSIONS.has(run.conclusion)
    )
    .sort((left, right) => right.createdAt - left.createdAt || compareDecimal(right.id, left.id));

  for (const candidate of candidates) {
    try {
      const jobs = validateCollection(
        await dependencies.listJobs(candidate.id, candidate.attempt),
        "jobs",
        `WASM reuse jobs for run ${candidate.id}`,
      );
      const artifacts = validateCollection(
        await dependencies.listArtifacts(candidate.id),
        "artifacts",
        `WASM reuse artifacts for run ${candidate.id}`,
      );
      return validateReusableCandidate({
        repository,
        sourceCommit,
        candidate,
        jobs,
        artifacts,
      });
    } catch {
      // A malformed, incomplete, expired or deleted candidate has no authority.
      // Continue only to an independently complete older exact-source attempt.
    }
  }
  return null;
}

function validateRun(value, { sourceCommit, label }) {
  requireObject(value, label);
  const id = decimalId(value.id, `${label} ID`);
  const attempt = decimalId(value.run_attempt, `${label} attempt`);
  if (
    value.event !== "workflow_dispatch" ||
    value.head_branch !== "main" ||
    value.head_sha !== sourceCommit ||
    value.path !== WORKFLOW_PATH ||
    typeof value.status !== "string" ||
    value.status.length === 0 ||
    (value.conclusion !== null && typeof value.conclusion !== "string")
  ) {
    throw new Error(`${label} differs from the exact canonical workflow identity`);
  }
  return Object.freeze({
    id,
    attempt,
    status: value.status,
    conclusion: value.conclusion,
    createdAt: timestamp(value.created_at, `${label} created_at`),
  });
}

function validateReusableCandidate({ repository, sourceCommit, candidate, jobs, artifacts }) {
  const matches = jobs.filter((job) => job?.name === WASM_JOB);
  if (matches.length !== 1) {
    throw new Error("WASM reuse candidate must contain exactly one producer job");
  }
  const job = matches[0];
  requireObject(job, "WASM reuse producer job");
  const startedAt = timestamp(job.started_at, "WASM reuse producer started_at");
  const completedAt = timestamp(job.completed_at, "WASM reuse producer completed_at");
  if (
    decimalId(job.run_id, "WASM reuse producer run ID") !== candidate.id ||
    decimalId(job.run_attempt, "WASM reuse producer attempt") !== candidate.attempt ||
    job.head_sha !== sourceCommit ||
    job.status !== "completed" ||
    job.conclusion !== "success" ||
    completedAt < startedAt ||
    !Array.isArray(job.steps)
  ) {
    throw new Error("WASM reuse producer job is not a completed exact-source success");
  }
  const uploads = job.steps.filter((step) => step?.name === WASM_UPLOAD_STEP);
  if (
    uploads.length !== 1 ||
    uploads[0].status !== "completed" ||
    uploads[0].conclusion !== "success"
  ) {
    throw new Error("WASM reuse producer upload was not successful");
  }

  const artifactName = artifactNameFor(sourceCommit, candidate.id, candidate.attempt);
  const artifactMatches = artifacts.filter((artifact) => artifact?.name === artifactName);
  if (artifactMatches.length !== 1) {
    throw new Error("WASM reuse candidate artifact is missing or ambiguous");
  }
  const artifact = artifactMatches[0];
  requireObject(artifact, "WASM reuse artifact");
  const artifactId = decimalId(artifact.id, "WASM reuse artifact ID");
  const size = artifact.size_in_bytes;
  const createdAt = timestamp(artifact.created_at, "WASM reuse artifact created_at");
  const expectedDownload =
    `https://api.github.com/repos/${repository}/actions/artifacts/${artifactId}/zip`;
  if (
    artifact.expired !== false ||
    !Number.isSafeInteger(size) ||
    size <= 0 ||
    size > MAX_ARTIFACT_BYTES ||
    !ARTIFACT_DIGEST.test(artifact.digest ?? "") ||
    artifact.archive_download_url !== expectedDownload ||
    createdAt < startedAt ||
    createdAt > completedAt ||
    decimalId(artifact.workflow_run?.id, "WASM reuse artifact run ID") !== candidate.id ||
    artifact.workflow_run?.head_branch !== "main" ||
    artifact.workflow_run?.head_sha !== sourceCommit
  ) {
    throw new Error("WASM reuse artifact does not match its producer authority");
  }
  return Object.freeze({
    runId: candidate.id,
    runAttempt: candidate.attempt,
    artifactId,
    artifactName,
    artifactDigest: artifact.digest,
    artifactBytes: size,
  });
}

function validateCollection(value, key, label) {
  requireObject(value, label);
  if (
    !Number.isSafeInteger(value.total_count) ||
    value.total_count < 0 ||
    value.total_count > MAX_ITEMS ||
    !Array.isArray(value[key]) ||
    value[key].length !== value.total_count
  ) {
    throw new Error(`${label} must be complete and bounded`);
  }
  return value[key];
}

function artifactNameFor(sourceCommit, runId, runAttempt) {
  return `accepted-wasm-build-${sourceCommit}-run-${runId}-attempt-${runAttempt}`;
}

function requireObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
}

function requirePattern(value, pattern, label) {
  const text = typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} has an invalid format`);
  return text;
}

function decimalId(value, label) {
  const text = typeof value === "number" && Number.isSafeInteger(value)
    ? String(value)
    : typeof value === "string" ? value : "";
  return requirePattern(text, DECIMAL_ID, label);
}

function timestamp(value, label) {
  const milliseconds = Date.parse(typeof value === "string" ? value : "");
  if (!Number.isFinite(milliseconds)) throw new Error(`${label} is invalid`);
  return milliseconds;
}

function compareDecimal(left, right) {
  const leftValue = BigInt(left);
  const rightValue = BigInt(right);
  return leftValue === rightValue ? 0 : leftValue > rightValue ? 1 : -1;
}

function runGhJson(arguments_) {
  const result = spawnSync("gh", arguments_, {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error("WASM reuse GitHub lookup failed");
  }
  try {
    return JSON.parse(result.stdout);
  } catch {
    throw new Error("WASM reuse GitHub lookup returned invalid JSON");
  }
}

async function writeGithubOutputs(path, values) {
  const lines = Object.entries(values).map(([key, value]) => `${key}=${value}\n`).join("");
  await appendFile(path, lines, "utf8");
}

async function main(arguments_) {
  const { values } = parseArgs({
    args: arguments_,
    options: {
      repository: { type: "string" },
      "source-commit": { type: "string" },
      "current-run-id": { type: "string" },
      "current-run-attempt": { type: "string" },
      "github-output": { type: "string" },
    },
    strict: true,
  });
  const outputPath = typeof values["github-output"] === "string"
    ? values["github-output"]
    : "";
  if (!outputPath) throw new Error("WASM reuse resolver requires --github-output");
  try {
    const result = await resolveReusableAcceptedWasmBuild({
      repository: values.repository,
      sourceCommit: values["source-commit"],
      currentRunId: values["current-run-id"],
      currentRunAttempt: values["current-run-attempt"],
    }, {
      listRuns: () => runGhJson([
        "api", "--method", "GET",
        `repos/${values.repository}/actions/workflows/release-cli.yml/runs`,
        "-f", "event=workflow_dispatch",
        "-f", "branch=main",
        "-f", `head_sha=${values["source-commit"]}`,
        "-f", "per_page=100",
      ]),
      listJobs: (runId, runAttempt) => runGhJson([
        "api", "--method", "GET",
        `repos/${values.repository}/actions/runs/${runId}/attempts/${runAttempt}/jobs`,
        "-f", "filter=all", "-f", "per_page=100", "-f", "page=1",
      ]),
      listArtifacts: (runId) => runGhJson([
        "api", "--method", "GET",
        `repos/${values.repository}/actions/runs/${runId}/artifacts`,
        "-f", "per_page=100", "-f", "page=1",
      ]),
    });
    if (result === null) {
      await writeGithubOutputs(outputPath, { reuse_available: "false" });
      console.log("accepted_wasm_reuse=miss reason=no-complete-exact-source-candidate");
      return;
    }
    await writeGithubOutputs(outputPath, {
      reuse_available: "true",
      reuse_run_id: result.runId,
      reuse_run_attempt: result.runAttempt,
      reuse_artifact_id: result.artifactId,
      reuse_artifact_name: result.artifactName,
      reuse_artifact_digest: result.artifactDigest,
    });
    console.log(
      `accepted_wasm_reuse=available prior_run=${result.runId}/${result.runAttempt} ` +
      `artifact_id=${result.artifactId} bytes=${result.artifactBytes}`,
    );
  } catch {
    await writeGithubOutputs(outputPath, { reuse_available: "false" });
    console.log("accepted_wasm_reuse=miss reason=lookup-or-authority-rejected");
  }
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2));
}

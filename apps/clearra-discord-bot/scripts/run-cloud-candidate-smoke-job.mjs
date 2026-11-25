#!/usr/bin/env node

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import { validateSolutionTransportArtifact } from "../src/clearra/solution-artifact.mjs";
import { findProductCapability } from "../src/discord/capability-registry.mjs";
import { validDiscordPcSolutionsResult } from "../src/discord/pc-solutions-result.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityMatchesRuntime,
} from "../src/job-service/runtime-identity.mjs";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const SMOKE_TIMEOUT_MS = 60_000;
const SMOKE_OUTPUT_BYTES = 1024 * 1024;
const SOLUTIONS = findProductCapability("pc.solutions");
// Keep the existing small empty-2L/IJLOO/no-Hold search. The pure registry owns
// its Unique semantics; a contract test compares every argument with the real
// public option lowerer without importing document codecs into this runtime.
const SMOKE_ARGUMENTS = Object.freeze([
  ...SOLUTIONS.engine.argvPrefix,
  "--lines", "2", "--board-mask", "0x0", "--height", "2", "--pieces", "5",
  "--queue", "IJLOO", "--no-hold", "--objective", SOLUTIONS.engine.pcObjective,
  "--rule", "srs-plus",
]);

export async function runCloudCandidateSmokeJob(options, dependencies = {}) {
  const sourceCommit = canonicalSourceCommit(options?.sourceCommit);
  const candidateUrl = canonicalCandidateOrigin(options?.candidateUrl);
  const authorizationToken = options?.authorizationToken;
  if (typeof authorizationToken !== "string" || authorizationToken.length === 0) {
    throw new Error("candidate smoke Job requires its managed Secret binding");
  }
  const expectedRuntimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);
  const now = dependencies.now ?? Date.now;
  const startedAt = now();
  if (!Number.isSafeInteger(startedAt) || startedAt < 0) {
    throw new Error("candidate smoke Job clock is invalid");
  }
  const Executor = dependencies.Executor ?? ClearraJobExecutor;
  const executor = new Executor({
    endpoint: new URL("/jobs", candidateUrl),
    authorizationToken,
    expectedRuntimeIdentity,
    searchTimeoutMs: SMOKE_TIMEOUT_MS,
    reverseSearchTimeoutMs: SMOKE_TIMEOUT_MS,
    forwardSearchTimeoutMs: SMOKE_TIMEOUT_MS,
    maxOutputBytes: SMOKE_OUTPUT_BYTES,
    maxArtifactBytes: SMOKE_OUTPUT_BYTES,
    pollIntervalMs: 250,
    cancelTimeoutMs: 2_000,
    now,
  });
  const jobId = `candidate-smoke-${sourceCommit.slice(0, 12)}-${startedAt.toString(36)}`;
  const result = await executor.execute([...SMOKE_ARGUMENTS], {
    jobId,
    deadlineUnixMs: startedAt + SMOKE_TIMEOUT_MS,
  });
  const solutionSetHash = validateSmokeResult(result, expectedRuntimeIdentity);
  return Object.freeze({ sourceCommit, jobId, solutionSetHash });
}

function validateSmokeResult(result, expectedRuntimeIdentity) {
  if (result?.exitCode !== 0 || result?.signal !== null) {
    throw new Error("candidate smoke Job did not finish successfully");
  }
  let payload;
  try {
    payload = JSON.parse(result?.stdout);
  } catch {
    throw new Error("candidate smoke Job returned invalid Clearra JSON");
  }
  const summary = payload?.summary;
  if (
    !validDiscordPcSolutionsResult(payload) ||
    summary.solution_found !== true ||
    summary.count_complete !== true ||
    summary.solution_count_calculated !== true ||
    summary.solution_set_materialized !== true ||
    !Number.isSafeInteger(summary.normalized_unique_solution_count) ||
    summary.normalized_unique_solution_count < 1 ||
    !/^cts1:[0-9a-f]{16}$/u.test(summary.normalized_solution_set_hash ?? "") ||
    summary.workers_requested !== "auto" ||
    !Number.isSafeInteger(summary.workers_used) ||
    summary.workers_used < 1 || summary.workers_used > 8 ||
    !productBuildIdentityMatchesRuntime(payload?.runtime_identity, expectedRuntimeIdentity)
  ) {
    throw new Error("candidate smoke Job returned an invalid PC result contract");
  }
  const artifact = validateSolutionTransportArtifact(result.artifact, SMOKE_OUTPUT_BYTES);
  if (artifact.solutionCount !== summary.normalized_unique_solution_count) {
    throw new Error("candidate smoke Job solution artifact differs from its exact Unique count");
  }
  return summary.normalized_solution_set_hash;
}

function canonicalSourceCommit(value) {
  const sourceCommit = typeof value === "string" ? value : "";
  if (!COMMIT_PATTERN.test(sourceCommit)) {
    throw new Error("source commit must be a full lowercase Git SHA");
  }
  return sourceCommit;
}

function canonicalCandidateOrigin(value) {
  let url;
  try {
    url = new URL(String(value));
  } catch {
    throw new Error("candidate URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/" ||
    !url.hostname.endsWith(".run.app") ||
    String(value) !== url.origin
  ) {
    throw new Error("candidate URL must be a credential-free HTTPS run.app origin");
  }
  return url.origin;
}

async function main() {
  const { values } = parseArgs({
    options: {
      "candidate-url": { type: "string" },
      "source-commit": { type: "string" },
    },
    strict: true,
  });
  try {
    const result = await runCloudCandidateSmokeJob({
      candidateUrl: values["candidate-url"],
      sourceCommit: values["source-commit"],
      authorizationToken: process.env.CLEARRA_CANDIDATE_JOB_TOKEN,
    });
    process.stdout.write(
      `candidate_smoke_job=passed source_commit=${result.sourceCommit} job_id=${result.jobId} solution_set_hash=${result.solutionSetHash}\n`,
    );
  } catch {
    process.stderr.write("candidate_smoke_job=failed\n");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

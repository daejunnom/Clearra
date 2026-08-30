#!/usr/bin/env node

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityMatchesRuntime,
} from "../src/job-service/runtime-identity.mjs";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const SMOKE_TIMEOUT_MS = 60_000;
const SMOKE_OUTPUT_BYTES = 1024 * 1024;
const SMOKE_ARGUMENTS = Object.freeze([
  "pc", "--lines", "2", "--queue", "IJLOO", "--fixed", "--no-hold",
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
  if (
    payload?.kind !== "pc" ||
    payload?.summary?.solution_found !== true ||
    typeof payload?.summary?.normalized_solution_set_hash !== "string" ||
    !productBuildIdentityMatchesRuntime(payload?.runtime_identity, expectedRuntimeIdentity)
  ) {
    throw new Error("candidate smoke Job returned an invalid PC result contract");
  }
  return payload.summary.normalized_solution_set_hash;
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

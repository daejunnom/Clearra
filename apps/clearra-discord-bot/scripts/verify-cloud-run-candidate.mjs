import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityMatchesRuntime,
} from "../src/job-service/runtime-identity.mjs";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SMOKE_TIMEOUT_MS = 60_000;
const SMOKE_OUTPUT_BYTES = 1024 * 1024;
const SMOKE_ARGUMENTS = Object.freeze([
  "pc",
  "--lines",
  "2",
  "--queue",
  "IJLOO",
  "--fixed",
  "--no-hold",
]);

export async function verifyCloudRunCandidate(options, dependencies = {}) {
  const sourceCommit = canonicalSourceCommit(options?.sourceCommit);
  const baseUrl = canonicalCandidateUrl(options?.baseUrl);
  const authorizationToken = options?.authorizationToken;
  if (typeof authorizationToken !== "string" || authorizationToken.length === 0) {
    throw new Error("candidate smoke requires the managed job bearer");
  }

  const expectedRuntimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);
  const now = dependencies.now ?? Date.now;
  const startedAt = now();
  const jobId = `candidate-smoke-${sourceCommit.slice(0, 12)}-${startedAt.toString(36)}`;
  const Executor = dependencies.Executor ?? ClearraJobExecutor;
  const executor = new Executor({
    endpoint: new URL("/jobs", baseUrl),
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
  const result = await executor.execute([...SMOKE_ARGUMENTS], {
    jobId,
    deadlineUnixMs: startedAt + SMOKE_TIMEOUT_MS,
  });
  validateSmokeResult(result, expectedRuntimeIdentity);
  return Object.freeze({ sourceCommit, jobId });
}

function validateSmokeResult(result, expectedRuntimeIdentity) {
  if (result?.exitCode !== 0 || result?.signal !== null) {
    throw new Error("candidate smoke did not finish successfully");
  }
  if (typeof result.stdout !== "string" || result.stdout.length === 0) {
    throw new Error("candidate smoke omitted the Clearra result");
  }
  let payload;
  try {
    payload = JSON.parse(result.stdout);
  } catch {
    throw new Error("candidate smoke returned invalid Clearra JSON");
  }
  if (
    payload?.kind !== "pc" ||
    payload?.summary?.solution_found !== true ||
    typeof payload?.summary?.normalized_solution_set_hash !== "string" ||
    !productBuildIdentityMatchesRuntime(
      payload?.runtime_identity,
      expectedRuntimeIdentity,
    )
  ) {
    throw new Error("candidate smoke returned an invalid PC result contract");
  }
}

function canonicalSourceCommit(value) {
  const sourceCommit = typeof value === "string" ? value.trim() : "";
  if (!COMMIT_PATTERN.test(sourceCommit)) {
    throw new Error("source commit must be a full lowercase Git SHA");
  }
  return sourceCommit;
}

function canonicalCandidateUrl(value) {
  let url;
  try {
    url = new URL(String(value));
  } catch {
    throw new Error("candidate URL is invalid");
  }
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) {
    throw new Error("candidate URL must be credential-free HTTPS");
  }
  url.pathname = "/";
  return url;
}

async function main() {
  const { values } = parseArgs({
    options: {
      "base-url": { type: "string" },
      "source-commit": { type: "string" },
    },
    strict: true,
  });
  try {
    const result = await verifyCloudRunCandidate({
      baseUrl: values["base-url"],
      sourceCommit: values["source-commit"],
      authorizationToken: process.env.CLEARRA_CANDIDATE_JOB_TOKEN,
    });
    console.log(`candidate_smoke=passed source_commit=${result.sourceCommit}`);
  } catch {
    console.error("candidate_smoke=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

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
const TYPED_PATH_ARGUMENTS = Object.freeze([
  "pc", "path", "--lines", "1", "--board-mask", "0x3f0",
  "--height", "1", "--pieces", "1", "--queue", "I", "--no-hold", "--rule", "srs-plus",
  "--backend", "cpu",
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
  const deadlineUnixMs = startedAt + SMOKE_TIMEOUT_MS;
  const result = await executor.execute([...SMOKE_ARGUMENTS], {
    jobId,
    deadlineUnixMs,
  });
  const solutionSetHash = validateSmokeResult(result, expectedRuntimeIdentity);
  // The promotion receipt is still the first probe's whole-family digest.
  // Use the same Cloud Job and deadline, without competing for the service's
  // single admission slot or overriding its approved worker policy.
  const typedPath = await executor.execute([...TYPED_PATH_ARGUMENTS], {
    jobId: `${jobId}-path`,
    deadlineUnixMs,
  });
  validateTypedPathResult(typedPath, expectedRuntimeIdentity);
  return Object.freeze({ sourceCommit, jobId, solutionSetHash });
}

function parseSmokeResult(result) {
  if (result?.exitCode !== 0 || result?.signal !== null) {
    throw new Error("candidate smoke Job did not finish successfully");
  }
  let payload;
  try {
    payload = JSON.parse(result?.stdout);
  } catch {
    throw new Error("candidate smoke Job returned invalid Clearra JSON");
  }
  return payload;
}

function validateSmokeResult(result, expectedRuntimeIdentity) {
  const payload = parseSmokeResult(result);
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

function validateTypedPathResult(result, expectedRuntimeIdentity) {
  // The production executor already validated and narrowed any producer
  // replay page; the HTTP caller receives only its canonical witness.
  const payload = parseSmokeResult(result);
  const summary = payload?.summary;
  const witness = summary?.canonical_witness;
  const step = witness?.steps?.[0];
  if (
    payload?.kind !== "pc-path-family.v2" ||
    summary?.capability_id !== "pc.path" ||
    summary?.payload_kind !== "canonical-pc-path-witness" ||
    summary?.complete !== true ||
    witness?.candidate_id !== "1" ||
    witness?.pattern_id !== "0" ||
    witness?.consumed_piece_count !== "1" ||
    witness?.terminal_hold_piece !== null ||
    !Array.isArray(witness?.steps) || witness.steps.length !== 1 ||
    !/^trk1:.+/u.test(witness.normalized_trace_key ?? "") ||
    step?.active_piece !== "I" ||
    step?.cleared_lines !== "1" ||
    !maskEquals(step?.board_before_mask, 0x3f0n) ||
    !maskEquals(step?.placement_mask, 0xfn) ||
    !maskEquals(step?.board_after_placement_mask, 0x3ffn) ||
    !maskEquals(step?.board_after_line_clear_mask, 0n) ||
    !productBuildIdentityMatchesRuntime(payload?.runtime_identity, expectedRuntimeIdentity)
  ) {
    throw new Error("candidate smoke Job returned an invalid typed PC path result");
  }
}

function maskEquals(value, expected) {
  return typeof value === "string" &&
    /^0x(?:[0-9a-f]{16}|[0-9a-f]{64})$/u.test(value) && BigInt(value) === expected;
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

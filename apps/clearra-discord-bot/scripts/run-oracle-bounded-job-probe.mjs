#!/usr/bin/env node

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityMatchesRuntime,
} from "../src/job-service/runtime-identity.mjs";

export const ORACLE_BOUNDED_JOB_PROBE_CONTRACT =
  "clearra.oracle-bounded-job-probe.v1";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const NONCE_PATTERN = /^[0-9a-f]{64}$/u;
const SOLUTION_SET_HASH_PATTERN = /^cts1:[0-9a-f]{16}$/u;
const PHASES = new Set(["candidate", "rollback"]);
const PROBE_TIMEOUT_MS = 60_000;
const PROBE_OUTPUT_BYTES = 1024 * 1024;
const PROBE_ARGUMENTS = Object.freeze([
  "pc", "--lines", "2", "--queue", "IJLOO", "--fixed", "--no-hold",
]);

export async function runOracleBoundedJobProbe(options, dependencies = {}) {
  const phase = canonicalPhase(options?.phase);
  const jobUrl = canonicalJobUrl(options?.jobUrl);
  const deploymentNonce = requiredMatch(
    options?.deploymentNonce,
    NONCE_PATTERN,
    "deployment nonce",
  );
  const expectedSourceCommit = optionalSourceCommit(
    options?.expectedSourceCommit,
  );
  if (phase === "candidate" && expectedSourceCommit === null) {
    throw new Error("candidate probe requires an exact source commit");
  }
  const authorizationToken = options?.authorizationToken;
  if (typeof authorizationToken !== "string" || authorizationToken.length === 0) {
    throw new Error("Oracle bounded Job probe requires its Vault token");
  }

  const expectedRuntimeIdentity = expectedSourceCommit === null
    ? null
    : currentRuntimeIdentityForCommit(expectedSourceCommit);
  const now = dependencies.now ?? Date.now;
  const startedAt = exactClock(now(), "probe start");
  const Executor = dependencies.Executor ?? ClearraJobExecutor;
  const executor = new Executor({
    endpoint: new URL(jobUrl),
    authorizationToken,
    expectedRuntimeIdentity,
    searchTimeoutMs: PROBE_TIMEOUT_MS,
    pcSearchTimeoutMs: PROBE_TIMEOUT_MS,
    buildSearchTimeoutMs: PROBE_TIMEOUT_MS,
    setupSearchTimeoutMs: PROBE_TIMEOUT_MS,
    forwardSearchTimeoutMs: PROBE_TIMEOUT_MS,
    structureSearchTimeoutMs: PROBE_TIMEOUT_MS,
    utilitySearchTimeoutMs: PROBE_TIMEOUT_MS,
    diagnosticTimeoutMs: PROBE_TIMEOUT_MS,
    maxOutputBytes: PROBE_OUTPUT_BYTES,
    pollIntervalMs: 250,
    cancelTimeoutMs: 2_000,
    now,
  });
  const jobId = [
    "oracle",
    phase,
    "probe",
    deploymentNonce.slice(0, 12),
    startedAt.toString(36),
  ].join("-");
  const result = await executor.execute([...PROBE_ARGUMENTS], {
    jobId,
    deadlineUnixMs: startedAt + PROBE_TIMEOUT_MS,
  });
  const solutionSetHash = validateProbeResult(
    result,
    expectedRuntimeIdentity,
  );
  const completedAtMilliseconds = exactClock(now(), "probe completion");
  if (completedAtMilliseconds < startedAt) {
    throw new Error("Oracle bounded Job probe clock moved backwards");
  }
  return Object.freeze({
    contract: ORACLE_BOUNDED_JOB_PROBE_CONTRACT,
    phase,
    jobUrl,
    expectedSourceCommit,
    jobId,
    solutionSetHash,
    completedAt: new Date(completedAtMilliseconds).toISOString(),
  });
}

function validateProbeResult(result, expectedRuntimeIdentity) {
  if (result?.exitCode !== 0 || result?.signal !== null) {
    throw new Error("Oracle bounded Job probe did not finish successfully");
  }
  let payload;
  try {
    payload = JSON.parse(result?.stdout);
  } catch {
    throw new Error("Oracle bounded Job probe returned invalid Clearra JSON");
  }
  if (
    payload?.kind !== "pc" ||
    payload?.summary?.solution_found !== true ||
    !SOLUTION_SET_HASH_PATTERN.test(
      payload?.summary?.normalized_solution_set_hash ?? "",
    ) ||
    (expectedRuntimeIdentity !== null &&
      !productBuildIdentityMatchesRuntime(
        payload?.runtime_identity,
        expectedRuntimeIdentity,
      ))
  ) {
    throw new Error("Oracle bounded Job probe returned an invalid PC result");
  }
  return payload.summary.normalized_solution_set_hash;
}

function canonicalPhase(value) {
  const phase = typeof value === "string" ? value : "";
  if (!PHASES.has(phase)) throw new Error("Oracle probe phase is invalid");
  return phase;
}

function canonicalJobUrl(value) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error("Oracle probe Job URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/jobs" ||
    !url.hostname.endsWith(".run.app")
  ) {
    throw new Error(
      "Oracle probe Job URL must be a credential-free HTTPS run.app /jobs URL",
    );
  }
  return `${url.origin}/jobs`;
}

function optionalSourceCommit(value) {
  if (value === null || value === undefined) return null;
  return requiredMatch(value, COMMIT_PATTERN, "expected source commit");
}

function requiredMatch(value, pattern, label) {
  const text = typeof value === "string" ? value.trim() : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function exactClock(value, label) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`Oracle bounded Job ${label} clock is invalid`);
  }
  return value;
}

async function main() {
  const { values } = parseArgs({
    options: {
      phase: { type: "string" },
      "job-url": { type: "string" },
      "deployment-nonce": { type: "string" },
      "expected-source-commit": { type: "string" },
    },
    strict: true,
  });
  const authorizationToken = process.env.CLEARRA_JOB_TOKEN;
  delete process.env.CLEARRA_JOB_TOKEN;
  try {
    const result = await runOracleBoundedJobProbe({
      phase: values.phase,
      jobUrl: values["job-url"],
      deploymentNonce: values["deployment-nonce"],
      expectedSourceCommit:
        values["expected-source-commit"] === "none"
          ? null
          : values["expected-source-commit"],
      authorizationToken,
    });
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch {
    process.stderr.write("oracle_bounded_job_probe=failed\n");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

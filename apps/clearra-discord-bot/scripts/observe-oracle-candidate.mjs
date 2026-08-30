#!/usr/bin/env node

import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";

import { currentRuntimeIdentityForCommit } from "../src/job-service/runtime-identity.mjs";
import { inspectActiveOracle } from "./produce-oracle-deployment-proof.mjs";

export const ORACLE_OBSERVATION_CONTRACT = "clearra.oracle.candidate-observation.v1";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const BOOT_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;
const POSITIVE_DECIMAL_PATTERN = /^[1-9][0-9]{0,19}$/u;
const SERVICE_NAME = "clearra-gateway.service";

export function observeOracleCandidate(options, dependencies = {}) {
  const sourceCommit = requiredMatch(
    options?.sourceCommit,
    COMMIT_PATTERN,
    "source commit",
  );
  const commitPrefix = sourceCommit.slice(0, 7);
  const candidateUrl = canonicalOrigin(options?.candidateUrl);
  const candidateRevision = requiredExact(
    options?.candidateRevision,
    `clearra-current-job-v080-${commitPrefix}`,
    "candidate revision",
  );
  const oracleReleaseId = requiredExact(
    options?.oracleReleaseId,
    `v0.8.0-${commitPrefix}`,
    "Oracle release ID",
  );
  const oracleReleaseSha256 = requiredMatch(
    options?.oracleReleaseSha256,
    SHA256_PATTERN,
    "Oracle release SHA-256",
  );
  const oracleSettingsSha256 = requiredMatch(
    options?.oracleSettingsSha256,
    SHA256_PATTERN,
    "Oracle settings SHA-256",
  );
  const deploymentNonce = requiredMatch(
    options?.deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const verifiedAfter = canonicalTimestamp(
    options?.verifiedAfter,
    "verified-after",
  );
  const runtimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);
  const jobUrl = `${candidateUrl}/jobs`;
  const inspect = dependencies.inspectActiveOracle ?? inspectActiveOracle;
  const active = inspect(
    {
      oracleReleaseId,
      oracleReleaseSha256,
      oracleSettingsSha256,
      verifiedAfter,
      expectedSettings: {
        CLEARRA_JOB_URL: jobUrl,
        CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: sourceCommit,
        CLEARRA_EXPECTED_ENGINE_BUILD_ID: sourceCommit,
        CLEARRA_EXPECTED_JOB_CONTRACT_REVISION:
          runtimeIdentity.contractSchemaVersion,
        CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID: runtimeIdentity.supplySemanticsId,
        CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION:
          runtimeIdentity.artifactSchemaVersion,
      },
    },
    dependencies,
  );
  requireExactObjectKeys(
    active,
    [
      "activeReleasePath",
      "activeReleaseSha256",
      "activeSettingsSha256",
      "freshOperationAt",
      "gatewayPid",
      "readyRecordObserved",
    ],
    "active Oracle observation",
  );
  requiredExact(
    active.activeReleasePath,
    `/opt/clearra/releases/${oracleReleaseId}`,
    "active Oracle release path",
  );
  requiredExact(
    active.activeReleaseSha256,
    oracleReleaseSha256,
    "active Oracle release SHA-256",
  );
  requiredExact(
    active.activeSettingsSha256,
    oracleSettingsSha256,
    "active Oracle settings SHA-256",
  );
  const gatewayPid = positiveSafeInteger(
    active.gatewayPid,
    "Oracle Gateway PID",
  );
  if (active.readyRecordObserved !== true) {
    throw new Error("Oracle Gateway READY record is unavailable");
  }
  const freshOperationAt = canonicalTimestamp(
    active.freshOperationAt,
    "fresh operation timestamp",
  );
  if (Date.parse(freshOperationAt) < Date.parse(verifiedAfter)) {
    throw new Error("Oracle Gateway operation predates the observation authority");
  }

  const run = dependencies.run ?? runCommand;
  const gatewayStartMonotonicUsec = positiveSafeInteger(
    run("/usr/bin/systemctl", [
      "show",
      "--property",
      "ExecMainStartTimestampMonotonic",
      "--value",
      SERVICE_NAME,
    ]).trim(),
    "Oracle Gateway monotonic start",
  );
  const bootId = requiredMatch(
    run("/usr/bin/cat", ["/proc/sys/kernel/random/boot_id"]).trim(),
    BOOT_ID_PATTERN,
    "Oracle boot ID",
  );
  const now = dependencies.now ?? (() => new Date());
  const observedAt = canonicalTimestamp(now().toISOString(), "observed-at");
  if (Date.parse(observedAt) < Date.parse(freshOperationAt)) {
    throw new Error("Oracle observation timestamp predates its fresh operation");
  }

  return Object.freeze({
    contract: ORACLE_OBSERVATION_CONTRACT,
    sourceCommit,
    candidateUrl,
    candidateRevision,
    jobUrl,
    oracleReleaseId,
    activeReleasePath: active.activeReleasePath,
    oracleReleaseSha256,
    oracleSettingsSha256,
    deploymentNonce,
    gatewayPid,
    gatewayStartMonotonicUsec,
    bootId,
    readyRecordObserved: true,
    freshOperationAt,
    observedAt,
    runtimeIdentity,
  });
}

function runCommand(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error(`Oracle observation failed: ${basename(command)}`);
  }
  return result.stdout;
}

function canonicalOrigin(value) {
  const text = typeof value === "string" ? value : "";
  let url;
  try {
    url = new URL(text);
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
    (text !== url.origin && text !== `${url.origin}/`)
  ) {
    throw new Error("candidate URL must be a canonical credential-free HTTPS origin");
  }
  return url.origin;
}

function canonicalTimestamp(value, label) {
  const text = typeof value === "string" ? value : "";
  const timestamp = Date.parse(text);
  if (!Number.isFinite(timestamp) || new Date(timestamp).toISOString() !== text) {
    throw new Error(`${label} must be a canonical ISO timestamp`);
  }
  return text;
}

function requiredMatch(value, pattern, label) {
  const text = typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function positiveSafeInteger(value, label) {
  const text = requiredMatch(value, POSITIVE_DECIMAL_PATTERN, label);
  const number = Number(text);
  if (!Number.isSafeInteger(number) || number < 1) {
    throw new Error(`${label} is invalid`);
  }
  return number;
}

function requiredExact(value, expected, label) {
  if (value !== expected) throw new Error(`${label} is invalid`);
  return value;
}

function requireExactObjectKeys(value, expected, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    throw new Error(`${label} keys do not match the closed schema`);
  }
}

async function main() {
  const { values } = parseArgs({
    options: {
      "source-commit": { type: "string" },
      "candidate-url": { type: "string" },
      "candidate-revision": { type: "string" },
      "oracle-release-id": { type: "string" },
      "oracle-release-sha256": { type: "string" },
      "oracle-settings-sha256": { type: "string" },
      "deployment-nonce": { type: "string" },
      "verified-after": { type: "string" },
    },
    strict: true,
  });
  try {
    if (typeof process.getuid !== "function" || process.getuid() !== 0) {
      throw new Error("Oracle candidate observation must run as root");
    }
    const observation = observeOracleCandidate({
      sourceCommit: values["source-commit"],
      candidateUrl: values["candidate-url"],
      candidateRevision: values["candidate-revision"],
      oracleReleaseId: values["oracle-release-id"],
      oracleReleaseSha256: values["oracle-release-sha256"],
      oracleSettingsSha256: values["oracle-settings-sha256"],
      deploymentNonce: values["deployment-nonce"],
      verifiedAfter: values["verified-after"],
    });
    process.stdout.write(`${JSON.stringify(observation)}\n`);
  } catch {
    process.stderr.write("oracle_candidate_observation=failed\n");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

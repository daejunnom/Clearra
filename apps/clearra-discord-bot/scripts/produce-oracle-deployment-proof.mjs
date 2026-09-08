import { createHash } from "node:crypto";
import {
  closeSync,
  fsyncSync,
  linkSync,
  lstatSync,
  openSync,
  readFileSync,
  realpathSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";

import { currentRuntimeIdentityForCommit } from "../src/job-service/runtime-identity.mjs";
import { observePriorRuntimeAuthority } from "./oracle-runtime-authority.mjs";
import { releaseTreeSha256 } from "./release-tree-digest.mjs";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const RELEASE_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const SOLUTION_SET_HASH_PATTERN = /^cts1:[0-9a-f]{16}$/;
const PROBE_CONTRACT = "clearra.oracle-bounded-job-probe.v1";
const RELEASE_ROOT = "/opt/clearra/releases";
const CURRENT_LINK = "/opt/clearra/current";
const SETTINGS_PATH = "/etc/clearra-gateway/settings";
const SERVICE_NAME = "clearra-gateway.service";
const PROBE_LAUNCHER = fileURLToPath(
  new URL("./run-oracle-bounded-job-probe", import.meta.url),
);

export function produceOracleCandidateProof(options, dependencies = {}) {
  const sourceCommit = requiredMatch(
    options?.sourceCommit,
    COMMIT_PATTERN,
    "source commit",
  );
  const candidateUrl = canonicalOrigin(options?.candidateUrl, "candidate URL");
  const candidateRevision = requiredMatch(
    options?.candidateRevision,
    RELEASE_PATTERN,
    "candidate revision",
  );
  const oracleReleaseId = requiredMatch(
    options?.oracleReleaseId,
    RELEASE_PATTERN,
    "Oracle release ID",
  );
  const oracleSettingsSha256 = requiredMatch(
    options?.oracleSettingsSha256,
    SHA256_PATTERN,
    "Oracle settings digest",
  );
  const oracleReleaseSha256 = requiredMatch(
    options?.oracleReleaseSha256,
    SHA256_PATTERN,
    "Oracle release tree digest",
  );
  const deploymentNonce = requiredMatch(
    options?.deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const verifiedAfter = canonicalTimestamp(options?.verifiedAfter);
  const jobUrl = `${candidateUrl}/jobs`;
  const runtimeIdentity = currentRuntimeIdentityForCommit(sourceCommit);
  inspectActiveOracle(
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
  const boundedJobProbe = executeOracleBoundedJobProbe(
    {
      phase: "candidate",
      jobUrl,
      deploymentNonce,
      expectedSourceCommit: sourceCommit,
    },
    dependencies,
  );
  requireProbeFreshness(boundedJobProbe, verifiedAfter);
  const proof = Object.freeze({
    sourceCommit,
    candidateUrl,
    candidateRevision,
    jobUrl,
    oracleReleaseId,
    oracleReleaseSha256,
    oracleSettingsSha256,
    deploymentNonce,
    gatewayReady: true,
    boundedJobSucceeded: true,
    runtimeIdentity,
  });
  (dependencies.writeProof ?? writeOneShotProof)(options?.proofPath, proof);
  return proof;
}

export function produceOracleRollbackProof(options, dependencies = {}) {
  const priorRevision = requiredMatch(
    options?.priorRevision,
    RELEASE_PATTERN,
    "prior Cloud revision",
  );
  const priorOracleReleaseId = requiredMatch(
    options?.priorOracleReleaseId,
    RELEASE_PATTERN,
    "prior Oracle release ID",
  );
  const priorOracleSettingsSha256 = requiredMatch(
    options?.priorOracleSettingsSha256,
    SHA256_PATTERN,
    "prior Oracle settings digest",
  );
  const priorOracleReleaseSha256 = requiredMatch(
    options?.priorOracleReleaseSha256,
    SHA256_PATTERN,
    "prior Oracle release tree digest",
  );
  const priorRuntimeAuthorityKind = options?.priorRuntimeAuthorityKind;
  const priorRuntimeAuthoritySha256 = requiredMatch(
    options?.priorRuntimeAuthoritySha256,
    SHA256_PATTERN,
    "prior runtime authority digest",
  );
  const deploymentNonce = requiredMatch(
    options?.deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const verifiedAfter = canonicalTimestamp(options?.verifiedAfter);
  const priorJobUrl = canonicalJobUrl(options?.priorJobUrl);
  const priorHealthUrl = `${new URL(priorJobUrl).origin}/health`;
  const run = dependencies.run ?? runCommand;
  let priorHealth;
  try {
    priorHealth = JSON.parse(
      run("/usr/bin/curl", [
        "--fail",
        "--silent",
        "--show-error",
        "--max-time",
        "15",
        priorHealthUrl,
      ]),
    );
  } catch {
    throw new Error("restored prior runtime health identity is unavailable");
  }
  const observedRuntimeAuthority = observePriorRuntimeAuthority({
    kind: priorRuntimeAuthorityKind,
    priorRevision,
    priorOracleReleaseId,
    health: priorHealth,
  });
  if (observedRuntimeAuthority.sha256 !== priorRuntimeAuthoritySha256) {
    throw new Error(
      "restored prior runtime authority does not match the captured authority",
    );
  }
  inspectActiveOracle(
    {
      oracleReleaseId: priorOracleReleaseId,
      oracleReleaseSha256: priorOracleReleaseSha256,
      oracleSettingsSha256: priorOracleSettingsSha256,
      verifiedAfter,
      expectedSettings: { CLEARRA_JOB_URL: priorJobUrl },
    },
    dependencies,
  );
  const boundedJobProbe = executeOracleBoundedJobProbe(
    {
      phase: "rollback",
      jobUrl: priorJobUrl,
      deploymentNonce,
      expectedSourceCommit:
        typeof priorHealth?.runtime?.sourceCommit === "string"
          ? priorHealth.runtime.sourceCommit
          : null,
    },
    dependencies,
  );
  requireProbeFreshness(boundedJobProbe, verifiedAfter);
  const proof = Object.freeze({
    priorRevision,
    priorOracleReleaseId,
    priorOracleReleaseSha256,
    priorOracleSettingsSha256,
    priorRuntimeAuthorityKind: observedRuntimeAuthority.kind,
    priorRuntimeAuthoritySha256,
    priorJobUrl,
    deploymentNonce,
    gatewayReady: true,
    boundedJobSucceeded: true,
  });
  (dependencies.writeProof ?? writeOneShotProof)(options?.proofPath, proof);
  return proof;
}

export function inspectActiveOracle(options, dependencies = {}) {
  const run = dependencies.run ?? runCommand;
  const readText =
    dependencies.readText ?? ((path) => readFileSync(path, "utf8"));
  const resolvePath = dependencies.realpath ?? realpathSync;
  const expectedRelease = `${RELEASE_ROOT}/${options.oracleReleaseId}`;
  const activeRelease = normalizePath(resolvePath(CURRENT_LINK));
  if (activeRelease !== expectedRelease) {
    throw new Error(
      "active Oracle symlink does not match the expected immutable release",
    );
  }
  const computeReleaseDigest =
    dependencies.releaseTreeSha256 ?? releaseTreeSha256;
  const actualReleaseSha256 = computeReleaseDigest(activeRelease);
  if (
    options.oracleReleaseSha256 !== undefined &&
    actualReleaseSha256 !== options.oracleReleaseSha256
  ) {
    throw new Error(
      "active Oracle release tree digest does not match the expected artifact",
    );
  }
  const settings = readText(SETTINGS_PATH);
  const actualSettingsSha256 = createHash("sha256")
    .update(settings, "utf8")
    .digest("hex");
  if (actualSettingsSha256 !== options.oracleSettingsSha256) {
    throw new Error(
      "active Oracle settings digest does not match the expected snapshot",
    );
  }
  assertExactSettings(settings, options.expectedSettings);

  if (
    run("/usr/bin/systemctl", ["is-active", SERVICE_NAME]).trim() !== "active"
  ) {
    throw new Error("Oracle Gateway service is not active");
  }
  const pid = run("/usr/bin/systemctl", [
    "show",
    "--property",
    "MainPID",
    "--value",
    SERVICE_NAME,
  ]).trim();
  // Exclude PID 0/1, not every valid PID whose first digit happens to be 1.
  if (!/^[1-9][0-9]*$/.test(pid) || !Number.isSafeInteger(Number(pid)) || Number(pid) <= 1)
    throw new Error("Oracle Gateway MainPID is invalid");
  const processCwd = normalizePath(resolvePath(`/proc/${pid}/cwd`));
  if (processCwd !== `${expectedRelease}/apps/clearra-discord-bot`) {
    throw new Error(
      "Oracle Gateway process is not running from the expected release",
    );
  }
  const journal = run("/usr/bin/journalctl", [
    `_SYSTEMD_UNIT=${SERVICE_NAME}`,
    `_PID=${pid}`,
    "--no-pager",
    "-n",
    "500",
    "-o",
    "cat",
  ]);
  if (!journal.includes("Oracle Gateway connected as ")) {
    throw new Error("current Oracle Gateway process has no READY record");
  }
  return Object.freeze({
    activeReleasePath: activeRelease,
    activeReleaseSha256: actualReleaseSha256,
    activeSettingsSha256: actualSettingsSha256,
    gatewayPid: pid,
    readyRecordObserved: true,
  });
}

export function executeOracleBoundedJobProbe(options, dependencies = {}) {
  const phase = requiredMatch(
    options?.phase,
    /^(?:candidate|rollback)$/,
    "Oracle probe phase",
  );
  const jobUrl = canonicalJobUrl(options?.jobUrl);
  const deploymentNonce = requiredMatch(
    options?.deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const expectedSourceCommit = options?.expectedSourceCommit === null
    ? null
    : requiredMatch(
        options?.expectedSourceCommit,
        COMMIT_PATTERN,
        "expected source commit",
      );
  if (phase === "candidate" && expectedSourceCommit === null) {
    throw new Error("candidate bounded Job probe requires its source commit");
  }
  const invoke = dependencies.runBoundedJobProbe ?? ((probeOptions) => {
    const run = dependencies.run ?? runCommand;
    const serialized = run("/bin/sh", [
      PROBE_LAUNCHER,
      "--phase",
      probeOptions.phase,
      "--job-url",
      probeOptions.jobUrl,
      "--deployment-nonce",
      probeOptions.deploymentNonce,
      "--expected-source-commit",
      probeOptions.expectedSourceCommit ?? "none",
    ]);
    try {
      return JSON.parse(serialized);
    } catch {
      throw new Error("Oracle bounded Job probe returned invalid JSON");
    }
  });
  const value = invoke({
    phase,
    jobUrl,
    deploymentNonce,
    expectedSourceCommit,
  });
  const expectedKeys = [
    "completedAt",
    "contract",
    "expectedSourceCommit",
    "jobId",
    "jobUrl",
    "phase",
    "solutionSetHash",
  ];
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Oracle bounded Job probe result is invalid");
  }
  const actualKeys = Object.keys(value).sort();
  if (
    actualKeys.length !== expectedKeys.length ||
    actualKeys.some((key, index) => key !== expectedKeys[index]) ||
    value.contract !== PROBE_CONTRACT ||
    value.phase !== phase ||
    value.jobUrl !== jobUrl ||
    value.expectedSourceCommit !== expectedSourceCommit ||
    typeof value.jobId !== "string" ||
    !/^oracle-(?:candidate|rollback)-probe-[a-z0-9-]{1,96}$/.test(
      value.jobId,
    ) ||
    !SOLUTION_SET_HASH_PATTERN.test(value.solutionSetHash ?? "")
  ) {
    throw new Error("Oracle bounded Job probe result violates its contract");
  }
  canonicalTimestamp(value.completedAt);
  return Object.freeze({ ...value });
}

function requireProbeFreshness(probe, verifiedAfter) {
  if (Date.parse(probe.completedAt) < Date.parse(verifiedAfter)) {
    throw new Error("Oracle bounded Job probe predates deployment authority");
  }
}

function assertExactSettings(serialized, expected) {
  const values = new Map();
  for (const rawLine of serialized.split(/\r?\n/u)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    const separator = line.indexOf("=");
    if (separator < 1) continue;
    const key = line.slice(0, separator);
    const value = line.slice(separator + 1);
    const existing = values.get(key) ?? [];
    existing.push(value);
    values.set(key, existing);
  }
  for (const [key, expectedValue] of Object.entries(expected)) {
    const actual = values.get(key) ?? [];
    if (actual.length !== 1 || actual[0] !== expectedValue) {
      throw new Error(
        `active Oracle setting ${key} does not match the deployment proof`,
      );
    }
  }
}

function writeOneShotProof(path, proof) {
  const proofPath = resolve(String(path ?? ""));
  const proofDirectory = dirname(proofPath);
  const proofKind = proof.sourceCommit ? "candidate" : "rollback";
  const expectedName = `clearra-oracle-${proofKind}-${proof.deploymentNonce}.json`;
  if (
    proofDirectory !== "/run/clearra-deploy" ||
    basename(proofPath) !== expectedName
  ) {
    throw new Error(
      "Oracle deployment proof path must use its nonce-bound root-only namespace",
    );
  }
  const directoryMetadata = lstatSync(proofDirectory);
  if (
    !directoryMetadata.isDirectory() ||
    directoryMetadata.isSymbolicLink() ||
    directoryMetadata.uid !== 0 ||
    (directoryMetadata.mode & 0o777) !== 0o700
  ) {
    throw new Error(
      "Oracle deployment proof directory must be root-owned mode 0700",
    );
  }
  const temporaryPath = `${proofPath}.tmp`;
  let descriptor;
  let temporaryCreated = false;
  try {
    descriptor = openSync(temporaryPath, "wx", 0o600);
    temporaryCreated = true;
    writeFileSync(descriptor, `${JSON.stringify(proof)}\n`, "utf8");
    fsyncSync(descriptor);
    closeSync(descriptor);
    descriptor = undefined;
    linkSync(temporaryPath, proofPath);
    unlinkSync(temporaryPath);
    temporaryCreated = false;
  } finally {
    if (descriptor !== undefined) closeSync(descriptor);
    if (temporaryCreated) {
      try {
        unlinkSync(temporaryPath);
      } catch {
        // Preserve the original proof-write failure while making retries safe.
      }
    }
  }
}

function runCommand(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error(`Oracle proof observation failed: ${basename(command)}`);
  }
  return result.stdout;
}

function canonicalOrigin(value, label) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname.endsWith(".run.app") ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/"
  ) {
    throw new Error(`${label} must be a credential-free HTTPS origin`);
  }
  return url.origin;
}

function canonicalJobUrl(value) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error("prior job URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/jobs"
  ) {
    throw new Error(
      "Job URL must be a credential-free HTTPS run.app /jobs URL",
    );
  }
  return `${url.origin}/jobs`;
}

function canonicalTimestamp(value) {
  const text = typeof value === "string" ? value.trim() : "";
  const timestamp = Date.parse(text);
  if (
    !Number.isFinite(timestamp) ||
    new Date(timestamp).toISOString() !== text
  ) {
    throw new Error("verified-after must be a canonical ISO timestamp");
  }
  return text;
}

function requiredMatch(value, pattern, label) {
  const text = typeof value === "string" ? value.trim() : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function normalizePath(value) {
  return String(value).replaceAll("\\", "/");
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      proof: { type: "string" },
      "source-commit": { type: "string" },
      "candidate-url": { type: "string" },
      "candidate-revision": { type: "string" },
      "oracle-release-id": { type: "string" },
      "oracle-release-sha256": { type: "string" },
      "oracle-settings-sha256": { type: "string" },
      "deployment-nonce": { type: "string" },
      "verified-after": { type: "string" },
      "prior-revision": { type: "string" },
      "prior-oracle-release-id": { type: "string" },
      "prior-oracle-release-sha256": { type: "string" },
      "prior-oracle-settings-sha256": { type: "string" },
      "prior-runtime-authority-kind": { type: "string" },
      "prior-runtime-authority-sha256": { type: "string" },
      "prior-job-url": { type: "string" },
    },
    strict: true,
  });
  try {
    if (typeof process.getuid !== "function" || process.getuid() !== 0) {
      throw new Error("Oracle deployment proof producer must run as root");
    }
    const common = {
      proofPath: values.proof,
      deploymentNonce: values["deployment-nonce"],
      verifiedAfter: values["verified-after"],
    };
    if (positionals[0] === "candidate" && positionals.length === 1) {
      produceOracleCandidateProof({
        ...common,
        sourceCommit: values["source-commit"],
        candidateUrl: values["candidate-url"],
        candidateRevision: values["candidate-revision"],
        oracleReleaseId: values["oracle-release-id"],
        oracleReleaseSha256: values["oracle-release-sha256"],
        oracleSettingsSha256: values["oracle-settings-sha256"],
      });
    } else if (positionals[0] === "rollback" && positionals.length === 1) {
      produceOracleRollbackProof({
        ...common,
        priorRevision: values["prior-revision"],
        priorOracleReleaseId: values["prior-oracle-release-id"],
        priorOracleReleaseSha256: values["prior-oracle-release-sha256"],
        priorOracleSettingsSha256: values["prior-oracle-settings-sha256"],
        priorRuntimeAuthorityKind: values["prior-runtime-authority-kind"],
        priorRuntimeAuthoritySha256: values["prior-runtime-authority-sha256"],
        priorJobUrl: values["prior-job-url"],
      });
    } else {
      throw new Error(
        "Oracle deployment proof mode must be candidate or rollback",
      );
    }
    console.log("oracle_deployment_proof=produced");
  } catch {
    console.error("oracle_deployment_proof=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

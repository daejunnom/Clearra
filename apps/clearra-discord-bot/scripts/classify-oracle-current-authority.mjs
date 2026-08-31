#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";

import {
  PRIOR_RUNTIME_IDENTITY_KIND,
  observePriorRuntimeAuthority,
} from "./oracle-runtime-authority.mjs";
import { releaseTreeSha256 } from "./release-tree-digest.mjs";

export const ORACLE_CURRENT_AUTHORITY_CLASSIFICATION =
  "clearra.oracle.current-authority-classification.v1";

const RELEASE_ROOT = "/opt/clearra/releases";
const CURRENT_LINK = "/opt/clearra/current";
const SETTINGS_PATH = "/etc/clearra-gateway/settings";
const RELEASE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const COMMIT = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;

export function classifyOracleCurrentAuthority(options, dependencies = {}) {
  const expected = validateOptions(options);
  const realpath = dependencies.realpath ?? realpathSync;
  const inspect = dependencies.lstat ?? lstatSync;
  const read = dependencies.readFile ?? readFileSync;
  const digestRelease = dependencies.releaseTreeSha256 ?? releaseTreeSha256;
  const fetchHealth = dependencies.fetchHealth ?? fetchHealthWithCurl;

  const activeReleasePath = normalizePath(realpath(dependencies.currentLink ?? CURRENT_LINK));
  const activeReleaseId = basename(activeReleasePath);
  if (
    activeReleasePath !== `${RELEASE_ROOT}/${activeReleaseId}` ||
    !RELEASE.test(activeReleaseId)
  ) {
    return other("active-release-outside-authority");
  }
  const releaseMetadata = inspect(activeReleasePath);
  const settingsMetadata = inspect(dependencies.settingsPath ?? SETTINGS_PATH);
  if (
    !releaseMetadata.isDirectory() ||
    releaseMetadata.isSymbolicLink() ||
    releaseMetadata.uid !== 0 ||
    !settingsMetadata.isFile() ||
    settingsMetadata.isSymbolicLink() ||
    settingsMetadata.uid !== 0
  ) {
    return other("active-filesystem-authority-invalid");
  }
  const activeReleaseSha256 = digestRelease(activeReleasePath);
  const settingsBytes = read(dependencies.settingsPath ?? SETTINGS_PATH);
  const activeSettingsSha256 = createHash("sha256").update(settingsBytes).digest("hex");
  if (!SHA256.test(activeReleaseSha256) || !SHA256.test(activeSettingsSha256)) {
    return other("active-digest-invalid");
  }
  let activeJobUrl;
  let health;
  try {
    activeJobUrl = exactJobUrl(Buffer.from(settingsBytes).toString("utf8"));
    health = fetchHealth(`${new URL(activeJobUrl).origin}/health`);
  } catch {
    return classification({
      state: "other",
      reason: "active-runtime-authority-unavailable",
      activeReleaseId,
      activeReleasePath,
      activeReleaseSha256,
      activeSettingsSha256,
      activeJobUrl: null,
      runtimeAuthorityKind: null,
      runtimeAuthoritySha256: null,
    });
  }

  const candidateFieldsMatch =
    activeReleaseId === expected.candidateReleaseId &&
    activeReleasePath === `${RELEASE_ROOT}/${expected.candidateReleaseId}` &&
    activeReleaseSha256 === expected.candidateReleaseSha256 &&
    activeSettingsSha256 === expected.candidateSettingsSha256 &&
    activeJobUrl === `${expected.candidateUrl}/jobs`;
  if (candidateFieldsMatch) {
    try {
      const runtime = observePriorRuntimeAuthority({
        kind: PRIOR_RUNTIME_IDENTITY_KIND,
        priorRevision: expected.candidateRevision,
        priorOracleReleaseId: expected.candidateReleaseId,
        health,
      });
      const identity = health?.runtime;
      if (
        identity?.sourceCommit !== expected.sourceCommit ||
        identity?.engineBuildId !== expected.sourceCommit
      ) {
        return other("candidate-runtime-identity-mismatch");
      }
      return classification({
        state: "candidate",
        reason: "exact-candidate-authority",
        activeReleaseId,
        activeReleasePath,
        activeReleaseSha256,
        activeSettingsSha256,
        activeJobUrl,
        runtimeAuthorityKind: runtime.kind,
        runtimeAuthoritySha256: runtime.sha256,
      });
    } catch {
      return other("candidate-runtime-authority-invalid");
    }
  }

  const priorFieldsMatch =
    activeReleaseId === expected.priorReleaseId &&
    activeReleasePath === `${RELEASE_ROOT}/${expected.priorReleaseId}` &&
    activeReleaseSha256 === expected.priorReleaseSha256 &&
    activeSettingsSha256 === expected.priorSettingsSha256 &&
    activeJobUrl === expected.priorJobUrl;
  if (priorFieldsMatch) {
    try {
      const runtime = observePriorRuntimeAuthority({
        kind: expected.priorRuntimeAuthorityKind,
        priorRevision: expected.priorRevision,
        priorOracleReleaseId: expected.priorReleaseId,
        health,
      });
      if (runtime.sha256 !== expected.priorRuntimeAuthoritySha256) {
        return other("prior-runtime-authority-mismatch");
      }
      return classification({
        state: "prior",
        reason: "exact-prior-authority",
        activeReleaseId,
        activeReleasePath,
        activeReleaseSha256,
        activeSettingsSha256,
        activeJobUrl,
        runtimeAuthorityKind: runtime.kind,
        runtimeAuthoritySha256: runtime.sha256,
      });
    } catch {
      return other("prior-runtime-authority-invalid");
    }
  }
  return classification({
    state: "other",
    reason: "active-authority-field-mismatch",
    activeReleaseId,
    activeReleasePath,
    activeReleaseSha256,
    activeSettingsSha256,
    activeJobUrl,
    runtimeAuthorityKind: null,
    runtimeAuthoritySha256: null,
  });
}

function validateOptions(options) {
  const sourceCommit = required(options?.sourceCommit, COMMIT, "source commit");
  const candidateReleaseId = required(options?.candidateReleaseId, RELEASE, "candidate release ID");
  if (candidateReleaseId !== `v0.8.0-${sourceCommit.slice(0, 7)}`) {
    throw new Error("candidate release ID differs from source commit");
  }
  const candidateRevision = required(options?.candidateRevision, RELEASE, "candidate revision");
  if (candidateRevision !== `clearra-current-job-v080-${sourceCommit.slice(0, 7)}`) {
    throw new Error("candidate revision differs from source commit");
  }
  return Object.freeze({
    sourceCommit,
    candidateUrl: canonicalOrigin(options?.candidateUrl),
    candidateRevision,
    candidateReleaseId,
    candidateReleaseSha256: required(options?.candidateReleaseSha256, SHA256, "candidate release SHA-256"),
    candidateSettingsSha256: required(options?.candidateSettingsSha256, SHA256, "candidate settings SHA-256"),
    priorRevision: required(options?.priorRevision, RELEASE, "prior revision"),
    priorReleaseId: required(options?.priorReleaseId, RELEASE, "prior release ID"),
    priorReleaseSha256: required(options?.priorReleaseSha256, SHA256, "prior release SHA-256"),
    priorSettingsSha256: required(options?.priorSettingsSha256, SHA256, "prior settings SHA-256"),
    priorRuntimeAuthorityKind: required(
      options?.priorRuntimeAuthorityKind,
      /^clearra\.rollback\.(?:runtime-identity|legacy-health-no-runtime)\.v1$/u,
      "prior runtime authority kind",
    ),
    priorRuntimeAuthoritySha256: required(
      options?.priorRuntimeAuthoritySha256,
      SHA256,
      "prior runtime authority SHA-256",
    ),
    priorJobUrl: canonicalJobUrl(options?.priorJobUrl),
  });
}

function classification(fields) {
  return Object.freeze({
    contract: ORACLE_CURRENT_AUTHORITY_CLASSIFICATION,
    ...fields,
  });
}

function other(reason) {
  return classification({
    state: "other",
    reason,
    activeReleaseId: null,
    activeReleasePath: null,
    activeReleaseSha256: null,
    activeSettingsSha256: null,
    activeJobUrl: null,
    runtimeAuthorityKind: null,
    runtimeAuthoritySha256: null,
  });
}

function exactJobUrl(serialized) {
  const values = serialized.split(/\r?\n/u)
    .filter((line) => line.startsWith("CLEARRA_JOB_URL="))
    .map((line) => line.slice("CLEARRA_JOB_URL=".length));
  if (values.length !== 1) throw new Error("active job URL setting is not exact");
  return canonicalJobUrl(values[0]);
}

function canonicalOrigin(value) {
  const url = canonicalUrl(value, "candidate URL");
  if (url.pathname !== "/" && url.pathname !== "") throw new Error("candidate URL is invalid");
  return url.origin;
}

function canonicalJobUrl(value) {
  const url = canonicalUrl(value, "job URL");
  if (url.pathname !== "/jobs") throw new Error("job URL is invalid");
  return `${url.origin}/jobs`;
}

function canonicalUrl(value, label) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  const input = String(value);
  const canonical = url.pathname === "/" ? [url.origin, `${url.origin}/`] : [`${url.origin}${url.pathname}`];
  if (
    url.protocol !== "https:" ||
    url.username || url.password || url.search || url.hash ||
    !canonical.includes(input)
  ) {
    throw new Error(`${label} is invalid`);
  }
  return url;
}

function fetchHealthWithCurl(url) {
  const result = spawnSync("/usr/bin/curl", [
    "--fail", "--silent", "--show-error", "--max-time", "15", url,
  ], { encoding: "utf8", shell: false, stdio: ["ignore", "pipe", "pipe"] });
  if (result.error || result.status !== 0) throw new Error("health request failed");
  return JSON.parse(result.stdout);
}

function required(value, pattern, label) {
  const text = typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function normalizePath(value) {
  return String(value).replaceAll("\\", "/");
}

async function main() {
  const { values } = parseArgs({
    options: {
      "source-commit": { type: "string" },
      "candidate-url": { type: "string" },
      "candidate-revision": { type: "string" },
      "candidate-release-id": { type: "string" },
      "candidate-release-sha256": { type: "string" },
      "candidate-settings-sha256": { type: "string" },
      "prior-revision": { type: "string" },
      "prior-release-id": { type: "string" },
      "prior-release-sha256": { type: "string" },
      "prior-settings-sha256": { type: "string" },
      "prior-runtime-authority-kind": { type: "string" },
      "prior-runtime-authority-sha256": { type: "string" },
      "prior-job-url": { type: "string" },
    },
    strict: true,
  });
  try {
    if (typeof process.getuid !== "function" || process.getuid() !== 0) {
      throw new Error("Oracle current authority classification must run as root");
    }
    const result = classifyOracleCurrentAuthority({
      sourceCommit: values["source-commit"],
      candidateUrl: values["candidate-url"],
      candidateRevision: values["candidate-revision"],
      candidateReleaseId: values["candidate-release-id"],
      candidateReleaseSha256: values["candidate-release-sha256"],
      candidateSettingsSha256: values["candidate-settings-sha256"],
      priorRevision: values["prior-revision"],
      priorReleaseId: values["prior-release-id"],
      priorReleaseSha256: values["prior-release-sha256"],
      priorSettingsSha256: values["prior-settings-sha256"],
      priorRuntimeAuthorityKind: values["prior-runtime-authority-kind"],
      priorRuntimeAuthoritySha256: values["prior-runtime-authority-sha256"],
      priorJobUrl: values["prior-job-url"],
    });
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch {
    process.stderr.write("oracle_current_authority=failed\n");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

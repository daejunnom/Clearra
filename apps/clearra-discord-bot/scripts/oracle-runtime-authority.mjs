import { createHash } from "node:crypto";

import { normalizeRuntimeIdentity } from "../src/job-service/runtime-identity.mjs";

export const PRIOR_RUNTIME_AUTHORITY_SCHEMA =
  "clearra.rollback.runtime-authority.v1";
export const PRIOR_RUNTIME_IDENTITY_KIND =
  "clearra.rollback.runtime-identity.v1";
export const PRIOR_RUNTIME_LEGACY_HEALTH_KIND =
  "clearra.rollback.legacy-health-no-runtime.v1";

const RELEASE_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const LEGACY_RELEASE_PATTERN = /^v0\.7\.4-([0-9a-f]{7})$/;
const LEGACY_REVISION_PATTERN = /^clearra-current-job-v075-([0-9a-f]{7})$/;
const LEGACY_HEALTH_KEYS = Object.freeze([
  "activeJobs",
  "status",
  "workerLimit",
]);
const RUNTIME_HEALTH_KEYS = Object.freeze([
  "activeJobs",
  "runtime",
  "status",
  "workerLimit",
]);
const RUNTIME_IDENTITY_KEYS = Object.freeze([
  "artifactSchemaVersion",
  "contractSchemaVersion",
  "engineBuildId",
  "schema",
  "sourceCommit",
  "supplySemanticsId",
]);

export function observePriorRuntimeAuthority(options) {
  const { kind, priorRevision, priorOracleReleaseId } =
    assertPriorRuntimeAuthorityContext(
      options?.kind,
      options?.priorRevision,
      options?.priorOracleReleaseId,
    );
  const health = requiredRecord(options?.health, "prior Cloud health");

  const observation =
    kind === PRIOR_RUNTIME_IDENTITY_KIND
      ? observeRuntimeIdentity(health)
      : observeLegacyHealth(health);
  const authority = {
    schema: PRIOR_RUNTIME_AUTHORITY_SCHEMA,
    kind,
    priorRevision,
    priorOracleReleaseId,
    observation,
  };
  return Object.freeze({
    kind,
    sha256: createHash("sha256")
      .update(JSON.stringify(canonicalJson(authority)), "utf8")
      .digest("hex"),
  });
}

export function assertPriorRuntimeAuthorityContext(
  value,
  revisionValue,
  releaseIdValue,
) {
  const kind = requiredKind(value);
  const priorRevision = requiredMatch(
    revisionValue,
    RELEASE_PATTERN,
    "prior Cloud revision",
  );
  const priorOracleReleaseId = requiredMatch(
    releaseIdValue,
    RELEASE_PATTERN,
    "prior Oracle release ID",
  );
  if (kind === PRIOR_RUNTIME_LEGACY_HEALTH_KIND) {
    const legacyRelease = LEGACY_RELEASE_PATTERN.exec(priorOracleReleaseId);
    const legacyRevision = LEGACY_REVISION_PATTERN.exec(priorRevision);
    if (
      !legacyRelease ||
      !legacyRevision ||
      legacyRelease[1] !== legacyRevision[1]
    ) {
      throw new Error(
        "legacy runtime authority is restricted to the matching v0.7.4 deployment",
      );
    }
  }
  return Object.freeze({ kind, priorRevision, priorOracleReleaseId });
}

function observeRuntimeIdentity(health) {
  if (!Object.hasOwn(health, "runtime")) {
    throw new Error("prior Cloud runtime identity is unavailable");
  }
  assertExactKeys(
    health,
    RUNTIME_HEALTH_KEYS,
    "prior Cloud runtime health fields do not match the approved profile",
  );
  assertHealthCounters(health, "prior Cloud runtime");
  const runtime = requiredRecord(
    health.runtime,
    "prior Cloud runtime identity",
  );
  assertExactKeys(
    runtime,
    RUNTIME_IDENTITY_KEYS,
    "prior Cloud runtime identity fields do not match the approved profile",
  );
  return Object.freeze({
    healthKeys: RUNTIME_HEALTH_KEYS,
    status: "ok",
    activeJobs: "dynamic-nonnegative-safe-integer",
    workerLimit: health.workerLimit,
    runtimeIdentity: normalizeRuntimeIdentity(runtime),
  });
}

function observeLegacyHealth(health) {
  if (Object.hasOwn(health, "runtime")) {
    throw new Error("legacy Cloud health must not contain runtime identity");
  }
  assertExactKeys(
    health,
    LEGACY_HEALTH_KEYS,
    "legacy Cloud health fields do not match the approved profile",
  );
  assertHealthCounters(health, "legacy Cloud");
  return Object.freeze({
    schema: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
    healthKeys: LEGACY_HEALTH_KEYS,
    status: "ok",
    activeJobs: "dynamic-nonnegative-safe-integer",
    workerLimit: health.workerLimit,
  });
}

function assertHealthCounters(health, label) {
  if (health.status !== "ok") {
    throw new Error(`${label} health status is not ready`);
  }
  if (!Number.isSafeInteger(health.activeJobs) || health.activeJobs < 0) {
    throw new Error(`${label} active job count is invalid`);
  }
  if (!Number.isSafeInteger(health.workerLimit) || health.workerLimit < 1) {
    throw new Error(`${label} worker limit is invalid`);
  }
}

function assertExactKeys(value, expectedKeys, message) {
  const actualKeys = Object.keys(value).sort();
  if (
    actualKeys.length !== expectedKeys.length ||
    actualKeys.some((key, index) => key !== expectedKeys[index])
  ) {
    throw new Error(message);
  }
}

function requiredKind(value) {
  if (
    value !== PRIOR_RUNTIME_IDENTITY_KIND &&
    value !== PRIOR_RUNTIME_LEGACY_HEALTH_KIND
  ) {
    throw new Error("prior runtime authority kind is invalid");
  }
  return value;
}

function requiredRecord(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function requiredMatch(value, pattern, label) {
  const text = typeof value === "string" ? value.trim() : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function canonicalJson(value) {
  if (Array.isArray(value)) return value.map(canonicalJson);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalJson(value[key])]),
    );
  }
  if (
    ["string", "number", "boolean"].includes(typeof value) ||
    value === null
  ) {
    return value;
  }
  throw new Error("prior runtime authority contains a non-JSON value");
}

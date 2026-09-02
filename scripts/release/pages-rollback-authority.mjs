// SRP rationale: rollback authority stays cohesive here because the behavior-level change reason is to capture, resolve, validate, and consume one exact Pages snapshot through a closed provenance chain.
import { createHash } from "node:crypto";
import { appendFile, lstat, open, readFile, readdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  canonicalSha256,
  rejectSecretMaterial,
  requireExactKeys,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";

import {
  resolveCanonicalAcceptanceHistory,
  validateCanonicalAcceptanceLookup,
} from "./canonical-acceptance-run.mjs";

import {
  LEGACY_PAGES_RELEASE_TAG,
  LEGACY_PAGES_SNAPSHOT_SHA,
  LEGACY_PAGES_TAG_OBJECT_SHA,
  LEGACY_PAGES_PAYLOAD,
  LEGACY_PAGES_PAYLOADS,
  createLegacyPublicReadbackEvidence,
  decodeLegacyPublicReadbackEvidence,
  encodeLegacyPublicReadbackEvidence,
  legacyReconstructedIdentitySha256,
  readLegacyReconstructedIdentity,
  validateLegacyPublicReadbackEvidence,
  validateLegacyForwardPublicAuthority,
  validateLegacyPagesPublicSnapshot,
  validateLegacyReconstructedIdentity,
} from "./pages-legacy-contract.mjs";

import {
  CLEARRA_ARTIFACT_SCHEMA_VERSION,
  CLEARRA_CONTRACT_SCHEMA_VERSION,
  CLEARRA_SUPPLY_SEMANTICS_ID,
} from "../tools/clearra-wasm-build-contract.mjs";

const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const DIGEST_PATTERN = /^sha256:[0-9a-f]{64}$/u;
const SHA_PATTERN = /^[0-9a-f]{40}$/u;
const DECIMAL_ID_PATTERN = /^[1-9][0-9]*$/u;
const RELEASE_TAG = "v0.8.0";
export const LEGACY_BOOTSTRAP_RELEASE_TAG = LEGACY_PAGES_RELEASE_TAG;
const MINIMUM_RETENTION_MS = 89 * 24 * 60 * 60 * 1000;
const HTTP_READ_TIMEOUT_MS = 30_000;
const MAX_JSON_RESPONSE_BYTES = 8 * 1024 * 1024;
export const MAX_PAGES_ROLLBACK_ARCHIVE_BYTES = 64 * 1024 * 1024;
export const MAX_PAGES_ROLLBACK_TAR_BYTES = 64 * 1024 * 1024;
const MAX_CAPTURE_REPORT_BYTES = 2 * 1024 * 1024;
const MAX_MODERN_PAGES_FILE_COUNT = 1_024;
const MAX_MODERN_PAGES_TOTAL_BYTES = 64 * 1024 * 1024;
const MAX_MODERN_PAGES_PATH_LENGTH = 512;
const MODERN_PAGES_PATH_PATTERN = /^[A-Za-z0-9._~-]+(?:\/[A-Za-z0-9._~-]+)*$/u;
const EXPECTED_CONTRACT = Object.freeze({
  schema: "clearra.pages.identity.v2",
  contractSchemaVersion: CLEARRA_CONTRACT_SCHEMA_VERSION,
  supplySemanticsId: CLEARRA_SUPPLY_SEMANTICS_ID,
  artifactSchemaVersion: CLEARRA_ARTIFACT_SCHEMA_VERSION,
});
const MODERN_PAGES_IDENTITY_FIELDS = Object.freeze([
  "acceptedRunAttempt",
  "acceptedRunId",
  "artifactSchemaVersion",
  "basePath",
  "contractSchemaVersion",
  "engineBuildId",
  "files",
  "schema",
  "sourceCommit",
  "supplySemanticsId",
  "version",
]);
const RECONSTRUCTED_PAGES_IDENTITY_FIELDS = Object.freeze([
  "artifactSchemaVersion",
  "contractSchemaVersion",
  "engineBuildId",
  "schema",
  "sourceCommit",
  "supplySemanticsId",
  "version",
]);
const RUNTIME_IDENTITY_FIELDS = Object.freeze([
  "source_commit",
  "engine_build_id",
  "contract_schema_version",
  "supply_semantics_id",
  "artifact_schema_version",
]);
export const PAGES_ROLLBACK_CAPTURE_REPORT_SCHEMA_ID =
  "clearra.pages.rollback-capture-authority.v2";
const CAPTURE_REPORT_FIELDS = Object.freeze([
  "schema_id",
  "repository",
  "snapshot_source_commit",
  "authority_source_commit",
  "capture_run_id",
  "capture_run_attempt",
  "workflow_path",
  "workflow_run_api_readback_sha256",
  "artifact_id",
  "artifact_name",
  "artifact_digest",
  "artifact_sha256",
  "artifact_archive_size_bytes",
  "artifact_tar_sha256",
  "artifact_tar_size_bytes",
  "artifact_api_readback_sha256",
  "artifact_created_at",
  "artifact_expires_at",
  "retention_seconds",
  "capture_kind",
  "legacy_snapshot",
  "status",
  "report_sha256",
]);

function fail(message) {
  throw new Error(message);
}

function requireString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    fail(`${label} must be a non-empty string`);
  }
  return value;
}

function requirePattern(value, pattern, label) {
  const text = requireString(value, label);
  if (!pattern.test(text)) {
    fail(`${label} has an invalid format`);
  }
  return text;
}

function requireSha(value, label) {
  return requirePattern(value, SHA_PATTERN, label);
}

function requireDecimalId(value, label) {
  return requirePattern(value, DECIMAL_ID_PATTERN, label);
}

function requireDate(value, label) {
  const text = requireString(value, label);
  const parsed = Date.parse(text);
  if (!Number.isFinite(parsed)) {
    fail(`${label} must be an ISO timestamp`);
  }
  return parsed;
}

function requireCanonicalDate(value, label) {
  const parsed = requireDate(value, label);
  if (new Date(parsed).toISOString() !== value) {
    fail(`${label} must be a canonical ISO timestamp`);
  }
  return parsed;
}

function requireObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  return value;
}

function requireBoundedByteSize(value, maximum, label) {
  if (!Number.isSafeInteger(value) || value <= 0 || value > maximum) {
    fail(`${label} must be a positive safe integer no greater than ${maximum}`);
  }
  return value;
}

function requireCompleteSinglePage(value, arrayField, label) {
  const payload = requireObject(value, label);
  const entries = payload[arrayField];
  if (!Array.isArray(entries)) {
    fail(`${label} must contain a ${arrayField} array`);
  }
  if (
    !Number.isSafeInteger(payload.total_count) ||
    payload.total_count < 0 ||
    payload.total_count > 100 ||
    payload.total_count !== entries.length
  ) {
    fail(`${label} must be one complete page with total_count equal to its array length`);
  }
  return entries;
}

export function validateCompleteDeploymentStatuses(
  firstPageValue,
  secondPageValue,
  label = "Pages deployment statuses",
) {
  if (
    !Array.isArray(firstPageValue) ||
    firstPageValue.length === 0 ||
    firstPageValue.length > 100
  ) {
    fail(`${label} first page must contain between 1 and 100 statuses`);
  }
  if (!Array.isArray(secondPageValue) || secondPageValue.length !== 0) {
    fail(`${label} second page must be exactly empty`);
  }
  return firstPageValue;
}

function captureArtifactAuthorityProjection(artifactValue) {
  const artifact = requireObject(artifactValue, "capture artifact API readback");
  const workflowRun = requireObject(
    artifact.workflow_run,
    "capture artifact workflow run API readback",
  );
  if (artifact.expired !== false) {
    fail("capture artifact API readback is expired");
  }
  return Object.freeze({
    id: requireDecimalId(String(artifact.id), "capture artifact API readback ID"),
    name: requireString(artifact.name, "capture artifact API readback name"),
    digest: requirePattern(
      artifact.digest,
      DIGEST_PATTERN,
      "capture artifact API readback digest",
    ),
    size_in_bytes: requireBoundedByteSize(
      artifact.size_in_bytes,
      MAX_PAGES_ROLLBACK_ARCHIVE_BYTES,
      "capture artifact API archive size",
    ),
    expired: false,
    created_at: new Date(requireDate(
      artifact.created_at,
      "capture artifact API readback created_at",
    )).toISOString(),
    expires_at: new Date(requireDate(
      artifact.expires_at,
      "capture artifact API readback expires_at",
    )).toISOString(),
    workflow_run: Object.freeze({
      id: requireDecimalId(String(workflowRun.id), "capture artifact workflow run ID"),
      head_branch: requireString(
        workflowRun.head_branch,
        "capture artifact workflow run branch",
      ),
      head_sha: requireSha(workflowRun.head_sha, "capture artifact workflow run SHA"),
    }),
  });
}

export function captureArtifactAuthoritySha256(artifactValue) {
  return canonicalSha256(captureArtifactAuthorityProjection(artifactValue));
}

function validatePagesIdentityCore(identityValue, manifestValue, expectedSha) {
  const sha = requireSha(expectedSha, "expected Pages SHA");
  const identity = requireObject(identityValue, "Pages identity");
  if (
    identity.schema !== EXPECTED_CONTRACT.schema ||
    identity.sourceCommit !== sha ||
    identity.engineBuildId !== sha ||
    identity.contractSchemaVersion !== EXPECTED_CONTRACT.contractSchemaVersion ||
    identity.supplySemanticsId !== EXPECTED_CONTRACT.supplySemanticsId ||
    identity.artifactSchemaVersion !== EXPECTED_CONTRACT.artifactSchemaVersion ||
    typeof identity.version !== "string" ||
    identity.version.length === 0
  ) {
    fail("Pages identity does not match the exact release contract");
  }
  const manifest = requireObject(manifestValue, "Pages WASM manifest");
  const runtimeIdentity = requireObject(
    requireObject(manifest.build, "Pages WASM build").runtime_identity,
    "Pages WASM runtime identity",
  );
  requireExactKeys(
    runtimeIdentity,
    RUNTIME_IDENTITY_FIELDS,
    "Pages WASM runtime identity",
  );
  if (
    runtimeIdentity.source_commit !== sha ||
    runtimeIdentity.engine_build_id !== sha ||
    runtimeIdentity.contract_schema_version !== EXPECTED_CONTRACT.contractSchemaVersion ||
    runtimeIdentity.supply_semantics_id !== EXPECTED_CONTRACT.supplySemanticsId ||
    runtimeIdentity.artifact_schema_version !== EXPECTED_CONTRACT.artifactSchemaVersion
  ) {
    fail("Pages WASM manifest does not match the exact release contract");
  }
  return Object.freeze({ identity, manifest });
}

export function validatePagesIdentity(identityValue, manifestValue, expectedSha) {
  const identity = requireObject(identityValue, "reconstructed Pages identity");
  requireExactKeys(
    identity,
    RECONSTRUCTED_PAGES_IDENTITY_FIELDS,
    "reconstructed Pages identity",
  );
  return validatePagesIdentityCore(identity, manifestValue, expectedSha);
}

export function validateLivePagesIdentity(identityValue, manifestValue, expectedSha) {
  const identity = requireObject(identityValue, "live Pages identity");
  requireExactKeys(identity, MODERN_PAGES_IDENTITY_FIELDS, "live Pages identity");
  const validated = validatePagesIdentityCore(identity, manifestValue, expectedSha);
  if (identity.basePath !== "/Clearra") {
    fail("live Pages identity base path differs from the exact release contract");
  }
  requireDecimalId(String(identity.acceptedRunId), "Pages accepted run ID");
  requireDecimalId(String(identity.acceptedRunAttempt), "Pages accepted run attempt");
  if (!Array.isArray(identity.files) || identity.files.length === 0) {
    fail("Pages identity files must be a non-empty array");
  }
  if (identity.files.length > MAX_MODERN_PAGES_FILE_COUNT) {
    fail("Pages identity file count exceeds the live authority limit");
  }
  let previousPath = "";
  let totalBytes = 0;
  const publicPaths = new Set();
  for (const [index, fileValue] of identity.files.entries()) {
    const file = requireObject(fileValue, `Pages identity file ${index}`);
    requireExactKeys(file, ["path", "sha256", "size"], `Pages identity file ${index}`);
    const segments = typeof file.path === "string" ? file.path.split("/") : [];
    if (
      typeof file.path !== "string" ||
      file.path.length === 0 ||
      file.path.length > MAX_MODERN_PAGES_PATH_LENGTH ||
      !MODERN_PAGES_PATH_PATTERN.test(file.path) ||
      segments.some((segment) => segment === "." || segment === "..") ||
      file.path === "clearra-build-identity.json" ||
      (previousPath && previousPath.localeCompare(file.path, "en") >= 0) ||
      !Number.isSafeInteger(file.size) ||
      file.size < 0 ||
      file.size > MAX_MODERN_PAGES_TOTAL_BYTES
    ) {
      fail("Pages identity file set is invalid or unsorted");
    }
    requirePattern(file.sha256, SHA256_PATTERN, `Pages identity file ${file.path} SHA-256`);
    totalBytes += file.size;
    if (!Number.isSafeInteger(totalBytes) || totalBytes > MAX_MODERN_PAGES_TOTAL_BYTES) {
      fail("Pages identity payload bytes exceed the live authority limit");
    }
    previousPath = file.path;
    publicPaths.add(file.path);
  }
  for (const path of publicPaths) {
    let slashIndex = path.indexOf("/");
    while (slashIndex !== -1) {
      if (publicPaths.has(path.slice(0, slashIndex))) {
        fail("Pages identity file set is invalid or unsorted");
      }
      slashIndex = path.indexOf("/", slashIndex + 1);
    }
  }
  return validated;
}

function exactRuntimeIdentity(expectedSha) {
  const sha = requireSha(expectedSha, "rollback manifest snapshot SHA");
  return Object.freeze({
    source_commit: sha,
    engine_build_id: sha,
    contract_schema_version: CLEARRA_CONTRACT_SCHEMA_VERSION,
    supply_semantics_id: CLEARRA_SUPPLY_SEMANTICS_ID,
    artifact_schema_version: CLEARRA_ARTIFACT_SCHEMA_VERSION,
  });
}

function validateRollbackManifestRuntimeIdentity(manifestValue, expectedSha, label) {
  const manifest = requireObject(manifestValue, label);
  const build = requireObject(manifest.build, `${label} build`);
  const identity = requireObject(build.runtime_identity, `${label} runtime identity`);
  requireExactKeys(identity, RUNTIME_IDENTITY_FIELDS, `${label} runtime identity`);
  const expected = exactRuntimeIdentity(expectedSha);
  for (const field of RUNTIME_IDENTITY_FIELDS) {
    if (identity[field] !== expected[field]) {
      fail(`${label} runtime identity differs from the exact snapshot contract`);
    }
  }
  return manifest;
}

async function readRollbackManifest(path, label) {
  const target = resolve(requireString(path, `${label} path`));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size <= 0) {
    fail(`${label} must be a non-empty regular non-link file`);
  }
  const raw = await readFile(target, "utf8");
  let manifest;
  try {
    manifest = JSON.parse(raw);
  } catch {
    fail(`${label} is not valid JSON`);
  }
  return Object.freeze({ target, metadata, raw, manifest });
}

export async function preparePagesRollbackManifests({
  captureMode,
  snapshotSha,
  staticManifestPath,
  buildManifestPath,
}) {
  if (captureMode !== "capture") {
    fail("modern rollback manifest preparation is available only for regular capture");
  }
  const sha = requireSha(snapshotSha, "rollback manifest snapshot SHA");
  const [staticRecord, buildRecord] = await Promise.all([
    readRollbackManifest(staticManifestPath, "static Pages WASM manifest"),
    readRollbackManifest(buildManifestPath, "built Pages WASM manifest"),
  ]);
  if (staticRecord.target === buildRecord.target) {
    fail("static and built Pages WASM manifests must be distinct files");
  }

  validateRollbackManifestRuntimeIdentity(staticRecord.manifest, sha, "static Pages WASM manifest");
  validateRollbackManifestRuntimeIdentity(buildRecord.manifest, sha, "built Pages WASM manifest");
  if (staticRecord.raw !== buildRecord.raw) {
    fail("static and built Pages WASM manifests differ before capture");
  }
  return Object.freeze({ mode: captureMode, updated: false });
}

export function validateLegacyPagesBootstrapAuthority(value) {
  requireExactKeys(value, [
    "repository",
    "legacyReleaseTag",
    "snapshotSha",
    "tagRef",
    "annotatedTag",
    "release",
    "deployments",
    "deploymentStatuses",
    "pageUrl",
    "identityStatus",
  ], "legacy Pages bootstrap authority");
  const repository = requirePattern(
    value.repository,
    /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u,
    "legacy Pages bootstrap repository",
  );
  if (repository !== "daejunnom/Clearra") {
    fail("legacy Pages bootstrap repository differs from the approved release");
  }
  const legacyReleaseTag = requireString(
    value.legacyReleaseTag,
    "legacy Pages bootstrap release tag",
  );
  if (legacyReleaseTag !== LEGACY_BOOTSTRAP_RELEASE_TAG) {
    fail("legacy Pages bootstrap accepts only the exact approved release tag");
  }
  const snapshotSha = requireSha(value.snapshotSha, "legacy Pages snapshot SHA");
  if (snapshotSha !== LEGACY_PAGES_SNAPSHOT_SHA) {
    fail("legacy Pages bootstrap snapshot differs from the approved v0.7.4 commit");
  }
  const tagRef = requireObject(value.tagRef, "legacy release tag ref");
  const tagObjectSha = requireSha(tagRef.object?.sha, "legacy annotated tag object SHA");
  if (
    tagRef.ref !== `refs/tags/${legacyReleaseTag}` ||
    tagRef.object?.type !== "tag"
  ) {
    fail("legacy Pages bootstrap release tag must be an annotated tag");
  }
  if (tagObjectSha !== LEGACY_PAGES_TAG_OBJECT_SHA) {
    fail("legacy Pages bootstrap tag object differs from the approved annotated tag");
  }
  const annotatedTag = requireObject(value.annotatedTag, "legacy annotated tag");
  if (
    annotatedTag.sha !== tagObjectSha ||
    annotatedTag.tag !== legacyReleaseTag ||
    annotatedTag.object?.type !== "commit" ||
    annotatedTag.object?.sha !== snapshotSha
  ) {
    fail("legacy annotated tag does not peel to the exact Pages snapshot");
  }
  requireObject(annotatedTag.tagger, "legacy annotated tagger");

  const release = requireObject(value.release, "legacy GitHub Release");
  if (
    release.tag_name !== legacyReleaseTag ||
    release.draft !== false ||
    release.prerelease !== false ||
    typeof release.published_at !== "string" ||
    !Number.isFinite(Date.parse(release.published_at))
  ) {
    fail("legacy Pages bootstrap requires the exact published stable Release");
  }

  if (!Array.isArray(value.deployments) || value.deployments.length !== 1) {
    fail("legacy Pages bootstrap requires one latest Pages deployment readback");
  }
  const deployment = requireObject(value.deployments[0], "latest Pages deployment");
  const deploymentId = requireDecimalId(
    String(deployment.id),
    "latest Pages deployment ID",
  );
  if (
    deployment.sha !== snapshotSha ||
    deployment.ref !== "main" ||
    deployment.task !== "deploy" ||
    deployment.environment !== "github-pages"
  ) {
    fail("latest Pages deployment is not the exact legacy snapshot");
  }
  if (
    !Array.isArray(value.deploymentStatuses) ||
    value.deploymentStatuses.length < 1
  ) {
    fail("latest Pages deployment has no status readback");
  }
  const latestStatus = requireObject(
    value.deploymentStatuses[0],
    "latest Pages deployment status",
  );
  const normalizedPageUrl = `${requireString(value.pageUrl, "Pages URL").replace(/\/$/u, "")}/`;
  if (
    latestStatus.state !== "success" ||
    latestStatus.environment !== "github-pages" ||
    latestStatus.environment_url !== normalizedPageUrl ||
    latestStatus.deployment_url !==
      `https://api.github.com/repos/${repository}/deployments/${deploymentId}`
  ) {
    fail("latest Pages deployment is not the active successful production snapshot");
  }
  if (value.identityStatus !== 404) {
    fail("legacy Pages bootstrap requires the public release identity to be absent");
  }
  return Object.freeze({ repository, legacyReleaseTag, snapshotSha, deploymentId });
}

export function validateCanonicalRuns(value, expectedSha, label = "canonical runs") {
  const sha = requireSha(expectedSha, `${label} SHA`);
  return validateCanonicalAcceptanceLookup(value, {
    sourceCommit: sha,
    expectedCount: 1,
    label,
  });
}

export function expectedCaptureArtifactName({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureRunAttempt,
}) {
  const snapshot = requireSha(snapshotSha, "snapshot SHA");
  const authority = requireSha(authoritySha, "authority SHA");
  const runId = requireDecimalId(String(captureRunId), "capture run ID");
  const attempt = requireDecimalId(String(captureRunAttempt), "capture run attempt");
  return `clearra-pages-rollback-${snapshot}-authority-${authority}-run-${runId}-attempt-${attempt}`;
}

export function expectedCaptureReportArtifactName({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureRunAttempt,
}) {
  const snapshot = requireSha(snapshotSha, "snapshot SHA");
  const authority = requireSha(authoritySha, "authority SHA");
  const runId = requireDecimalId(String(captureRunId), "capture run ID");
  const attempt = requireDecimalId(String(captureRunAttempt), "capture run attempt");
  return `clearra-pages-rollback-capture-authority-${snapshot}-authority-${authority}-run-${runId}-attempt-${attempt}`;
}

function validateLegacySnapshotEvidence(value, {
  snapshotSha,
  authoritySha,
  captureRunId,
  captureRunAttempt,
}) {
  const legacy = requireObject(value, "legacy Pages capture snapshot");
  requireExactKeys(legacy, [
    "identity",
    "legacy_identity_sha256",
    "initial_public_readback",
    "preartifact_public_readback",
    "rebuilt_payloads",
    "rebuilt_payload_set_sha256",
  ], "legacy Pages capture snapshot");
  const identity = validateLegacyReconstructedIdentity(legacy.identity, {
    expectedSnapshotSha: snapshotSha,
    expectedAuthoritySha: authoritySha,
    expectedCaptureRunId: captureRunId,
    expectedCaptureRunAttempt: captureRunAttempt,
  });
  if (legacy.legacy_identity_sha256 !== legacyReconstructedIdentitySha256(identity)) {
    fail("legacy reconstructed identity SHA-256 differs from its canonical producer bytes");
  }
  const initial = validateLegacyPublicReadbackEvidence(
    legacy.initial_public_readback,
    { expectedPhase: "initial" },
  );
  const preartifact = validateLegacyPublicReadbackEvidence(
    legacy.preartifact_public_readback,
    { expectedPhase: "preartifact" },
  );
  const { phase: initialPhase, report_sha256: initialReportSha, ...initialProjection } = initial;
  const {
    phase: preartifactPhase,
    report_sha256: preartifactReportSha,
    ...preartifactProjection
  } = preartifact;
  void initialPhase;
  void initialReportSha;
  void preartifactPhase;
  void preartifactReportSha;
  if (canonicalJson(initialProjection) !== canonicalJson(preartifactProjection)) {
    fail("legacy Pages public authority changed between initial and preartifact readback");
  }
  if (
    initial.repository !== preartifact.repository ||
    initial.page_url !== preartifact.page_url ||
    initial.deployment_id !== preartifact.deployment_id ||
    initial.payload_set_sha256 !== preartifact.payload_set_sha256
  ) {
    fail("legacy Pages public authority projection changed between readbacks");
  }
  if (canonicalJson(initial.payloads) !== canonicalJson(preartifact.payloads)) {
    fail("legacy Pages public payload changed between initial and preartifact readback");
  }
  if (canonicalJson(legacy.rebuilt_payloads) !== canonicalJson(LEGACY_PAGES_PAYLOADS)) {
    fail("rebuilt legacy Pages payload projection differs from the approved bytes");
  }
  if (legacy.rebuilt_payload_set_sha256 !== canonicalSha256(LEGACY_PAGES_PAYLOADS)) {
    fail("rebuilt legacy Pages payload-set SHA-256 differs from the approved bytes");
  }
  if (canonicalJson(identity.payloads) !== canonicalJson(legacy.rebuilt_payloads)) {
    fail("legacy reconstructed identity differs from the rebuilt payload evidence");
  }
  return legacy;
}

export function validateRollbackCaptureReport(report, {
  expectedSnapshotSha,
  expectedAuthoritySha,
} = {}) {
  requireExactKeys(report, CAPTURE_REPORT_FIELDS, "Pages rollback capture report");
  verifyCanonicalReportHash(report, "Pages rollback capture report");
  if (report.schema_id !== PAGES_ROLLBACK_CAPTURE_REPORT_SCHEMA_ID) {
    fail("Pages rollback capture report schema is invalid");
  }
  requirePattern(report.repository, /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u, "repository");
  const snapshot = requireSha(report.snapshot_source_commit, "snapshot source commit");
  const authority = requireSha(report.authority_source_commit, "authority source commit");
  if (expectedSnapshotSha !== undefined && snapshot !== expectedSnapshotSha) {
    fail("Pages rollback capture report snapshot differs from the expected source");
  }
  if (expectedAuthoritySha !== undefined && authority !== expectedAuthoritySha) {
    fail("Pages rollback capture report authority differs from the expected source");
  }
  const runId = requireDecimalId(report.capture_run_id, "capture run ID");
  const runAttempt = requireDecimalId(report.capture_run_attempt, "capture run attempt");
  if (report.workflow_path !== ".github/workflows/pages-rollback.yml") {
    fail("Pages rollback capture report workflow path is invalid");
  }
  requirePattern(
    report.workflow_run_api_readback_sha256,
    SHA256_PATTERN,
    "capture workflow run API readback SHA-256",
  );
  requireDecimalId(report.artifact_id, "capture artifact ID");
  if (report.artifact_name !== expectedCaptureArtifactName({
    snapshotSha: snapshot,
    authoritySha: authority,
    captureRunId: runId,
    captureRunAttempt: runAttempt,
  })) {
    fail("Pages rollback capture report artifact name is not run-attempt-bound");
  }
  const digest = requirePattern(
    report.artifact_digest,
    DIGEST_PATTERN,
    "capture artifact digest",
  );
  requirePattern(report.artifact_sha256, SHA256_PATTERN, "capture artifact SHA-256");
  if (digest.slice("sha256:".length) !== report.artifact_sha256) {
    fail("Pages rollback capture artifact digest and SHA-256 differ");
  }
  requireBoundedByteSize(
    report.artifact_archive_size_bytes,
    MAX_PAGES_ROLLBACK_ARCHIVE_BYTES,
    "capture artifact API archive size",
  );
  requirePattern(
    report.artifact_tar_sha256,
    SHA256_PATTERN,
    "capture Pages tar SHA-256",
  );
  requireBoundedByteSize(
    report.artifact_tar_size_bytes,
    MAX_PAGES_ROLLBACK_TAR_BYTES,
    "capture Pages tar size",
  );
  requirePattern(
    report.artifact_api_readback_sha256,
    SHA256_PATTERN,
    "capture artifact API readback SHA-256",
  );
  const created = requireCanonicalDate(report.artifact_created_at, "capture artifact created_at");
  const expires = requireCanonicalDate(report.artifact_expires_at, "capture artifact expires_at");
  if (
    !Number.isSafeInteger(report.retention_seconds) ||
    report.retention_seconds < MINIMUM_RETENTION_MS / 1000 ||
    expires - created !== report.retention_seconds * 1000
  ) {
    fail("Pages rollback capture report retention is invalid");
  }
  if (report.capture_kind === "legacy-v0.7.4") {
    if (snapshot !== LEGACY_PAGES_SNAPSHOT_SHA) {
      fail("legacy Pages capture kind is restricted to the approved v0.7.4 snapshot");
    }
    validateLegacySnapshotEvidence(report.legacy_snapshot, {
      snapshotSha: snapshot,
      authoritySha: authority,
      captureRunId: runId,
      captureRunAttempt: runAttempt,
    });
  } else if (report.capture_kind === "modern-v2") {
    if (snapshot === LEGACY_PAGES_SNAPSHOT_SHA) {
      fail("approved v0.7.4 snapshot must not be fabricated as a modern Pages capture");
    }
    if (report.legacy_snapshot !== null) {
      fail("modern Pages capture report must not contain legacy snapshot authority");
    }
  } else {
    fail("Pages rollback capture report kind is invalid");
  }
  if (report.status !== "captured") {
    fail("Pages rollback capture report is not captured");
  }
  rejectSecretMaterial(report, "Pages rollback capture report");
  return report;
}

export async function produceRollbackCaptureReport(input, {
  getGithubJson,
  readArtifactTar,
  sleep = (milliseconds) => new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds)),
  attempts = 1,
} = {}) {
  if (
    typeof getGithubJson !== "function" ||
    (readArtifactTar !== undefined && typeof readArtifactTar !== "function")
  ) {
    fail("Pages rollback capture producer requires a GitHub reader and optional tar reader");
  }
  const repository = requirePattern(
    input.repository,
    /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u,
    "repository",
  );
  const snapshot = requireSha(input.snapshotSha, "snapshot SHA");
  const authority = requireSha(input.authoritySha, "authority SHA");
  const runId = requireDecimalId(String(input.captureRunId), "capture run ID");
  const runAttempt = requireDecimalId(
    String(input.captureRunAttempt),
    "capture run attempt",
  );
  const artifactId = requireDecimalId(String(input.artifactId), "capture artifact ID");
  const captureKind = input.captureMode === "bootstrap-capture"
    ? "legacy-v0.7.4"
    : input.captureMode === "capture"
      ? "modern-v2"
      : fail("capture report mode must be capture or bootstrap-capture");
  const legacySnapshot = captureKind === "legacy-v0.7.4"
    ? validateLegacySnapshotEvidence(input.legacySnapshot, {
      snapshotSha: snapshot,
      authoritySha: authority,
      captureRunId: runId,
      captureRunAttempt: runAttempt,
    })
    : null;
  if (captureKind === "modern-v2" && input.legacySnapshot != null) {
    fail("regular capture must not supply legacy snapshot authority");
  }
  const artifactName = expectedCaptureArtifactName({
    snapshotSha: snapshot,
    authoritySha: authority,
    captureRunId: runId,
    captureRunAttempt: runAttempt,
  });
  if (input.artifactName !== artifactName) {
    fail("capture artifact name is not bound to its exact run attempt");
  }
  if (!Number.isSafeInteger(attempts) || attempts < 1 || attempts > 10) {
    fail("capture artifact readback attempts must be 1 through 10");
  }
  const run = await getGithubJson(`/actions/runs/${runId}`, "capture workflow run");
  requireObject(run, "capture workflow run");
  if (
    String(run.id) !== runId ||
    String(run.run_attempt) !== runAttempt ||
    run.event !== "workflow_dispatch" ||
    run.status !== "in_progress" ||
    run.conclusion !== null ||
    run.head_branch !== "main" ||
    run.head_sha !== authority ||
    run.path !== ".github/workflows/pages-rollback.yml"
  ) {
    fail("capture workflow run does not match the active exact-main attempt");
  }
  let artifactValue;
  let digest;
  let artifactReadbackError;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      artifactValue = requireObject(
        await getGithubJson(`/actions/artifacts/${artifactId}`, "capture artifact"),
        "capture artifact",
      );
      const artifactRun = requireObject(
        artifactValue.workflow_run,
        "capture artifact workflow run",
      );
      digest = requirePattern(
        artifactValue.digest,
        DIGEST_PATTERN,
        "capture artifact digest",
      );
      if (
        String(artifactValue.id) !== artifactId ||
        artifactValue.name !== artifactName ||
        artifactValue.expired !== false ||
        String(artifactRun.id) !== runId ||
        artifactRun.head_branch !== "main" ||
        artifactRun.head_sha !== authority
      ) {
        fail("capture artifact metadata does not match its exact active run authority");
      }
      artifactReadbackError = undefined;
      break;
    } catch (error) {
      artifactReadbackError = error;
      if (attempt < attempts) await sleep(2_000);
    }
  }
  if (artifactReadbackError) throw artifactReadbackError;
  const createdMilliseconds = requireDate(artifactValue.created_at, "artifact created_at");
  const expiresMilliseconds = requireDate(artifactValue.expires_at, "artifact expires_at");
  const artifactArchiveSize = requireBoundedByteSize(
    artifactValue.size_in_bytes,
    MAX_PAGES_ROLLBACK_ARCHIVE_BYTES,
    "capture artifact API archive size",
  );
  const retentionSeconds = (expiresMilliseconds - createdMilliseconds) / 1000;
  if (
    !Number.isSafeInteger(retentionSeconds) ||
    retentionSeconds < MINIMUM_RETENTION_MS / 1000
  ) {
    fail("capture artifact retention is shorter than the durable authority policy");
  }
  const tarPath = resolve(requireString(input.artifactTarPath, "capture artifact tar path"));
  const metadata = await lstat(tarPath);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    fail("capture artifact tar must be a regular non-link file");
  }
  const tarSize = requireBoundedByteSize(
    metadata.size,
    MAX_PAGES_ROLLBACK_TAR_BYTES,
    "capture artifact tar size",
  );
  const tarDirectoryEntries = (await readdir(dirname(tarPath))).sort();
  if (tarDirectoryEntries.length !== 1 || tarDirectoryEntries[0] !== "artifact.tar") {
    fail("downloaded capture artifact must contain exactly one artifact.tar");
  }
  const tarBytes = readArtifactTar === undefined
    ? await readBoundedRegularFile(tarPath, MAX_PAGES_ROLLBACK_TAR_BYTES, "capture artifact tar")
    : Buffer.from(await readArtifactTar(tarPath));
  if (tarBytes.byteLength !== tarSize) {
    fail("capture artifact tar size changed during the sealed read");
  }
  const report = sealCanonicalReport({
    schema_id: PAGES_ROLLBACK_CAPTURE_REPORT_SCHEMA_ID,
    repository,
    snapshot_source_commit: snapshot,
    authority_source_commit: authority,
    capture_run_id: runId,
    capture_run_attempt: runAttempt,
    workflow_path: ".github/workflows/pages-rollback.yml",
    workflow_run_api_readback_sha256: canonicalSha256(run),
    artifact_id: artifactId,
    artifact_name: artifactName,
    artifact_digest: digest,
    artifact_sha256: digest.slice("sha256:".length),
    artifact_archive_size_bytes: artifactArchiveSize,
    artifact_tar_sha256: createHash("sha256").update(tarBytes).digest("hex"),
    artifact_tar_size_bytes: tarSize,
    artifact_api_readback_sha256: captureArtifactAuthoritySha256(artifactValue),
    artifact_created_at: new Date(createdMilliseconds).toISOString(),
    artifact_expires_at: new Date(expiresMilliseconds).toISOString(),
    retention_seconds: retentionSeconds,
    capture_kind: captureKind,
    legacy_snapshot: legacySnapshot === null ? null : structuredClone(legacySnapshot),
    status: "captured",
  });
  validateRollbackCaptureReport(report, {
    expectedSnapshotSha: snapshot,
    expectedAuthoritySha: authority,
  });
  return report;
}

export async function writeRollbackCaptureReport(path, report) {
  validateRollbackCaptureReport(report);
  const target = resolve(requireString(path, "Pages rollback capture report path"));
  await assertSafeDirectoryChain(dirname(target));
  const bytes = `${canonicalJson(report)}\n`;
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(bytes, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  return createHash("sha256").update(bytes, "utf8").digest("hex");
}

export function validatePagesMutationCaptureKind(mode, captureKind) {
  if (mode === "forward") {
    if (!new Set(["legacy-v0.7.4", "modern-v2"]).has(captureKind)) {
      fail("Pages forward capture report kind is unsupported");
    }
    return captureKind;
  }
  if (mode === "restore") {
    if (captureKind !== "legacy-v0.7.4") {
      fail("Pages restore requires a sealed v0.7.4 capture report");
    }
    return captureKind;
  }
  fail("Pages mutation capture kind is available only for forward or restore");
}

export function validatePagesAuthorityPhase(mode, phase) {
  const allowed = new Set(["capture", "bootstrap-capture"]).has(mode)
    ? new Set(["initial", "preartifact"])
    : new Set(["forward", "restore"]).has(mode)
      ? new Set(["initial", "predeploy"])
      : null;
  if (allowed === null || !allowed.has(phase)) {
    fail("Pages authority phase is invalid for its mode");
  }
  return phase;
}

export async function readBoundedRegularFile(path, maximumBytes, label) {
  const target = resolve(requireString(path, `${label} path`));
  if (!Number.isSafeInteger(maximumBytes) || maximumBytes <= 0) {
    fail(`${label} maximum byte length is invalid`);
  }
  const pathMetadata = await lstat(target);
  if (!pathMetadata.isFile() || pathMetadata.isSymbolicLink()) {
    fail(`${label} must be a regular non-link file`);
  }
  const expectedSize = requireBoundedByteSize(pathMetadata.size, maximumBytes, `${label} size`);
  const handle = await open(target, "r");
  try {
    const openedMetadata = await handle.stat();
    if (!openedMetadata.isFile() || openedMetadata.size !== expectedSize) {
      fail(`${label} changed before its bounded read`);
    }
    const bytes = Buffer.allocUnsafe(expectedSize);
    let offset = 0;
    while (offset < expectedSize) {
      const { bytesRead } = await handle.read(
        bytes,
        offset,
        expectedSize - offset,
        offset,
      );
      if (bytesRead === 0) fail(`${label} ended during its bounded read`);
      offset += bytesRead;
    }
    const overflowProbe = Buffer.allocUnsafe(1);
    const overflow = await handle.read(overflowProbe, 0, 1, expectedSize);
    const finalMetadata = await handle.stat();
    if (overflow.bytesRead !== 0 || finalMetadata.size !== expectedSize) {
      fail(`${label} grew during its bounded read`);
    }
    return bytes;
  } finally {
    await handle.close();
  }
}

export async function readRollbackCaptureReport(path) {
  const target = resolve(requireString(path, "Pages rollback capture report path"));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    fail("Pages rollback capture report must be a regular non-link file");
  }
  requireBoundedByteSize(
    metadata.size,
    MAX_CAPTURE_REPORT_BYTES,
    "Pages rollback capture report size",
  );
  const raw = (await readBoundedRegularFile(
    target,
    MAX_CAPTURE_REPORT_BYTES,
    "Pages rollback capture report",
  )).toString("utf8");
  let report;
  try {
    report = JSON.parse(raw);
  } catch {
    fail("Pages rollback capture report is not valid JSON");
  }
  if (raw !== `${canonicalJson(report)}\n`) {
    fail("Pages rollback capture report bytes are not canonical JSON");
  }
  validateRollbackCaptureReport(report);
  return Object.freeze({
    report,
    file_sha256: createHash("sha256").update(raw, "utf8").digest("hex"),
  });
}

export function resolveCaptureReportArtifact({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureRun,
  captureArtifacts,
  consumerRun,
}) {
  const snapshot = requireSha(snapshotSha, "snapshot SHA");
  const authority = requireSha(authoritySha, "authority SHA");
  const runId = requireDecimalId(String(captureRunId), "capture run ID");
  const run = requireObject(captureRun, "capture run");
  const runAttempt = requireDecimalId(String(run.run_attempt), "capture run attempt");
  if (
    String(run.id) !== runId ||
    run.event !== "workflow_dispatch" ||
    run.status !== "completed" ||
    run.conclusion !== "success" ||
    run.head_branch !== "main" ||
    run.head_sha !== authority ||
    run.path !== ".github/workflows/pages-rollback.yml"
  ) {
    fail("capture report run is not the successful exact-main rollback authority");
  }
  const expectedName = expectedCaptureReportArtifactName({
    snapshotSha: snapshot,
    authoritySha: authority,
    captureRunId: runId,
    captureRunAttempt: runAttempt,
  });
  const artifacts = requireCompleteSinglePage(
    captureArtifacts,
    "artifacts",
    "capture run artifacts",
  );
  const matches = artifacts.filter((artifact) => artifact?.name === expectedName);
  if (matches.length !== 1) {
    fail("capture run must contain exactly one sealed report artifact");
  }
  const artifact = requireObject(matches[0], "capture report artifact");
  const artifactRun = requireObject(
    artifact.workflow_run,
    "capture report artifact workflow run",
  );
  const artifactId = requireDecimalId(String(artifact.id), "capture report artifact ID");
  const artifactDigest = requirePattern(
    artifact.digest,
    DIGEST_PATTERN,
    "capture report artifact digest",
  );
  if (
    artifact.expired !== false ||
    String(artifactRun.id) !== runId ||
    artifactRun.head_branch !== "main" ||
    artifactRun.head_sha !== authority
  ) {
    fail("capture report artifact differs from its exact run authority");
  }
  validateDurableArtifactDates(artifact, "capture report artifact");
  validateIndependentConsumerRun(consumerRun, {
    captureRunId: runId,
    captureCompletedAt: run.updated_at,
    authoritySha: authority,
  });
  return Object.freeze({
    capture_run_attempt: runAttempt,
    report_artifact_id: artifactId,
    report_artifact_name: expectedName,
    report_artifact_digest: artifactDigest,
  });
}

export function validateCaptureReportArtifact({
  report,
  reportArtifactId,
  reportArtifactName,
  reportArtifactDigest,
  artifact,
}) {
  validateRollbackCaptureReport(report);
  const expectedName = expectedCaptureReportArtifactName({
    snapshotSha: report.snapshot_source_commit,
    authoritySha: report.authority_source_commit,
    captureRunId: report.capture_run_id,
    captureRunAttempt: report.capture_run_attempt,
  });
  const expectedId = requireDecimalId(
    String(reportArtifactId),
    "capture report artifact ID",
  );
  const expectedDigest = requirePattern(
    reportArtifactDigest,
    DIGEST_PATTERN,
    "capture report artifact digest",
  );
  if (reportArtifactName !== expectedName) {
    fail("capture report artifact name differs from the resolved authority");
  }
  const value = requireObject(artifact, "capture report artifact");
  const run = requireObject(value.workflow_run, "capture report artifact workflow run");
  if (
    String(value.id) !== expectedId ||
    value.name !== expectedName ||
    value.digest !== expectedDigest ||
    value.expired !== false ||
    String(run.id) !== report.capture_run_id ||
    run.head_branch !== "main" ||
    run.head_sha !== report.authority_source_commit
  ) {
    fail("downloaded capture report artifact differs from the resolved API authority");
  }
  validateDurableArtifactDates(value, "capture report artifact");
  return value;
}

function validateDurableArtifactDates(artifact, label) {
  const created = requireDate(artifact.created_at, `${label} created_at`);
  const expires = requireDate(artifact.expires_at, `${label} expires_at`);
  if (expires - created < MINIMUM_RETENTION_MS) {
    fail(`${label} retention is shorter than the durable authority policy`);
  }
}

function validateIndependentConsumerRun(consumerRun, {
  captureRunId,
  captureCompletedAt,
  authoritySha,
}) {
  const consumer = requireObject(consumerRun, "consumer run");
  if (
    String(consumer.id) === captureRunId ||
    consumer.event !== "workflow_dispatch" ||
    consumer.status !== "in_progress" ||
    consumer.conclusion !== null ||
    consumer.head_branch !== "main" ||
    consumer.head_sha !== authoritySha
  ) {
    fail("consumer run is not an independent active exact-main workflow_dispatch run");
  }
  const captureCompleted = requireDate(captureCompletedAt, "capture run updated_at");
  const consumerCreated = requireDate(consumer.created_at, "consumer run created_at");
  if (captureCompleted >= consumerCreated) {
    fail("capture run must complete before the consuming Pages mutation starts");
  }
}

export function validateRunAttemptPolicy(mode, runAttempt) {
  const attempt = requireDecimalId(String(runAttempt), "current run attempt");
  if (!new Set(["capture", "bootstrap-capture"]).has(mode) && attempt !== "1") {
    fail("forward and restore mutations require a fresh workflow dispatch, not a rerun");
  }
  return attempt;
}

export function validatePagesCaptureRequestInputs(value) {
  requireExactKeys(value, [
    "mode",
    "legacyReleaseTag",
    "requestedCurrentPagesSha",
    "captureRunId",
    "restoreAuthorization",
  ], "Pages capture request inputs");
  if (!new Set(["capture", "bootstrap-capture"]).has(value.mode)) {
    fail("Pages capture request input validation requires a capture mode");
  }
  if (value.mode === "bootstrap-capture") {
    if (value.legacyReleaseTag !== LEGACY_BOOTSTRAP_RELEASE_TAG) {
      fail("legacy Pages bootstrap accepts only the exact approved release tag");
    }
  } else {
    assertEmpty(value.legacyReleaseTag, "legacy release tag");
  }
  assertEmpty(value.requestedCurrentPagesSha, "requested current Pages SHA");
  assertEmpty(value.captureRunId, "capture run ID");
  assertEmpty(value.restoreAuthorization, "restore authorization");
  return value;
}

export function validateCaptureAuthority({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureArtifactId,
  captureArtifactName,
  captureArtifactDigest,
  captureTarSha256,
  captureRun,
  captureJobs,
  artifact,
  consumerRun,
}) {
  const snapshot = requireSha(snapshotSha, "snapshot SHA");
  const authority = requireSha(authoritySha, "authority SHA");
  const runId = requireDecimalId(String(captureRunId), "capture run ID");
  const artifactId = requireDecimalId(String(captureArtifactId), "capture artifact ID");
  const digest = requirePattern(captureArtifactDigest, DIGEST_PATTERN, "capture artifact digest");
  requirePattern(captureTarSha256, SHA256_PATTERN, "capture Pages tar SHA-256");

  const run = requireObject(captureRun, "capture run");
  if (
    String(run.id) !== runId ||
    run.event !== "workflow_dispatch" ||
    run.status !== "completed" ||
    run.conclusion !== "success" ||
    run.head_branch !== "main" ||
    run.head_sha !== authority ||
    run.path !== ".github/workflows/pages-rollback.yml"
  ) {
    fail("capture run is not the successful exact-main rollback workflow authority");
  }
  const expectedName = expectedCaptureArtifactName({
    snapshotSha: snapshot,
    authoritySha: authority,
    captureRunId: runId,
    captureRunAttempt: run.run_attempt,
  });
  if (captureArtifactName !== expectedName) {
    fail("capture artifact name is not bound to its exact run attempt");
  }

  const jobs = requireCompleteSinglePage(captureJobs, "jobs", "capture jobs");
  const captureBuildJobs = jobs.filter((job) => job?.name === "capture-build");
  if (captureBuildJobs.length !== 1) {
    fail("capture run must contain exactly one capture-build job");
  }
  const captureBuild = requireObject(captureBuildJobs[0], "capture-build job");
  if (captureBuild.status !== "completed" || captureBuild.conclusion !== "success") {
    fail("capture-build job did not complete successfully");
  }

  const artifactValue = requireObject(artifact, "capture artifact");
  const artifactRun = requireObject(artifactValue.workflow_run, "capture artifact workflow run");
  if (
    String(artifactValue.id) !== artifactId ||
    artifactValue.name !== expectedName ||
    artifactValue.expired !== false ||
    artifactValue.digest !== digest ||
    String(artifactRun.id) !== runId ||
    artifactRun.head_branch !== "main" ||
    artifactRun.head_sha !== authority
  ) {
    fail("capture artifact metadata does not match its exact run authority");
  }

  const artifactCreated = requireDate(artifactValue.created_at, "artifact created_at");
  const artifactExpires = requireDate(artifactValue.expires_at, "artifact expires_at");
  const jobStarted = requireDate(captureBuild.started_at, "capture-build started_at");
  const jobCompleted = requireDate(captureBuild.completed_at, "capture-build completed_at");
  const captureCompleted = requireDate(run.updated_at, "capture run updated_at");
  if (
    artifactExpires - artifactCreated < MINIMUM_RETENTION_MS ||
    artifactCreated < jobStarted ||
    artifactCreated > jobCompleted ||
    jobCompleted > captureCompleted
  ) {
    fail("capture artifact retention or job ordering is invalid");
  }

  if (consumerRun !== undefined) {
    const consumer = requireObject(consumerRun, "consumer run");
    if (
      String(consumer.id) === runId ||
      consumer.event !== "workflow_dispatch" ||
      consumer.head_branch !== "main" ||
      consumer.head_sha !== authority
    ) {
      fail("consumer run is not an independent exact-main workflow_dispatch run");
    }
    const consumerCreated = requireDate(consumer.created_at, "consumer run created_at");
    if (captureCompleted >= consumerCreated) {
      fail("capture run must complete before the consuming Pages mutation starts");
    }
  }
}

function env(name, { optional = false } = {}) {
  const value = process.env[name];
  if ((value === undefined || value === "") && !optional) {
    fail(`${name} is required`);
  }
  return value ?? "";
}

function assertEmpty(value, label) {
  if (value !== "") {
    fail(`${label} must be empty in this mode`);
  }
}

function assertExact(value, expected, label) {
  if (value !== expected) {
    fail(`${label} differs from the exact authority`);
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      fail("Pages rollback capture report path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

export async function readBoundedResponseBytes(response, label, {
  maximumBytes,
  exactBytes,
} = {}) {
  if (!Number.isSafeInteger(maximumBytes) || maximumBytes <= 0) {
    fail(`${label} response byte limit is invalid`);
  }
  if (!response.ok) {
    fail(`${label} request failed with HTTP ${response.status}`);
  }
  const contentLengthValue = response.headers?.get?.("content-length");
  if (contentLengthValue != null) {
    if (!/^[0-9]+$/u.test(contentLengthValue)) {
      fail(`${label} response Content-Length is invalid`);
    }
    const contentLength = Number(contentLengthValue);
    if (!Number.isSafeInteger(contentLength) || contentLength > maximumBytes) {
      fail(`${label} response Content-Length exceeds the byte limit`);
    }
  }
  if (response.body === null || typeof response.body?.getReader !== "function") {
    fail(`${label} response body is not a bounded readable stream`);
  }
  const reader = response.body.getReader();
  const chunks = [];
  let total = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      const chunk = Buffer.from(value);
      total += chunk.byteLength;
      if (total > maximumBytes) {
        await reader.cancel("response byte limit exceeded").catch(() => {});
        fail(`${label} response body exceeds the byte limit`);
      }
      chunks.push(chunk);
    }
  } finally {
    reader.releaseLock();
  }
  if (exactBytes !== undefined && total !== exactBytes) {
    fail(`${label} response body differs from the exact byte length`);
  }
  return Buffer.concat(chunks, total);
}

async function parseJsonResponse(response, label) {
  const text = (await readBoundedResponseBytes(response, label, {
    maximumBytes: MAX_JSON_RESPONSE_BYTES,
  })).toString("utf8");
  try {
    return JSON.parse(text);
  } catch {
    fail(`${label} response is not JSON`);
  }
}

function apiClient({ repository, token, apiUrl }) {
  const base = `${apiUrl.replace(/\/$/u, "")}/repos/${repository}`;
  const headers = {
    Accept: "application/vnd.github+json",
    Authorization: `Bearer ${token}`,
    "X-GitHub-Api-Version": "2022-11-28",
  };
  return {
    async get(path, label) {
      return parseJsonResponse(await fetch(`${base}${path}`, {
        headers,
        cache: "no-store",
        redirect: "error",
        signal: AbortSignal.timeout(HTTP_READ_TIMEOUT_MS),
      }), label);
    },
    async requireAbsent(path, label) {
      const response = await fetch(`${base}${path}`, {
        headers,
        cache: "no-store",
        redirect: "error",
        signal: AbortSignal.timeout(HTTP_READ_TIMEOUT_MS),
      });
      if (response.status !== 404) {
        fail(`${label} must be absent before a pre-release Pages rollback`);
      }
    },
  };
}

async function fetchPublicJson(url, label) {
  const response = await fetch(url, {
    headers: { Accept: "application/json" },
    cache: "no-store",
    redirect: "error",
    signal: AbortSignal.timeout(HTTP_READ_TIMEOUT_MS),
  });
  return parseJsonResponse(response, label);
}

async function fetchPublicStatus(url) {
  const response = await fetch(url, {
    headers: { Accept: "application/json" },
    cache: "no-store",
    redirect: "error",
    signal: AbortSignal.timeout(HTTP_READ_TIMEOUT_MS),
  });
  return response.status;
}

async function fetchPublicBytes(url, label) {
  const expectedSize = arguments[2];
  requireBoundedByteSize(expectedSize, MAX_PAGES_ROLLBACK_TAR_BYTES, `${label} expected size`);
  const response = await fetch(url, {
    headers: { Accept: "application/octet-stream" },
    cache: "no-store",
    redirect: "error",
    signal: AbortSignal.timeout(HTTP_READ_TIMEOUT_MS),
  });
  return readBoundedResponseBytes(response, label, {
    maximumBytes: expectedSize,
    exactBytes: expectedSize,
  });
}

export function canonicalAcceptanceQuery(sha) {
  const sourceCommit = requireSha(sha, "canonical acceptance query SHA");
  return new URLSearchParams({
    event: "workflow_dispatch",
    branch: "main",
    head_sha: sourceCommit,
    per_page: "100",
  }).toString();
}

async function canonicalRuns(api, sha, label) {
  const query = canonicalAcceptanceQuery(sha);
  await resolveCanonicalAcceptanceHistory({
    sourceCommit: sha,
    expectedCount: 1,
    label,
  }, {
    listRuns() {
      return api.get(
        `/actions/workflows/release-cli.yml/runs?${query}`,
        label,
      );
    },
    getAttempt(runId, runAttempt) {
      return api.get(
        `/actions/runs/${runId}/attempts/${runAttempt}`,
        `${label} historical attempt`,
      );
    },
  });
}

function validateCaptureReportConsumerBinding(report, {
  repository,
  snapshotSha,
  authoritySha,
  captureRunId,
}) {
  if (
    report.repository !== repository ||
    report.snapshot_source_commit !== snapshotSha ||
    report.authority_source_commit !== authoritySha ||
    report.capture_run_id !== captureRunId
  ) {
    fail("sealed rollback capture report differs from the requested authority");
  }
  return report;
}

async function validateSealedCaptureConsumerAuthority({
  api,
  reportRecord,
  reportFields,
  repository,
  snapshotSha,
  authoritySha,
  currentRun,
}) {
  const { report, file_sha256: reportFileSha256 } = reportRecord;
  validateCaptureReportConsumerBinding(report, {
    repository,
    snapshotSha,
    authoritySha,
    captureRunId: reportFields.captureRunId,
  });
  const captureRunId = requireDecimalId(report.capture_run_id, "capture run ID");
  const captureArtifactId = requireDecimalId(report.artifact_id, "capture artifact ID");
  const reportArtifactId = requireDecimalId(
    reportFields.reportArtifactId,
    "capture report artifact ID",
  );
  const [captureRun, captureJobs, captureArtifacts, artifact, reportArtifact] =
    await Promise.all([
      api.get(`/actions/runs/${captureRunId}`, "capture run"),
      api.get(`/actions/runs/${captureRunId}/jobs?per_page=100`, "capture jobs"),
      api.get(
        `/actions/runs/${captureRunId}/artifacts?per_page=100`,
        "capture run artifacts",
      ),
      api.get(`/actions/artifacts/${captureArtifactId}`, "capture artifact"),
      api.get(`/actions/artifacts/${reportArtifactId}`, "capture report artifact"),
    ]);
  const resolvedReportArtifact = resolveCaptureReportArtifact({
    snapshotSha,
    authoritySha,
    captureRunId,
    captureRun,
    captureArtifacts,
    consumerRun: currentRun,
  });
  if (
    resolvedReportArtifact.capture_run_attempt !== report.capture_run_attempt ||
    resolvedReportArtifact.report_artifact_id !== reportArtifactId ||
    resolvedReportArtifact.report_artifact_name !== reportFields.reportArtifactName ||
    resolvedReportArtifact.report_artifact_digest !== reportFields.reportArtifactDigest
  ) {
    fail("capture report artifact list differs from the resolved consumer authority");
  }
  validateCaptureReportArtifact({
    report,
    reportArtifactId,
    reportArtifactName: reportFields.reportArtifactName,
    reportArtifactDigest: reportFields.reportArtifactDigest,
    artifact: reportArtifact,
  });
  validateCaptureAuthority({
    snapshotSha,
    authoritySha,
    captureRunId,
    captureArtifactId,
    captureArtifactName: report.artifact_name,
    captureArtifactDigest: report.artifact_digest,
    captureTarSha256: report.artifact_tar_sha256,
    captureRun,
    captureJobs,
    artifact,
    consumerRun: currentRun,
  });
  if (captureArtifactAuthoritySha256(artifact) !== report.artifact_api_readback_sha256) {
    fail("capture artifact API readback changed after the sealed report was produced");
  }
  return Object.freeze({ report, file_sha256: reportFileSha256 });
}

async function verifyLegacyForwardPublicAuthority({
  repository,
  pageUrl,
  cacheBuster,
  phase,
  sealedReadback,
}, {
  getGithubJson,
  readPublicStatus = fetchPublicStatus,
  readPublicBytes = fetchPublicBytes,
  validateLegacyAuthority = validateLegacyForwardPublicAuthority,
}) {
  if (
    typeof getGithubJson !== "function" ||
    typeof readPublicStatus !== "function" ||
    typeof readPublicBytes !== "function" ||
    typeof validateLegacyAuthority !== "function"
  ) {
    fail("legacy Pages forward verification requires closed GitHub and public readers");
  }
  const sealedDeploymentId = requireDecimalId(
    String(sealedReadback?.deployment_id),
    "sealed legacy Pages deployment ID",
  );
  const tagRef = await getGithubJson(
    `/git/ref/tags/${encodeURIComponent(LEGACY_PAGES_RELEASE_TAG)}`,
    "legacy release tag ref",
  );
  const annotatedTagSha = requireSha(
    tagRef?.object?.sha,
    "legacy annotated tag object SHA",
  );
  const [annotatedTag, release, deployment, deploymentStatusesFirstPage, deploymentStatusesSecondPage, identityStatus, manifestBytes, bindingsBytes, wasmBytes] =
    await Promise.all([
      getGithubJson(`/git/tags/${annotatedTagSha}`, "legacy annotated tag"),
      getGithubJson(
        `/releases/tags/${encodeURIComponent(LEGACY_PAGES_RELEASE_TAG)}`,
        "legacy GitHub Release",
      ),
      getGithubJson(`/deployments/${sealedDeploymentId}`, "sealed Pages deployment"),
      getGithubJson(
        `/deployments/${sealedDeploymentId}/statuses?per_page=100&page=1`,
        "sealed Pages deployment statuses first page",
      ),
      getGithubJson(
        `/deployments/${sealedDeploymentId}/statuses?per_page=100&page=2`,
        "sealed Pages deployment statuses second page",
      ),
      readPublicStatus(
        `${pageUrl}/clearra-build-identity.json?authority=${cacheBuster}`,
      ),
      readPublicBytes(
        `${pageUrl}/${LEGACY_PAGES_PAYLOAD.manifest.path}?authority=${cacheBuster}`,
        "legacy public Pages WASM manifest",
        LEGACY_PAGES_PAYLOAD.manifest.bytes,
      ),
      readPublicBytes(
        `${pageUrl}/${LEGACY_PAGES_PAYLOAD.bindings.path}?authority=${cacheBuster}`,
        "legacy public Pages WASM bindings",
        LEGACY_PAGES_PAYLOAD.bindings.bytes,
      ),
      readPublicBytes(
        `${pageUrl}/${LEGACY_PAGES_PAYLOAD.wasm.path}?authority=${cacheBuster}`,
        "legacy public Pages WASM binary",
        LEGACY_PAGES_PAYLOAD.wasm.bytes,
      ),
    ]);
  const deploymentStatuses = validateCompleteDeploymentStatuses(
    deploymentStatusesFirstPage,
    deploymentStatusesSecondPage,
    "sealed Pages deployment statuses",
  );
  return validateLegacyAuthority({
    phase,
    sealedReadback,
    repository,
    pageUrl: `${pageUrl}/`,
    identityStatus,
    tagRef,
    annotatedTag,
    release,
    deployment,
    deploymentStatuses,
    manifestBytes,
    bindingsBytes,
    wasmBytes,
  });
}

async function verifyModernPagesPublicAuthority({
  pageUrl,
  cacheBuster,
  currentPagesSha,
}, { readPublicJson = fetchPublicJson } = {}) {
  if (typeof readPublicJson !== "function") {
    fail("modern Pages verification requires a public JSON reader");
  }
  const [identity, manifest] = await Promise.all([
    readPublicJson(
      `${pageUrl}/clearra-build-identity.json?authority=${cacheBuster}`,
      "live Pages identity",
    ),
    readPublicJson(
      `${pageUrl}/wasm/clearra_wasm.manifest.json?authority=${cacheBuster}`,
      "live Pages WASM manifest",
    ),
  ]);
  validateLivePagesIdentity(identity, manifest, currentPagesSha);
}

export async function verifyCurrentPagesAgainstCapture({
  mode,
  phase,
  validatedCaptureReport,
  repository,
  pageUrl,
  cacheBuster,
  currentPagesSha,
}, dependencies = {}) {
  validatePagesAuthorityPhase(mode, phase);
  const report = requireObject(validatedCaptureReport, "validated capture report");
  const captureKind = validatePagesMutationCaptureKind(mode, report.capture_kind);
  if (mode === "forward") {
    if (captureKind === "legacy-v0.7.4") {
      return verifyLegacyForwardPublicAuthority({
        repository,
        pageUrl,
        cacheBuster,
        phase,
        sealedReadback: requireObject(
          report.legacy_snapshot,
          "validated legacy capture snapshot",
        ).preartifact_public_readback,
      }, dependencies);
    }
    if (captureKind === "modern-v2") {
      return verifyModernPagesPublicAuthority({
        pageUrl,
        cacheBuster,
        currentPagesSha,
      }, dependencies);
    }
    fail("Pages forward capture report kind is unsupported");
  }
  return verifyModernPagesPublicAuthority({
    pageUrl,
    cacheBuster,
    currentPagesSha,
  }, dependencies);
}

async function captureReportMain() {
  const repository = requireString(env("GITHUB_REPOSITORY"), "GitHub repository");
  const token = requireString(env("GH_TOKEN"), "GitHub token");
  const api = apiClient({
    repository,
    token,
    apiUrl: requireString(env("GITHUB_API_URL"), "GitHub API URL"),
  });
  const authoritySha = requireSha(env("AUTHORITY_SHA"), "authority SHA");
  assertExact(env("GITHUB_REF"), "refs/heads/main", "workflow ref");
  assertExact(requireSha(env("GITHUB_SHA"), "workflow SHA"), authoritySha, "workflow SHA");
  const captureMode = env("PAGES_CAPTURE_MODE");
  let legacySnapshot = null;
  if (captureMode === "bootstrap-capture") {
    const identity = await readLegacyReconstructedIdentity(env("LEGACY_IDENTITY_PATH"));
    legacySnapshot = {
      identity,
      legacy_identity_sha256: legacyReconstructedIdentitySha256(identity),
      initial_public_readback: decodeLegacyPublicReadbackEvidence(
        env("LEGACY_INITIAL_EVIDENCE_BASE64"),
        { expectedPhase: "initial" },
      ),
      preartifact_public_readback: decodeLegacyPublicReadbackEvidence(
        env("LEGACY_PREARTIFACT_EVIDENCE_BASE64"),
        { expectedPhase: "preartifact" },
      ),
      rebuilt_payloads: LEGACY_PAGES_PAYLOADS.map((payload) => ({ ...payload })),
      rebuilt_payload_set_sha256: canonicalSha256(LEGACY_PAGES_PAYLOADS),
    };
  } else if (captureMode === "capture") {
    for (const name of [
      "LEGACY_IDENTITY_PATH",
      "LEGACY_INITIAL_EVIDENCE_BASE64",
      "LEGACY_PREARTIFACT_EVIDENCE_BASE64",
    ]) {
      assertEmpty(env(name, { optional: true }), name);
    }
  } else {
    fail("PAGES_CAPTURE_MODE must be capture or bootstrap-capture for capture report sealing");
  }
  const report = await produceRollbackCaptureReport({
    repository,
    captureMode,
    legacySnapshot,
    snapshotSha: env("SNAPSHOT_SHA"),
    authoritySha,
    captureRunId: env("GITHUB_RUN_ID"),
    captureRunAttempt: env("GITHUB_RUN_ATTEMPT"),
    artifactId: env("CAPTURE_ARTIFACT_ID"),
    artifactName: env("CAPTURE_ARTIFACT_NAME"),
    artifactTarPath: env("CAPTURE_TAR_PATH"),
  }, {
    getGithubJson(path, label) {
      return api.get(path, label);
    },
    attempts: 6,
  });
  const reportPath = env("CAPTURE_REPORT_PATH");
  const reportFileSha256 = await writeRollbackCaptureReport(reportPath, report);
  const reportArtifactName = expectedCaptureReportArtifactName({
    snapshotSha: report.snapshot_source_commit,
    authoritySha: report.authority_source_commit,
    captureRunId: report.capture_run_id,
    captureRunAttempt: report.capture_run_attempt,
  });
  await appendFile(env("GITHUB_OUTPUT"), [
    `report_artifact_name=${reportArtifactName}`,
    `artifact_digest=${report.artifact_digest}`,
    `artifact_tar_sha256=${report.artifact_tar_sha256}`,
    `report_sha256=${report.report_sha256}`,
    `report_file_sha256=${reportFileSha256}`,
    `capture_kind=${report.capture_kind}`,
    "",
  ].join("\n"), "utf8");
  console.log(
    `pages_rollback_capture_report=sealed run=${report.capture_run_id}/${report.capture_run_attempt} artifact=${report.artifact_id}`,
  );
}

async function resolveCaptureReportMain(mode) {
  const consumerMode = mode === "resolve-forward" ? "forward" : "restore";
  const repository = requireString(env("GITHUB_REPOSITORY"), "GitHub repository");
  const authoritySha = requireSha(env("AUTHORITY_SHA"), "authority SHA");
  const snapshotSha = requireSha(env("SNAPSHOT_SHA"), "snapshot SHA");
  const runId = requireDecimalId(env("GITHUB_RUN_ID"), "current run ID");
  const runAttempt = validateRunAttemptPolicy(
    consumerMode,
    env("GITHUB_RUN_ATTEMPT"),
  );
  assertExact(env("GITHUB_REF"), "refs/heads/main", "workflow ref");
  assertExact(requireSha(env("GITHUB_SHA"), "workflow SHA"), authoritySha, "workflow SHA");
  const api = apiClient({
    repository,
    token: requireString(env("GH_TOKEN"), "GitHub token"),
    apiUrl: requireString(env("GITHUB_API_URL"), "GitHub API URL"),
  });
  const [remoteMain, consumerRun] = await Promise.all([
    api.get("/git/ref/heads/main", "remote main"),
    api.get(`/actions/runs/${runId}`, "current workflow run"),
  ]);
  assertExact(remoteMain?.object?.sha, authoritySha, "remote main SHA");
  const expectedPath = consumerMode === "forward"
    ? ".github/workflows/pages.yml"
    : ".github/workflows/pages-rollback.yml";
  if (
    String(consumerRun.id) !== runId ||
    String(consumerRun.run_attempt) !== runAttempt ||
    consumerRun.path !== expectedPath
  ) {
    fail("capture report consumer run differs from the exact workflow attempt");
  }
  const captureRunId = requireDecimalId(env("CAPTURE_RUN_ID"), "capture run ID");
  const [captureRun, captureArtifacts] = await Promise.all([
    api.get(`/actions/runs/${captureRunId}`, "capture run"),
    api.get(
      `/actions/runs/${captureRunId}/artifacts?per_page=100`,
      "capture run artifacts",
    ),
  ]);
  const resolved = resolveCaptureReportArtifact({
    snapshotSha,
    authoritySha,
    captureRunId,
    captureRun,
    captureArtifacts,
    consumerRun,
  });
  await appendFile(env("GITHUB_OUTPUT"), [
    `capture_run_attempt=${resolved.capture_run_attempt}`,
    `report_artifact_id=${resolved.report_artifact_id}`,
    `report_artifact_name=${resolved.report_artifact_name}`,
    `report_artifact_digest=${resolved.report_artifact_digest}`,
    "",
  ].join("\n"), "utf8");
  console.log(
    `pages_rollback_capture_report=resolved mode=${consumerMode} run=${captureRunId}/${resolved.capture_run_attempt}`,
  );
}

async function prepareRollbackManifestsMain() {
  const result = await preparePagesRollbackManifests({
    captureMode: env("PAGES_CAPTURE_MODE"),
    snapshotSha: env("SNAPSHOT_SHA"),
    staticManifestPath: env("STATIC_MANIFEST_PATH"),
    buildManifestPath: env("BUILD_MANIFEST_PATH"),
  });
  console.log(
    `pages_rollback_manifests=passed mode=${result.mode} updated=${result.updated ? "yes" : "no"}`,
  );
}

async function main() {
  const mode = env("PAGES_AUTHORITY_MODE");
  if (mode === "prepare-manifests") {
    await prepareRollbackManifestsMain();
    return;
  }
  if (mode === "capture-report") {
    await captureReportMain();
    return;
  }
  if (mode === "resolve-forward" || mode === "resolve-restore") {
    await resolveCaptureReportMain(mode);
    return;
  }
  if (!new Set(["capture", "bootstrap-capture", "forward", "restore"]).has(mode)) {
    fail("PAGES_AUTHORITY_MODE must be prepare-manifests, capture, bootstrap-capture, capture-report, resolve-forward, resolve-restore, forward, or restore");
  }
  const phase = validatePagesAuthorityPhase(mode, env("PAGES_AUTHORITY_PHASE"));
  const authoritySha = requireSha(env("AUTHORITY_SHA"), "authority SHA");
  const snapshotSha = requireSha(env("SNAPSHOT_SHA"), "snapshot SHA");
  const currentPagesSha = requireSha(env("CURRENT_PAGES_SHA"), "current Pages SHA");
  const repository = requireString(env("GITHUB_REPOSITORY"), "GitHub repository");
  const runId = requireDecimalId(env("GITHUB_RUN_ID"), "current run ID");
  const runAttempt = validateRunAttemptPolicy(mode, env("GITHUB_RUN_ATTEMPT"));
  const githubRef = env("GITHUB_REF");
  const githubSha = requireSha(env("GITHUB_SHA"), "workflow SHA");
  const token = requireString(env("GH_TOKEN"), "GitHub token");
  const apiUrl = requireString(env("GITHUB_API_URL"), "GitHub API URL");
  const legacyReleaseTag = env("LEGACY_RELEASE_TAG", { optional: true });
  const requestedCurrentPagesSha = env(
    "REQUESTED_CURRENT_PAGES_SHA",
    { optional: true },
  );
  const captureRunIdInput = env("CAPTURE_RUN_ID", { optional: true });
  const restoreAuthorization = env("RESTORE_AUTHORIZATION", { optional: true });
  const expectedPath = mode === "forward"
    ? ".github/workflows/pages.yml"
    : ".github/workflows/pages-rollback.yml";

  if (mode === "capture" || mode === "bootstrap-capture") {
    validatePagesCaptureRequestInputs({
      mode,
      legacyReleaseTag,
      requestedCurrentPagesSha,
      captureRunId: captureRunIdInput,
      restoreAuthorization,
    });
  } else {
    assertEmpty(legacyReleaseTag, "legacy release tag");
  }

  assertExact(githubRef, "refs/heads/main", "workflow ref");
  assertExact(githubSha, authoritySha, "workflow SHA");
  if (mode === "restore") {
    assertExact(currentPagesSha, authoritySha, "restore current Pages SHA");
  } else {
    assertExact(currentPagesSha, snapshotSha, `${mode} current Pages SHA`);
  }

  const api = apiClient({ repository, token, apiUrl });
  const remoteMain = await api.get("/git/ref/heads/main", "remote main");
  assertExact(remoteMain?.object?.sha, authoritySha, "remote main SHA");

  const currentRun = await api.get(`/actions/runs/${runId}`, "current workflow run");
  if (
    String(currentRun.id) !== runId ||
    String(currentRun.run_attempt) !== runAttempt ||
    currentRun.event !== "workflow_dispatch" ||
    currentRun.head_branch !== "main" ||
    currentRun.head_sha !== authoritySha ||
    currentRun.path !== expectedPath
  ) {
    fail("current workflow run is not bound to the exact main authority");
  }

  const comparison = await api.get(
    `/compare/${snapshotSha}...${authoritySha}`,
    "snapshot ancestry",
  );
  if (!new Set(["ahead", "identical"]).has(comparison.status)) {
    fail("snapshot SHA must be the authority main SHA or its ancestor");
  }
  await canonicalRuns(api, snapshotSha, "snapshot canonical runs");
  if (authoritySha !== snapshotSha) {
    await canonicalRuns(api, authoritySha, "authority canonical runs");
  }

  const pages = await api.get("/pages", "Pages configuration");
  const pageUrl = requireString(pages.html_url, "Pages URL").replace(/\/$/u, "");
  const cacheBuster = encodeURIComponent(`${mode}-${phase}-${runId}`);
  const captureReportFields = {
    captureRunId: captureRunIdInput,
    reportPath: env("CAPTURE_REPORT_PATH", { optional: true }),
    reportArtifactId: env("CAPTURE_REPORT_ARTIFACT_ID", { optional: true }),
    reportArtifactName: env("CAPTURE_REPORT_ARTIFACT_NAME", { optional: true }),
    reportArtifactDigest: env("CAPTURE_REPORT_ARTIFACT_DIGEST", { optional: true }),
  };
  let captureReportRecord = null;
  if (mode === "forward" || mode === "restore") {
    captureReportRecord = await validateSealedCaptureConsumerAuthority({
      api,
      reportRecord: await readRollbackCaptureReport(captureReportFields.reportPath),
      reportFields: captureReportFields,
      repository,
      snapshotSha,
      authoritySha,
      currentRun,
    });
    validatePagesMutationCaptureKind(
      mode,
      captureReportRecord.report.capture_kind,
    );
    await appendFile(env("GITHUB_OUTPUT"), [
      `capture_artifact_id=${captureReportRecord.report.artifact_id}`,
      `capture_artifact_name=${captureReportRecord.report.artifact_name}`,
      `capture_artifact_digest=${captureReportRecord.report.artifact_digest}`,
      `capture_tar_sha256=${captureReportRecord.report.artifact_tar_sha256}`,
      `capture_report_file_sha256=${captureReportRecord.file_sha256}`,
      "",
    ].join("\n"), "utf8");
    if (mode === "forward") {
      assertEmpty(restoreAuthorization, "restore authorization");
    } else {
      assertExact(
        restoreAuthorization,
        `ROLLBACK:${currentPagesSha}:TO:${snapshotSha}`,
        "restore authorization",
      );
    }
  }
  if (mode === "bootstrap-capture") {
    const tagRef = await api.get(
      `/git/ref/tags/${encodeURIComponent(legacyReleaseTag)}`,
      "legacy release tag ref",
    );
    if (tagRef?.object?.type !== "tag") {
      fail("legacy Pages bootstrap release tag must be an annotated tag");
    }
    const annotatedTag = await api.get(
      `/git/tags/${requireSha(tagRef.object.sha, "legacy annotated tag object SHA")}`,
      "legacy annotated tag",
    );
    const [release, deployments, identityStatus, manifestBytes, bindingsBytes, wasmBytes] = await Promise.all([
      api.get(
        `/releases/tags/${encodeURIComponent(legacyReleaseTag)}`,
        "legacy GitHub Release",
      ),
      api.get(
        "/deployments?environment=github-pages&per_page=1",
        "latest Pages deployment",
      ),
      fetchPublicStatus(
        `${pageUrl}/clearra-build-identity.json?authority=${cacheBuster}`,
      ),
      fetchPublicBytes(
        `${pageUrl}/${LEGACY_PAGES_PAYLOAD.manifest.path}?authority=${cacheBuster}`,
        "legacy public Pages WASM manifest",
        LEGACY_PAGES_PAYLOAD.manifest.bytes,
      ),
      fetchPublicBytes(
        `${pageUrl}/${LEGACY_PAGES_PAYLOAD.bindings.path}?authority=${cacheBuster}`,
        "legacy public Pages WASM bindings",
        LEGACY_PAGES_PAYLOAD.bindings.bytes,
      ),
      fetchPublicBytes(
        `${pageUrl}/${LEGACY_PAGES_PAYLOAD.wasm.path}?authority=${cacheBuster}`,
        "legacy public Pages WASM binary",
        LEGACY_PAGES_PAYLOAD.wasm.bytes,
      ),
    ]);
    if (!Array.isArray(deployments) || deployments.length !== 1) {
      fail("legacy Pages bootstrap requires one latest Pages deployment readback");
    }
    const deploymentId = requireDecimalId(
      String(deployments[0]?.id),
      "latest Pages deployment ID",
    );
    const [deploymentStatusesFirstPage, deploymentStatusesSecondPage] = await Promise.all([
      api.get(
        `/deployments/${deploymentId}/statuses?per_page=100&page=1`,
        "latest Pages deployment statuses first page",
      ),
      api.get(
        `/deployments/${deploymentId}/statuses?per_page=100&page=2`,
        "latest Pages deployment statuses second page",
      ),
    ]);
    const deploymentStatuses = validateCompleteDeploymentStatuses(
      deploymentStatusesFirstPage,
      deploymentStatusesSecondPage,
      "latest Pages deployment statuses",
    );
    const bootstrapAuthority = validateLegacyPagesBootstrapAuthority({
      repository,
      legacyReleaseTag,
      snapshotSha,
      tagRef,
      annotatedTag,
      release,
      deployments,
      deploymentStatuses,
      pageUrl,
      identityStatus,
    });
    validateLegacyPagesPublicSnapshot({
      identityStatus,
      manifestBytes,
      bindingsBytes,
      wasmBytes,
    });
    const evidence = createLegacyPublicReadbackEvidence({
      phase,
      repository,
      pageUrl: `${pageUrl}/`,
      deploymentId: bootstrapAuthority.deploymentId,
      identityStatus,
      tagRef,
      annotatedTag,
      release,
      deployment: deployments[0],
      deploymentStatuses,
      manifestBytes,
      bindingsBytes,
      wasmBytes,
    });
    await appendFile(
      env("GITHUB_OUTPUT"),
      `legacy_evidence_base64=${encodeLegacyPublicReadbackEvidence(evidence)}\n`,
      "utf8",
    );
  } else if (mode === "forward" || mode === "restore") {
    await verifyCurrentPagesAgainstCapture({
      mode,
      phase,
      validatedCaptureReport: captureReportRecord.report,
      repository,
      pageUrl,
      cacheBuster,
      currentPagesSha,
    }, {
      getGithubJson(path, label) {
        return api.get(path, label);
      },
    });
  } else {
    await verifyModernPagesPublicAuthority({
      pageUrl,
      cacheBuster,
      currentPagesSha,
    });
  }

  await api.requireAbsent(`/git/ref/tags/${RELEASE_TAG}`, `${RELEASE_TAG} tag`);
  await api.requireAbsent(`/releases/tags/${RELEASE_TAG}`, `${RELEASE_TAG} release`);

  if (mode === "capture" || mode === "bootstrap-capture") {
    for (const [label, value] of Object.entries(captureReportFields)) {
      assertEmpty(value, label);
    }
    assertEmpty(restoreAuthorization, "restore authorization");
  }

  console.log(`pages_rollback_authority=passed mode=${mode} phase=${phase}`);
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    console.error(`pages_rollback_authority=failed reason=${error.message}`);
    process.exitCode = 2;
  });
}

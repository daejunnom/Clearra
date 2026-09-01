// SRP rationale: rollback authority stays cohesive here because the behavior-level change reason is to capture, resolve, validate, and consume one exact Pages snapshot through a closed provenance chain.
import { createHash, randomUUID } from "node:crypto";
import { appendFile, lstat, open, readFile, rename, rm } from "node:fs/promises";
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
  CLEARRA_ARTIFACT_SCHEMA_VERSION,
  CLEARRA_CONTRACT_SCHEMA_VERSION,
  CLEARRA_SUPPLY_SEMANTICS_ID,
  serializeClearraWasmManifest,
} from "../tools/clearra-wasm-build-contract.mjs";

const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const DIGEST_PATTERN = /^sha256:[0-9a-f]{64}$/u;
const SHA_PATTERN = /^[0-9a-f]{40}$/u;
const DECIMAL_ID_PATTERN = /^[1-9][0-9]*$/u;
const RELEASE_TAG = "v0.8.0";
export const LEGACY_BOOTSTRAP_RELEASE_TAG = "v0.7.4";
const LEGACY_WASM_MANIFEST_BYTES = 768;
const LEGACY_WASM_CAPABILITIES_SHA256 =
  "6e6e2c1e973f62c6d6fa28f571b326104aec625e6879c4aca67df3364029d98b";
const MINIMUM_RETENTION_MS = 89 * 24 * 60 * 60 * 1000;
const EXPECTED_CONTRACT = Object.freeze({
  schema: "clearra.pages.identity.v2",
  contractSchemaVersion: CLEARRA_CONTRACT_SCHEMA_VERSION,
  supplySemanticsId: CLEARRA_SUPPLY_SEMANTICS_ID,
  artifactSchemaVersion: CLEARRA_ARTIFACT_SCHEMA_VERSION,
});
export const PAGES_ROLLBACK_CAPTURE_REPORT_SCHEMA_ID =
  "clearra.pages.rollback-capture-authority.v1";
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
  "artifact_tar_sha256",
  "artifact_api_readback_sha256",
  "artifact_created_at",
  "artifact_expires_at",
  "retention_seconds",
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

export function validatePagesIdentity(identityValue, manifestValue, expectedSha) {
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
  if (
    runtimeIdentity.source_commit !== sha ||
    runtimeIdentity.engine_build_id !== sha ||
    runtimeIdentity.contract_schema_version !== EXPECTED_CONTRACT.contractSchemaVersion ||
    runtimeIdentity.supply_semantics_id !== EXPECTED_CONTRACT.supplySemanticsId ||
    runtimeIdentity.artifact_schema_version !== EXPECTED_CONTRACT.artifactSchemaVersion
  ) {
    fail("Pages WASM manifest does not match the exact release contract");
  }
}

const RUNTIME_IDENTITY_FIELDS = Object.freeze([
  "source_commit",
  "engine_build_id",
  "contract_schema_version",
  "supply_semantics_id",
  "artifact_schema_version",
]);

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

function validateLegacyArtifact(value, label, prefix, suffix) {
  const artifact = requireObject(value, label);
  requireExactKeys(artifact, ["path", "bytes", "sha256"], label);
  const sha256 = requirePattern(artifact.sha256, SHA256_PATTERN, `${label} SHA-256`);
  if (!Number.isSafeInteger(artifact.bytes) || artifact.bytes <= 0) {
    fail(`${label} bytes must be a positive safe integer`);
  }
  if (artifact.path !== `${prefix}.${sha256.slice(0, 24)}${suffix}`) {
    fail(`${label} path does not match the v0.7.4 content-addressed artifact`);
  }
}

function serializeLegacyWasmManifest(manifest) {
  const json = JSON.stringify(manifest);
  const byteLength = Buffer.byteLength(json, "utf8") + 1;
  if (byteLength > LEGACY_WASM_MANIFEST_BYTES) {
    fail("legacy Pages WASM manifest exceeds the v0.7.4 fixed-byte contract");
  }
  return `${json}${" ".repeat(LEGACY_WASM_MANIFEST_BYTES - byteLength)}\n`;
}

export function validateLegacyPagesWasmManifest(manifestValue, rawBytes, label) {
  const manifest = requireObject(manifestValue, label);
  requireExactKeys(manifest, ["schema_version", "build", "bindings", "wasm"], label);
  if (manifest.schema_version !== 1) {
    fail(`${label} schema_version is not the v0.7.4 manifest schema`);
  }
  const build = requireObject(manifest.build, `${label} build`);
  if (Object.hasOwn(build, "runtime_identity")) {
    fail(`${label} runtime_identity must be absent before legacy bootstrap`);
  }
  requireExactKeys(
    build,
    ["contract_version", "source_sha256", "source_file_count", "capabilities_sha256"],
    `${label} build`,
  );
  if (
    build.contract_version !== 1 ||
    !SHA256_PATTERN.test(build.source_sha256) ||
    !Number.isSafeInteger(build.source_file_count) ||
    build.source_file_count <= 0 ||
    build.capabilities_sha256 !== LEGACY_WASM_CAPABILITIES_SHA256
  ) {
    fail(`${label} build is not the exact v0.7.4 legacy contract`);
  }
  validateLegacyArtifact(manifest.bindings, `${label} bindings`, "clearra_wasm", ".js");
  validateLegacyArtifact(manifest.wasm, `${label} wasm`, "clearra_wasm_bg", ".wasm");
  if (rawBytes !== serializeLegacyWasmManifest(manifest)) {
    fail(`${label} bytes do not match the deterministic v0.7.4 producer format`);
  }
  return manifest;
}

function manifestWithoutRuntimeIdentity(manifestValue) {
  const manifest = structuredClone(manifestValue);
  if (manifest?.build && typeof manifest.build === "object") {
    delete manifest.build.runtime_identity;
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

async function stageAtomicManifestReplacement(record, bytes) {
  const temporaryPath = `${record.target}.clearra-${randomUUID()}.tmp`;
  const handle = await open(temporaryPath, "wx", record.metadata.mode & 0o777);
  try {
    await handle.writeFile(bytes, "utf8");
    await handle.sync();
  } catch (error) {
    await handle.close();
    await rm(temporaryPath, { force: true });
    throw error;
  }
  await handle.close();
  return temporaryPath;
}

export async function preparePagesRollbackManifests({
  captureMode,
  snapshotSha,
  staticManifestPath,
  buildManifestPath,
}) {
  if (!new Set(["capture", "bootstrap-capture"]).has(captureMode)) {
    fail("rollback manifest preparation mode must be capture or bootstrap-capture");
  }
  const sha = requireSha(snapshotSha, "rollback manifest snapshot SHA");
  const [staticRecord, buildRecord] = await Promise.all([
    readRollbackManifest(staticManifestPath, "static Pages WASM manifest"),
    readRollbackManifest(buildManifestPath, "built Pages WASM manifest"),
  ]);
  if (staticRecord.target === buildRecord.target) {
    fail("static and built Pages WASM manifests must be distinct files");
  }

  if (captureMode === "capture") {
    validateRollbackManifestRuntimeIdentity(staticRecord.manifest, sha, "static Pages WASM manifest");
    validateRollbackManifestRuntimeIdentity(buildRecord.manifest, sha, "built Pages WASM manifest");
    if (canonicalJson(staticRecord.manifest) !== canonicalJson(buildRecord.manifest)) {
      fail("static and built Pages WASM manifests differ before capture");
    }
    return Object.freeze({ mode: captureMode, updated: false });
  }

  validateLegacyPagesWasmManifest(
    staticRecord.manifest,
    staticRecord.raw,
    "static Pages WASM manifest",
  );
  validateLegacyPagesWasmManifest(
    buildRecord.manifest,
    buildRecord.raw,
    "built Pages WASM manifest",
  );
  if (staticRecord.raw !== buildRecord.raw) {
    fail("static and built v0.7.4 Pages WASM manifests differ before bootstrap");
  }

  const originalProjection = canonicalJson(staticRecord.manifest);
  const upgradedManifest = structuredClone(staticRecord.manifest);
  upgradedManifest.build.runtime_identity = exactRuntimeIdentity(sha);
  if (canonicalJson(manifestWithoutRuntimeIdentity(upgradedManifest)) !== originalProjection) {
    fail("legacy Pages WASM manifest non-identity content changed during bootstrap");
  }
  const upgradedBytes = serializeClearraWasmManifest(upgradedManifest);
  let staticTemporaryPath = "";
  let buildTemporaryPath = "";
  let staticCommitted = false;
  let buildCommitted = false;
  try {
    staticTemporaryPath = await stageAtomicManifestReplacement(staticRecord, upgradedBytes);
    buildTemporaryPath = await stageAtomicManifestReplacement(buildRecord, upgradedBytes);
    const [currentStatic, currentBuild] = await Promise.all([
      readFile(staticRecord.target, "utf8"),
      readFile(buildRecord.target, "utf8"),
    ]);
    if (currentStatic !== staticRecord.raw || currentBuild !== buildRecord.raw) {
      fail("Pages WASM manifests changed while bootstrap replacement was staged");
    }
    await rename(staticTemporaryPath, staticRecord.target);
    staticCommitted = true;
    try {
      await rename(buildTemporaryPath, buildRecord.target);
      buildCommitted = true;
    } catch (commitError) {
      let restoreTemporaryPath = "";
      try {
        restoreTemporaryPath = await stageAtomicManifestReplacement(
          staticRecord,
          staticRecord.raw,
        );
        await rename(restoreTemporaryPath, staticRecord.target);
        restoreTemporaryPath = "";
      } catch (restoreError) {
        throw new AggregateError(
          [commitError, restoreError],
          "Pages WASM manifest transaction could not restore its first atomic replacement",
        );
      } finally {
        if (restoreTemporaryPath !== "") {
          await rm(restoreTemporaryPath, { force: true });
        }
      }
      throw commitError;
    }
  } finally {
    if (!staticCommitted && staticTemporaryPath !== "") {
      await rm(staticTemporaryPath, { force: true });
    }
    if (!buildCommitted && buildTemporaryPath !== "") {
      await rm(buildTemporaryPath, { force: true });
    }
  }

  const [preparedStatic, preparedBuild] = await Promise.all([
    readRollbackManifest(staticRecord.target, "prepared static Pages WASM manifest"),
    readRollbackManifest(buildRecord.target, "prepared built Pages WASM manifest"),
  ]);
  validateRollbackManifestRuntimeIdentity(preparedStatic.manifest, sha, "prepared static Pages WASM manifest");
  validateRollbackManifestRuntimeIdentity(preparedBuild.manifest, sha, "prepared built Pages WASM manifest");
  if (
    preparedStatic.raw !== upgradedBytes ||
    preparedBuild.raw !== upgradedBytes ||
    canonicalJson(manifestWithoutRuntimeIdentity(preparedStatic.manifest)) !== originalProjection ||
    canonicalJson(manifestWithoutRuntimeIdentity(preparedBuild.manifest)) !== originalProjection
  ) {
    fail("prepared Pages WASM manifests did not preserve the exact legacy artifact contract");
  }
  return Object.freeze({ mode: captureMode, updated: true });
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
  const legacyReleaseTag = requireString(
    value.legacyReleaseTag,
    "legacy Pages bootstrap release tag",
  );
  if (legacyReleaseTag !== LEGACY_BOOTSTRAP_RELEASE_TAG) {
    fail("legacy Pages bootstrap accepts only the exact approved release tag");
  }
  const snapshotSha = requireSha(value.snapshotSha, "legacy Pages snapshot SHA");
  const tagRef = requireObject(value.tagRef, "legacy release tag ref");
  const tagObjectSha = requireSha(tagRef.object?.sha, "legacy annotated tag object SHA");
  if (
    tagRef.ref !== `refs/tags/${legacyReleaseTag}` ||
    tagRef.object?.type !== "tag"
  ) {
    fail("legacy Pages bootstrap release tag must be an annotated tag");
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
  requirePattern(
    report.artifact_tar_sha256,
    SHA256_PATTERN,
    "capture Pages tar SHA-256",
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
  if (report.status !== "captured") {
    fail("Pages rollback capture report is not captured");
  }
  rejectSecretMaterial(report, "Pages rollback capture report");
  return report;
}

export async function produceRollbackCaptureReport(input, {
  getGithubJson,
  readArtifactTar = readFile,
  sleep = (milliseconds) => new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds)),
  attempts = 1,
} = {}) {
  if (typeof getGithubJson !== "function" || typeof readArtifactTar !== "function") {
    fail("Pages rollback capture producer requires GitHub and tar readers");
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
  const retentionSeconds = (expiresMilliseconds - createdMilliseconds) / 1000;
  if (
    !Number.isSafeInteger(retentionSeconds) ||
    retentionSeconds < MINIMUM_RETENTION_MS / 1000
  ) {
    fail("capture artifact retention is shorter than the durable authority policy");
  }
  const tarPath = resolve(requireString(input.artifactTarPath, "capture artifact tar path"));
  const metadata = await lstat(tarPath);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size <= 0) {
    fail("capture artifact tar must be a non-empty regular non-link file");
  }
  const tarBytes = await readArtifactTar(tarPath);
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
    artifact_tar_sha256: createHash("sha256").update(tarBytes).digest("hex"),
    artifact_api_readback_sha256: canonicalSha256(artifactValue),
    artifact_created_at: new Date(createdMilliseconds).toISOString(),
    artifact_expires_at: new Date(expiresMilliseconds).toISOString(),
    retention_seconds: retentionSeconds,
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

export async function readRollbackCaptureReport(path) {
  const target = resolve(requireString(path, "Pages rollback capture report path"));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size <= 0) {
    fail("Pages rollback capture report must be a non-empty regular non-link file");
  }
  const raw = await readFile(target, "utf8");
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
  const payload = requireObject(captureArtifacts, "capture run artifacts");
  if (!Array.isArray(payload.artifacts)) {
    fail("capture run artifacts must contain an artifacts array");
  }
  const matches = payload.artifacts.filter((artifact) => artifact?.name === expectedName);
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

  const jobsPayload = requireObject(captureJobs, "capture jobs");
  if (!Array.isArray(jobsPayload.jobs)) {
    fail("capture jobs must contain a jobs array");
  }
  const captureBuildJobs = jobsPayload.jobs.filter((job) => job?.name === "capture-build");
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

async function parseJsonResponse(response, label) {
  const text = await response.text();
  if (!response.ok) {
    fail(`${label} request failed with HTTP ${response.status}`);
  }
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
      return parseJsonResponse(await fetch(`${base}${path}`, { headers }), label);
    },
    async requireAbsent(path, label) {
      const response = await fetch(`${base}${path}`, { headers });
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
  });
  return parseJsonResponse(response, label);
}

async function fetchPublicStatus(url) {
  const response = await fetch(url, {
    headers: { Accept: "application/json" },
    cache: "no-store",
  });
  return response.status;
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
  const report = await produceRollbackCaptureReport({
    repository,
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
  const phase = env("PAGES_AUTHORITY_PHASE");
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
    const [release, deployments, identityStatus] = await Promise.all([
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
    ]);
    if (!Array.isArray(deployments) || deployments.length !== 1) {
      fail("legacy Pages bootstrap requires one latest Pages deployment readback");
    }
    const deploymentId = requireDecimalId(
      String(deployments[0]?.id),
      "latest Pages deployment ID",
    );
    const deploymentStatuses = await api.get(
      `/deployments/${deploymentId}/statuses?per_page=100`,
      "latest Pages deployment statuses",
    );
    validateLegacyPagesBootstrapAuthority({
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
  } else {
    const [identity, manifest] = await Promise.all([
      fetchPublicJson(
        `${pageUrl}/clearra-build-identity.json?authority=${cacheBuster}`,
        "live Pages identity",
      ),
      fetchPublicJson(
        `${pageUrl}/wasm/clearra_wasm.manifest.json?authority=${cacheBuster}`,
        "live Pages WASM manifest",
      ),
    ]);
    validatePagesIdentity(identity, manifest, currentPagesSha);
  }

  await api.requireAbsent(`/git/ref/tags/${RELEASE_TAG}`, `${RELEASE_TAG} tag`);
  await api.requireAbsent(`/releases/tags/${RELEASE_TAG}`, `${RELEASE_TAG} release`);

  const captureReportFields = {
    captureRunId: captureRunIdInput,
    reportPath: env("CAPTURE_REPORT_PATH", { optional: true }),
    reportArtifactId: env("CAPTURE_REPORT_ARTIFACT_ID", { optional: true }),
    reportArtifactName: env("CAPTURE_REPORT_ARTIFACT_NAME", { optional: true }),
    reportArtifactDigest: env("CAPTURE_REPORT_ARTIFACT_DIGEST", { optional: true }),
  };

  if (mode === "capture" || mode === "bootstrap-capture") {
    for (const [label, value] of Object.entries(captureReportFields)) {
      assertEmpty(value, label);
    }
    assertEmpty(restoreAuthorization, "restore authorization");
  } else {
    const { report, file_sha256: reportFileSha256 } =
      await readRollbackCaptureReport(captureReportFields.reportPath);
    if (
      report.repository !== repository ||
      report.snapshot_source_commit !== snapshotSha ||
      report.authority_source_commit !== authoritySha ||
      report.capture_run_id !== captureReportFields.captureRunId
    ) {
      fail("sealed rollback capture report differs from the requested authority");
    }
    const captureRun = await api.get(
      `/actions/runs/${requireDecimalId(report.capture_run_id, "capture run ID")}`,
      "capture run",
    );
    const [captureJobs, artifact, reportArtifact] = await Promise.all([
      api.get(`/actions/runs/${report.capture_run_id}/jobs?per_page=100`, "capture jobs"),
      api.get(
        `/actions/artifacts/${requireDecimalId(report.artifact_id, "capture artifact ID")}`,
        "capture artifact",
      ),
      api.get(
        `/actions/artifacts/${requireDecimalId(
          captureReportFields.reportArtifactId,
          "capture report artifact ID",
        )}`,
        "capture report artifact",
      ),
    ]);
    validateCaptureReportArtifact({
      report,
      reportArtifactId: captureReportFields.reportArtifactId,
      reportArtifactName: captureReportFields.reportArtifactName,
      reportArtifactDigest: captureReportFields.reportArtifactDigest,
      artifact: reportArtifact,
    });
    validateCaptureAuthority({
      snapshotSha,
      authoritySha,
      captureRunId: report.capture_run_id,
      captureArtifactId: report.artifact_id,
      captureArtifactName: report.artifact_name,
      captureArtifactDigest: report.artifact_digest,
      captureTarSha256: report.artifact_tar_sha256,
      captureRun,
      captureJobs,
      artifact,
      consumerRun: currentRun,
    });
    if (canonicalSha256(artifact) !== report.artifact_api_readback_sha256) {
      fail("capture artifact API readback changed after the sealed report was produced");
    }
    await appendFile(env("GITHUB_OUTPUT"), [
      `capture_artifact_id=${report.artifact_id}`,
      `capture_artifact_name=${report.artifact_name}`,
      `capture_artifact_digest=${report.artifact_digest}`,
      `capture_tar_sha256=${report.artifact_tar_sha256}`,
      `capture_report_file_sha256=${reportFileSha256}`,
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

  console.log(`pages_rollback_authority=passed mode=${mode} phase=${phase}`);
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    console.error(`pages_rollback_authority=failed reason=${error.message}`);
    process.exitCode = 2;
  });
}

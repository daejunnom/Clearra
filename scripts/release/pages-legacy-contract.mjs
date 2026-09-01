// SRP: this module is the only owner of the immutable v0.7.4 Pages payload
// contract. It reconstructs provenance around that payload without rewriting
// the legacy manifest or claiming modern runtime semantics for legacy bytes.
import { createHash } from "node:crypto";
import { lstat, open, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  canonicalSha256,
  requireExactKeys,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";

const SHA_PATTERN = /^[0-9a-f]{40}$/u;
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const DECIMAL_ID_PATTERN = /^[1-9][0-9]*$/u;

export const LEGACY_PAGES_IDENTITY_SCHEMA =
  "clearra.pages.identity.legacy-reconstructed.v1";
export const LEGACY_PAGES_READBACK_SCHEMA =
  "clearra.pages.legacy-public-readback.v1";
export const LEGACY_PAGES_RELEASE_TAG = "v0.7.4";
export const LEGACY_PAGES_VERSION = "0.7.4";
export const LEGACY_PAGES_BASE_PATH = "/Clearra";
export const LEGACY_PAGES_WORKFLOW_PATH = ".github/workflows/pages-rollback.yml";
export const LEGACY_PAGES_ORIGINAL_IDENTITY_STATUS = 404;
export const LEGACY_PAGES_TAG_OBJECT_SHA =
  "a95973dbc1c3c1919478328d12e4d25ddaedea71";
export const LEGACY_PAGES_SNAPSHOT_SHA =
  "0438d85f90b47c4ce89835f6a6d665a0415aa25a";

const LEGACY_BUILD = Object.freeze({
  contract_version: 1,
  source_sha256:
    "b21b3ad64148d69d86136a7b5ea6795910c5fcc7c1d42cc8675252944359ffaf",
  source_file_count: 2280,
  capabilities_sha256:
    "6e6e2c1e973f62c6d6fa28f571b326104aec625e6879c4aca67df3364029d98b",
});

export const LEGACY_PAGES_PAYLOAD = Object.freeze({
  manifest: Object.freeze({
    path: "wasm/clearra_wasm.manifest.json",
    bytes: 768,
    sha256:
      "64520ce1a37e1e710748a6644339ea0b497a685169e4a13bb3f07e3585352e53",
  }),
  bindings: Object.freeze({
    path: "wasm/clearra_wasm.512fd4e7ea2b5432679da44c.js",
    bytes: 42201,
    sha256:
      "512fd4e7ea2b5432679da44c1b74850b2d9022f2dc41b6d05db8822c00bf471a",
  }),
  wasm: Object.freeze({
    path: "wasm/clearra_wasm_bg.7690c81f5a63702a9154b4e9.wasm",
    bytes: 4235934,
    sha256:
      "7690c81f5a63702a9154b4e9bcce77a71457291cb2abf74c0fa4968fefffd276",
  }),
});

export const LEGACY_PAGES_PAYLOADS = Object.freeze([
  Object.freeze({ role: "bindings", ...LEGACY_PAGES_PAYLOAD.bindings }),
  Object.freeze({ role: "manifest", ...LEGACY_PAGES_PAYLOAD.manifest }),
  Object.freeze({ role: "wasm", ...LEGACY_PAGES_PAYLOAD.wasm }),
]);

const IDENTITY_FIELDS = Object.freeze([
  "schema",
  "version",
  "basePath",
  "releaseTag",
  "tagObjectSha",
  "sourceCommit",
  "reconstructionAuthorityCommit",
  "captureRunId",
  "captureRunAttempt",
  "workflowPath",
  "capturedPublicIdentityStatus",
  "payloads",
]);

function fail(message) {
  throw new Error(message);
}

function requireObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  return value;
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    fail(`${label} has an invalid format`);
  }
  return value;
}

function requireExact(value, expected, label) {
  if (value !== expected) fail(`${label} differs from the approved v0.7.4 contract`);
  return value;
}

function requireBytes(value, label) {
  if (!(value instanceof Uint8Array)) fail(`${label} must be bytes`);
  return Buffer.from(value.buffer, value.byteOffset, value.byteLength);
}

function validateDescriptor(value, expected, label) {
  const descriptor = requireObject(value, label);
  requireExactKeys(descriptor, ["path", "bytes", "sha256"], label);
  requireExact(descriptor.path, expected.path, `${label} path`);
  requireExact(descriptor.bytes, expected.bytes, `${label} byte length`);
  requireExact(descriptor.sha256, expected.sha256, `${label} SHA-256`);
  return descriptor;
}

function validatePayloadProjection(value, label) {
  if (!Array.isArray(value) || value.length !== LEGACY_PAGES_PAYLOADS.length) {
    fail(`${label} must contain the exact sorted v0.7.4 payload set`);
  }
  for (const [index, expected] of LEGACY_PAGES_PAYLOADS.entries()) {
    const descriptor = requireObject(value[index], `${label} ${index}`);
    requireExactKeys(descriptor, ["role", "path", "bytes", "sha256"], `${label} ${index}`);
    requireExact(descriptor.role, expected.role, `${label} ${index} role`);
    validateDescriptor(
      { path: descriptor.path, bytes: descriptor.bytes, sha256: descriptor.sha256 },
      expected,
      `${label} ${index}`,
    );
  }
  return value;
}

function validatePayloadBytes(value, expected, label) {
  const bytes = requireBytes(value, label);
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (bytes.byteLength !== expected.bytes || digest !== expected.sha256) {
    fail(`${label} differs from the approved v0.7.4 bytes`);
  }
  return bytes;
}

export function validateLegacyPagesWasmManifest(manifestValue, rawBytes, label) {
  const bytes = validatePayloadBytes(rawBytes, LEGACY_PAGES_PAYLOAD.manifest, label);
  let manifest;
  try {
    manifest = manifestValue ?? JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    fail(`${label} is not valid UTF-8 JSON`);
  }
  requireObject(manifest, label);
  requireExactKeys(manifest, ["schema_version", "build", "bindings", "wasm"], label);
  requireExact(manifest.schema_version, 1, `${label} schema_version`);
  const build = requireObject(manifest.build, `${label} build`);
  if (Object.hasOwn(build, "runtime_identity")) {
    fail(`${label} must not fabricate modern runtime_identity semantics`);
  }
  requireExactKeys(
    build,
    ["contract_version", "source_sha256", "source_file_count", "capabilities_sha256"],
    `${label} build`,
  );
  for (const [field, expected] of Object.entries(LEGACY_BUILD)) {
    requireExact(build[field], expected, `${label} build ${field}`);
  }
  validateDescriptor(manifest.bindings, {
    path: LEGACY_PAGES_PAYLOAD.bindings.path.slice("wasm/".length),
    bytes: LEGACY_PAGES_PAYLOAD.bindings.bytes,
    sha256: LEGACY_PAGES_PAYLOAD.bindings.sha256,
  }, `${label} bindings`);
  validateDescriptor(manifest.wasm, {
    path: LEGACY_PAGES_PAYLOAD.wasm.path.slice("wasm/".length),
    bytes: LEGACY_PAGES_PAYLOAD.wasm.bytes,
    sha256: LEGACY_PAGES_PAYLOAD.wasm.sha256,
  }, `${label} wasm`);
  return manifest;
}

export function validateLegacyPagesPayloadBytes({
  manifestBytes,
  bindingsBytes,
  wasmBytes,
}, label = "legacy Pages payload") {
  const manifest = validateLegacyPagesWasmManifest(
    undefined,
    manifestBytes,
    `${label} manifest`,
  );
  validatePayloadBytes(bindingsBytes, LEGACY_PAGES_PAYLOAD.bindings, `${label} bindings`);
  validatePayloadBytes(wasmBytes, LEGACY_PAGES_PAYLOAD.wasm, `${label} wasm`);
  return Object.freeze({
    manifest,
    manifest_sha256: LEGACY_PAGES_PAYLOAD.manifest.sha256,
    bindings_sha256: LEGACY_PAGES_PAYLOAD.bindings.sha256,
    wasm_sha256: LEGACY_PAGES_PAYLOAD.wasm.sha256,
  });
}

export function validateLegacyPagesPublicSnapshot(value, label = "public legacy Pages snapshot") {
  const readback = requireObject(value, label);
  requireExactKeys(
    readback,
    ["identityStatus", "manifestBytes", "bindingsBytes", "wasmBytes"],
    label,
  );
  if (readback.identityStatus !== 404) {
    fail(`${label} must not expose a fabricated modern or reconstructed identity`);
  }
  return validateLegacyPagesPayloadBytes(readback, label);
}

export function createLegacyReconstructedIdentity({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureRunAttempt,
}) {
  requireExact(snapshotSha, LEGACY_PAGES_SNAPSHOT_SHA, "legacy Pages snapshot SHA");
  const authority = requirePattern(
    authoritySha,
    SHA_PATTERN,
    "legacy Pages reconstruction authority SHA",
  );
  const runId = requirePattern(String(captureRunId), DECIMAL_ID_PATTERN, "legacy Pages capture run ID");
  const runAttempt = requirePattern(
    String(captureRunAttempt),
    DECIMAL_ID_PATTERN,
    "legacy Pages capture run attempt",
  );
  return Object.freeze({
    schema: LEGACY_PAGES_IDENTITY_SCHEMA,
    version: LEGACY_PAGES_VERSION,
    basePath: LEGACY_PAGES_BASE_PATH,
    releaseTag: LEGACY_PAGES_RELEASE_TAG,
    tagObjectSha: LEGACY_PAGES_TAG_OBJECT_SHA,
    sourceCommit: LEGACY_PAGES_SNAPSHOT_SHA,
    reconstructionAuthorityCommit: authority,
    captureRunId: runId,
    captureRunAttempt: runAttempt,
    workflowPath: LEGACY_PAGES_WORKFLOW_PATH,
    capturedPublicIdentityStatus: LEGACY_PAGES_ORIGINAL_IDENTITY_STATUS,
    payloads: LEGACY_PAGES_PAYLOADS.map((payload) => ({ ...payload })),
  });
}

export function validateLegacyReconstructedIdentity(identityValue, {
  expectedSnapshotSha = LEGACY_PAGES_SNAPSHOT_SHA,
  expectedAuthoritySha,
  expectedCaptureRunId,
  expectedCaptureRunAttempt,
} = {}) {
  const identity = requireObject(identityValue, "legacy reconstructed Pages identity");
  requireExactKeys(identity, IDENTITY_FIELDS, "legacy reconstructed Pages identity");
  requireExact(identity.schema, LEGACY_PAGES_IDENTITY_SCHEMA, "legacy Pages identity schema");
  requireExact(identity.version, LEGACY_PAGES_VERSION, "legacy Pages version");
  requireExact(identity.basePath, LEGACY_PAGES_BASE_PATH, "legacy Pages base path");
  requireExact(identity.releaseTag, LEGACY_PAGES_RELEASE_TAG, "legacy Pages release tag");
  requireExact(identity.tagObjectSha, LEGACY_PAGES_TAG_OBJECT_SHA, "legacy Pages tag object SHA");
  requireExact(identity.sourceCommit, LEGACY_PAGES_SNAPSHOT_SHA, "legacy Pages source commit");
  requireExact(expectedSnapshotSha, LEGACY_PAGES_SNAPSHOT_SHA, "expected legacy Pages snapshot SHA");
  const authority = requirePattern(
    identity.reconstructionAuthorityCommit,
    SHA_PATTERN,
    "legacy Pages reconstruction authority SHA",
  );
  const runId = requirePattern(
    identity.captureRunId,
    DECIMAL_ID_PATTERN,
    "legacy Pages capture run ID",
  );
  const runAttempt = requirePattern(
    identity.captureRunAttempt,
    DECIMAL_ID_PATTERN,
    "legacy Pages capture run attempt",
  );
  if (expectedAuthoritySha !== undefined && authority !== expectedAuthoritySha) {
    fail("legacy Pages reconstruction authority differs from the capture report");
  }
  if (expectedCaptureRunId !== undefined && runId !== String(expectedCaptureRunId)) {
    fail("legacy Pages capture run differs from the capture report");
  }
  if (
    expectedCaptureRunAttempt !== undefined &&
    runAttempt !== String(expectedCaptureRunAttempt)
  ) {
    fail("legacy Pages capture attempt differs from the capture report");
  }
  requireExact(identity.workflowPath, LEGACY_PAGES_WORKFLOW_PATH, "legacy Pages workflow path");
  requireExact(
    identity.capturedPublicIdentityStatus,
    LEGACY_PAGES_ORIGINAL_IDENTITY_STATUS,
    "legacy Pages original public identity status",
  );
  validatePayloadProjection(identity.payloads, "legacy identity payloads");
  return identity;
}

export function serializeLegacyReconstructedIdentity(identityValue) {
  const identity = validateLegacyReconstructedIdentity(identityValue);
  const canonicalProducerIdentity = createLegacyReconstructedIdentity({
    snapshotSha: identity.sourceCommit,
    authoritySha: identity.reconstructionAuthorityCommit,
    captureRunId: identity.captureRunId,
    captureRunAttempt: identity.captureRunAttempt,
  });
  return `${JSON.stringify(canonicalProducerIdentity, null, 2)}\n`;
}

export function legacyReconstructedIdentitySha256(identityValue) {
  return createHash("sha256")
    .update(serializeLegacyReconstructedIdentity(identityValue), "utf8")
    .digest("hex");
}

export function validateLegacyReconstructedIdentityBytes(rawBytes, options = {}) {
  const bytes = requireBytes(rawBytes, "legacy reconstructed Pages identity bytes");
  let identity;
  try {
    identity = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    fail("legacy reconstructed Pages identity is not valid UTF-8 JSON");
  }
  validateLegacyReconstructedIdentity(identity, options);
  if (!bytes.equals(Buffer.from(serializeLegacyReconstructedIdentity(identity), "utf8"))) {
    fail("legacy reconstructed Pages identity bytes are not canonical producer bytes");
  }
  return identity;
}

const READBACK_FIELDS = Object.freeze([
  "schema_id",
  "phase",
  "repository",
  "page_url",
  "release_tag",
  "tag_object_sha",
  "snapshot_source_commit",
  "deployment_id",
  "identity_status",
  "payloads",
  "payload_set_sha256",
  "tag_ref_api_readback_sha256",
  "annotated_tag_api_readback_sha256",
  "release_api_readback_sha256",
  "deployment_api_readback_sha256",
  "deployment_statuses_api_readback_sha256",
  "status",
  "report_sha256",
]);

export function createLegacyPublicReadbackEvidence({
  phase,
  repository,
  pageUrl,
  deploymentId,
  identityStatus,
  tagRef,
  annotatedTag,
  release,
  deployment,
  deploymentStatuses,
  manifestBytes,
  bindingsBytes,
  wasmBytes,
}) {
  if (!new Set(["initial", "preartifact"]).has(phase)) {
    fail("legacy Pages public readback phase must be initial or preartifact");
  }
  if (typeof repository !== "string" || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(repository)) {
    fail("legacy Pages public readback repository is invalid");
  }
  if (typeof pageUrl !== "string" || pageUrl !== "https://daejunnom.github.io/Clearra/") {
    fail("legacy Pages public readback URL differs from the approved site");
  }
  requirePattern(String(deploymentId), DECIMAL_ID_PATTERN, "legacy Pages deployment ID");
  validateLegacyPagesPublicSnapshot({
    identityStatus,
    manifestBytes,
    bindingsBytes,
    wasmBytes,
  });
  const report = sealCanonicalReport({
    schema_id: LEGACY_PAGES_READBACK_SCHEMA,
    phase,
    repository,
    page_url: pageUrl,
    release_tag: LEGACY_PAGES_RELEASE_TAG,
    tag_object_sha: LEGACY_PAGES_TAG_OBJECT_SHA,
    snapshot_source_commit: LEGACY_PAGES_SNAPSHOT_SHA,
    deployment_id: String(deploymentId),
    identity_status: identityStatus,
    payloads: LEGACY_PAGES_PAYLOADS.map((payload) => ({ ...payload })),
    payload_set_sha256: canonicalSha256(LEGACY_PAGES_PAYLOADS),
    tag_ref_api_readback_sha256: canonicalSha256({
      ref: tagRef?.ref,
      object: tagRef?.object,
    }),
    annotated_tag_api_readback_sha256: canonicalSha256({
      sha: annotatedTag?.sha,
      tag: annotatedTag?.tag,
      object: annotatedTag?.object,
      tagger: annotatedTag?.tagger,
    }),
    release_api_readback_sha256: canonicalSha256({
      tag_name: release?.tag_name,
      draft: release?.draft,
      prerelease: release?.prerelease,
      published_at: release?.published_at,
    }),
    deployment_api_readback_sha256: canonicalSha256({
      id: deployment?.id,
      sha: deployment?.sha,
      ref: deployment?.ref,
      task: deployment?.task,
      environment: deployment?.environment,
    }),
    deployment_statuses_api_readback_sha256: canonicalSha256(
      Array.isArray(deploymentStatuses)
        ? deploymentStatuses.map((status) => ({
          state: status?.state,
          environment: status?.environment,
          environment_url: status?.environment_url,
          deployment_url: status?.deployment_url,
        }))
        : deploymentStatuses,
    ),
    status: "verified",
  });
  return validateLegacyPublicReadbackEvidence(report, { expectedPhase: phase });
}

export function validateLegacyPublicReadbackEvidence(value, { expectedPhase } = {}) {
  const evidence = requireObject(value, "legacy Pages public readback evidence");
  requireExactKeys(evidence, READBACK_FIELDS, "legacy Pages public readback evidence");
  verifyCanonicalReportHash(evidence, "legacy Pages public readback evidence");
  requireExact(evidence.schema_id, LEGACY_PAGES_READBACK_SCHEMA, "legacy Pages readback schema");
  if (!new Set(["initial", "preartifact"]).has(evidence.phase)) {
    fail("legacy Pages public readback phase is invalid");
  }
  if (expectedPhase !== undefined && evidence.phase !== expectedPhase) {
    fail("legacy Pages public readback phase differs from its capture phase");
  }
  requireExact(evidence.repository, "daejunnom/Clearra", "legacy Pages repository");
  requireExact(evidence.page_url, "https://daejunnom.github.io/Clearra/", "legacy Pages URL");
  requireExact(evidence.release_tag, LEGACY_PAGES_RELEASE_TAG, "legacy Pages release tag");
  requireExact(evidence.tag_object_sha, LEGACY_PAGES_TAG_OBJECT_SHA, "legacy Pages tag object SHA");
  requireExact(evidence.snapshot_source_commit, LEGACY_PAGES_SNAPSHOT_SHA, "legacy Pages snapshot SHA");
  requirePattern(evidence.deployment_id, DECIMAL_ID_PATTERN, "legacy Pages deployment ID");
  requireExact(evidence.identity_status, 404, "legacy Pages public identity status");
  validatePayloadProjection(evidence.payloads, "legacy readback payloads");
  requireExact(
    evidence.payload_set_sha256,
    canonicalSha256(LEGACY_PAGES_PAYLOADS),
    "legacy Pages payload-set SHA-256",
  );
  for (const field of [
    "tag_ref_api_readback_sha256",
    "annotated_tag_api_readback_sha256",
    "release_api_readback_sha256",
    "deployment_api_readback_sha256",
    "deployment_statuses_api_readback_sha256",
  ]) {
    requirePattern(evidence[field], SHA256_PATTERN, `legacy Pages ${field}`);
  }
  requireExact(evidence.status, "verified", "legacy Pages readback status");
  return evidence;
}

export function encodeLegacyPublicReadbackEvidence(value) {
  const evidence = validateLegacyPublicReadbackEvidence(value);
  return Buffer.from(canonicalJson(evidence), "utf8").toString("base64");
}

export function decodeLegacyPublicReadbackEvidence(value, { expectedPhase } = {}) {
  if (typeof value !== "string" || value.length === 0 || !/^[A-Za-z0-9+/]+={0,2}$/u.test(value)) {
    fail("legacy Pages public readback evidence base64 is invalid");
  }
  let evidence;
  try {
    evidence = JSON.parse(Buffer.from(value, "base64").toString("utf8"));
  } catch {
    fail("legacy Pages public readback evidence base64 is not JSON");
  }
  if (Buffer.from(canonicalJson(evidence), "utf8").toString("base64") !== value) {
    fail("legacy Pages public readback evidence base64 is not canonical");
  }
  return validateLegacyPublicReadbackEvidence(evidence, { expectedPhase });
}

export function validateLegacyDeployedPagesSnapshot({
  identity,
  expectedIdentity,
  manifestBytes,
  bindingsBytes,
  wasmBytes,
}) {
  const expected = validateLegacyReconstructedIdentity(expectedIdentity);
  const actual = validateLegacyReconstructedIdentity(identity, {
    expectedAuthoritySha: expected.reconstructionAuthorityCommit,
    expectedCaptureRunId: expected.captureRunId,
    expectedCaptureRunAttempt: expected.captureRunAttempt,
  });
  if (canonicalJson(actual) !== canonicalJson(expected)) {
    fail("live legacy reconstructed identity differs from the sealed capture identity");
  }
  return validateLegacyPagesPayloadBytes({ manifestBytes, bindingsBytes, wasmBytes }, "live legacy Pages payload");
}

async function readRegularFile(path, label) {
  const target = resolve(path);
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size <= 0) {
    fail(`${label} must be a non-empty regular non-link file`);
  }
  return readFile(target);
}

async function readPayloadAtRoot(root, label) {
  const resolvedRoot = resolve(root);
  return {
    manifestBytes: await readRegularFile(
      resolve(resolvedRoot, ...LEGACY_PAGES_PAYLOAD.manifest.path.split("/")),
      `${label} manifest`,
    ),
    bindingsBytes: await readRegularFile(
      resolve(resolvedRoot, ...LEGACY_PAGES_PAYLOAD.bindings.path.split("/")),
      `${label} bindings`,
    ),
    wasmBytes: await readRegularFile(
      resolve(resolvedRoot, ...LEGACY_PAGES_PAYLOAD.wasm.path.split("/")),
      `${label} wasm`,
    ),
  };
}

export async function readLegacyReconstructedIdentity(path) {
  const raw = await readRegularFile(path, "legacy reconstructed Pages identity");
  return validateLegacyReconstructedIdentityBytes(raw);
}

export async function stampLegacyReconstructedPagesIdentity({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureRunAttempt,
  staticRoot,
  buildRoot,
}) {
  const resolvedStaticRoot = resolve(staticRoot);
  const resolvedBuildRoot = resolve(buildRoot);
  if (resolvedStaticRoot === resolvedBuildRoot) {
    fail("legacy static and built Pages roots must be distinct");
  }
  const [staticPayload, builtPayload] = await Promise.all([
    readPayloadAtRoot(resolvedStaticRoot, "rebuilt static legacy Pages payload"),
    readPayloadAtRoot(resolvedBuildRoot, "rebuilt deployable legacy Pages payload"),
  ]);
  validateLegacyPagesPayloadBytes(staticPayload, "rebuilt static legacy Pages payload");
  validateLegacyPagesPayloadBytes(builtPayload, "rebuilt deployable legacy Pages payload");
  for (const field of ["manifestBytes", "bindingsBytes", "wasmBytes"]) {
    if (!Buffer.from(staticPayload[field]).equals(Buffer.from(builtPayload[field]))) {
      fail("rebuilt static and deployable legacy Pages payloads differ");
    }
  }
  const identity = createLegacyReconstructedIdentity({
    snapshotSha,
    authoritySha,
    captureRunId,
    captureRunAttempt,
  });
  const identityPath = resolve(resolvedBuildRoot, "clearra-build-identity.json");
  let handle;
  try {
    handle = await open(identityPath, "wx", 0o600);
  } catch (error) {
    if (error?.code === "EEXIST") {
      fail("legacy reconstructed Pages identity must be absent before stamping");
    }
    throw error;
  }
  try {
    await handle.writeFile(serializeLegacyReconstructedIdentity(identity), "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  const verified = await readLegacyReconstructedIdentity(identityPath);
  if (canonicalJson(verified) !== canonicalJson(identity)) {
    fail("stamped legacy reconstructed Pages identity changed during writeback");
  }
  return identity;
}

function requiredEnvironment(name) {
  const value = process.env[name];
  if (typeof value !== "string" || value.length === 0) fail(`${name} is required`);
  return value;
}

async function main() {
  if (requiredEnvironment("PAGES_LEGACY_CONTRACT_MODE") !== "stamp") {
    fail("PAGES_LEGACY_CONTRACT_MODE must be stamp");
  }
  const identity = await stampLegacyReconstructedPagesIdentity({
    snapshotSha: requiredEnvironment("SNAPSHOT_SHA"),
    authoritySha: requiredEnvironment("AUTHORITY_SHA"),
    captureRunId: requiredEnvironment("GITHUB_RUN_ID"),
    captureRunAttempt: requiredEnvironment("GITHUB_RUN_ATTEMPT"),
    staticRoot: requiredEnvironment("LEGACY_STATIC_ROOT"),
    buildRoot: requiredEnvironment("LEGACY_BUILD_ROOT"),
  });
  console.log(
    `pages_legacy_contract=stamped source=${identity.sourceCommit} authority=${identity.reconstructionAuthorityCommit} run=${identity.captureRunId}/${identity.captureRunAttempt}`,
  );
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    console.error(`pages_legacy_contract=failed reason=${error.message}`);
    process.exitCode = 2;
  });
}

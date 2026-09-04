import { createHash } from "node:crypto";
import { appendFile, lstat, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { verifyAcceptedPagesBuild } from "./accepted-pages-build.mjs";
import { canonicalJson, canonicalSha256, requireExactKeys } from "./canonical-release-evidence.mjs";

const SHA = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const DIGEST = /^sha256:[0-9a-f]{64}$/u;
const ID = /^[1-9][0-9]*$/u;
const MAX_FILES = 1_024;
const MAX_TOTAL_BYTES = 64 * 1024 * 1024;
const MAX_IDENTITY_BYTES = 8 * 1024 * 1024;
const MIN_RETENTION_MS = 89 * 24 * 60 * 60 * 1_000;

function fail(message) { throw new Error(message); }
function required(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) fail(`${label} is invalid`);
  return value;
}

export function acceptedPagesArtifactName(sourceCommit, runId, runAttempt = "1") {
  return `accepted-pages-build-${required(sourceCommit, SHA, "source commit")}-run-${required(String(runId), ID, "accepted run ID")}-attempt-${required(String(runAttempt), ID, "accepted run attempt")}`;
}

export function resolveAcceptedPagesArtifact({ sourceCommit, acceptedRun, artifactPages }) {
  const sha = required(sourceCommit, SHA, "source commit");
  if (String(acceptedRun?.run_attempt) !== "1" || acceptedRun?.head_sha !== sha || acceptedRun?.status !== "completed" || acceptedRun?.conclusion !== "success" || acceptedRun?.event !== "workflow_dispatch" || acceptedRun?.head_branch !== "main" || acceptedRun?.path !== ".github/workflows/release-cli.yml") {
    fail("accepted Pages source must be the successful canonical attempt-1 run");
  }
  const runId = required(String(acceptedRun.id), ID, "accepted run ID");
  const expectedName = acceptedPagesArtifactName(sha, runId, "1");
  if (!Array.isArray(artifactPages) || artifactPages.length < 2) fail("accepted Pages artifact pagination is incomplete");
  const flattened = [];
  for (let index = 0; index < artifactPages.length; index += 1) {
    const page = artifactPages[index];
    if (!page || !Array.isArray(page.artifacts) || !Number.isSafeInteger(page.total_count)) fail("accepted Pages artifact page is invalid");
    if (index < artifactPages.length - 1 && page.artifacts.length !== 100 && page.total_count > flattened.length + page.artifacts.length) fail("accepted Pages artifact pagination has an early partial page");
    flattened.push(...page.artifacts);
  }
  if (artifactPages.at(-1).artifacts.length !== 0 || artifactPages[0].total_count !== flattened.length) fail("accepted Pages artifact pagination is not proven complete");
  const matches = flattened.filter((artifact) => artifact?.name === expectedName);
  if (matches.length !== 1) fail("accepted Pages run must contain exactly one run-attempt-bound build artifact");
  const artifact = matches[0];
  const created = Date.parse(artifact.created_at);
  const expires = Date.parse(artifact.expires_at);
  if (String(artifact.workflow_run?.id) !== runId || artifact.workflow_run?.head_sha !== sha || artifact.expired !== false || !DIGEST.test(artifact.digest) || !Number.isFinite(created) || !Number.isFinite(expires) || expires - created < MIN_RETENTION_MS) {
    fail("accepted Pages artifact metadata is not durable and exact-run-bound");
  }
  return Object.freeze({
    accepted_run_id: runId,
    accepted_run_attempt: "1",
    accepted_artifact_id: required(String(artifact.id), ID, "accepted artifact ID"),
    accepted_artifact_name: expectedName,
    accepted_artifact_digest: artifact.digest,
    accepted_artifact_api_readback_sha256: canonicalSha256(artifact),
    accepted_artifact_created_at: new Date(created).toISOString(),
    accepted_artifact_expires_at: new Date(expires).toISOString(),
  });
}

export function validateCanonicalCaptureEvidence(value, { sourceCommit } = {}) {
  requireExactKeys(value, ["accepted_run_id", "accepted_run_attempt", "accepted_artifact_id", "accepted_artifact_name", "accepted_artifact_digest", "accepted_artifact_api_readback_sha256", "accepted_artifact_created_at", "accepted_artifact_expires_at", "identity", "identity_sha256", "identity_bytes_sha256", "identity_bytes_size", "file_set_sha256", "file_count", "total_bytes", "initial_public_readback", "preartifact_public_readback"], "canonical Pages capture evidence");
  const sha = required(sourceCommit ?? value.identity?.sourceCommit, SHA, "canonical source commit");
  required(value.accepted_run_id, ID, "accepted run ID");
  if (value.accepted_run_attempt !== "1") fail("canonical Pages capture requires accepted attempt 1");
  required(value.accepted_artifact_id, ID, "accepted artifact ID");
  if (value.accepted_artifact_name !== acceptedPagesArtifactName(sha, value.accepted_run_id, "1")) fail("accepted artifact name is not exact-run-bound");
  required(value.accepted_artifact_digest, DIGEST, "accepted artifact digest");
  required(value.accepted_artifact_api_readback_sha256, SHA256, "accepted artifact API hash");
  const created = Date.parse(value.accepted_artifact_created_at);
  const expires = Date.parse(value.accepted_artifact_expires_at);
  if (!Number.isFinite(created) || !Number.isFinite(expires) || expires - created < MIN_RETENTION_MS) fail("accepted artifact evidence is not durable");
  if (canonicalSha256(value.identity) !== value.identity_sha256) fail("canonical identity hash differs");
  if (String(value.identity.acceptedRunId) !== value.accepted_run_id || String(value.identity.acceptedRunAttempt) !== "1" || value.identity.sourceCommit !== sha) fail("canonical identity differs from accepted authority");
  const files = value.identity.files;
  if (!Array.isArray(files) || files.length !== value.file_count || files.length === 0 || files.length > MAX_FILES) fail("canonical file count is invalid");
  const total = files.reduce((sum, file) => sum + file.size, 0);
  if (!Number.isSafeInteger(total) || total !== value.total_bytes || total > MAX_TOTAL_BYTES || canonicalSha256(files) !== value.file_set_sha256) fail("canonical file descriptor set is invalid");
  let previousPath = "";
  for (const [index, file] of files.entries()) {
    requireExactKeys(file, ["path", "sha256", "size"], `canonical file ${index}`);
    if (typeof file.path !== "string" || file.path.length === 0 || file.path === "clearra-build-identity.json" || (previousPath && previousPath.localeCompare(file.path, "en") >= 0) || !Number.isSafeInteger(file.size) || file.size < 0 || !SHA256.test(file.sha256)) fail("canonical file descriptor is invalid or unsorted");
    previousPath = file.path;
  }
  for (const phase of ["initial_public_readback", "preartifact_public_readback"]) {
    const readback = value[phase];
    requireExactKeys(readback, ["identity_sha256", "identity_bytes_sha256", "identity_bytes_size", "file_set_sha256", "file_count", "total_bytes"], phase);
    if (readback.identity_sha256 !== value.identity_sha256 || readback.identity_bytes_sha256 !== value.identity_bytes_sha256 || readback.identity_bytes_size !== value.identity_bytes_size || readback.file_set_sha256 !== value.file_set_sha256 || readback.file_count !== value.file_count || readback.total_bytes !== value.total_bytes) fail("public Pages bytes changed or differ from the accepted artifact");
  }
  return value;
}

export async function verifyCanonicalBuildAndPublic({ buildPath, sourceCommit, acceptedRunId, acceptedRunAttempt = "1", basePath, version, pageUrl, cacheBuster, fetchBytes = defaultFetchBytes }) {
  const identityPath = resolve(buildPath, "clearra-build-identity.json");
  const stat = await lstat(identityPath);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > MAX_IDENTITY_BYTES) fail("accepted Pages identity is not a bounded regular file");
  const identityBytes = await readFile(identityPath);
  let recordedIdentity;
  try { recordedIdentity = JSON.parse(identityBytes.toString("utf8")); } catch { fail("accepted Pages identity is not JSON"); }
  const identity = await verifyAcceptedPagesBuild(buildPath, { sourceCommit, acceptedRunId, acceptedRunAttempt, basePath, version: version ?? recordedIdentity.version });
  const liveIdentity = await fetchBytes(new URL(`clearra-build-identity.json?authority=${encodeURIComponent(cacheBuster)}-identity`, pageUrl).toString(), MAX_IDENTITY_BYTES);
  if (!Buffer.from(liveIdentity).equals(identityBytes)) fail("public Pages identity bytes differ from the accepted artifact");
  let totalBytes = 0;
  for (const [index, file] of identity.files.entries()) {
    const bytes = Buffer.from(await fetchBytes(new URL(`${file.path}?authority=${encodeURIComponent(cacheBuster)}-${index}`, pageUrl).toString(), file.size));
    if (bytes.byteLength !== file.size || createHash("sha256").update(bytes).digest("hex") !== file.sha256) fail(`public Pages payload differs from accepted artifact: ${file.path}`);
    totalBytes += bytes.byteLength;
  }
  return Object.freeze({ identity, identity_sha256: canonicalSha256(identity), identity_bytes_sha256: createHash("sha256").update(identityBytes).digest("hex"), identity_bytes_size: identityBytes.byteLength, file_set_sha256: canonicalSha256(identity.files), file_count: identity.files.length, total_bytes: totalBytes });
}

async function defaultFetchBytes(url, maximumBytes) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 30_000);
  try {
    const response = await fetch(url, { cache: "no-store", redirect: "error", headers: { "cache-control": "no-store", pragma: "no-cache" }, signal: controller.signal });
    return readCanonicalPublicResponse(response, maximumBytes);
  } finally { clearTimeout(timer); }
}

export async function readCanonicalPublicResponse(response, maximumBytes) {
    if (!Number.isSafeInteger(maximumBytes) || maximumBytes < 0 || !response.ok || response.body == null) fail(`public Pages read failed or has invalid bound: ${response.status}`);
    const contentLength = response.headers.get("content-length");
    if (contentLength !== null && (!/^[0-9]+$/u.test(contentLength) || Number(contentLength) > maximumBytes)) fail("public Pages response exceeds its bound");
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
          await reader.cancel("bounded public Pages response exceeded");
          fail("public Pages response exceeds its bound");
        }
        chunks.push(chunk);
      }
    } finally {
      reader.releaseLock();
    }
    return Buffer.concat(chunks, total);
}

async function main() {
  const mode = process.argv[2];
  if (mode === "verify-public") {
    const result = await verifyCanonicalBuildAndPublic({ buildPath: process.env.CANONICAL_BUILD_PATH, sourceCommit: process.env.SNAPSHOT_SHA, acceptedRunId: process.env.ACCEPTED_RUN_ID, acceptedRunAttempt: "1", basePath: process.env.PAGES_BASE_PATH, pageUrl: process.env.PAGES_URL, cacheBuster: `${process.env.GITHUB_RUN_ID}-${process.env.PAGES_AUTHORITY_PHASE}` });
    const output = Buffer.from(canonicalJson(result)).toString("base64");
    await appendFile(process.env.GITHUB_OUTPUT, `canonical_readback_base64=${output}\n`, "utf8");
    return;
  }
  fail("usage: pages-canonical-capture.mjs verify-public");
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) main().catch((error) => { console.error(`pages_canonical_capture=failed reason=${error.message}`); process.exitCode = 2; });

import { createHash } from "node:crypto";
import { appendFile, readdir, lstat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

import {
  MAX_PAGES_ROLLBACK_TAR_BYTES,
  readBoundedRegularFile,
  readRollbackCaptureReport,
  validateRollbackCaptureReport,
} from "./pages-rollback-authority.mjs";
import { canonicalJson, canonicalSha256 } from "./canonical-release-evidence.mjs";
import {
  LEGACY_PAGES_PAYLOAD,
  legacyReconstructedIdentitySha256,
  validateLegacyDeployedPagesSnapshot,
  validateLegacyReconstructedIdentityBytes,
} from "./pages-legacy-contract.mjs";

const BLOCK_SIZE = 512;
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const SHA_PATTERN = /^[0-9a-f]{40}$/u;
const UTF8 = new TextDecoder("utf-8", { fatal: true });
const REQUIRED_IDENTITY_PATH = "clearra-build-identity.json";
const REQUIRED_MANIFEST_PATH = "wasm/clearra_wasm.manifest.json";

function fail(message) {
  throw new Error(message);
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    fail(`${label} has an invalid format`);
  }
  return value;
}

function isZeroBlock(buffer, offset) {
  for (let index = offset; index < offset + BLOCK_SIZE; index += 1) {
    if (buffer[index] !== 0) {
      return false;
    }
  }
  return true;
}

function decodeField(buffer, start, length, label) {
  const end = buffer.indexOf(0, start);
  const boundedEnd = end === -1 || end >= start + length ? start + length : end;
  try {
    return UTF8.decode(buffer.subarray(start, boundedEnd));
  } catch {
    fail(`${label} is not valid UTF-8`);
  }
}

function parseOctalField(buffer, start, length, label) {
  const raw = buffer.subarray(start, start + length);
  if ((raw[0] & 0x80) !== 0) {
    fail(`${label} must use the portable octal tar encoding`);
  }
  const text = Buffer.from(raw)
    .toString("ascii")
    .replace(/\0.*$/u, "")
    .trim();
  if (text === "") {
    return 0;
  }
  if (!/^[0-7]+$/u.test(text)) {
    fail(`${label} is not a valid octal tar field`);
  }
  const value = Number.parseInt(text, 8);
  if (!Number.isSafeInteger(value) || value < 0) {
    fail(`${label} exceeds the supported tar range`);
  }
  return value;
}

function validateHeaderChecksum(buffer, offset) {
  const stored = parseOctalField(buffer, offset + 148, 8, "tar checksum");
  let calculated = 0;
  for (let index = 0; index < BLOCK_SIZE; index += 1) {
    calculated += index >= 148 && index < 156 ? 0x20 : buffer[offset + index];
  }
  if (stored !== calculated) {
    fail("Pages rollback tar header checksum is invalid");
  }
}

function validateGnuUstarHeader(buffer, offset) {
  const magic = buffer.subarray(offset + 257, offset + 263);
  const version = buffer.subarray(offset + 263, offset + 265);
  if (
    !magic.equals(Buffer.from("ustar ", "ascii")) ||
    !version.equals(Buffer.from(" \0", "ascii"))
  ) {
    fail("Pages rollback tar must use the exact GNU ustar header emitted by the Pages workflow");
  }
}

function validateMemberPrefixCollisions(entries, path, type) {
  for (const [existingPath, existing] of entries) {
    if (
      (existing.type === "0" && path.startsWith(`${existingPath}/`)) ||
      (type === "0" && existingPath.startsWith(`${path}/`))
    ) {
      fail("Pages rollback tar contains a file and descendant path collision");
    }
  }
}

function normalizeMemberPath(rawPath, type) {
  if (
    rawPath.length === 0 ||
    rawPath.includes("\\") ||
    rawPath.startsWith("/") ||
    /^[A-Za-z]:/u.test(rawPath) ||
    /[\u0000-\u001f\u007f]/u.test(rawPath)
  ) {
    fail("Pages rollback tar contains an unsafe member path");
  }

  let normalized = rawPath;
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  normalized = normalized.replace(/\/+$/u, "");
  if (normalized === "" || normalized === ".") {
    if (type !== "5") {
      fail("Pages rollback tar root entry must be a directory");
    }
    return ".";
  }

  const segments = normalized.split("/");
  if (segments.some((segment) => segment === "" || segment === "." || segment === "..")) {
    fail("Pages rollback tar contains an unsafe member path");
  }
  return normalized;
}

export function parseRollbackTar(bufferValue) {
  const buffer = Buffer.isBuffer(bufferValue) ? bufferValue : Buffer.from(bufferValue);
  if (buffer.length < BLOCK_SIZE * 2 || buffer.length % BLOCK_SIZE !== 0) {
    fail("Pages rollback tar must contain complete 512-byte blocks");
  }

  const entries = new Map();
  let offset = 0;
  let foundEnd = false;
  while (offset + BLOCK_SIZE <= buffer.length) {
    if (isZeroBlock(buffer, offset)) {
      if (offset + BLOCK_SIZE * 2 > buffer.length || !isZeroBlock(buffer, offset + BLOCK_SIZE)) {
        fail("Pages rollback tar must end with two zero blocks");
      }
      for (let index = offset; index < buffer.length; index += 1) {
        if (buffer[index] !== 0) {
          fail("Pages rollback tar contains data after its end marker");
        }
      }
      foundEnd = true;
      break;
    }

    validateHeaderChecksum(buffer, offset);
    validateGnuUstarHeader(buffer, offset);
    const name = decodeField(buffer, offset, 100, "tar member name");
    const rawPath = name;
    const typeByte = buffer[offset + 156];
    const type = typeByte === 0 ? "0" : String.fromCharCode(typeByte);
    if (type !== "0" && type !== "5") {
      fail("Pages rollback tar contains a link or special entry");
    }
    const path = normalizeMemberPath(rawPath, type);
    if (entries.has(path)) {
      fail("Pages rollback tar contains a duplicate member path");
    }
    validateMemberPrefixCollisions(entries, path, type);

    const size = parseOctalField(buffer, offset + 124, 12, "tar member size");
    if (type === "5" && size !== 0) {
      fail("Pages rollback tar directory entry must be empty");
    }
    const dataStart = offset + BLOCK_SIZE;
    const dataEnd = dataStart + size;
    if (dataEnd > buffer.length) {
      fail("Pages rollback tar member exceeds the archive boundary");
    }
    const nextOffset = dataStart + Math.ceil(size / BLOCK_SIZE) * BLOCK_SIZE;
    for (let index = dataEnd; index < nextOffset; index += 1) {
      if (buffer[index] !== 0) {
        fail("Pages rollback tar member padding must be zero-filled");
      }
    }
    entries.set(path, {
      type,
      content: type === "0" ? buffer.subarray(dataStart, dataEnd) : Buffer.alloc(0),
    });
    offset = nextOffset;
  }

  if (!foundEnd) {
    fail("Pages rollback tar has no valid end marker");
  }
  return entries;
}

function parseRequiredJson(entries, path, label) {
  const entry = entries.get(path);
  if (entry?.type !== "0") {
    fail(`${label} is missing from the Pages rollback tar`);
  }
  try {
    return JSON.parse(UTF8.decode(entry.content));
  } catch {
    fail(`${label} is not valid UTF-8 JSON`);
  }
}

function requireFileEntry(entries, path, label) {
  const entry = entries.get(path);
  if (entry?.type !== "0") {
    fail(`${label} is missing from the Pages rollback tar`);
  }
  return entry.content;
}

export function validateRollbackPackageBuffer(buffer, {
  expectedSha,
  expectedTarSha256,
  captureReport,
}) {
  if (!(buffer instanceof Uint8Array) || buffer.byteLength > MAX_PAGES_ROLLBACK_TAR_BYTES) {
    fail("Pages rollback tar exceeds the product byte limit");
  }
  const sha = requirePattern(expectedSha, SHA_PATTERN, "snapshot SHA");
  const expectedDigest = requirePattern(
    expectedTarSha256,
    SHA256_PATTERN,
    "captured Pages tar SHA-256",
  );
  const actualDigest = createHash("sha256").update(buffer).digest("hex");
  if (actualDigest !== expectedDigest) {
    fail("Downloaded Pages artifact.tar differs from the captured SHA-256");
  }

  validateRollbackCaptureReport(captureReport, { expectedSnapshotSha: sha });
  if (buffer.byteLength !== captureReport.artifact_tar_size_bytes) {
    fail("Pages rollback tar size differs from the sealed capture report");
  }
  if (captureReport.artifact_tar_sha256 !== expectedDigest) {
    fail("Pages rollback tar SHA-256 differs from the sealed capture report");
  }

  const entries = parseRollbackTar(buffer);
  if (captureReport.capture_kind === "legacy-v0.7.4") {
    const identityBytes = requireFileEntry(entries, REQUIRED_IDENTITY_PATH, "legacy Pages identity");
    const identity = validateLegacyReconstructedIdentityBytes(identityBytes, {
      expectedSnapshotSha: sha,
      expectedAuthoritySha: captureReport.authority_source_commit,
      expectedCaptureRunId: captureReport.capture_run_id,
      expectedCaptureRunAttempt: captureReport.capture_run_attempt,
    });
    const expectedIdentity = captureReport.legacy_snapshot.identity;
    if (
      captureReport.legacy_snapshot.legacy_identity_sha256 !==
      legacyReconstructedIdentitySha256(identity)
    ) {
      fail("legacy Pages tar identity SHA-256 differs from the sealed capture report");
    }
    validateLegacyDeployedPagesSnapshot({
      identity,
      expectedIdentity,
      manifestBytes: requireFileEntry(
        entries,
        LEGACY_PAGES_PAYLOAD.manifest.path,
        "legacy Pages WASM manifest",
      ),
      bindingsBytes: requireFileEntry(
        entries,
        LEGACY_PAGES_PAYLOAD.bindings.path,
        "legacy Pages WASM bindings",
      ),
      wasmBytes: requireFileEntry(
        entries,
        LEGACY_PAGES_PAYLOAD.wasm.path,
        "legacy Pages WASM binary",
      ),
    });
    return { actualDigest, entries, identity, captureKind: captureReport.capture_kind };
  }
  if (captureReport.capture_kind !== "canonical-v2") {
    fail("Pages rollback capture report kind is unsupported");
  }
  const identity = parseRequiredJson(entries, REQUIRED_IDENTITY_PATH, "Pages identity");
  const canonical = captureReport.canonical_snapshot;
  if (canonicalJson(identity) !== canonicalJson(canonical.identity)) {
    fail("Pages tar identity differs from the full sealed accepted identity");
  }
  const expectedFiles = new Map(identity.files.map((file) => [file.path, file]));
  const regularPaths = [...entries]
    .filter(([, entry]) => entry.type === "0")
    .map(([path]) => path)
    .sort();
  const expectedPaths = [REQUIRED_IDENTITY_PATH, ...identity.files.map((file) => file.path)].sort();
  if (canonicalJson(regularPaths) !== canonicalJson(expectedPaths)) {
    fail("Pages tar regular file set differs from the accepted identity");
  }
  for (const [path, descriptor] of expectedFiles) {
    const content = requireFileEntry(entries, path, `accepted Pages file ${path}`);
    if (content.byteLength !== descriptor.size || createHash("sha256").update(content).digest("hex") !== descriptor.sha256) {
      fail(`Pages tar file differs from accepted identity: ${path}`);
    }
  }
  return {
    actualDigest,
    entries,
    identity,
    identitySha256: canonicalSha256(identity),
    fileSetSha256: canonicalSha256(identity.files),
    captureKind: captureReport.capture_kind,
  };
}

export async function validateRollbackPackageDirectory(
  directory,
  { expectedSha, expectedTarSha256, captureReport },
) {
  const resolvedDirectory = resolve(directory);
  const names = (await readdir(resolvedDirectory)).sort();
  if (names.length !== 1 || names[0] !== "artifact.tar") {
    fail("Pages rollback download must contain exactly one artifact.tar");
  }
  const tarPath = resolve(resolvedDirectory, "artifact.tar");
  const stat = await lstat(tarPath);
  if (!stat.isFile() || stat.isSymbolicLink()) {
    fail("Pages rollback artifact.tar must be a regular file");
  }
  if (stat.size !== captureReport.artifact_tar_size_bytes) {
    fail("Pages rollback artifact.tar size differs from the sealed capture report");
  }
  return validateRollbackPackageBuffer(await readBoundedRegularFile(
    tarPath,
    captureReport.artifact_tar_size_bytes,
    "Pages rollback artifact.tar",
  ), {
    expectedSha,
    expectedTarSha256,
    captureReport,
  });
}

export function validateRollbackPackageCaptureKind(captureReport, expectedCaptureKind) {
  validateRollbackCaptureReport(captureReport);
  if (!new Set(["legacy-v0.7.4", "canonical-v2"]).has(expectedCaptureKind)) {
    fail("Pages rollback expected capture kind is invalid");
  }
  if (captureReport.capture_kind !== expectedCaptureKind) {
    fail("Pages rollback package capture kind differs from the required mutation kind");
  }
  return expectedCaptureKind;
}

async function main() {
  const directory = process.env.PAGES_ROLLBACK_PACKAGE_DIR;
  const expectedSha = process.env.SNAPSHOT_SHA;
  const expectedTarSha256 = process.env.SNAPSHOT_TAR_SHA256;
  const captureReportPath = process.env.PAGES_ROLLBACK_CAPTURE_REPORT_PATH;
  const expectedCaptureKind = process.env.PAGES_ROLLBACK_EXPECTED_CAPTURE_KIND;
  if (typeof directory !== "string" || directory.length === 0) {
    fail("PAGES_ROLLBACK_PACKAGE_DIR is required");
  }
  if (typeof captureReportPath !== "string" || captureReportPath.length === 0) {
    fail("PAGES_ROLLBACK_CAPTURE_REPORT_PATH is required");
  }
  const { report } = await readRollbackCaptureReport(captureReportPath);
  if (typeof expectedCaptureKind === "string" && expectedCaptureKind.length > 0) {
    validateRollbackPackageCaptureKind(report, expectedCaptureKind);
  }
  const validated = await validateRollbackPackageDirectory(directory, {
    expectedSha,
    expectedTarSha256,
    captureReport: report,
  });
  const githubOutput = process.env.GITHUB_OUTPUT;
  if (typeof githubOutput === "string" && githubOutput.length > 0) {
    await appendFile(githubOutput, [
      `capture_kind=${validated.captureKind}`,
      `canonical_identity_sha256=${validated.captureKind === "canonical-v2" ? validated.identitySha256 : ""}`,
      `canonical_file_set_sha256=${validated.captureKind === "canonical-v2" ? validated.fileSetSha256 : ""}`,
      "",
    ].join("\n"), "utf8");
  }
  console.log("pages_rollback_package=passed");
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    console.error(`pages_rollback_package=failed reason=${error.message}`);
    process.exitCode = 2;
  });
}

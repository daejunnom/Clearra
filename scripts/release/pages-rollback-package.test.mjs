import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { canonicalSha256, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import { expectedCaptureArtifactName } from "./pages-rollback-authority.mjs";
import {
  LEGACY_PAGES_PAYLOADS,
  LEGACY_PAGES_READBACK_SCHEMA,
  LEGACY_PAGES_RELEASE_TAG,
  LEGACY_PAGES_SNAPSHOT_SHA,
  LEGACY_PAGES_TAG_OBJECT_SHA,
  createLegacyReconstructedIdentity,
  legacyReconstructedIdentitySha256,
} from "./pages-legacy-contract.mjs";

import {
  parseRollbackTar,
  validateRollbackPackageCaptureKind,
  validateRollbackPackageBuffer,
} from "./pages-rollback-package.mjs";

const SHA = "1".repeat(40);
const AUTHORITY = "2".repeat(40);

function captureReport(tarSha256, tarSize) {
  return sealCanonicalReport({
    schema_id: "clearra.pages.rollback-capture-authority.v2",
    repository: "daejunnom/Clearra",
    snapshot_source_commit: SHA,
    authority_source_commit: AUTHORITY,
    capture_run_id: "12345",
    capture_run_attempt: "1",
    workflow_path: ".github/workflows/pages-rollback.yml",
    workflow_run_api_readback_sha256: "4".repeat(64),
    artifact_id: "67890",
    artifact_name: expectedCaptureArtifactName({
      snapshotSha: SHA,
      authoritySha: AUTHORITY,
      captureRunId: "12345",
      captureRunAttempt: "1",
    }),
    artifact_digest: `sha256:${"3".repeat(64)}`,
    artifact_sha256: "3".repeat(64),
    artifact_archive_size_bytes: 6_000_000,
    artifact_tar_sha256: tarSha256,
    artifact_tar_size_bytes: tarSize,
    artifact_api_readback_sha256: "5".repeat(64),
    artifact_created_at: "2026-08-28T00:00:00.000Z",
    artifact_expires_at: "2026-11-26T00:00:00.000Z",
    retention_seconds: 90 * 24 * 60 * 60,
    capture_kind: "modern-v2",
    legacy_snapshot: null,
    status: "captured",
  });
}

function legacyReadback(phase) {
  return sealCanonicalReport({
    schema_id: LEGACY_PAGES_READBACK_SCHEMA,
    phase,
    repository: "daejunnom/Clearra",
    page_url: "https://daejunnom.github.io/Clearra/",
    release_tag: LEGACY_PAGES_RELEASE_TAG,
    tag_object_sha: LEGACY_PAGES_TAG_OBJECT_SHA,
    snapshot_source_commit: LEGACY_PAGES_SNAPSHOT_SHA,
    deployment_id: "6181925865",
    identity_status: 404,
    payloads: LEGACY_PAGES_PAYLOADS.map((payload) => ({ ...payload })),
    payload_set_sha256: canonicalSha256(LEGACY_PAGES_PAYLOADS),
    tag_ref_api_readback_sha256: "1".repeat(64),
    annotated_tag_api_readback_sha256: "2".repeat(64),
    release_api_readback_sha256: "3".repeat(64),
    deployment_api_readback_sha256: "4".repeat(64),
    deployment_statuses_api_readback_sha256: "5".repeat(64),
    status: "verified",
  });
}

function legacyCaptureReport(tarSha256, tarSize) {
  const identityValue = createLegacyReconstructedIdentity({
    snapshotSha: LEGACY_PAGES_SNAPSHOT_SHA,
    authoritySha: AUTHORITY,
    captureRunId: "12345",
    captureRunAttempt: "1",
  });
  return sealCanonicalReport({
    schema_id: "clearra.pages.rollback-capture-authority.v2",
    repository: "daejunnom/Clearra",
    snapshot_source_commit: LEGACY_PAGES_SNAPSHOT_SHA,
    authority_source_commit: AUTHORITY,
    capture_run_id: "12345",
    capture_run_attempt: "1",
    workflow_path: ".github/workflows/pages-rollback.yml",
    workflow_run_api_readback_sha256: "6".repeat(64),
    artifact_id: "67890",
    artifact_name: expectedCaptureArtifactName({
      snapshotSha: LEGACY_PAGES_SNAPSHOT_SHA,
      authoritySha: AUTHORITY,
      captureRunId: "12345",
      captureRunAttempt: "1",
    }),
    artifact_digest: `sha256:${"7".repeat(64)}`,
    artifact_sha256: "7".repeat(64),
    artifact_archive_size_bytes: 6_000_000,
    artifact_tar_sha256: tarSha256,
    artifact_tar_size_bytes: tarSize,
    artifact_api_readback_sha256: "8".repeat(64),
    artifact_created_at: "2026-08-28T00:00:00.000Z",
    artifact_expires_at: "2026-11-26T00:00:00.000Z",
    retention_seconds: 90 * 24 * 60 * 60,
    capture_kind: "legacy-v0.7.4",
    legacy_snapshot: {
      identity: identityValue,
      legacy_identity_sha256: legacyReconstructedIdentitySha256(identityValue),
      initial_public_readback: legacyReadback("initial"),
      preartifact_public_readback: legacyReadback("preartifact"),
      rebuilt_payloads: LEGACY_PAGES_PAYLOADS.map((payload) => ({ ...payload })),
      rebuilt_payload_set_sha256: canonicalSha256(LEGACY_PAGES_PAYLOADS),
    },
    status: "captured",
  });
}

test("restore package admits only a sealed legacy capture kind before deployment", () => {
  const modern = captureReport("1".repeat(64), 1024);
  const legacy = legacyCaptureReport("2".repeat(64), 1024);
  assert.equal(
    validateRollbackPackageCaptureKind(legacy, "legacy-v0.7.4"),
    "legacy-v0.7.4",
  );
  assert.throws(
    () => validateRollbackPackageCaptureKind(modern, "legacy-v0.7.4"),
    /differs from the required mutation kind/u,
  );
  assert.throws(
    () => validateRollbackPackageCaptureKind(legacy, "unknown"),
    /expected capture kind is invalid/u,
  );
});

function identity() {
  return {
    schema: "clearra.pages.identity.v2",
    sourceCommit: SHA,
    engineBuildId: SHA,
    contractSchemaVersion: "clearra.search.contract.v2",
    supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
    artifactSchemaVersion: "clearra.solution-data.v1",
    version: "0.7.5",
  };
}

function manifest() {
  return {
    build: {
      runtime_identity: {
        source_commit: SHA,
        engine_build_id: SHA,
        contract_schema_version: "clearra.search.contract.v2",
        supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
        artifact_schema_version: "clearra.solution-data.v1",
      },
    },
  };
}

function octal(value, length) {
  const text = value.toString(8).padStart(length - 1, "0");
  return Buffer.from(`${text}\0`, "ascii");
}

function tarHeader(path, size, type = "0") {
  const header = Buffer.alloc(512);
  const name = Buffer.from(path, "utf8");
  if (name.length > 100) {
    throw new Error("test path is too long");
  }
  name.copy(header, 0);
  octal(type === "5" ? 0o755 : 0o644, 8).copy(header, 100);
  octal(0, 8).copy(header, 108);
  octal(0, 8).copy(header, 116);
  octal(size, 12).copy(header, 124);
  octal(0, 12).copy(header, 136);
  header.fill(0x20, 148, 156);
  header[156] = type.charCodeAt(0);
  Buffer.from("ustar ", "ascii").copy(header, 257);
  Buffer.from(" \0", "ascii").copy(header, 263);
  let checksum = 0;
  for (const byte of header) {
    checksum += byte;
  }
  const checksumText = checksum.toString(8).padStart(6, "0");
  Buffer.from(`${checksumText}\0 `, "ascii").copy(header, 148);
  return header;
}

function rewriteHeaderChecksum(buffer, offset = 0) {
  buffer.fill(0x20, offset + 148, offset + 156);
  let checksum = 0;
  for (let index = offset; index < offset + 512; index += 1) checksum += buffer[index];
  const checksumText = checksum.toString(8).padStart(6, "0");
  Buffer.from(`${checksumText}\0 `, "ascii").copy(buffer, offset + 148);
}

function makeTar(entries) {
  const chunks = [];
  for (const entry of entries) {
    const content = entry.type === "5"
      ? Buffer.alloc(0)
      : Buffer.from(entry.content ?? "", "utf8");
    chunks.push(tarHeader(entry.path, content.length, entry.type ?? "0"));
    chunks.push(content);
    const padding = (512 - (content.length % 512)) % 512;
    if (padding > 0) {
      chunks.push(Buffer.alloc(padding));
    }
  }
  chunks.push(Buffer.alloc(1024));
  return Buffer.concat(chunks);
}

function validTar() {
  return makeTar([
    { path: "./", type: "5" },
    { path: "./wasm/", type: "5" },
    { path: "./clearra-build-identity.json", content: JSON.stringify(identity()) },
    { path: "./wasm/clearra_wasm.manifest.json", content: JSON.stringify(manifest()) },
  ]);
}

test("validates the exact tar hash and both complete identity documents", () => {
  const tar = validTar();
  const expectedTarSha256 = createHash("sha256").update(tar).digest("hex");
  const result = validateRollbackPackageBuffer(tar, {
    expectedSha: SHA,
    expectedTarSha256,
    captureReport: captureReport(expectedTarSha256, tar.byteLength),
  });
  assert.equal(result.actualDigest, expectedTarSha256);
  assert.equal(result.entries.get("clearra-build-identity.json").type, "0");
});

test("rejects an incorrect tar authority before reading identity", () => {
  const tar = validTar();
  assert.throws(
    () => validateRollbackPackageBuffer(tar, {
      expectedSha: SHA,
      expectedTarSha256: "2".repeat(64),
      captureReport: captureReport("2".repeat(64), tar.byteLength),
    }),
    /differs from the captured SHA-256/u,
  );
});

test("rejects traversal, links, duplicate identities, and forged identity", () => {
  const unsafeArchives = [
    makeTar([{ path: "../escape", content: "bad" }]),
    makeTar([{ path: "./linked", type: "2", content: "" }]),
    makeTar([
      { path: "./clearra-build-identity.json", content: JSON.stringify(identity()) },
      { path: "./clearra-build-identity.json", content: JSON.stringify(identity()) },
    ]),
    makeTar([
      {
        path: "./clearra-build-identity.json",
        content: JSON.stringify({ ...identity(), sourceCommit: "2".repeat(40) }),
      },
      { path: "./wasm/clearra_wasm.manifest.json", content: JSON.stringify(manifest()) },
    ]),
  ];
  for (const tar of unsafeArchives) {
    if (tar === unsafeArchives[0] || tar === unsafeArchives[1] || tar === unsafeArchives[2]) {
      assert.throws(() => parseRollbackTar(tar));
      continue;
    }
    assert.throws(() => validateRollbackPackageBuffer(tar, {
      expectedSha: SHA,
      expectedTarSha256: createHash("sha256").update(tar).digest("hex"),
      captureReport: captureReport(
        createHash("sha256").update(tar).digest("hex"),
        tar.byteLength,
      ),
    }));
  }
});

test("rejects corrupted headers and data after the tar end marker", () => {
  const corrupted = Buffer.from(validTar());
  corrupted[0] ^= 1;
  assert.throws(() => parseRollbackTar(corrupted), /checksum/u);

  const trailing = Buffer.concat([validTar(), Buffer.alloc(512)]);
  trailing[trailing.length - 1] = 1;
  assert.throws(() => parseRollbackTar(trailing), /after its end marker/u);
});

test("rejects non-portable headers, nonzero padding, and file descendant collisions", () => {
  const arbitraryMagic = makeTar([{ path: "./file", content: "x" }]);
  arbitraryMagic[257] = "x".charCodeAt(0);
  rewriteHeaderChecksum(arbitraryMagic);
  assert.throws(() => parseRollbackTar(arbitraryMagic), /GNU ustar/u);

  const nonzeroPadding = makeTar([{ path: "./file", content: "x" }]);
  nonzeroPadding[513] = 1;
  assert.throws(() => parseRollbackTar(nonzeroPadding), /padding/u);

  const collision = makeTar([
    { path: "./file", content: "x" },
    { path: "./file/descendant", content: "y" },
  ]);
  assert.throws(() => parseRollbackTar(collision), /descendant path collision/u);
});

test("sealed legacy mode rejects a fabricated v2 identity before payload downgrade", () => {
  const tar = makeTar([
    { path: "./", type: "5" },
    { path: "./wasm/", type: "5" },
    {
      path: "./clearra-build-identity.json",
      content: JSON.stringify({ ...identity(), sourceCommit: LEGACY_PAGES_SNAPSHOT_SHA }),
    },
    { path: "./wasm/clearra_wasm.manifest.json", content: JSON.stringify(manifest()) },
  ]);
  const tarSha256 = createHash("sha256").update(tar).digest("hex");
  assert.throws(
    () => validateRollbackPackageBuffer(tar, {
      expectedSha: LEGACY_PAGES_SNAPSHOT_SHA,
      expectedTarSha256: tarSha256,
      captureReport: legacyCaptureReport(tarSha256, tar.byteLength),
    }),
    /closed schema|legacy Pages identity schema/u,
  );
});

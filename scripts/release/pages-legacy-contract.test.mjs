import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { canonicalSha256, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import {
  expectedCaptureArtifactName,
  validateRollbackCaptureReport,
} from "./pages-rollback-authority.mjs";
import {
  LEGACY_PAGES_BASE_PATH,
  LEGACY_PAGES_IDENTITY_SCHEMA,
  LEGACY_PAGES_ORIGINAL_IDENTITY_STATUS,
  LEGACY_PAGES_PAYLOAD,
  LEGACY_PAGES_PAYLOADS,
  LEGACY_PAGES_READBACK_SCHEMA,
  LEGACY_PAGES_RELEASE_TAG,
  LEGACY_PAGES_SNAPSHOT_SHA,
  LEGACY_PAGES_TAG_OBJECT_SHA,
  LEGACY_PAGES_VERSION,
  LEGACY_PAGES_WORKFLOW_PATH,
  createLegacyReconstructedIdentity,
  decodeLegacyPublicReadbackEvidence,
  encodeLegacyPublicReadbackEvidence,
  legacyReconstructedIdentitySha256,
  serializeLegacyReconstructedIdentity,
  validateLegacyPagesPublicSnapshot,
  validateLegacyPagesWasmManifest,
  validateLegacyPublicReadbackEvidence,
  validateLegacyReconstructedIdentity,
  validateLegacyReconstructedIdentityBytes,
} from "./pages-legacy-contract.mjs";

const AUTHORITY = "2".repeat(40);

function exactManifestBytes() {
  const manifest = {
    schema_version: 1,
    build: {
      contract_version: 1,
      source_sha256:
        "b21b3ad64148d69d86136a7b5ea6795910c5fcc7c1d42cc8675252944359ffaf",
      source_file_count: 2280,
      capabilities_sha256:
        "6e6e2c1e973f62c6d6fa28f571b326104aec625e6879c4aca67df3364029d98b",
    },
    bindings: {
      path: LEGACY_PAGES_PAYLOAD.bindings.path.slice("wasm/".length),
      bytes: LEGACY_PAGES_PAYLOAD.bindings.bytes,
      sha256: LEGACY_PAGES_PAYLOAD.bindings.sha256,
    },
    wasm: {
      path: LEGACY_PAGES_PAYLOAD.wasm.path.slice("wasm/".length),
      bytes: LEGACY_PAGES_PAYLOAD.wasm.bytes,
      sha256: LEGACY_PAGES_PAYLOAD.wasm.sha256,
    },
  };
  const json = JSON.stringify(manifest);
  return Buffer.from(`${json}${" ".repeat(768 - Buffer.byteLength(json) - 1)}\n`, "utf8");
}

function readbackEvidence(phase) {
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

test("approved legacy constants match the real annotated v0.7.4 git object", () => {
  assert.equal(LEGACY_PAGES_RELEASE_TAG, "v0.7.4");
  assert.equal(LEGACY_PAGES_VERSION, "0.7.4");
  assert.equal(LEGACY_PAGES_BASE_PATH, "/Clearra");
  assert.equal(LEGACY_PAGES_WORKFLOW_PATH, ".github/workflows/pages-rollback.yml");
  assert.equal(LEGACY_PAGES_ORIGINAL_IDENTITY_STATUS, 404);
  const tag = spawnSync("git", ["cat-file", "-p", LEGACY_PAGES_TAG_OBJECT_SHA], {
    encoding: "utf8",
    shell: false,
  });
  assert.equal(tag.status, 0, tag.stderr);
  assert.match(tag.stdout, new RegExp(`^object ${LEGACY_PAGES_SNAPSHOT_SHA}$`, "mu"));
  assert.match(tag.stdout, /^type commit$/mu);
  assert.match(tag.stdout, /^tag v0\.7\.4$/mu);
});

test("approved manifest bytes preserve the 768-byte v1 schema without v2 semantics", () => {
  const bytes = exactManifestBytes();
  assert.equal(bytes.byteLength, LEGACY_PAGES_PAYLOAD.manifest.bytes);
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    LEGACY_PAGES_PAYLOAD.manifest.sha256,
  );
  const manifest = validateLegacyPagesWasmManifest(undefined, bytes, "v0.7.4 manifest");
  assert.equal(manifest.schema_version, 1);
  assert.equal(Object.hasOwn(manifest.build, "runtime_identity"), false);

  const fabricated = structuredClone(manifest);
  fabricated.build.runtime_identity = {
    source_commit: LEGACY_PAGES_SNAPSHOT_SHA,
  };
  assert.throws(
    () => validateLegacyPagesWasmManifest(fabricated, bytes, "fabricated manifest"),
    /modern runtime_identity semantics/u,
  );
});

test("legacy identity is separate, canonical, exact-keyed, and capture-bound", () => {
  const identity = createLegacyReconstructedIdentity({
    snapshotSha: LEGACY_PAGES_SNAPSHOT_SHA,
    authoritySha: AUTHORITY,
    captureRunId: "12345",
    captureRunAttempt: "1",
  });
  assert.equal(identity.schema, LEGACY_PAGES_IDENTITY_SCHEMA);
  assert.equal(identity.version, "0.7.4");
  assert.equal(identity.sourceCommit, LEGACY_PAGES_SNAPSHOT_SHA);
  assert.deepEqual(identity.payloads, LEGACY_PAGES_PAYLOADS);
  const bytes = Buffer.from(serializeLegacyReconstructedIdentity(identity), "utf8");
  assert.deepEqual(validateLegacyReconstructedIdentityBytes(bytes, {
    expectedAuthoritySha: AUTHORITY,
    expectedCaptureRunId: "12345",
    expectedCaptureRunAttempt: "1",
  }), identity);
  assert.equal(
    legacyReconstructedIdentitySha256(identity),
    createHash("sha256").update(bytes).digest("hex"),
  );
  const reordered = Object.fromEntries(Object.entries(JSON.parse(bytes)).reverse());
  assert.throws(
    () => validateLegacyReconstructedIdentityBytes(
      Buffer.from(`${JSON.stringify(reordered, null, 2)}\n`, "utf8"),
    ),
    /canonical producer bytes/u,
  );

  const fabricatedV2 = {
    schema: "clearra.pages.identity.v2",
    sourceCommit: LEGACY_PAGES_SNAPSHOT_SHA,
  };
  assert.throws(
    () => validateLegacyReconstructedIdentity(fabricatedV2),
    /closed schema|legacy Pages identity schema/u,
  );
  assert.throws(
    () => validateLegacyReconstructedIdentity({ ...identity, bypass: true }),
    /closed schema/u,
  );
});

test("public readback evidence is phase-bound, canonical, and tamper-evident", () => {
  const initial = readbackEvidence("initial");
  assert.equal(validateLegacyPublicReadbackEvidence(initial), initial);
  const encoded = encodeLegacyPublicReadbackEvidence(initial);
  assert.deepEqual(
    decodeLegacyPublicReadbackEvidence(encoded, { expectedPhase: "initial" }),
    initial,
  );
  assert.throws(
    () => decodeLegacyPublicReadbackEvidence(encoded, { expectedPhase: "preartifact" }),
    /phase differs/u,
  );
  const tampered = structuredClone(initial);
  tampered.payloads[0].bytes += 1;
  assert.throws(
    () => validateLegacyPublicReadbackEvidence(tampered),
    /canonical content/u,
  );
});

test("public identity or any public byte drift fails closed", () => {
  const manifestBytes = exactManifestBytes();
  assert.throws(
    () => validateLegacyPagesPublicSnapshot({
      identityStatus: 200,
      manifestBytes,
      bindingsBytes: Buffer.alloc(1),
      wasmBytes: Buffer.alloc(1),
    }),
    /must not expose/u,
  );
  assert.throws(
    () => validateLegacyPagesPublicSnapshot({
      identityStatus: 404,
      manifestBytes,
      bindingsBytes: Buffer.alloc(LEGACY_PAGES_PAYLOAD.bindings.bytes),
      wasmBytes: Buffer.alloc(LEGACY_PAGES_PAYLOAD.wasm.bytes),
    }),
    /bindings differs/u,
  );
  const driftedManifest = Buffer.from(manifestBytes);
  driftedManifest[0] ^= 1;
  assert.throws(
    () => validateLegacyPagesWasmManifest(undefined, driftedManifest, "drifted manifest"),
    /approved v0\.7\.4 bytes/u,
  );
});

test("bootstrap path never assumes the absent v0.7.4 modern release helper", async () => {
  const missing = spawnSync("git", [
    "cat-file",
    "-e",
    `${LEGACY_PAGES_SNAPSHOT_SHA}:scripts/release/validate-release-metadata.mjs`,
  ], { encoding: "utf8", shell: false });
  assert.notEqual(missing.status, 0, "v0.7.4 fixture must continue proving the helper is absent");

  const workflow = await readFile(
    new URL("../../.github/workflows/pages-rollback.yml", import.meta.url),
    "utf8",
  );
  const legacyStamp = workflow.slice(
    workflow.indexOf("- name: Stamp separate reconstructed v0.7.4 identity"),
    workflow.indexOf("- name: Validate portable rollback tree"),
  );
  assert.match(legacyStamp, /node authority-source\/scripts\/release\/pages-legacy-contract\.mjs/u);
  assert.doesNotMatch(legacyStamp, /snapshot-source\/scripts\/release/u);

  assert.match(
    workflow,
    /node-version: \$\{\{ inputs\.mode == 'bootstrap-capture' && '22\.23\.2' \|\| '22' \}\}/u,
  );
  assert.match(
    workflow,
    /RUSTUP_TOOLCHAIN: \$\{\{ inputs\.mode == 'bootstrap-capture' && '1\.98\.0' \|\| 'stable' \}\}/u,
  );
  const rustSetup = workflow.slice(
    workflow.indexOf("- name: Prepare Rust WASM toolchain"),
    workflow.indexOf("- name: Install GUI dependencies"),
  );
  const bootstrapGuard = rustSetup.indexOf(
    'if [[ "$CAPTURE_MODE" == "bootstrap-capture" ]]; then',
  );
  const homeGuard = rustSetup.indexOf('[[ "$HOME" == "/home/runner" ]]');
  const cargoHomeGuard = rustSetup.indexOf(
    '[[ "${CARGO_HOME:-$HOME/.cargo}" == "/home/runner/.cargo" ]]',
  );
  const toolchainInstall = rustSetup.indexOf(
    'rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal',
  );
  assert.ok(bootstrapGuard >= 0 && bootstrapGuard < homeGuard);
  assert.ok(homeGuard < cargoHomeGuard && cargoHomeGuard < toolchainInstall);
  assert.doesNotMatch(rustSetup, /^\s+(?:HOME|CARGO_HOME):/mu);
});

test("sealed v0.7.4 fixture is bound end-to-end across capture package and deployment", async () => {
  const captureRunId = "12345";
  const captureRunAttempt = "1";
  const identity = createLegacyReconstructedIdentity({
    snapshotSha: LEGACY_PAGES_SNAPSHOT_SHA,
    authoritySha: AUTHORITY,
    captureRunId,
    captureRunAttempt,
  });
  const capture = sealCanonicalReport({
    schema_id: "clearra.pages.rollback-capture-authority.v2",
    repository: "daejunnom/Clearra",
    snapshot_source_commit: LEGACY_PAGES_SNAPSHOT_SHA,
    authority_source_commit: AUTHORITY,
    capture_run_id: captureRunId,
    capture_run_attempt: captureRunAttempt,
    workflow_path: LEGACY_PAGES_WORKFLOW_PATH,
    workflow_run_api_readback_sha256: "6".repeat(64),
    artifact_id: "67890",
    artifact_name: expectedCaptureArtifactName({
      snapshotSha: LEGACY_PAGES_SNAPSHOT_SHA,
      authoritySha: AUTHORITY,
      captureRunId,
      captureRunAttempt,
    }),
    artifact_digest: `sha256:${"7".repeat(64)}`,
    artifact_sha256: "7".repeat(64),
    artifact_archive_size_bytes: 6_000_000,
    artifact_tar_sha256: "8".repeat(64),
    artifact_tar_size_bytes: 8_000_000,
    artifact_api_readback_sha256: "9".repeat(64),
    artifact_created_at: "2026-08-28T00:00:00.000Z",
    artifact_expires_at: "2026-11-26T00:00:00.000Z",
    retention_seconds: 90 * 24 * 60 * 60,
    capture_kind: "legacy-v0.7.4",
    legacy_snapshot: {
      identity,
      legacy_identity_sha256: legacyReconstructedIdentitySha256(identity),
      initial_public_readback: readbackEvidence("initial"),
      preartifact_public_readback: readbackEvidence("preartifact"),
      rebuilt_payloads: LEGACY_PAGES_PAYLOADS.map((payload) => ({ ...payload })),
      rebuilt_payload_set_sha256: canonicalSha256(LEGACY_PAGES_PAYLOADS),
    },
    status: "captured",
  });
  assert.equal(
    validateRollbackCaptureReport(capture, {
      expectedSnapshotSha: LEGACY_PAGES_SNAPSHOT_SHA,
      expectedAuthoritySha: AUTHORITY,
    }),
    capture,
  );

  const [workflow, packageSource, deploymentSource] = await Promise.all([
    readFile(new URL("../../.github/workflows/pages-rollback.yml", import.meta.url), "utf8"),
    readFile(new URL("./pages-rollback-package.mjs", import.meta.url), "utf8"),
    readFile(new URL("./pages-deployment-authority.mjs", import.meta.url), "utf8"),
  ]);
  const captureSeal = workflow.indexOf("Seal exact rollback capture authority");
  const capturePackage = workflow.indexOf(
    "Verify downloaded rollback package against sealed capture report",
  );
  const captureReportUpload = workflow.indexOf("Upload sealed rollback capture authority");
  const restoreDeploy = workflow.indexOf("actions/deploy-pages@v4");
  const restoreSeal = workflow.indexOf(
    "Seal restored Pages authority from API and public readback",
  );
  assert.ok(captureSeal >= 0 && captureSeal < capturePackage);
  assert.ok(capturePackage < captureReportUpload);
  assert.ok(restoreDeploy > captureReportUpload && restoreDeploy < restoreSeal);
  assert.match(packageSource, /validateRollbackCaptureReport\(captureReport/u);
  assert.match(packageSource, /validateLegacyReconstructedIdentityBytes\(identityBytes/u);
  assert.match(packageSource, /validateLegacyDeployedPagesSnapshot\(\{/u);
  assert.match(deploymentSource, /validateLegacySnapshot = validateLegacyDeployedPagesSnapshot/u);
  assert.match(deploymentSource, /rollbackCaptureReport\.legacy_snapshot\.identity/u);
  assert.match(deploymentSource, /rollback_report_artifact_api_readback_sha256/u);
});

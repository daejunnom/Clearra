import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { canonicalSha256, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import {
  LEGACY_BOOTSTRAP_RELEASE_TAG,
  captureArtifactAuthoritySha256,
  canonicalAcceptanceQuery,
  expectedCaptureArtifactName,
  expectedCaptureReportArtifactName,
  preparePagesRollbackManifests,
  produceRollbackCaptureReport,
  readRollbackCaptureReport,
  readBoundedResponseBytes,
  resolveCaptureReportArtifact,
  validateCanonicalRuns,
  validateCompleteDeploymentStatuses,
  validateCaptureAuthority,
  validateCaptureReportArtifact,
  validateLegacyPagesBootstrapAuthority,
  validateLivePagesIdentity,
  validatePagesCaptureRequestInputs,
  validatePagesAuthorityPhase,
  validatePagesIdentity,
  validatePagesMutationCaptureKind,
  validateRollbackCaptureReport,
  validateRollbackCaptureReportDiagnostic,
  validateRunAttemptPolicy,
  verifyCurrentPagesAgainstCapture,
  writeRollbackCaptureReport,
} from "./pages-rollback-authority.mjs";

const SNAPSHOT = "1".repeat(40);
const AUTHORITY = "2".repeat(40);
const LEGACY_SNAPSHOT = "0438d85f90b47c4ce89835f6a6d665a0415aa25a";
const ARTIFACT_DIGEST = `sha256:${"3".repeat(64)}`;
const TAR_DIGEST = "4".repeat(64);
const TAG_OBJECT = "a95973dbc1c3c1919478328d12e4d25ddaedea71";

function legacyBootstrapFixture() {
  return {
    repository: "daejunnom/Clearra",
    legacyReleaseTag: LEGACY_BOOTSTRAP_RELEASE_TAG,
    snapshotSha: LEGACY_SNAPSHOT,
    tagRef: {
      ref: `refs/tags/${LEGACY_BOOTSTRAP_RELEASE_TAG}`,
      object: { type: "tag", sha: TAG_OBJECT },
    },
    annotatedTag: {
      sha: TAG_OBJECT,
      tag: LEGACY_BOOTSTRAP_RELEASE_TAG,
      object: { type: "commit", sha: LEGACY_SNAPSHOT },
      tagger: {
        name: "Release Bot",
        email: "release@example.invalid",
        date: "2026-08-12T14:17:00Z",
      },
    },
    release: {
      tag_name: LEGACY_BOOTSTRAP_RELEASE_TAG,
      draft: false,
      prerelease: false,
      published_at: "2026-08-12T15:11:26Z",
    },
    deployments: [{
      id: 6181925865,
      sha: LEGACY_SNAPSHOT,
      ref: "main",
      task: "deploy",
      environment: "github-pages",
    }],
    deploymentStatuses: [{
      state: "success",
      environment: "github-pages",
      environment_url: "https://daejunnom.github.io/Clearra/",
      deployment_url:
        "https://api.github.com/repos/daejunnom/Clearra/deployments/6181925865",
    }],
    pageUrl: "https://daejunnom.github.io/Clearra",
    identityStatus: 404,
  };
}

function pagesIdentity(sha = SNAPSHOT) {
  return {
    identity: {
      schema: "clearra.pages.identity.v2",
      sourceCommit: sha,
      engineBuildId: sha,
      contractSchemaVersion: "clearra.search.contract.v2",
      supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
      artifactSchemaVersion: "clearra.solution-data.v1",
      version: "0.7.5",
      acceptedRunId: "12345",
      acceptedRunAttempt: "1",
      basePath: "/Clearra",
      files: [
        { path: "index.html", size: 128, sha256: "c".repeat(64) },
        { path: "wasm/clearra_wasm_bg.wasm", size: 256, sha256: "d".repeat(64) },
      ],
    },
    manifest: {
      build: {
        runtime_identity: {
          source_commit: sha,
          engine_build_id: sha,
          contract_schema_version: "clearra.search.contract.v2",
          supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
          artifact_schema_version: "clearra.solution-data.v1",
        },
      },
    },
  };
}

function reconstructedPagesIdentity(sha = SNAPSHOT) {
  const { identity, manifest } = pagesIdentity(sha);
  return {
    identity: {
      schema: identity.schema,
      sourceCommit: identity.sourceCommit,
      engineBuildId: identity.engineBuildId,
      contractSchemaVersion: identity.contractSchemaVersion,
      supplySemanticsId: identity.supplySemanticsId,
      artifactSchemaVersion: identity.artifactSchemaVersion,
      version: identity.version,
    },
    manifest,
  };
}

function v074LegacyWasmManifest() {
  const bindingsSha256 = "a".repeat(64);
  const wasmSha256 = "b".repeat(64);
  return {
    schema_version: 1,
    build: {
      contract_version: 1,
      source_sha256: "c".repeat(64),
      source_file_count: 127,
      capabilities_sha256:
        "6e6e2c1e973f62c6d6fa28f571b326104aec625e6879c4aca67df3364029d98b",
    },
    bindings: {
      path: `clearra_wasm.${bindingsSha256.slice(0, 24)}.js`,
      bytes: 65432,
      sha256: bindingsSha256,
    },
    wasm: {
      path: `clearra_wasm_bg.${wasmSha256.slice(0, 24)}.wasm`,
      bytes: 7654321,
      sha256: wasmSha256,
    },
  };
}

function serializeV074LegacyWasmManifest(manifest) {
  const json = JSON.stringify(manifest);
  const byteLength = Buffer.byteLength(json, "utf8") + 1;
  assert.ok(byteLength <= 768, "test fixture must fit the v0.7.4 manifest contract");
  return `${json}${" ".repeat(768 - byteLength)}\n`;
}

function exactRuntimeIdentity(sha = SNAPSHOT) {
  return {
    source_commit: sha,
    engine_build_id: sha,
    contract_schema_version: "clearra.search.contract.v2",
    supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
    artifact_schema_version: "clearra.solution-data.v1",
  };
}

async function writeManifestPair(root, staticBytes, buildBytes = staticBytes) {
  const staticManifestPath = join(root, "static-manifest.json");
  const buildManifestPath = join(root, "build-manifest.json");
  await Promise.all([
    writeFile(staticManifestPath, staticBytes, "utf8"),
    writeFile(buildManifestPath, buildBytes, "utf8"),
  ]);
  return { staticManifestPath, buildManifestPath };
}

function canonicalRun(overrides = {}) {
  return {
    id: 1,
    run_attempt: 1,
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
    head_branch: "main",
    head_sha: SNAPSHOT,
    path: ".github/workflows/release-cli.yml",
    ...overrides,
  };
}

function captureFixture() {
  const runId = "12345";
  const attempt = 2;
  const artifactId = "67890";
  const artifactName = expectedCaptureArtifactName({
    snapshotSha: SNAPSHOT,
    authoritySha: AUTHORITY,
    captureRunId: runId,
    captureRunAttempt: attempt,
  });
  const captureRun = {
    id: Number(runId),
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
    head_branch: "main",
    head_sha: AUTHORITY,
    path: ".github/workflows/pages-rollback.yml",
    run_attempt: attempt,
    created_at: "2026-08-28T00:00:00Z",
    updated_at: "2026-08-28T00:10:00Z",
  };
  const captureJobs = {
    total_count: 2,
    jobs: [
      {
        name: "capture-authority",
        status: "completed",
        conclusion: "success",
        started_at: "2026-08-28T00:00:01Z",
        completed_at: "2026-08-28T00:01:00Z",
      },
      {
        name: "capture-build",
        status: "completed",
        conclusion: "success",
        started_at: "2026-08-28T00:01:01Z",
        completed_at: "2026-08-28T00:09:59Z",
      },
    ],
  };
  const artifact = {
    id: Number(artifactId),
    name: artifactName,
    expired: false,
    digest: ARTIFACT_DIGEST,
    size_in_bytes: 6_000_000,
    created_at: "2026-08-28T00:09:00Z",
    expires_at: "2026-11-26T00:09:00Z",
    workflow_run: {
      id: Number(runId),
      head_branch: "main",
      head_sha: AUTHORITY,
    },
  };
  const consumerRun = {
    id: 99999,
    event: "workflow_dispatch",
    head_branch: "main",
    head_sha: AUTHORITY,
    created_at: "2026-08-28T00:11:00Z",
  };
  return {
    snapshotSha: SNAPSHOT,
    authoritySha: AUTHORITY,
    captureRunId: runId,
    captureArtifactId: artifactId,
    captureArtifactName: artifactName,
    captureArtifactDigest: ARTIFACT_DIGEST,
    captureTarSha256: TAR_DIGEST,
    captureRun,
    captureJobs,
    artifact,
    consumerRun,
  };
}

test("full Pages and WASM identity contract is required", () => {
  const valid = pagesIdentity();
  validateLivePagesIdentity(valid.identity, valid.manifest, SNAPSHOT);
  const reconstructed = reconstructedPagesIdentity();
  validatePagesIdentity(reconstructed.identity, reconstructed.manifest, SNAPSHOT);
  assert.throws(
    () => validatePagesIdentity(valid.identity, valid.manifest, SNAPSHOT),
    /closed schema/u,
  );

  for (const mutate of [
    (value) => { value.identity.sourceCommit = AUTHORITY; },
    (value) => { value.identity.contractSchemaVersion = "forged"; },
    (value) => { value.identity.basePath = "/forged"; },
    (value) => { value.identity.files[0].path = "../index.html"; },
    (value) => { value.identity.files[0].path = "index.html?stale=1"; },
    (value) => {
      value.identity.files = ["a", "a_", "a-", "a.", "a/x"].map((path) => ({
        path,
        size: 1,
        sha256: "c".repeat(64),
      }));
    },
    (value) => {
      value.identity.files = Array.from({ length: 1_025 }, (_entry, index) => ({
        path: `assets/${String(index).padStart(4, "0")}.js`,
        size: 0,
        sha256: "c".repeat(64),
      }));
    },
    (value) => {
      value.identity.files = [
        { path: "a", size: (32 * 1024 * 1024) + 1, sha256: "c".repeat(64) },
        { path: "b", size: (32 * 1024 * 1024) + 1, sha256: "d".repeat(64) },
      ];
    },
    (value) => { value.identity.files[0].extra = true; },
    (value) => { value.identity.acceptedRunId = "0"; },
    (value) => { value.identity.acceptedRunAttempt = "0"; },
    (value) => { value.identity.extra = true; },
    (value) => { value.identity.version = ""; },
    (value) => { value.manifest.build.runtime_identity.engine_build_id = AUTHORITY; },
    (value) => { value.manifest.build.runtime_identity.artifact_schema_version = "forged"; },
  ]) {
    const forged = structuredClone(valid);
    mutate(forged);
    assert.throws(() =>
      validateLivePagesIdentity(forged.identity, forged.manifest, SNAPSHOT));
  }
});

test("regular capture verifies manifests without rewriting them and rejects missing or wrong identity", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-pages-capture-manifest-"));
  try {
    const current = v074LegacyWasmManifest();
    current.build.runtime_identity = exactRuntimeIdentity();
    const originalBytes = `${JSON.stringify(current, null, 2)}\n`;
    const paths = await writeManifestPair(root, originalBytes);
    assert.deepEqual(await preparePagesRollbackManifests({
      captureMode: "capture",
      snapshotSha: SNAPSHOT,
      ...paths,
    }), { mode: "capture", updated: false });
    assert.equal(await readFile(paths.staticManifestPath, "utf8"), originalBytes);
    assert.equal(await readFile(paths.buildManifestPath, "utf8"), originalBytes);

    const missingIdentity = v074LegacyWasmManifest();
    const missingBytes = serializeV074LegacyWasmManifest(missingIdentity);
    await writeManifestPair(root, missingBytes);
    await assert.rejects(
      preparePagesRollbackManifests({
        captureMode: "capture",
        snapshotSha: SNAPSHOT,
        ...paths,
      }),
      /runtime identity must be an object/u,
    );
    assert.equal(await readFile(paths.staticManifestPath, "utf8"), missingBytes);

    const wrongIdentity = v074LegacyWasmManifest();
    wrongIdentity.build.runtime_identity = exactRuntimeIdentity(AUTHORITY);
    const wrongBytes = `${JSON.stringify(wrongIdentity)}\n`;
    await writeManifestPair(root, wrongBytes);
    await assert.rejects(
      preparePagesRollbackManifests({
        captureMode: "capture",
        snapshotSha: SNAPSHOT,
        ...paths,
      }),
      /differs from the exact snapshot contract/u,
    );
    assert.equal(await readFile(paths.buildManifestPath, "utf8"), wrongBytes);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("legacy manifest handling is excluded from the modern preparation path", async () => {
  await assert.rejects(
    preparePagesRollbackManifests({
      captureMode: "bootstrap-capture",
      snapshotSha: LEGACY_SNAPSHOT,
    }),
    /only for regular capture/u,
  );
});

test("legacy Pages bootstrap binds one approved annotated release to the active 404 site", () => {
  const valid = legacyBootstrapFixture();
  assert.deepEqual(validateLegacyPagesBootstrapAuthority(valid), {
    repository: "daejunnom/Clearra",
    legacyReleaseTag: "v0.7.4",
    snapshotSha: LEGACY_SNAPSHOT,
    deploymentId: "6181925865",
  });

  const cases = [
    ["arbitrary tag", (value) => { value.legacyReleaseTag = "v0.7.3"; }],
    ["lightweight tag", (value) => { value.tagRef.object.type = "commit"; }],
    ["mismatched tag", (value) => { value.annotatedTag.object.sha = AUTHORITY; }],
    ["moved tag object and snapshot", (value) => {
      value.snapshotSha = AUTHORITY;
      value.tagRef.object.sha = "5".repeat(40);
      value.annotatedTag.sha = "5".repeat(40);
      value.annotatedTag.object.sha = AUTHORITY;
      value.deployments[0].sha = AUTHORITY;
    }],
    ["draft release", (value) => { value.release.draft = true; }],
    ["prerelease", (value) => { value.release.prerelease = true; }],
    ["inactive snapshot", (value) => { value.deployments[0].sha = AUTHORITY; }],
    ["failed deployment", (value) => { value.deploymentStatuses[0].state = "failure"; }],
    ["identity-valid site", (value) => { value.identityStatus = 200; }],
  ];
  for (const [label, mutate] of cases) {
    const forged = structuredClone(valid);
    mutate(forged);
    assert.throws(
      () => validateLegacyPagesBootstrapAuthority(forged),
      undefined,
      label,
    );
  }

  const extra = { ...structuredClone(valid), bypass: true };
  assert.throws(
    () => validateLegacyPagesBootstrapAuthority(extra),
    /closed schema/u,
  );
});

test("bootstrap capture requires its explicit tag and rejects every restore-only extra input", () => {
  const valid = {
    mode: "bootstrap-capture",
    legacyReleaseTag: LEGACY_BOOTSTRAP_RELEASE_TAG,
    requestedCurrentPagesSha: "",
    captureRunId: "",
    restoreAuthorization: "",
  };
  assert.equal(validatePagesCaptureRequestInputs(valid), valid);
  assert.doesNotThrow(() => validatePagesCaptureRequestInputs({
    ...valid,
    mode: "capture",
    legacyReleaseTag: "",
  }));
  for (const mutate of [
    (value) => { value.legacyReleaseTag = ""; },
    (value) => { value.requestedCurrentPagesSha = SNAPSHOT; },
    (value) => { value.captureRunId = "12345"; },
    (value) => { value.restoreAuthorization = "ROLLBACK:anything"; },
  ]) {
    const forged = { ...valid };
    mutate(forged);
    assert.throws(() => validatePagesCaptureRequestInputs(forged));
  }
  assert.throws(
    () => validatePagesCaptureRequestInputs({ ...valid, bypass: true }),
    /closed schema/u,
  );
});

test("Pages rollback workflow keeps bootstrap capture read-only and reuses the sealed capture path", async () => {
  const workflow = await readFile(
    new URL("../../.github/workflows/pages-rollback.yml", import.meta.url),
    "utf8",
  );
  for (const marker of [
    "- bootstrap-capture",
    "legacy_release_tag:",
    "required only for bootstrap-capture",
    "inputs.mode == 'capture' || inputs.mode == 'bootstrap-capture'",
    "PAGES_AUTHORITY_MODE: ${{ inputs.mode }}",
    "LEGACY_RELEASE_TAG: ${{ inputs.legacy_release_tag }}",
    "REQUESTED_CURRENT_PAGES_SHA: ${{ inputs.current_pages_sha }}",
    "Build previous Pages source",
    "Download exact accepted Pages build without rebuilding",
    "Prove accepted build and current public bytes before capture",
    "Reprove accepted build and public bytes immediately before sealing",
    "Stamp separate reconstructed v0.7.4 identity",
    "node authority-source/scripts/release/pages-legacy-contract.mjs",
    "legacy_initial_evidence_base64",
    "LEGACY_PREARTIFACT_EVIDENCE_BASE64",
    "Download uploaded rollback artifact by exact run-bound name",
    "Seal exact rollback capture authority",
    "Verify downloaded rollback package against sealed capture report",
  ]) {
    assert.ok(workflow.includes(marker), `missing bootstrap workflow marker: ${marker}`);
  }
  const captureJobs = workflow.slice(
    workflow.indexOf("\n  capture-authority:"),
    workflow.indexOf("\n  restore-authority:"),
  );
  assert.doesNotMatch(captureJobs, /pages: write|id-token: write|actions\/deploy-pages/u);
  assert.equal(
    (captureJobs.match(/deployments: read/gu) ?? []).length,
    2,
    "both bootstrap authority readbacks must have deployment read permission",
  );
  assert.equal(
    (captureJobs.match(/PAGES_AUTHORITY_MODE: \$\{\{ inputs\.mode \}\}/gu) ?? []).length,
    2,
  );
  assert.equal(
    (captureJobs.match(/actions\/upload-pages-artifact@v3/gu) ?? []).length,
    1,
  );
  assert.doesNotMatch(captureJobs, /Prepare exact rollback manifest contract|Stamp exact rollback identity/u);
  const legacyStamp = captureJobs.slice(
    captureJobs.indexOf("- name: Stamp separate reconstructed v0.7.4 identity"),
    captureJobs.indexOf("- name: Validate portable rollback tree"),
  );
  assert.match(legacyStamp, /if: \$\{\{ inputs\.mode == 'bootstrap-capture' \}\}/u);
  assert.doesNotMatch(legacyStamp, /snapshot-source\/scripts\/release/u);
  assert.ok(
    captureJobs.indexOf("Seal exact rollback capture authority") <
      captureJobs.indexOf("Verify downloaded rollback package against sealed capture report"),
    "freshly sealed capture report must validate the downloaded tar before report upload",
  );
  const restorePackage = workflow.slice(
    workflow.indexOf("\n  restore-package:"),
    workflow.indexOf("\n  deploy-restore:"),
  );
  assert.match(
    restorePackage,
    /PAGES_ROLLBACK_EXPECTED_CAPTURE_KIND: \$\{\{ needs\.restore-authority\.outputs\.capture_kind \}\}/u,
  );
  assert.match(restorePackage, /canonical_identity_sha256/u);
  assert.match(restorePackage, /canonical_file_set_sha256/u);
  const restoreDeploy = workflow.slice(workflow.indexOf("\n  deploy-restore:"));
  for (const permission of [
    "contents: read",
    "actions: read",
    "deployments: read",
    "pages: write",
    "id-token: write",
  ]) {
    assert.ok(
      restoreDeploy.includes(permission),
      `restore deploy permission is missing: ${permission}`,
    );
  }
});

test("authority phases are closed for capture and mutation modes", () => {
  for (const mode of ["capture", "bootstrap-capture"]) {
    assert.equal(validatePagesAuthorityPhase(mode, "initial"), "initial");
    assert.equal(validatePagesAuthorityPhase(mode, "preartifact"), "preartifact");
    assert.throws(
      () => validatePagesAuthorityPhase(mode, "predeploy"),
      /phase is invalid/u,
    );
  }
  for (const mode of ["forward", "restore"]) {
    assert.equal(validatePagesAuthorityPhase(mode, "initial"), "initial");
    assert.equal(validatePagesAuthorityPhase(mode, "predeploy"), "predeploy");
    assert.throws(
      () => validatePagesAuthorityPhase(mode, "preartifact"),
      /phase is invalid/u,
    );
  }
});

test("forward and restore reject old modern-v2 and admit canonical-v2", () => {
  assert.equal(
    validatePagesMutationCaptureKind("forward", "legacy-v0.7.4"),
    "legacy-v0.7.4",
  );
  assert.equal(
    validatePagesMutationCaptureKind("forward", "canonical-v2"),
    "canonical-v2",
  );
  assert.equal(
    validatePagesMutationCaptureKind("restore", "legacy-v0.7.4"),
    "legacy-v0.7.4",
  );
  assert.equal(
    validatePagesMutationCaptureKind("restore", "canonical-v2"),
    "canonical-v2",
  );
  assert.throws(() => validatePagesMutationCaptureKind("forward", "modern-v2"), /unsupported/u);
  assert.throws(() => validatePagesMutationCaptureKind("restore", "modern-v2"), /unsupported/u);
  assert.throws(
    () => validatePagesMutationCaptureKind("forward", "unknown"),
    /forward capture report kind is unsupported/u,
  );
});

test("old modern-v2 capture report is diagnostic-only and never mutation authority", () => {
  const diagnostic = sealCanonicalReport({
    schema_id: "clearra.pages.rollback-capture-authority.v2", repository: "daejunnom/Clearra",
    snapshot_source_commit: SNAPSHOT, authority_source_commit: AUTHORITY, capture_run_id: "12345", capture_run_attempt: "1",
    workflow_path: ".github/workflows/pages-rollback.yml", workflow_run_api_readback_sha256: "1".repeat(64), artifact_id: "2",
    artifact_name: expectedCaptureArtifactName({ snapshotSha: SNAPSHOT, authoritySha: AUTHORITY, captureRunId: "12345", captureRunAttempt: "1" }),
    artifact_digest: `sha256:${"2".repeat(64)}`, artifact_sha256: "2".repeat(64), artifact_archive_size_bytes: 1,
    artifact_tar_sha256: "3".repeat(64), artifact_tar_size_bytes: 1, artifact_api_readback_sha256: "4".repeat(64),
    artifact_created_at: "2026-08-01T00:00:00.000Z", artifact_expires_at: "2026-10-30T00:00:00.000Z", retention_seconds: 90 * 24 * 60 * 60,
    capture_kind: "modern-v2", legacy_snapshot: null, status: "captured",
  });
  assert.equal(validateRollbackCaptureReportDiagnostic(diagnostic), diagnostic);
  assert.throws(() => validateRollbackCaptureReport(diagnostic), /closed schema|schema is invalid/u);
  assert.throws(() => validatePagesMutationCaptureKind("restore", diagnostic.capture_kind), /unsupported/u);
});

test("current Pages routing never falls back between legacy bytes and modern identity", async () => {
  const legacyCalls = { github: 0, json: 0, status: 0, bytes: 0, validated: 0 };
  await verifyCurrentPagesAgainstCapture({
    mode: "forward",
    phase: "initial",
    validatedCaptureReport: {
      capture_kind: "legacy-v0.7.4",
      legacy_snapshot: {
        preartifact_public_readback: { deployment_id: "6181925865" },
      },
    },
    repository: "daejunnom/Clearra",
    pageUrl: "https://daejunnom.github.io/Clearra",
    cacheBuster: "forward-initial-12345",
    currentPagesSha: LEGACY_SNAPSHOT,
  }, {
    async getGithubJson(path) {
      legacyCalls.github += 1;
      if (path.includes("/git/ref/tags/")) {
        return legacyBootstrapFixture().tagRef;
      }
      if (path.includes("/git/tags/")) {
        return legacyBootstrapFixture().annotatedTag;
      }
      if (path.includes("/releases/tags/")) {
        return legacyBootstrapFixture().release;
      }
      if (path.endsWith("/statuses?per_page=100&page=1")) {
        return legacyBootstrapFixture().deploymentStatuses;
      }
      if (path.endsWith("/statuses?per_page=100&page=2")) return [];
      if (path === "/deployments/6181925865") {
        return legacyBootstrapFixture().deployments[0];
      }
      assert.fail(`unexpected legacy GitHub read: ${path}`);
    },
    async readPublicJson() {
      legacyCalls.json += 1;
      assert.fail("legacy forward must not read modern identity JSON");
    },
    async readPublicStatus() {
      legacyCalls.status += 1;
      return 404;
    },
    async readPublicBytes(_url, _label, expectedSize) {
      legacyCalls.bytes += 1;
      assert.ok(Number.isSafeInteger(expectedSize));
      return Buffer.alloc(1);
    },
    validateLegacyAuthority(value) {
      legacyCalls.validated += 1;
      assert.equal(value.identityStatus, 404);
      assert.equal(value.deployment.id, 6181925865);
      return value;
    },
  });
  assert.deepEqual(legacyCalls, {
    github: 6,
    json: 0,
    status: 1,
    bytes: 3,
    validated: 1,
  });

  for (const mode of ["forward", "restore"]) {
    let reads = 0;
    await assert.rejects(verifyCurrentPagesAgainstCapture({
      mode,
      phase: mode === "forward" ? "predeploy" : "initial",
      validatedCaptureReport: { capture_kind: "modern-v2" },
      repository: "daejunnom/Clearra",
      pageUrl: "https://daejunnom.github.io/Clearra",
      cacheBuster: `${mode}-old-modern`,
      currentPagesSha: SNAPSHOT,
    }, {
      async readPublicJson() { reads += 1; },
      async readPublicBytes() { reads += 1; },
      async readPublicStatus() { reads += 1; },
      async getGithubJson() { reads += 1; },
    }), /unsupported/u);
    assert.equal(reads, 0, "old modern-v2 must fail before any mutation precondition read");
  }
});

test("deployment status readback proves pagination completeness", () => {
  const page = Array.from({ length: 100 }, (_value, index) => ({
    id: index + 1,
    state: "success",
  }));
  assert.equal(validateCompleteDeploymentStatuses(page, []).length, 100);
  assert.throws(
    () => validateCompleteDeploymentStatuses(page, [{ id: 101, state: "success" }]),
    /second page must be exactly empty/u,
  );
  assert.throws(
    () => validateCompleteDeploymentStatuses([], []),
    /first page must contain between 1 and 100 statuses/u,
  );
  assert.throws(
    () => validateCompleteDeploymentStatuses([...page, { id: 101 }], []),
    /first page must contain between 1 and 100 statuses/u,
  );
});

test("Pages forward deployment keeps mutation authority in the deploy job only", async () => {
  const workflow = await readFile(
    new URL("../../.github/workflows/pages.yml", import.meta.url),
    "utf8",
  );
  const jobsIndex = workflow.indexOf("\njobs:");
  const deployIndex = workflow.indexOf("\n  deploy:", jobsIndex);
  assert.notEqual(jobsIndex, -1);
  assert.notEqual(deployIndex, -1);

  const workflowAuthority = workflow.slice(0, jobsIndex);
  const predeployJobs = workflow.slice(jobsIndex, deployIndex);
  const deployJob = workflow.slice(deployIndex);
  assert.match(workflowAuthority, /permissions:\s+contents: read\s+actions: read/u);
  assert.doesNotMatch(workflowAuthority, /pages: write|id-token: write/u);
  assert.doesNotMatch(predeployJobs, /pages: write|id-token: write/u);
  assert.match(
    predeployJobs,
    /accepted-source:\s+permissions:\s+contents: read\s+actions: read\s+deployments: read/u,
  );
  const buildJob = predeployJobs.slice(predeployJobs.indexOf("\n  build:"));
  assert.doesNotMatch(buildJob, /deployments: read/u);
  assert.match(
    deployJob,
    /permissions:\s+contents: read\s+actions: read\s+deployments: read\s+pages: write\s+id-token: write/u,
  );
});

test("legacy public identity absence probe rejects redirects", async () => {
  const authoritySource = await readFile(
    new URL("./pages-rollback-authority.mjs", import.meta.url),
    "utf8",
  );
  const statusStart = authoritySource.indexOf("async function fetchPublicStatus(url)");
  const bytesStart = authoritySource.indexOf("async function fetchPublicBytes(url, label)");
  assert.notEqual(statusStart, -1);
  assert.ok(bytesStart > statusStart);
  const statusProbe = authoritySource.slice(statusStart, bytesStart);
  assert.match(statusProbe, /cache: "no-store"/u);
  assert.match(statusProbe, /redirect: "error"/u);
});

test("canonical acceptance requires one exact success and rejects duplicate or wrong authority", () => {
  validateCanonicalRuns(
    { total_count: 1, workflow_runs: [canonicalRun()] },
    SNAPSHOT,
  );
  assert.throws(
    () => validateCanonicalRuns(
      {
        total_count: 2,
        workflow_runs: [canonicalRun(), canonicalRun({ id: 2 })],
      },
      SNAPSHOT,
    ),
    /exactly 1/u,
  );
  assert.throws(
    () => validateCanonicalRuns(
      { total_count: 1, workflow_runs: [canonicalRun({ event: "push" })] },
      SNAPSHOT,
    ),
    /exact canonical run identity/u,
  );
  assert.throws(
    () => validateCanonicalRuns(
      { total_count: 1, workflow_runs: [canonicalRun({ head_sha: AUTHORITY })] },
      SNAPSHOT,
    ),
    /exact canonical run identity/u,
  );
});

test("canonical acceptance query pins the main branch and a non-truncating exact SHA page", () => {
  const query = new URLSearchParams(canonicalAcceptanceQuery(SNAPSHOT));
  assert.deepEqual(Object.fromEntries(query), {
    event: "workflow_dispatch",
    branch: "main",
    head_sha: SNAPSHOT,
    per_page: "100",
  });
});

test("capture authority binds the run attempt, job, artifact, retention, and consumer order", () => {
  const valid = captureFixture();
  validateCaptureAuthority(valid);

  const cases = [
    (value) => { value.captureArtifactName += "-forged"; },
    (value) => { value.captureRun.path = ".github/workflows/pages.yml"; },
    (value) => { value.captureJobs.jobs[1].conclusion = "failure"; },
    (value) => { value.captureJobs.jobs.push(structuredClone(value.captureJobs.jobs[1])); },
    (value) => { value.captureJobs.total_count += 1; },
    (value) => { value.captureJobs.total_count = 101; },
    (value) => { value.artifact.expired = true; },
    (value) => { value.artifact.digest = `sha256:${"5".repeat(64)}`; },
    (value) => { value.artifact.workflow_run.head_sha = SNAPSHOT; },
    (value) => { value.artifact.expires_at = "2026-08-29T00:09:00Z"; },
    (value) => { value.artifact.created_at = "2026-08-28T00:10:01Z"; },
    (value) => { value.consumerRun.created_at = "2026-08-28T00:09:00Z"; },
  ];
  for (const mutate of cases) {
    const forged = structuredClone(valid);
    mutate(forged);
    assert.throws(() => validateCaptureAuthority(forged));
  }
});

test("capture artifact readback hash uses a closed authority projection", () => {
  const artifact = captureFixture().artifact;
  const expected = captureArtifactAuthoritySha256(artifact);
  assert.equal(
    captureArtifactAuthoritySha256({
      ...artifact,
      updated_at: "2099-01-01T00:00:00Z",
      archive_download_url: "https://api.github.com/unstable-metadata",
    }),
    expected,
  );
  assert.notEqual(
    captureArtifactAuthoritySha256({ ...artifact, digest: `sha256:${"f".repeat(64)}` }),
    expected,
  );
});

test("bounded response reader rejects Content-Length, chunked, and exact-size drift", async () => {
  await assert.rejects(
    readBoundedResponseBytes(new Response("12345", {
      headers: { "content-length": "5" },
    }), "oversized header", { maximumBytes: 4 }),
    /Content-Length exceeds/u,
  );
  const chunked = new Response(new ReadableStream({
    start(controller) {
      controller.enqueue(Uint8Array.from([1, 2, 3]));
      controller.enqueue(Uint8Array.from([4, 5, 6]));
      controller.close();
    },
  }));
  await assert.rejects(
    readBoundedResponseBytes(chunked, "chunked overrun", { maximumBytes: 5 }),
    /body exceeds/u,
  );
  await assert.rejects(
    readBoundedResponseBytes(new Response("123"), "exact payload", {
      maximumBytes: 4,
      exactBytes: 4,
    }),
    /exact byte length/u,
  );
});

test("capture names are unique per authority, run, and rerun attempt", () => {
  const first = expectedCaptureArtifactName({
    snapshotSha: SNAPSHOT,
    authoritySha: AUTHORITY,
    captureRunId: "12345",
    captureRunAttempt: "1",
  });
  const retry = expectedCaptureArtifactName({
    snapshotSha: SNAPSHOT,
    authoritySha: AUTHORITY,
    captureRunId: "12345",
    captureRunAttempt: "2",
  });
  assert.notEqual(first, retry);
});

test("capture reruns are unique while forward and restore require a fresh dispatch", () => {
  assert.equal(validateRunAttemptPolicy("capture", "2"), "2");
  assert.equal(validateRunAttemptPolicy("bootstrap-capture", "2"), "2");
  assert.equal(validateRunAttemptPolicy("forward", "1"), "1");
  assert.equal(validateRunAttemptPolicy("restore", "1"), "1");
  assert.throws(
    () => validateRunAttemptPolicy("forward", "2"),
    /fresh workflow dispatch/u,
  );
  assert.throws(
    () => validateRunAttemptPolicy("restore", "2"),
    /fresh workflow dispatch/u,
  );
});

test("capture report seals actual artifact ID, digest, run attempt, tar hash, and retention", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-pages-rollback-report-"));
  try {
    const runId = "12345";
    const runAttempt = "2";
    const artifactId = "67890";
    const artifactName = expectedCaptureArtifactName({
      snapshotSha: SNAPSHOT,
      authoritySha: AUTHORITY,
      captureRunId: runId,
      captureRunAttempt: runAttempt,
    });
    const tarPath = join(root, "artifact.tar");
    await writeFile(tarPath, "exact-pages-tar", "utf8");
    const acceptedIdentity = pagesIdentity(SNAPSHOT).identity;
    const identityBytes = Buffer.from(JSON.stringify(acceptedIdentity));
    const canonicalReadback = {
      identity_sha256: canonicalSha256(acceptedIdentity),
      identity_bytes_sha256: createHash("sha256").update(identityBytes).digest("hex"),
      identity_bytes_size: identityBytes.byteLength,
      file_set_sha256: canonicalSha256(acceptedIdentity.files),
      file_count: acceptedIdentity.files.length,
      total_bytes: acceptedIdentity.files.reduce((sum, file) => sum + file.size, 0),
    };
    const canonicalSnapshot = {
      accepted_run_id: "12345", accepted_run_attempt: "1", accepted_artifact_id: "77777",
      accepted_artifact_name: `accepted-pages-build-${SNAPSHOT}-run-12345-attempt-1`, accepted_artifact_digest: `sha256:${"a".repeat(64)}`,
      accepted_artifact_api_readback_sha256: "b".repeat(64), accepted_artifact_created_at: "2026-08-28T00:00:00.000Z", accepted_artifact_expires_at: "2026-11-26T00:00:00.000Z",
      identity: acceptedIdentity, ...canonicalReadback,
      initial_public_readback: canonicalReadback, preartifact_public_readback: canonicalReadback,
    };
    const responses = {
      [`/actions/runs/${runId}`]: {
        id: Number(runId),
        run_attempt: Number(runAttempt),
        event: "workflow_dispatch",
        status: "in_progress",
        conclusion: null,
        head_branch: "main",
        head_sha: AUTHORITY,
        path: ".github/workflows/pages-rollback.yml",
      },
      [`/actions/artifacts/${artifactId}`]: {
        id: Number(artifactId),
        name: artifactName,
        expired: false,
        digest: ARTIFACT_DIGEST,
        size_in_bytes: 6_000_000,
        created_at: "2026-08-28T00:09:00Z",
        expires_at: "2026-11-26T00:09:00Z",
        workflow_run: {
          id: Number(runId),
          head_branch: "main",
          head_sha: AUTHORITY,
        },
      },
    };
    const report = await produceRollbackCaptureReport({
      repository: "daejunnom/Clearra",
      captureMode: "capture",
      snapshotSha: SNAPSHOT,
      authoritySha: AUTHORITY,
      captureRunId: runId,
      captureRunAttempt: runAttempt,
      artifactId,
      artifactName,
      artifactTarPath: tarPath,
      canonicalSnapshot,
    }, {
      async getGithubJson(path) {
        assert.ok(Object.hasOwn(responses, path), `unexpected GitHub read: ${path}`);
        return structuredClone(responses[path]);
      },
    });
    assert.equal(report.artifact_sha256, ARTIFACT_DIGEST.slice("sha256:".length));
    assert.equal(report.artifact_archive_size_bytes, 6_000_000);
    assert.equal(report.artifact_tar_size_bytes, Buffer.byteLength("exact-pages-tar"));
    assert.equal(
      report.artifact_tar_sha256,
      createHash("sha256").update("exact-pages-tar", "utf8").digest("hex"),
    );
    assert.equal(report.retention_seconds, 90 * 24 * 60 * 60);
    assert.equal(validateRollbackCaptureReport(report, {
      expectedSnapshotSha: SNAPSHOT,
      expectedAuthoritySha: AUTHORITY,
    }), report);
    assert.match(
      expectedCaptureReportArtifactName({
        snapshotSha: SNAPSHOT,
        authoritySha: AUTHORITY,
        captureRunId: runId,
        captureRunAttempt: runAttempt,
      }),
      /-run-12345-attempt-2$/u,
    );
    const reportPath = join(root, "capture-report.json");
    const fileSha256 = await writeRollbackCaptureReport(reportPath, report);
    assert.equal(
      fileSha256,
      createHash("sha256").update(await readFile(reportPath)).digest("hex"),
    );
    const reread = await readRollbackCaptureReport(reportPath);
    assert.deepEqual(reread.report, report);
    assert.equal(reread.file_sha256, fileSha256);
    const reportArtifactName = expectedCaptureReportArtifactName({
      snapshotSha: SNAPSHOT,
      authoritySha: AUTHORITY,
      captureRunId: runId,
      captureRunAttempt: runAttempt,
    });
    const reportArtifactDigest = `sha256:${"8".repeat(64)}`;
    validateCaptureReportArtifact({
      report,
      reportArtifactId: "99999",
      reportArtifactName,
      reportArtifactDigest,
      artifact: {
        id: 99999,
        name: reportArtifactName,
        digest: reportArtifactDigest,
        expired: false,
        created_at: "2026-08-28T00:09:30Z",
        expires_at: "2026-11-26T00:09:30Z",
        workflow_run: {
          id: Number(runId),
          head_branch: "main",
          head_sha: AUTHORITY,
        },
      },
    });
    const tampered = { ...report, artifact_tar_sha256: "9".repeat(64) };
    assert.throws(
      () => validateRollbackCaptureReport(tampered),
      /canonical content/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("capture report rejects short retention and wrong active run attempt", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-pages-rollback-reject-"));
  try {
    const artifactName = expectedCaptureArtifactName({
      snapshotSha: SNAPSHOT,
      authoritySha: AUTHORITY,
      captureRunId: "12345",
      captureRunAttempt: "2",
    });
    const tarPath = join(root, "artifact.tar");
    await writeFile(tarPath, "tar", "utf8");
    const base = {
      "/actions/runs/12345": {
        id: 12345,
        run_attempt: 2,
        event: "workflow_dispatch",
        status: "in_progress",
        conclusion: null,
        head_branch: "main",
        head_sha: AUTHORITY,
        path: ".github/workflows/pages-rollback.yml",
      },
      "/actions/artifacts/67890": {
        id: 67890,
        name: artifactName,
        expired: false,
        digest: ARTIFACT_DIGEST,
        size_in_bytes: 6_000_000,
        created_at: "2026-08-28T00:00:00Z",
        expires_at: "2026-08-29T00:00:00Z",
        workflow_run: { id: 12345, head_branch: "main", head_sha: AUTHORITY },
      },
    };
    const input = {
      repository: "daejunnom/Clearra",
      captureMode: "capture",
      snapshotSha: SNAPSHOT,
      authoritySha: AUTHORITY,
      captureRunId: "12345",
      captureRunAttempt: "2",
      artifactId: "67890",
      artifactName,
      artifactTarPath: tarPath,
    };
    await assert.rejects(
      produceRollbackCaptureReport(input, {
        async getGithubJson(path) { return structuredClone(base[path]); },
      }),
      /shorter than the durable authority policy/u,
    );
    base["/actions/runs/12345"].run_attempt = 3;
    base["/actions/artifacts/67890"].expires_at = "2026-11-26T00:00:00Z";
    await assert.rejects(
      produceRollbackCaptureReport(input, {
        async getGithubJson(path) { return structuredClone(base[path]); },
      }),
      /active exact-main attempt/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("resolves exactly one durable sealed report artifact from the completed capture attempt", () => {
  const reportName = expectedCaptureReportArtifactName({
    snapshotSha: SNAPSHOT,
    authoritySha: AUTHORITY,
    captureRunId: "12345",
    captureRunAttempt: "2",
  });
  const captureRun = {
    id: 12345,
    run_attempt: 2,
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
    head_branch: "main",
    head_sha: AUTHORITY,
    path: ".github/workflows/pages-rollback.yml",
    updated_at: "2026-08-28T00:10:00Z",
  };
  const reportArtifact = {
    id: 99999,
    name: reportName,
    digest: `sha256:${"8".repeat(64)}`,
    expired: false,
    created_at: "2026-08-28T00:09:30Z",
    expires_at: "2026-11-26T00:09:30Z",
    workflow_run: { id: 12345, head_branch: "main", head_sha: AUTHORITY },
  };
  const consumerRun = {
    id: 77777,
    event: "workflow_dispatch",
    status: "in_progress",
    conclusion: null,
    head_branch: "main",
    head_sha: AUTHORITY,
    created_at: "2026-08-28T00:11:00Z",
  };
  const input = {
    snapshotSha: SNAPSHOT,
    authoritySha: AUTHORITY,
    captureRunId: "12345",
    captureRun,
    captureArtifacts: {
      total_count: 2,
      artifacts: [
        { id: 67890, name: "unrelated" },
        reportArtifact,
      ],
    },
    consumerRun,
  };
  const resolved = resolveCaptureReportArtifact(input);
  assert.deepEqual(resolved, {
    capture_run_attempt: "2",
    report_artifact_id: "99999",
    report_artifact_name: reportName,
    report_artifact_digest: `sha256:${"8".repeat(64)}`,
  });

  const laterConsumer = structuredClone(input);
  laterConsumer.consumerAuthoritySha = "3".repeat(40);
  laterConsumer.consumerRun.head_sha = laterConsumer.consumerAuthoritySha;
  assert.deepEqual(resolveCaptureReportArtifact(laterConsumer), resolved);
  assert.throws(
    () => resolveCaptureReportArtifact({
      ...laterConsumer,
      consumerAuthoritySha: AUTHORITY,
    }),
    /independent active exact-main/u,
  );

  const duplicate = structuredClone(input);
  duplicate.captureArtifacts.artifacts.push(structuredClone(reportArtifact));
  duplicate.captureArtifacts.total_count += 1;
  assert.throws(
    () => resolveCaptureReportArtifact(duplicate),
    /exactly one sealed report artifact/u,
  );
  const hidden = structuredClone(input);
  hidden.captureArtifacts.total_count += 1;
  assert.throws(
    () => resolveCaptureReportArtifact(hidden),
    /complete page with total_count/u,
  );
  const truncated = structuredClone(input);
  truncated.captureArtifacts.total_count = 101;
  assert.throws(
    () => resolveCaptureReportArtifact(truncated),
    /complete page with total_count/u,
  );
  const shortRetention = structuredClone(input);
  shortRetention.captureArtifacts.artifacts[1].expires_at = "2026-08-29T00:09:30Z";
  assert.throws(
    () => resolveCaptureReportArtifact(shortRetention),
    /retention is shorter/u,
  );
});

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
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
  validateCaptureAuthority,
  validateCaptureReportArtifact,
  validateLegacyPagesBootstrapAuthority,
  validatePagesCaptureRequestInputs,
  validatePagesIdentity,
  validateRollbackCaptureReport,
  validateRunAttemptPolicy,
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
  validatePagesIdentity(valid.identity, valid.manifest, SNAPSHOT);

  for (const mutate of [
    (value) => { value.identity.sourceCommit = AUTHORITY; },
    (value) => { value.identity.contractSchemaVersion = "forged"; },
    (value) => { value.identity.version = ""; },
    (value) => { value.manifest.build.runtime_identity.engine_build_id = AUTHORITY; },
    (value) => { value.manifest.build.runtime_identity.artifact_schema_version = "forged"; },
  ]) {
    const forged = structuredClone(valid);
    mutate(forged);
    assert.throws(
      () => validatePagesIdentity(forged.identity, forged.manifest, SNAPSHOT),
      /does not match/u,
    );
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
    "Prepare exact rollback manifest contract",
    "PAGES_AUTHORITY_MODE: prepare-manifests",
    "PAGES_CAPTURE_MODE: ${{ inputs.mode }}",
    "Stamp exact rollback identity",
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
  assert.ok(
    captureJobs.indexOf("Prepare exact rollback manifest contract") <
      captureJobs.indexOf("Stamp exact rollback identity"),
    "manifest preparation must finish before the shared identity verification and stamp",
  );
  const modernStamp = captureJobs.slice(
    captureJobs.indexOf("- name: Stamp exact rollback identity"),
    captureJobs.indexOf("- name: Stamp separate reconstructed v0.7.4 identity"),
  );
  assert.match(modernStamp, /if: \$\{\{ inputs\.mode == 'capture' \}\}/u);
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

  const duplicate = structuredClone(input);
  duplicate.captureArtifacts.artifacts.push(structuredClone(reportArtifact));
  assert.throws(
    () => resolveCaptureReportArtifact(duplicate),
    /exactly one sealed report artifact/u,
  );
  const shortRetention = structuredClone(input);
  shortRetention.captureArtifacts.artifacts[1].expires_at = "2026-08-29T00:09:30Z";
  assert.throws(
    () => resolveCaptureReportArtifact(shortRetention),
    /retention is shorter/u,
  );
});

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { canonicalSha256, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import {
  expectedCaptureArtifactName,
  expectedCaptureReportArtifactName,
} from "./pages-rollback-authority.mjs";
import {
  LEGACY_PAGES_PAYLOAD,
  LEGACY_PAGES_PAYLOADS,
  LEGACY_PAGES_READBACK_SCHEMA,
  LEGACY_PAGES_RELEASE_TAG,
  LEGACY_PAGES_SNAPSHOT_SHA,
  LEGACY_PAGES_TAG_OBJECT_SHA,
  createLegacyReconstructedIdentity,
  legacyReconstructedIdentitySha256,
} from "./pages-legacy-contract.mjs";

import {
  producePagesDeploymentAuthority,
  validatePagesDeploymentAuthorityReport,
  writePagesDeploymentAuthorityReport,
} from "./pages-deployment-authority.mjs";

const SOURCE = "1".repeat(40);
const AUTHORITY = "2".repeat(40);
const ARTIFACT_SHA256 = "3".repeat(64);

function identity(sourceCommit = SOURCE) {
  const base = {
    schema: "clearra.pages.identity.v2",
    sourceCommit,
    engineBuildId: sourceCommit,
    contractSchemaVersion: "clearra.search.contract.v2",
    supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
    artifactSchemaVersion: "clearra.solution-data.v1",
    version: "0.8.0",
  };
  return {
    ...base,
    acceptedRunId: "11111",
    acceptedRunAttempt: "2",
    basePath: "/Clearra",
    files: [
      { path: "index.html", size: 128, sha256: "4".repeat(64) },
      { path: "wasm/clearra_wasm_bg.wasm", size: 256, sha256: "5".repeat(64) },
    ],
  };
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

function legacyCaptureReport() {
  const captureRunId = "12345";
  const captureRunAttempt = "1";
  const identityValue = createLegacyReconstructedIdentity({
    snapshotSha: LEGACY_PAGES_SNAPSHOT_SHA,
    authoritySha: AUTHORITY,
    captureRunId,
    captureRunAttempt,
  });
  return sealCanonicalReport({
    schema_id: "clearra.pages.rollback-capture-authority.v2",
    repository: "daejunnom/Clearra",
    snapshot_source_commit: LEGACY_PAGES_SNAPSHOT_SHA,
    authority_source_commit: AUTHORITY,
    capture_run_id: captureRunId,
    capture_run_attempt: captureRunAttempt,
    workflow_path: ".github/workflows/pages-rollback.yml",
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

function fixture({ mode = "forward" } = {}) {
  const rollbackCaptureReport = mode === "restore" ? legacyCaptureReport() : null;
  const sourceCommit = mode === "restore" ? LEGACY_PAGES_SNAPSHOT_SHA : SOURCE;
  const workflowSourceCommit = mode === "forward" ? SOURCE : AUTHORITY;
  const workflowPath = mode === "forward"
    ? ".github/workflows/pages.yml"
    : ".github/workflows/pages-rollback.yml";
  const input = {
    mode,
    repository: "daejunnom/Clearra",
    sourceCommit,
    workflowSourceCommit,
    workflowRunId: "22222",
    workflowRunAttempt: "3",
    artifactId: "33333",
    artifactName: "github-pages",
    pageUrl: "https://daejunnom.github.io/Clearra/",
    basePath: "/Clearra",
    acceptedRunId: mode === "forward" ? "11111" : undefined,
    acceptedRunAttempt: mode === "forward" ? "2" : undefined,
    rollbackCaptureReport,
    rollbackCaptureRunId: mode === "restore" ? rollbackCaptureReport.capture_run_id : undefined,
    rollbackReportArtifactId: mode === "restore" ? "44444" : undefined,
    rollbackReportArtifactName: mode === "restore"
      ? expectedCaptureReportArtifactName({
        snapshotSha: rollbackCaptureReport.snapshot_source_commit,
        authoritySha: rollbackCaptureReport.authority_source_commit,
        captureRunId: rollbackCaptureReport.capture_run_id,
        captureRunAttempt: rollbackCaptureReport.capture_run_attempt,
      })
      : undefined,
    rollbackReportArtifactDigest: mode === "restore"
      ? `sha256:${"a".repeat(64)}`
      : undefined,
    rollbackReportFileSha256: mode === "restore" ? "b".repeat(64) : undefined,
  };
  const responses = {
    "/actions/runs/22222": {
      id: 22222,
      run_attempt: 3,
      event: "workflow_dispatch",
      head_branch: "main",
      head_sha: workflowSourceCommit,
      path: workflowPath,
      status: "in_progress",
      conclusion: null,
    },
    "/actions/artifacts/33333": {
      id: 33333,
      name: "github-pages",
      expired: false,
      digest: `sha256:${ARTIFACT_SHA256}`,
      created_at: "2026-08-30T00:00:00Z",
      expires_at: "2026-08-31T00:00:00Z",
      workflow_run: {
        id: 22222,
        head_branch: "main",
        head_sha: workflowSourceCommit,
      },
    },
    "/pages": { html_url: "https://daejunnom.github.io/Clearra" },
    [`/pages/deployments/${workflowSourceCommit}`]: { status: "succeed" },
  };
  if (mode === "restore") {
    responses["/actions/artifacts/44444"] = {
      id: 44444,
      name: input.rollbackReportArtifactName,
      digest: input.rollbackReportArtifactDigest,
      expired: false,
      created_at: "2026-08-28T00:09:30Z",
      expires_at: "2026-11-26T00:09:30Z",
      workflow_run: {
        id: Number(rollbackCaptureReport.capture_run_id),
        head_branch: "main",
        head_sha: AUTHORITY,
      },
    };
  }
  return {
    input,
    responses,
    dependencies: {
      async getGithubJson(path) {
        assert.ok(Object.hasOwn(responses, path), `unexpected GitHub read: ${path}`);
        return structuredClone(responses[path]);
      },
      async fetchPublicJson(url) {
        assert.match(url, /clearra-build-identity\.json\?authority=22222-3-1$/u);
        return mode === "restore"
          ? structuredClone(rollbackCaptureReport.legacy_snapshot.identity)
          : identity(SOURCE);
      },
      async fetchPublicBytes(url, _label, expectedSize) {
        assert.match(url, /\/wasm\/clearra_wasm(?:_bg)?[./]/u);
        const descriptor = url.includes("manifest.json")
          ? LEGACY_PAGES_PAYLOAD.manifest
          : url.includes("_bg.")
            ? LEGACY_PAGES_PAYLOAD.wasm
            : LEGACY_PAGES_PAYLOAD.bindings;
        assert.equal(expectedSize, descriptor.bytes);
        return Buffer.from("fixture");
      },
      validateLegacySnapshot(value) {
        assert.deepEqual(value.identity, rollbackCaptureReport.legacy_snapshot.identity);
        assert.deepEqual(value.expectedIdentity, rollbackCaptureReport.legacy_snapshot.identity);
        assert.equal(value.manifestBytes.toString(), "fixture");
        assert.equal(value.bindingsBytes.toString(), "fixture");
        assert.equal(value.wasmBytes.toString(), "fixture");
      },
      async sleep() {
        assert.fail("happy-path producer must not sleep");
      },
      attempts: 2,
    },
  };
}

test("seals forward artifact, run-attempt, deployment status, and live identity API readbacks", async () => {
  const { input, dependencies } = fixture();
  const report = await producePagesDeploymentAuthority(input, dependencies);
  assert.equal(report.deployment_id, SOURCE);
  assert.equal(report.artifact_sha256, ARTIFACT_SHA256);
  assert.equal(report.accepted_run_id, "11111");
  assert.equal(report.deployment_status, "succeed");
  assert.equal(validatePagesDeploymentAuthorityReport(report, {
    expectedSourceCommit: SOURCE,
  }), report);
  for (const field of [
    "artifact_api_readback_sha256",
    "workflow_run_api_readback_sha256",
    "deployment_api_readback_sha256",
    "pages_configuration_api_readback_sha256",
    "live_identity_sha256",
    "report_sha256",
  ]) {
    assert.match(report[field], /^[0-9a-f]{64}$/u);
  }
  assert.doesNotMatch(JSON.stringify(report), /token|password|credential/iu);
});

test("restore authority derives accepted identity and queries the deploy action workflow SHA", async () => {
  const { input, dependencies } = fixture({ mode: "restore" });
  const report = await producePagesDeploymentAuthority(input, dependencies);
  assert.equal(report.source_commit, LEGACY_PAGES_SNAPSHOT_SHA);
  assert.equal(report.workflow_source_commit, AUTHORITY);
  assert.equal(report.deployment_id, AUTHORITY);
  assert.equal(report.accepted_run_id, null);
  assert.equal(report.workflow_path, ".github/workflows/pages-rollback.yml");
  assert.equal(report.rollback_capture_run_id, "12345");
  assert.equal(report.rollback_report_artifact_id, "44444");
  assert.equal(report.rollback_capture_report_sha256, input.rollbackCaptureReport.report_sha256);
  assert.equal(report.rollback_artifact_sha256, input.rollbackCaptureReport.artifact_sha256);
  assert.equal(report.rollback_tar_sha256, input.rollbackCaptureReport.artifact_tar_sha256);
  assert.equal(report.rollback_report_file_sha256, input.rollbackReportFileSha256);
  assert.equal(report.live_payload_set_sha256, canonicalSha256(LEGACY_PAGES_PAYLOADS));
  validatePagesDeploymentAuthorityReport(report);
});

test("rejects artifact digest, run attempt, deployment status, and public identity drift", async () => {
  {
    const { input, dependencies, responses } = fixture();
    responses["/actions/artifacts/33333"].digest = "legacy-digest";
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /artifact digest has an invalid format/u,
    );
  }
  {
    const { input, dependencies, responses } = fixture();
    responses["/actions/runs/22222"].run_attempt = 4;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /active exact-main attempt/u,
    );
  }
  {
    const { input, dependencies, responses } = fixture();
    responses[`/pages/deployments/${SOURCE}`].status = "queued";
    dependencies.attempts = 1;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /has not converged to succeed/u,
    );
  }
  {
    const { input, dependencies } = fixture();
    dependencies.fetchPublicJson = async () => identity(AUTHORITY);
    dependencies.attempts = 1;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /differs from the deployed source contract/u,
    );
  }
});

test("restore late binding rejects report artifact substitution and public byte drift", async () => {
  {
    const { input, dependencies, responses } = fixture({ mode: "restore" });
    responses["/actions/artifacts/44444"].digest = `sha256:${"c".repeat(64)}`;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /downloaded capture report artifact differs/u,
    );
  }
  {
    const { input, dependencies } = fixture({ mode: "restore" });
    dependencies.validateLegacySnapshot = () => {
      throw new Error("live legacy Pages payload differs from approved bytes");
    };
    dependencies.attempts = 1;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /payload differs from approved bytes/u,
    );
  }
  {
    const { input, dependencies } = fixture({ mode: "restore" });
    input.rollbackCaptureReport = structuredClone(input.rollbackCaptureReport);
    input.rollbackCaptureReport.legacy_snapshot.identity.captureRunId = "99999";
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      /canonical content/u,
    );
  }
});

test("writes one canonical exclusive report file and rejects tampering", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-pages-authority-"));
  try {
    const { input, dependencies } = fixture();
    const report = await producePagesDeploymentAuthority(input, dependencies);
    const path = join(root, "pages-authority.json");
    const fileSha256 = await writePagesDeploymentAuthorityReport(path, report);
    const raw = await readFile(path, "utf8");
    assert.equal(
      fileSha256,
      createHash("sha256").update(raw, "utf8").digest("hex"),
    );
    assert.equal(raw.endsWith("\n"), true);
    await assert.rejects(
      writePagesDeploymentAuthorityReport(path, report),
      /EEXIST/u,
    );
    const tampered = { ...report, artifact_sha256: "f".repeat(64) };
    assert.throws(
      () => validatePagesDeploymentAuthorityReport(tampered),
      /canonical content/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

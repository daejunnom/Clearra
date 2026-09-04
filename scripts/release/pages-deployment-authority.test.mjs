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
  publicBytesReader,
  validatePagesDeploymentAuthorityReport,
  writePagesDeploymentAuthorityReport,
} from "./pages-deployment-authority.mjs";

const SOURCE = "1".repeat(40);
const AUTHORITY = "2".repeat(40);
const LATER_AUTHORITY = "4".repeat(40);
const ARTIFACT_SHA256 = "3".repeat(64);
const FORWARD_PAYLOADS = new Map([
  ["index.html", Buffer.from("accepted index fixture\n", "utf8")],
  ["wasm/clearra_wasm_bg.wasm", Buffer.from([0, 97, 115, 109, 1, 0, 0, 0])],
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function forwardFileDescriptors() {
  return [...FORWARD_PAYLOADS].map(([path, bytes]) => ({
    path,
    size: bytes.byteLength,
    sha256: sha256(bytes),
  }));
}

function identity(sourceCommit = SOURCE, files = forwardFileDescriptors()) {
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
    files,
  };
}

function reconstructedIdentity(sourceCommit = SOURCE) {
  const { acceptedRunId, acceptedRunAttempt, basePath, files, ...reconstructed } =
    identity(sourceCommit);
  void acceptedRunId;
  void acceptedRunAttempt;
  void basePath;
  void files;
  return reconstructed;
}

function modernManifest(sourceCommit = SOURCE) {
  return {
    build: {
      runtime_identity: {
        source_commit: sourceCommit,
        engine_build_id: sourceCommit,
        contract_schema_version: "clearra.search.contract.v2",
        supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
        artifact_schema_version: "clearra.solution-data.v1",
      },
    },
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
    schema_id: "clearra.pages.rollback-capture-authority.v3",
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
    canonical_snapshot: null,
    status: "captured",
  });
}

function modernCaptureReport() {
  const captureRunId = "12345";
  const captureRunAttempt = "1";
  const acceptedIdentity = { ...identity(), acceptedRunAttempt: "1" };
  const identityBytes = Buffer.from(JSON.stringify(acceptedIdentity));
  const readback = {
    identity_sha256: canonicalSha256(acceptedIdentity), identity_bytes_sha256: sha256(identityBytes), identity_bytes_size: identityBytes.byteLength,
    file_set_sha256: canonicalSha256(acceptedIdentity.files), file_count: acceptedIdentity.files.length,
    total_bytes: acceptedIdentity.files.reduce((sum, file) => sum + file.size, 0),
  };
  return sealCanonicalReport({
    schema_id: "clearra.pages.rollback-capture-authority.v3",
    repository: "daejunnom/Clearra",
    snapshot_source_commit: SOURCE,
    authority_source_commit: AUTHORITY,
    capture_run_id: captureRunId,
    capture_run_attempt: captureRunAttempt,
    workflow_path: ".github/workflows/pages-rollback.yml",
    workflow_run_api_readback_sha256: "6".repeat(64),
    artifact_id: "67890",
    artifact_name: expectedCaptureArtifactName({
      snapshotSha: SOURCE,
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
    capture_kind: "canonical-v2",
    legacy_snapshot: null,
    canonical_snapshot: {
      accepted_run_id: "11111", accepted_run_attempt: "1", accepted_artifact_id: "77777",
      accepted_artifact_name: `accepted-pages-build-${SOURCE}-run-11111-attempt-1`, accepted_artifact_digest: `sha256:${"c".repeat(64)}`,
      accepted_artifact_api_readback_sha256: "d".repeat(64), accepted_artifact_created_at: "2026-08-28T00:00:00.000Z", accepted_artifact_expires_at: "2026-11-26T00:00:00.000Z",
      identity: acceptedIdentity, ...readback, initial_public_readback: readback, preartifact_public_readback: readback,
    },
    status: "captured",
  });
}

function fixture({ mode = "forward", restoreKind = "legacy-v0.7.4" } = {}) {
  const rollbackCaptureReport = mode === "restore"
    ? restoreKind === "canonical-v2" ? modernCaptureReport() : legacyCaptureReport()
    : null;
  const sourceCommit = mode === "restore"
    ? rollbackCaptureReport.snapshot_source_commit
    : SOURCE;
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
    restoredCaptureKind: mode === "restore" ? restoreKind : undefined,
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
      async fetchPublicJson(url, label) {
        if (mode === "restore" && restoreKind === "canonical-v2") {
          assert.match(url, /clearra-build-identity\.json\?authority=22222-3-1$/u);
          return structuredClone(rollbackCaptureReport.canonical_snapshot.identity);
        }
        assert.match(url, /clearra-build-identity\.json\?authority=22222-3-1$/u);
        return mode === "restore"
          ? structuredClone(rollbackCaptureReport.legacy_snapshot.identity)
          : identity(SOURCE);
      },
      async fetchPublicBytes(url, _label, expectedSize) {
        if (mode === "forward") {
          const parsed = new URL(url);
          assert.equal(parsed.origin, "https://daejunnom.github.io");
          assert.match(parsed.search, /^\?authority=22222-3-1-[12]$/u);
          const path = parsed.pathname.replace(/^\/Clearra\//u, "");
          const payload = FORWARD_PAYLOADS.get(path);
          assert.ok(payload, `unexpected forward payload read: ${path}`);
          assert.equal(expectedSize, payload.byteLength);
          return Buffer.from(payload);
        }
        if (restoreKind === "canonical-v2") {
          const path = new URL(url).pathname.replace(/^\/Clearra\//u, "");
          return Buffer.from(FORWARD_PAYLOADS.get(path));
        }
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

test("seals forward artifact, deployment, identity, and every live payload byte readback", async () => {
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

test("canonical-v2 restore binds every live file to the accepted rollback package", async () => {
  const { input, dependencies } = fixture({
    mode: "restore",
    restoreKind: "canonical-v2",
  });
  const report = await producePagesDeploymentAuthority(input, dependencies);
  assert.equal(report.source_commit, SOURCE);
  assert.equal(report.accepted_run_id, "11111");
  assert.equal(report.live_payload_set_sha256, canonicalSha256(input.rollbackCaptureReport.canonical_snapshot.identity.files));
  validatePagesDeploymentAuthorityReport(report);

  const drift = fixture({ mode: "restore", restoreKind: "canonical-v2" });
  drift.dependencies.fetchPublicJson = async () => identity(SOURCE);
  drift.dependencies.attempts = 1;
  await assert.rejects(
    producePagesDeploymentAuthority(drift.input, drift.dependencies),
    /differs from the full accepted identity/u,
  );

  const later = fixture({ mode: "restore", restoreKind: "canonical-v2" });
  later.input.workflowSourceCommit = LATER_AUTHORITY;
  later.responses["/actions/runs/22222"].head_sha = LATER_AUTHORITY;
  later.responses["/actions/artifacts/33333"].workflow_run.head_sha = LATER_AUTHORITY;
  delete later.responses[`/pages/deployments/${AUTHORITY}`];
  later.responses[`/pages/deployments/${LATER_AUTHORITY}`] = { status: "succeed" };
  later.responses[`/compare/${AUTHORITY}...${LATER_AUTHORITY}`] = { status: "ahead" };
  const laterReport = await producePagesDeploymentAuthority(
    later.input,
    later.dependencies,
  );
  assert.equal(laterReport.workflow_source_commit, LATER_AUTHORITY);
  assert.equal(laterReport.deployment_id, LATER_AUTHORITY);

  later.responses[`/compare/${AUTHORITY}...${LATER_AUTHORITY}`] = { status: "diverged" };
  await assert.rejects(
    producePagesDeploymentAuthority(later.input, later.dependencies),
    /capture authority must be workflow main or its strict ancestor/u,
  );
});

test("forward live seal rejects missing, redirected, wrong-sized, and hash-drifted payloads", async () => {
  for (const [message, reader, expected] of [
    [
      "missing",
      async () => {
        throw new Error("live Pages payload request failed with HTTP 404");
      },
      /HTTP 404/u,
    ],
    [
      "redirected",
      async () => {
        throw new Error("live Pages payload redirect was rejected");
      },
      /redirect was rejected/u,
    ],
    [
      "wrong-sized",
      async (_url, _label, expectedSize) => Buffer.alloc(expectedSize + 1),
      /exact byte length/u,
    ],
    [
      "hash-drifted",
      async (_url, _label, expectedSize) => Buffer.alloc(expectedSize, 0x78),
      /identity SHA-256/u,
    ],
  ]) {
    const { input, dependencies } = fixture();
    dependencies.fetchPublicBytes = reader;
    dependencies.attempts = 1;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      expected,
      message,
    );
  }
});

test("forward live seal retries partial CDN convergence and only seals one complete generation", async () => {
  const { input, dependencies } = fixture();
  let sleeps = 0;
  let payloadReads = 0;
  dependencies.fetchPublicJson = async (url) => {
    assert.match(url, /clearra-build-identity\.json\?authority=22222-3-[12]$/u);
    return identity(SOURCE);
  };
  dependencies.fetchPublicBytes = async (url, _label, expectedSize) => {
    payloadReads += 1;
    const parsed = new URL(url);
    const path = parsed.pathname.replace(/^\/Clearra\//u, "");
    const payload = FORWARD_PAYLOADS.get(path);
    assert.ok(payload, `unexpected forward payload read: ${path}`);
    assert.equal(expectedSize, payload.byteLength);
    if (parsed.searchParams.get("authority") === "22222-3-1-1") {
      return Buffer.alloc(expectedSize, 0x78);
    }
    return Buffer.from(payload);
  };
  dependencies.sleep = async () => {
    sleeps += 1;
  };
  const report = await producePagesDeploymentAuthority(input, dependencies);
  assert.equal(report.status, "active");
  assert.equal(sleeps, 1);
  assert.equal(payloadReads, 3);
});

test("forward identity manifest rejects duplicate, unsafe, excessive-count, and excessive-byte sets", async () => {
  const descriptor = (path, size = 1) => ({
    path,
    size,
    sha256: "d".repeat(64),
  });
  const rejectFiles = async (files, expected, message) => {
    const { input, dependencies } = fixture();
    dependencies.fetchPublicJson = async () => identity(SOURCE, files);
    dependencies.fetchPublicBytes = async () => {
      assert.fail("invalid identity file sets must fail before public payload reads");
    };
    dependencies.attempts = 1;
    await assert.rejects(
      producePagesDeploymentAuthority(input, dependencies),
      expected,
      message,
    );
  };

  await rejectFiles(
    [descriptor("index.html"), descriptor("index.html")],
    /invalid or unsorted/u,
    "duplicate path",
  );
  await rejectFiles(
    [descriptor("b.js"), descriptor("a.js")],
    /invalid or unsorted/u,
    "unsorted path",
  );
  await rejectFiles(
    [descriptor("assets"), descriptor("assets/entry.js")],
    /invalid or unsorted/u,
    "file and descendant collision",
  );
  await rejectFiles(
    ["a", "a_", "a-", "a.", "a/x"].map((path) => descriptor(path)),
    /invalid or unsorted/u,
    "non-adjacent file and descendant collision",
  );
  for (const path of [
    "../index.html",
    "/index.html",
    "assets//entry.js",
    "assets/./entry.js",
    "assets/%2e%2e/entry.js",
    "assets/entry.js?stale=1",
    "assets\\entry.js",
    "https:entry.js",
    "clearra-build-identity.json",
  ]) {
    await rejectFiles(
      [descriptor(path)],
      /invalid or unsorted/u,
      `unsafe path ${path}`,
    );
  }

  await rejectFiles(
    Array.from({ length: 1_025 }, (_value, index) =>
      descriptor(`assets/${String(index).padStart(4, "0")}.js`, 0)),
    /file count exceeds/u,
    "file count cap",
  );
  await rejectFiles(
    [
      descriptor("a.bin", 32 * 1024 * 1024 + 1),
      descriptor("b.bin", 32 * 1024 * 1024),
    ],
    /payload bytes exceed/u,
    "aggregate byte cap",
  );
  await rejectFiles(
    [descriptor("too-large.bin", 64 * 1024 * 1024 + 1)],
    /invalid or unsorted/u,
    "per-file byte cap",
  );
});

test("public byte reader enforces no-store, no-redirect, exact bounded streaming", async () => {
  const payload = Buffer.from("ok", "utf8");
  const result = await publicBytesReader(
    "https://daejunnom.github.io/Clearra/index.html?authority=fixture",
    "live Pages payload index.html",
    payload.byteLength,
    {
      async fetchImpl(url, options) {
        assert.match(url, /^https:\/\/daejunnom\.github\.io\/Clearra\//u);
        assert.equal(options.method, "GET");
        assert.equal(options.cache, "no-store");
        assert.equal(options.redirect, "error");
        assert.equal(options.headers.Accept, "application/octet-stream");
        assert.ok(options.signal instanceof AbortSignal);
        return new Response(payload, {
          status: 200,
          headers: { "content-length": String(payload.byteLength) },
        });
      },
    },
  );
  assert.deepEqual(result, payload);

  await assert.rejects(
    publicBytesReader("https://example.invalid/missing", "missing payload", 2, {
      async fetchImpl() {
        return new Response("no", { status: 404 });
      },
    }),
    /HTTP 404/u,
  );
  await assert.rejects(
    publicBytesReader("https://example.invalid/redirect", "redirected payload", 2, {
      async fetchImpl(_url, options) {
        assert.equal(options.redirect, "error");
        throw new TypeError("redirect rejected by fetch");
      },
    }),
    /redirect rejected/u,
  );
  await assert.rejects(
    publicBytesReader("https://example.invalid/oversize", "oversized payload", 2, {
      async fetchImpl() {
        return new Response("abc", {
          status: 200,
          headers: { "content-length": "3" },
        });
      },
    }),
    /Content-Length exceeds/u,
  );
  assert.deepEqual(
    await publicBytesReader("https://example.invalid/empty", "empty payload", 0, {
      async fetchImpl() {
        return new Response(new Uint8Array(0), {
          status: 200,
          headers: { "content-length": "0" },
        });
      },
    }),
    Buffer.alloc(0),
  );
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

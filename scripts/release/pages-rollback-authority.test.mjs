import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  canonicalAcceptanceQuery,
  expectedCaptureArtifactName,
  expectedCaptureReportArtifactName,
  produceRollbackCaptureReport,
  readRollbackCaptureReport,
  resolveCaptureReportArtifact,
  validateCanonicalRuns,
  validateCaptureAuthority,
  validateCaptureReportArtifact,
  validatePagesIdentity,
  validateRollbackCaptureReport,
  validateRunAttemptPolicy,
  writeRollbackCaptureReport,
} from "./pages-rollback-authority.mjs";

const SNAPSHOT = "1".repeat(40);
const AUTHORITY = "2".repeat(40);
const ARTIFACT_DIGEST = `sha256:${"3".repeat(64)}`;
const TAR_DIGEST = "4".repeat(64);

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
        created_at: "2026-08-28T00:00:00Z",
        expires_at: "2026-08-29T00:00:00Z",
        workflow_run: { id: 12345, head_branch: "main", head_sha: AUTHORITY },
      },
    };
    const input = {
      repository: "daejunnom/Clearra",
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

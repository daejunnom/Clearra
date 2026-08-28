import assert from "node:assert/strict";
import test from "node:test";
import {
  expectedCaptureArtifactName,
  validateCanonicalRuns,
  validateCaptureAuthority,
  validatePagesIdentity,
  validateRunAttemptPolicy,
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
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
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

test("canonical acceptance permits multiple exact successes and rejects wrong authority", () => {
  validateCanonicalRuns(
    { workflow_runs: [canonicalRun(), canonicalRun({ id: 2 })] },
    SNAPSHOT,
  );
  assert.throws(
    () => validateCanonicalRuns(
      { workflow_runs: [canonicalRun({ event: "push" })] },
      SNAPSHOT,
    ),
    /no successful exact-SHA/u,
  );
  assert.throws(
    () => validateCanonicalRuns(
      { workflow_runs: [canonicalRun({ head_sha: AUTHORITY })] },
      SNAPSHOT,
    ),
    /no successful exact-SHA/u,
  );
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

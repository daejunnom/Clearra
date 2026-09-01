import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  createAcceptanceStageEvidence,
  createDeploymentStageEvidence,
  selectCanonicalDriftEvidencePaths,
  validateFinalSourceStageEvidence,
  validateOracleObservation,
  validateOracleRollbackCapture,
} from "./final-source-stage-evidence.mjs";
import { canonicalSha256, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import { createReleaseGateReports } from "./canonical-acceptance-evidence.mjs";
import { createCanonicalDiscordCatalog } from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";
import { expectedCaptureArtifactName } from "./pages-rollback-authority.mjs";

const COMMIT = "1".repeat(40);
const HASH = "a".repeat(64);
const APP_ID = "223456789012345678";
const RUN_ID = "456";
const RUN_ATTEMPT = "1";

test("acceptance stage hashes LF Git blobs rather than core.autocrlf worktree bytes", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "clearra-final-source-autocrlf-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  runFixtureGit(root, ["init", "--quiet"]);
  runFixtureGit(root, ["config", "core.autocrlf", "true"]);
  runFixtureGit(root, ["config", "core.safecrlf", "false"]);
  runFixtureGit(root, ["config", "user.name", "Clearra Test"]);
  runFixtureGit(root, ["config", "user.email", "clearra-test@example.invalid"]);

  const contracts = join(root, "tests", "fixtures", "contracts");
  await mkdir(contracts, { recursive: true });
  const implementationPath =
    "tests/fixtures/contracts/upstream_drift_implementation_start.v1.json";
  const freezePath =
    "tests/fixtures/contracts/upstream_drift_release_freeze_retry15.v1.json";
  const registryPath =
    "tests/fixtures/contracts/product_capability_registry.v1.json";
  const trackedFiles = new Map([
    [registryPath, {
      schema_id: "clearra.product-capability-registry.v1",
      release_readiness: {
        status: "ready",
        allowed_terminal_implementation_statuses: ["implemented"],
      },
      capability_implementation: [],
      result_affecting_option_exposure: [],
      requirements: [{
        id: "REQ-V080-020",
        implementation_status: "implemented",
        implementation_evidence: [implementationPath, freezePath],
      }],
    }],
    [implementationPath, {
      schema_id: "clearra.upstream-drift-audit.v1",
      phase: "implementation-start",
      status: "no-drift",
    }],
    [freezePath, {
      schema_id: "clearra.upstream-drift-audit.v1",
      phase: "release-freeze",
      status: "no-drift",
    }],
    ["tests/fixtures/contracts/legacy_alias_equivalence.v1.json", {
      schema_id: "clearra.legacy-alias-equivalence.v1",
      status: "passed",
    }],
  ]);
  await writeFile(
    join(root, ".git", "info", "attributes"),
    "*.json text\n*.tsv text\n",
    "utf8",
  );
  for (const [repositoryPath, value] of trackedFiles) {
    await writeFile(join(root, ...repositoryPath.split("/")), `${JSON.stringify(value)}\n`, "utf8");
  }
  await writeFile(
    join(contracts, "search_option_contract.tsv"),
    "option\tresult_affecting\nscore\ttrue\n",
    "utf8",
  );
  runFixtureGit(root, ["add", "--all"]);
  runFixtureGit(root, ["commit", "--quiet", "-m", "fixture"]);
  const commit = runFixtureGit(root, ["rev-parse", "HEAD"]).trim();

  await rm(join(root, "tests"), { recursive: true, force: true });
  runFixtureGit(root, ["read-tree", "--reset", "-u", "HEAD"]);
  assert.equal(runFixtureGit(root, ["status", "--porcelain=v1"]), "");

  const registryWorktreePath = join(root, ...registryPath.split("/"));
  const worktreeBytes = await readFile(registryWorktreePath);
  const blobId = runFixtureGit(root, ["rev-parse", `${commit}:${registryPath}`]).trim();
  const blobBytes = runFixtureGitBytes(root, ["cat-file", "blob", blobId]);
  assert.equal(worktreeBytes.includes(Buffer.from("\r\n")), true);
  assert.equal(blobBytes.includes(Buffer.from("\r\n")), false);
  assert.equal(blobBytes.includes(Buffer.from("\n")), true);
  assert.notEqual(sha256(worktreeBytes), sha256(blobBytes));

  const options = {
    expectedSourceCommit: commit,
    sourceRoot: root,
    acceptanceEvidence: acceptanceEvidence(commit),
    acceptanceEvidenceFileSha256: HASH,
  };
  const dependencies = {
    validateAuditSnapshot() {
      return true;
    },
  };
  const crlfStage = await createAcceptanceStageEvidence(options, dependencies);
  const contractsEvent = crlfStage.events.find(({ kind }) => kind === "contracts");
  const registryInput = crlfStage.producer_inputs.find(
    ({ role }) => role === "product-registry",
  );
  assert.equal(contractsEvent.payload.product_registry_sha256, sha256(blobBytes));
  assert.equal(registryInput.file_sha256, sha256(blobBytes));
  assert.equal(
    crlfStage.events.find(({ kind }) => kind === "source").payload.tree,
    runFixtureGit(root, ["rev-parse", `${commit}^{tree}`]).trim(),
  );

  await writeFile(
    registryWorktreePath,
    Buffer.concat([worktreeBytes, Buffer.from("semantic-change\r\n")]),
  );
  assert.notEqual(runFixtureGit(root, ["status", "--porcelain=v1"]), "");
  await assert.rejects(
    createAcceptanceStageEvidence(options, dependencies),
    /source worktree is not clean/u,
  );
});

test("deployment stage projects only fieldwise-validated actual producer authorities", () => {
  const fixture = deploymentFixture();
  const stage = createDeploymentStageEvidence(fixture.options);
  assert.equal(validateFinalSourceStageEvidence(stage, {
    expectedStage: "deployment",
    expectedSourceCommit: COMMIT,
  }), stage);
  assert.deepEqual(stage.producer_inputs.map(({ role }) => role), [
    "cloud-candidate-smoke",
    "discord-canonical-catalog",
    "discord-catalog-sync",
    "discord-command-sync-authority",
    "discord-prior-snapshot",
    "oracle-candidate-observation",
    "oracle-rollback-capture",
    "pages-deployment",
    "production-observation",
    "production-probe-spec",
    "rollback-snapshot",
  ]);
  assert.deepEqual(stage.events.map(({ kind }) => kind), [
    "deployment-pages",
    "deployment-discord",
    "rollback-snapshot",
    "observation",
  ]);
});

test("deployment stage rejects Pages, Discord, Cloud, and Oracle cross-producer drift", () => {
  for (const mutate of [
    (options) => { options.pagesDeploymentAuthority.artifact_sha256 = "f".repeat(64); },
    (options) => { options.discordCatalogSyncReport.command_sync_authority_sha256 = "f".repeat(64); },
    (options) => { options.cloudCandidateSmokeReport.candidate_revision = "wrong-revision"; },
    (options) => { options.oracleRollbackCapture.deploymentNonce = "f".repeat(64); },
  ]) {
    const fixture = deploymentFixture();
    const options = structuredClone(fixture.options);
    mutate(options);
    assert.throws(
      () => createDeploymentStageEvidence(options),
      /canonical content|differs|revision\/tag/u,
    );
  }
});

test("deployment stage binds the exact 1200-second interval and adapter set to its probe spec", () => {
  {
    const fixture = deploymentFixture();
    const options = structuredClone(fixture.options);
    options.productionProbeSpec.interval_seconds = 1199;
    assert.throws(
      () => createDeploymentStageEvidence(options),
      /probe spec interval is not the exact release window/u,
    );
  }

  {
    const fixture = deploymentFixture();
    const options = structuredClone(fixture.options);
    options.productionProbeSpec.probes[0].sha256 = "b".repeat(64);
    options.productionObservationReport.probe_spec_sha256 = canonicalSha256(
      options.productionProbeSpec,
    );
    const { report_sha256: ignoredReportSha256, ...unsignedObservation } =
      options.productionObservationReport;
    void ignoredReportSha256;
    options.productionObservationReport = sealCanonicalReport(unsignedObservation);
    assert.throws(
      () => createDeploymentStageEvidence(options),
      /observation adapters differ from its exact probe spec/u,
    );
  }
});

test("Oracle capture and observation adapters are closed, source-bound, and secret-free", () => {
  const fixture = deploymentFixture();
  assert.equal(validateOracleRollbackCapture(fixture.options.oracleRollbackCapture), fixture.options.oracleRollbackCapture);
  assert.equal(validateOracleObservation(fixture.options.oracleObservation, {
    expectedSourceCommit: COMMIT,
    cloudCandidateSmokeReport: fixture.options.cloudCandidateSmokeReport,
  }), fixture.options.oracleObservation);
  assert.throws(
    () => validateOracleRollbackCapture({
      ...fixture.options.oracleRollbackCapture,
      credential: "forbidden",
    }),
    /closed schema/u,
  );
  assert.throws(
    () => validateOracleObservation({
      ...fixture.options.oracleObservation,
      sourceCommit: "2".repeat(40),
    }, { expectedSourceCommit: COMMIT }),
    /source differs/u,
  );
  assert.throws(
    () => validateOracleObservation({
      ...fixture.options.oracleObservation,
      verifiedAfter: "2026-08-30T00:02:00.001Z",
    }, { expectedSourceCommit: COMMIT }),
    /verified-after\/fresh operation authority is out of order/u,
  );
});

test("deployment stage binds direct Oracle verified-after authority to production identity", () => {
  const fixture = deploymentFixture();
  const options = structuredClone(fixture.options);
  const oracleSurface = options.productionObservationReport.surfaces.find(
    ({ surface }) => surface === "oracle",
  );
  oracleSurface.identity.verified_after = "2026-08-29T23:59:59.999Z";
  oracleSurface.identity_sha256 = canonicalSha256(oracleSurface.identity);
  for (const observation of oracleSurface.observations) {
    observation.identity_sha256 = oracleSurface.identity_sha256;
    observation.freshness.verified_after = oracleSurface.identity.verified_after;
    observation.freshness.operation_marker = canonicalSha256({
      contract: "clearra.oracle.candidate-observation.v1",
      source_commit: oracleSurface.identity.source_commit,
      candidate_revision: oracleSurface.identity.candidate_revision,
      verified_after: oracleSurface.identity.verified_after,
      fresh_operation_at: observation.freshness.fresh_operation_at,
      observed_at: observation.freshness.observed_at,
    });
  }
  const { report_sha256: ignoredReportSha256, ...unsignedObservation } =
    options.productionObservationReport;
  void ignoredReportSha256;
  options.productionObservationReport = sealCanonicalReport(unsignedObservation);
  assert.throws(
    () => createDeploymentStageEvidence(options),
    /observed Oracle identity differs from its direct read-only authority/u,
  );
});

test("release-freeze selection uses the final registry evidence entry without retry hardcoding", () => {
  const implementation = "tests/fixtures/contracts/upstream_drift_implementation_start.v1.json";
  const retry9 = "tests/fixtures/contracts/upstream_drift_release_freeze_retry9.v1.json";
  const retry15 = "tests/fixtures/contracts/upstream_drift_release_freeze_retry15.v1.json";
  assert.deepEqual(selectCanonicalDriftEvidencePaths({
    requirements: [{
      id: "REQ-V080-020",
      implementation_evidence: [implementation, retry9, retry15],
    }],
  }), {
    implementationStartPath: implementation,
    releaseFreezePath: retry15,
  });
});

function acceptanceEvidence(sourceCommit) {
  const authority = {
    repository: "daejunnom/Clearra",
    version: "0.8.0",
    sourceCommit,
    runId: RUN_ID,
    runAttempt: RUN_ATTEMPT,
    basePath: "/Clearra",
  };
  const reports = createReleaseGateReports(authority, {
    rust: "rustc 1.91.0",
    cargo: "cargo 1.91.0",
    node: "v22.18.0",
    npm: "10.9.3",
    wasm_bindgen: "wasm-bindgen 0.2.126",
    cmake: "cmake version 3.31.0",
    powershell: "5.1.26100.4768",
  });
  return sealCanonicalReport({
    schema_id: "clearra.canonical-acceptance-evidence.v1",
    repository: authority.repository,
    release_version: authority.version,
    pages_base_path: authority.basePath,
    source_commit: sourceCommit,
    run_id: RUN_ID,
    run_attempt: RUN_ATTEMPT,
    workflow_path: ".github/workflows/release-cli.yml",
    status: "passed",
    jobs: [
      "metadata",
      "ctk3",
      "linux-cli",
      "discord-bot",
      "release-acceptance-foundation-no-product-debt",
      "release-acceptance-foundation-adversarial-correctness",
      "release-acceptance-foundation-desktop-host",
      "release-acceptance-sanitizer",
      "release-acceptance-rust",
      "release-acceptance-pages",
      "release-acceptance",
      "windows-cli",
      "windows-gui",
    ].map((name, index) => ({
      name,
      job_id: String(9000 + index),
      status: "passed",
    })),
    accepted_inputs: {
      ctk3_manifest_sha256: "1".repeat(64),
      pages_identity_sha256: "2".repeat(64),
      gate_index_sha256: reports.index.report_sha256,
    },
    final_source_fragments: {
      toolchains: {
        source_commit: sourceCommit,
        manifest_sha256: reports.toolchainManifest.report_sha256,
        rust: reports.toolchainManifest.rust,
        node: reports.toolchainManifest.node,
        wasm_bindgen: reports.toolchainManifest.wasm_bindgen,
      },
      canonical_gate: {
        id: `release-acceptance-run-${RUN_ID}-attempt-${RUN_ATTEMPT}`,
        sha256: reports.gate.report_sha256,
        source_commit: sourceCommit,
        status: "passed",
        readiness_open_count: 0,
      },
      surface_reports: reports.surfaces.map((report) => ({
        id: `${report.surface}-run-${RUN_ID}-attempt-${RUN_ATTEMPT}`,
        sha256: report.report_sha256,
        source_commit: sourceCommit,
        surface: report.surface,
        status: "passed",
      })),
      release_artifacts: [
        ["linux-cli", "Clearra-CLI-v0.8.0-linux-x86_64"],
        ["windows-cli", "Clearra-CLI-v0.8.0-windows-x86_64.exe"],
        ["windows-gui", "Clearra-GUI-v0.8.0-windows-x86_64.exe"],
      ].map(([role, name], index) => ({
        role,
        name,
        sha256: String(index + 3).repeat(64),
        size_bytes: index + 1,
        source_commit: sourceCommit,
      })),
    },
  });
}

function runFixtureGit(cwd, arguments_) {
  const result = spawnSync("git", ["--no-replace-objects", ...arguments_], {
    cwd,
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  if (result.error || result.signal || result.status !== 0) {
    throw new Error(`fixture Git command failed: ${arguments_[0]}`);
  }
  return result.stdout;
}

function runFixtureGitBytes(cwd, arguments_) {
  const result = spawnSync("git", ["--no-replace-objects", ...arguments_], {
    cwd,
    encoding: null,
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  if (result.error || result.signal || result.status !== 0) {
    throw new Error(`fixture Git byte command failed: ${arguments_[0]}`);
  }
  return Buffer.from(result.stdout);
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function deploymentFixture() {
  const catalogFileSha256 = "1".repeat(64);
  const authorityFileSha256 = "2".repeat(64);
  const catalog = createCanonicalDiscordCatalog({
    sourceCommit: COMMIT,
    commands: [{ type: 1, name: "help", description: "Show help" }],
  });
  const priorUnsigned = {
    schema_id: "clearra.discord.command-catalog-snapshot.v1",
    source_commit: COMMIT,
    application_id: APP_ID,
    captured_at: "2026-08-30T00:00:00.000Z",
    command_count: catalog.command_count,
    catalog_sha256: catalog.catalog_sha256,
    commands: catalog.commands,
  };
  const prior = { ...priorUnsigned, snapshot_sha256: canonicalSha256(priorUnsigned) };
  const syncAuthority = sealCanonicalReport({
    schema_id: "clearra.discord.command-sync-authority.v1",
    source_commit: COMMIT,
    repository: "daejunnom/Clearra",
    release_version: "0.8.0",
    pages_base_path: "/Clearra",
    accepted_run_id: RUN_ID,
    accepted_run_attempt: RUN_ATTEMPT,
    accepted_ctk3_manifest_sha256: "3".repeat(64),
    canonical_acceptance_evidence_sha256: "4".repeat(64),
    canonical_acceptance_evidence_file_sha256: "5".repeat(64),
    command_catalog_sha256: catalog.catalog_sha256,
    command_catalog_file_sha256: catalogFileSha256,
  });
  const sync = sealCanonicalReport({
    schema_id: "clearra.discord.command-catalog-sync.v1",
    source_commit: COMMIT,
    application_id: APP_ID,
    started_at: "2026-08-30T00:00:00.000Z",
    ended_at: "2026-08-30T00:00:01.000Z",
    status: "synchronized",
    changed: false,
    command_count: catalog.command_count,
    expected_catalog_sha256: catalog.catalog_sha256,
    accepted_run_id: RUN_ID,
    accepted_run_attempt: RUN_ATTEMPT,
    accepted_ctk3_manifest_sha256: syncAuthority.accepted_ctk3_manifest_sha256,
    canonical_acceptance_evidence_sha256: syncAuthority.canonical_acceptance_evidence_sha256,
    canonical_acceptance_evidence_file_sha256: syncAuthority.canonical_acceptance_evidence_file_sha256,
    command_catalog_file_sha256: catalogFileSha256,
    command_sync_authority_sha256: syncAuthority.report_sha256,
    command_sync_authority_file_sha256: authorityFileSha256,
    prior_snapshot_sha256: prior.snapshot_sha256,
    prior_catalog_sha256: prior.catalog_sha256,
    current_before_sha256: prior.catalog_sha256,
    current_after_sha256: catalog.catalog_sha256,
  });
  const smoke = sealCanonicalReport({
    schema_id: "clearra.cloud.candidate-smoke.v1",
    source_commit: COMMIT,
    project_id: "clearra-prod1",
    region: "asia-northeast1",
    service_name: "clearra-current-job",
    candidate_revision: "clearra-current-job-v080-1111111",
    candidate_tag: "candidate-1111111",
    candidate_url: "https://v080---clearra-current-job.example.run.app",
    image_digest: `sha256:${HASH}`,
    started_at: "2026-08-30T00:01:00.000Z",
    ended_at: "2026-08-30T00:02:00.000Z",
    smoke_job: "clearra-v080-candidate-smoke-1111111",
    execution_name: "execution-1",
    job_id: "candidate-smoke-111111111111-a",
    zero_traffic_verified: true,
    service_readback_sha256: "6".repeat(64),
    revision_readback_sha256: "7".repeat(64),
    execution_readback_sha256: "8".repeat(64),
    solution_set_hash: "cts1:0123456789abcdef",
    status: "passed",
  });
  const pages = pagesAuthority();
  const rollback = pagesRollbackCapture();
  const oracleCapture = oracleRollbackCapture();
  const oracleObservation = oracleObservationAuthority(smoke, oracleCapture.deploymentNonce);
  const spec = probeSpec(smoke, oracleObservation);
  const observation = productionObservation({
    catalog,
    prior,
    sync,
    smoke,
    pages,
    oracleObservation,
    spec,
  });
  return {
    options: {
      expectedSourceCommit: COMMIT,
      pagesDeploymentAuthority: pages,
      pagesDeploymentAuthorityFileSha256: "9".repeat(64),
      pagesRollbackCapture: rollback,
      pagesRollbackCaptureFileSha256: "a".repeat(64),
      discordCatalog: catalog,
      discordCatalogFileSha256: catalogFileSha256,
      discordPriorSnapshot: prior,
      discordPriorSnapshotFileSha256: "b".repeat(64),
      discordCommandSyncAuthority: syncAuthority,
      discordCommandSyncAuthorityFileSha256: authorityFileSha256,
      discordCatalogSyncReport: sync,
      discordCatalogSyncFileSha256: "c".repeat(64),
      cloudCandidateSmokeReport: smoke,
      cloudCandidateSmokeFileSha256: "d".repeat(64),
      oracleRollbackCapture: oracleCapture,
      oracleRollbackCaptureFileSha256: "e".repeat(64),
      oracleObservation,
      oracleObservationFileSha256: "f".repeat(64),
      productionProbeSpec: spec,
      productionProbeSpecFileSha256: "1".repeat(64),
      productionObservationReport: observation,
      productionObservationFileSha256: "2".repeat(64),
    },
  };
}

function pagesAuthority() {
  return sealCanonicalReport({
    schema_id: "clearra.pages.deployment-authority.v2",
    mode: "forward",
    repository: "daejunnom/Clearra",
    source_commit: COMMIT,
    workflow_source_commit: COMMIT,
    workflow_run_id: "22222",
    workflow_run_attempt: "1",
    workflow_path: ".github/workflows/pages.yml",
    accepted_run_id: RUN_ID,
    accepted_run_attempt: RUN_ATTEMPT,
    artifact_id: "33333",
    artifact_name: "github-pages",
    artifact_digest: `sha256:${HASH}`,
    artifact_sha256: HASH,
    artifact_api_readback_sha256: "1".repeat(64),
    workflow_run_api_readback_sha256: "2".repeat(64),
    deployment_id: COMMIT,
    deployment_status: "succeed",
    deployment_api_readback_sha256: "3".repeat(64),
    page_url: "https://daejunnom.github.io/Clearra/",
    base_path: "/Clearra",
    pages_configuration_api_readback_sha256: "4".repeat(64),
    live_identity_sha256: "5".repeat(64),
    live_payload_set_sha256: null,
    rollback_capture_report_sha256: null,
    rollback_artifact_sha256: null,
    rollback_tar_sha256: null,
    rollback_capture_run_id: null,
    rollback_report_artifact_id: null,
    rollback_report_artifact_name: null,
    rollback_report_artifact_digest: null,
    rollback_report_artifact_api_readback_sha256: null,
    rollback_report_file_sha256: null,
    status: "active",
  });
}

function pagesRollbackCapture() {
  const snapshot = "2".repeat(40);
  const captureRunId = "777";
  const captureAttempt = "1";
  return sealCanonicalReport({
    schema_id: "clearra.pages.rollback-capture-authority.v2",
    repository: "daejunnom/Clearra",
    snapshot_source_commit: snapshot,
    authority_source_commit: COMMIT,
    capture_run_id: captureRunId,
    capture_run_attempt: captureAttempt,
    workflow_path: ".github/workflows/pages-rollback.yml",
    workflow_run_api_readback_sha256: "6".repeat(64),
    artifact_id: "778",
    artifact_name: expectedCaptureArtifactName({
      snapshotSha: snapshot,
      authoritySha: COMMIT,
      captureRunId,
      captureRunAttempt: captureAttempt,
    }),
    artifact_digest: `sha256:${"7".repeat(64)}`,
    artifact_sha256: "7".repeat(64),
    artifact_archive_size_bytes: 6_000_000,
    artifact_tar_sha256: "8".repeat(64),
    artifact_tar_size_bytes: 8_000_000,
    artifact_api_readback_sha256: "9".repeat(64),
    artifact_created_at: "2026-08-30T00:00:00.000Z",
    artifact_expires_at: "2026-11-28T00:00:00.000Z",
    retention_seconds: 90 * 24 * 60 * 60,
    capture_kind: "modern-v2",
    legacy_snapshot: null,
    status: "captured",
  });
}

function oracleRollbackCapture() {
  const nonce = "9".repeat(64);
  return {
    priorRevision: "clearra-current-job-v075-042ec21",
    priorOracleRelease: "/opt/clearra/releases/v0.7.5-042ec21",
    priorOracleReleaseId: "v0.7.5-042ec21",
    priorOracleReleaseSha256: "a".repeat(64),
    priorOracleSettingsBackup: `/etc/clearra-gateway/settings.pre-v0.8.0-${nonce}`,
    priorOracleSettingsSha256: "b".repeat(64),
    priorRuntimeAuthorityKind: "clearra.rollback.legacy-health-no-runtime.v1",
    priorRuntimeAuthoritySha256: "c".repeat(64),
    priorJobUrl: "https://prior.example.run.app/jobs",
    deploymentNonce: nonce,
  };
}

function oracleObservationAuthority(smoke, nonce) {
  return {
    contract: "clearra.oracle.candidate-observation.v1",
    sourceCommit: COMMIT,
    candidateUrl: smoke.candidate_url,
    candidateRevision: smoke.candidate_revision,
    jobUrl: `${smoke.candidate_url}/jobs`,
    oracleReleaseId: "v0.8.0-1111111",
    activeReleasePath: "/opt/clearra/releases/v0.8.0-1111111",
    oracleReleaseSha256: "d".repeat(64),
    oracleSettingsSha256: "e".repeat(64),
    deploymentNonce: nonce,
    gatewayPid: 1234,
    gatewayStartMonotonicUsec: 123456789,
    bootId: "12345678-1234-1234-9234-123456789abc",
    readyRecordObserved: true,
    verifiedAfter: "2026-08-30T00:00:00.000Z",
    freshOperationAt: "2026-08-30T00:02:00.000Z",
    observedAt: "2026-08-30T00:02:01.000Z",
    runtimeIdentity: {
      schema: "clearra.runtime.identity.v2",
      sourceCommit: COMMIT,
      engineBuildId: COMMIT,
      contractSchemaVersion: "clearra.search.contract.v2",
      supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
      artifactSchemaVersion: "clearra.solution-data.v1",
    },
  };
}

function probeSpec(smoke, oracle) {
  const adapters = [
    { surface: "cloud", runtime: "node", arguments: [] },
    { surface: "discord", runtime: "node", arguments: [] },
    {
      surface: "oracle",
      runtime: "powershell",
      arguments: [
        "-Operation", "observe-candidate",
        "-ScriptReleaseId", oracle.oracleReleaseId,
        "-ScriptReleaseSha256", oracle.oracleReleaseSha256,
        "-SourceCommit", COMMIT,
        "-CandidateUrl", smoke.candidate_url,
        "-CandidateRevision", smoke.candidate_revision,
        "-OracleReleaseId", oracle.oracleReleaseId,
        "-OracleReleaseSha256", oracle.oracleReleaseSha256,
        "-OracleSettingsSha256", oracle.oracleSettingsSha256,
        "-DeploymentNonce", oracle.deploymentNonce,
        "-VerifiedAfter", "2026-08-30T00:00:00.000Z",
      ],
    },
    { surface: "pages", runtime: "node", arguments: [] },
  ];
  return {
    schema_id: "clearra.production-observation-probe-spec.v1",
    source_commit: COMMIT,
    interval_seconds: 1200,
    probes: adapters.map((adapter) => ({
      ...adapter,
      path: resolve("test-fixtures", `${adapter.surface}-probe`),
      sha256: HASH,
      timeout_seconds: 30,
    })),
  };
}

function productionObservation({ catalog, prior, sync, smoke, pages, oracleObservation, spec }) {
  const identities = {
    cloud: {
      source_commit: COMMIT, engine_build_id: COMMIT, revision: smoke.candidate_revision,
      image_digest: smoke.image_digest, traffic_percent: 100, cpu: "8", memory: "16Gi",
      concurrency: 1, min_instances: 0, max_instances: 4, startup_cpu_boost: true,
      contract_schema_version: "clearra.search.contract.v2",
      supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
      artifact_schema_version: "clearra.solution-data.v1",
      job_smoke_report_sha256: smoke.report_sha256,
      stable_url: "https://clearra-current-job.example.run.app/",
      tagged_url: `${smoke.candidate_url}/`, status: "active",
    },
    discord: {
      source_commit: COMMIT, application_id: APP_ID,
      command_catalog_sha256: catalog.catalog_sha256,
      command_catalog_prior_snapshot_sha256: prior.snapshot_sha256,
      command_catalog_readback_sha256: sync.current_after_sha256,
      command_catalog_sync_report_sha256: sync.report_sha256,
      accepted_run_id: sync.accepted_run_id,
      accepted_run_attempt: sync.accepted_run_attempt,
      accepted_ctk3_manifest_sha256: sync.accepted_ctk3_manifest_sha256,
      canonical_acceptance_evidence_sha256: sync.canonical_acceptance_evidence_sha256,
      canonical_acceptance_evidence_file_sha256: sync.canonical_acceptance_evidence_file_sha256,
      command_catalog_file_sha256: sync.command_catalog_file_sha256,
      command_sync_authority_sha256: sync.command_sync_authority_sha256,
      command_sync_authority_file_sha256: sync.command_sync_authority_file_sha256,
      command_count: 1, command_names: ["1:help"], status: "active",
    },
    oracle: {
      source_commit: COMMIT, release_id: oracleObservation.oracleReleaseId,
      release_tree_sha256: oracleObservation.oracleReleaseSha256,
      settings_sha256: oracleObservation.oracleSettingsSha256,
      candidate_revision: oracleObservation.candidateRevision,
      candidate_url: `${oracleObservation.candidateUrl}/`, job_url: oracleObservation.jobUrl,
      deployment_nonce: oracleObservation.deploymentNonce,
      gateway_pid: oracleObservation.gatewayPid,
      gateway_start_monotonic_usec: oracleObservation.gatewayStartMonotonicUsec,
      boot_id: oracleObservation.bootId, ready_record_observed: true,
      verified_after: oracleObservation.verifiedAfter, status: "active",
    },
    pages: {
      source_commit: COMMIT, engine_build_id: COMMIT, version: "0.8.0",
      deployment_id: pages.deployment_id, artifact_sha256: pages.artifact_sha256,
      base_path: pages.base_path, url: pages.page_url, status: "active",
    },
  };
  return sealCanonicalReport({
    schema_id: "clearra.production-observation.v1",
    source_commit: COMMIT,
    started_at: "2026-08-30T00:00:00.000Z",
    ended_at: "2026-08-30T00:20:00.000Z",
    duration_seconds: 1200,
    interval_seconds: 1200,
    probe_spec_sha256: canonicalSha256(spec),
    probe_adapters: ["cloud", "discord", "oracle", "pages"].map((surface) => ({ surface, sha256: HASH })),
    status: "passed",
    surfaces: ["cloud", "discord", "oracle", "pages"].map((surface) => {
      const identity = identities[surface];
      const identitySha256 = canonicalSha256(identity);
      return {
        surface, identity, identity_sha256: identitySha256, observation_count: 2,
        observations: [0, 1].map((sequence) => ({
          sequence,
          observed_at: sequence === 0 ? "2026-08-30T00:00:00.000Z" : "2026-08-30T00:20:00.000Z",
          identity_sha256: identitySha256,
          freshness: freshness(surface, identity, sequence),
        })),
      };
    }),
  });
}

function freshness(surface, identity, sequence) {
  const observedAt = sequence === 0 ? "2026-08-30T00:00:00.000Z" : "2026-08-30T00:20:00.000Z";
  const probeId = (sequence + 1).toString(16).padStart(64, "0");
  if (surface === "oracle") {
    return {
      operation_marker: canonicalSha256({
        contract: "clearra.oracle.candidate-observation.v1",
        source_commit: identity.source_commit,
        candidate_revision: identity.candidate_revision,
        verified_after: identity.verified_after,
        fresh_operation_at: observedAt,
        observed_at: observedAt,
      }),
      verified_after: identity.verified_after,
      fresh_operation_at: observedAt,
      observed_at: observedAt,
    };
  }
  if (surface === "discord") return { probe_id: probeId, readback_sha256: identity.command_catalog_readback_sha256 };
  if (surface === "cloud") {
    return {
      probe_id: probeId,
      service_readback_sha256: "1".repeat(64),
      revision_readback_sha256: "2".repeat(64),
      stable_health_sha256: "3".repeat(64),
      tagged_health_sha256: "4".repeat(64),
    };
  }
  return {
    probe_id: probeId,
    deployment_readback_sha256: "7".repeat(64),
    identity_readback_sha256: "8".repeat(64),
  };
}

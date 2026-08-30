import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  createCanonicalDiscordCatalog,
  normalizeDiscordCatalog,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";
import {
  canonicalJson,
  canonicalSha256,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";
import {
  probeCloudProductionSurface,
  probeDiscordProductionSurface,
  probePagesProductionSurface,
} from "./production-surface-probe-adapter.mjs";

const COMMIT = "1".repeat(40);
const HASH = "a".repeat(64);
const APPLICATION_ID = "223456789012345678";
const IMAGE_DIGEST = `sha256:${"f".repeat(64)}`;
const NOW = "2026-08-30T00:00:10.000Z";

function fileSha256(value) {
  return createHash("sha256")
    .update(`${canonicalJson(value)}\n`, "utf8")
    .digest("hex");
}

test("Discord adapter performs one independent GET and binds the sealed catalog reports", async () => {
  const commands = [
    { name: "help", description: "Show help" },
    { type: 3, name: "Get original GIF" },
  ];
  const catalog = createCanonicalDiscordCatalog({ sourceCommit: COMMIT, commands });
  const readback = commands.map((command, index) => ({
    ...structuredClone(command),
    type: command.type ?? 1,
    application_id: APPLICATION_ID,
    id: String(300000000000000000n + BigInt(index)),
    version: String(400000000000000000n + BigInt(index)),
  }));
  const readbackSha256 = canonicalSha256(normalizeDiscordCatalog(readback, {
    allowResponseMetadata: true,
  }));
  const catalogFileSha256 = fileSha256(catalog);
  const syncAuthority = sealCanonicalReport({
    schema_id: "clearra.discord.command-sync-authority.v1",
    source_commit: COMMIT,
    repository: "daejunnom/Clearra",
    release_version: "0.8.0",
    pages_base_path: "/Clearra",
    accepted_run_id: "123456789",
    accepted_run_attempt: "2",
    accepted_ctk3_manifest_sha256: "2".repeat(64),
    canonical_acceptance_evidence_sha256: "3".repeat(64),
    canonical_acceptance_evidence_file_sha256: "4".repeat(64),
    command_catalog_sha256: catalog.catalog_sha256,
    command_catalog_file_sha256: catalogFileSha256,
  });
  const syncAuthorityFileSha256 = fileSha256(syncAuthority);
  const syncReport = sealCanonicalReport({
    schema_id: "clearra.discord.command-catalog-sync.v1",
    source_commit: COMMIT,
    application_id: APPLICATION_ID,
    started_at: "2026-08-30T00:00:00.000Z",
    ended_at: "2026-08-30T00:00:01.000Z",
    status: "synchronized",
    changed: true,
    command_count: 2,
    expected_catalog_sha256: catalog.catalog_sha256,
    accepted_run_id: syncAuthority.accepted_run_id,
    accepted_run_attempt: syncAuthority.accepted_run_attempt,
    accepted_ctk3_manifest_sha256: syncAuthority.accepted_ctk3_manifest_sha256,
    canonical_acceptance_evidence_sha256:
      syncAuthority.canonical_acceptance_evidence_sha256,
    canonical_acceptance_evidence_file_sha256:
      syncAuthority.canonical_acceptance_evidence_file_sha256,
    command_catalog_file_sha256: catalogFileSha256,
    command_sync_authority_sha256: syncAuthority.report_sha256,
    command_sync_authority_file_sha256: syncAuthorityFileSha256,
    prior_snapshot_sha256: "8".repeat(64),
    prior_catalog_sha256: "9".repeat(64),
    current_before_sha256: "9".repeat(64),
    current_after_sha256: readbackSha256,
  });
  let calls = 0;
  const result = await probeDiscordProductionSurface({
    sourceCommit: COMMIT,
    applicationId: APPLICATION_ID,
    catalog,
    catalogFileSha256,
    syncAuthority,
    syncAuthorityFileSha256,
    syncReport,
    sequence: 4,
    rest: {
      async getGlobalCommands() {
        calls += 1;
        return structuredClone(readback);
      },
    },
    now: () => NOW,
  });

  assert.equal(calls, 1);
  assert.equal(result.surface, "discord");
  assert.equal(result.identity.command_catalog_readback_sha256, readbackSha256);
  assert.equal(result.identity.command_catalog_prior_snapshot_sha256, "8".repeat(64));
  for (const field of [
    "accepted_run_id",
    "accepted_run_attempt",
    "accepted_ctk3_manifest_sha256",
    "canonical_acceptance_evidence_sha256",
    "canonical_acceptance_evidence_file_sha256",
    "command_catalog_file_sha256",
    "command_sync_authority_sha256",
    "command_sync_authority_file_sha256",
  ]) {
    assert.equal(result.identity[field], syncReport[field]);
  }
  assert.deepEqual(result.identity.command_names, ["1:help", "3:Get original GIF"]);
  assert.doesNotMatch(JSON.stringify(result), /token|secret|password/iu);

  const stale = structuredClone(syncReport);
  stale.current_after_sha256 = "7".repeat(64);
  stale.report_sha256 = canonicalSha256((({ report_sha256, ...unsigned }) => unsigned)(stale));
  await assert.rejects(
    probeDiscordProductionSurface({
      sourceCommit: COMMIT,
      applicationId: APPLICATION_ID,
      catalog,
      catalogFileSha256,
      syncAuthority,
      syncAuthorityFileSha256,
      syncReport: stale,
      sequence: 5,
      rest: { async getGlobalCommands() { return structuredClone(readback); } },
      now: () => NOW,
    }),
    /differs from the sealed sync readback/u,
  );

  await assert.rejects(
    probeDiscordProductionSurface({
      sourceCommit: COMMIT,
      applicationId: APPLICATION_ID,
      catalog,
      catalogFileSha256,
      syncAuthority,
      syncAuthorityFileSha256: "f".repeat(64),
      syncReport,
      sequence: 6,
      rest: { async getGlobalCommands() { return structuredClone(readback); } },
      now: () => NOW,
    }),
    /differs from the command sync authority file bytes/u,
  );
});

test("Cloud adapter binds service/revision control plane and both existing health URLs", async () => {
  const service = cloudServiceFixture();
  const revision = cloudRevisionFixture();
  const health = cloudHealthFixture();
  const controlCalls = [];
  const fetchCalls = [];
  const result = await probeCloudProductionSurface({
    sourceCommit: COMMIT,
    projectId: "clearra-prod1",
    region: "asia-northeast1",
    serviceName: "clearra-current-job",
    revision: "clearra-current-job-v080-1111111",
    tag: "candidate-1111111",
    imageDigest: IMAGE_DIGEST,
    smokeReport: cloudSmokeReportFixture(),
    sequence: 2,
    async runControlPlane(request) {
      controlCalls.push(request);
      return structuredClone(request.kind === "service" ? service : revision);
    },
    async fetchJson(url) {
      fetchCalls.push(url);
      return structuredClone(health);
    },
    now: () => NOW,
  });

  assert.deepEqual(controlCalls.map(({ kind }) => kind), ["service", "revision"]);
  assert.equal(fetchCalls.length, 2);
  assert.match(fetchCalls[0], /\/health\?source=/u);
  assert.equal(result.identity.revision, "clearra-current-job-v080-1111111");
  assert.equal(result.identity.image_digest, IMAGE_DIGEST);
  assert.equal(result.identity.traffic_percent, 100);
  assert.equal(result.identity.startup_cpu_boost, true);
  assert.equal(
    result.identity.job_smoke_report_sha256,
    cloudSmokeReportFixture().report_sha256,
  );

  const drifted = cloudRevisionFixture();
  drifted.spec.containers[0].resources.limits.cpu = "4";
  await assert.rejects(
    probeCloudProductionSurface({
      sourceCommit: COMMIT,
      projectId: "clearra-prod1",
      region: "asia-northeast1",
      serviceName: "clearra-current-job",
      revision: "clearra-current-job-v080-1111111",
      tag: "candidate-1111111",
      imageDigest: IMAGE_DIGEST,
      smokeReport: cloudSmokeReportFixture(),
      sequence: 3,
      async runControlPlane(request) {
        return structuredClone(request.kind === "service" ? service : drifted);
      },
      async fetchJson() { return structuredClone(health); },
      now: () => NOW,
    }),
    /resource contract differs/u,
  );
});

test("Cloud adapter rejects a smoke report without managed execution authority", async () => {
  const legacy = cloudSmokeReportFixture();
  const { execution_readback_sha256, report_sha256, ...unsignedLegacy } = legacy;
  const resealedLegacy = sealCanonicalReport(unsignedLegacy);
  assert.equal(typeof execution_readback_sha256, "string");
  assert.equal(typeof report_sha256, "string");

  await assert.rejects(
    probeCloudProductionSurface({
      sourceCommit: COMMIT,
      projectId: "clearra-prod1",
      region: "asia-northeast1",
      serviceName: "clearra-current-job",
      revision: "clearra-current-job-v080-1111111",
      tag: "candidate-1111111",
      imageDigest: IMAGE_DIGEST,
      smokeReport: resealedLegacy,
      sequence: 4,
      async runControlPlane(request) {
        return structuredClone(
          request.kind === "service" ? cloudServiceFixture() : cloudRevisionFixture(),
        );
      },
      async fetchJson() { return cloudHealthFixture(); },
      now: () => NOW,
    }),
    /fields differ from the closed schema/u,
  );
});

test("Pages adapter validates the sealed report, live deployment status, and accepted-build identity", async () => {
  const readback = pagesIdentityFixture();
  const deploymentReport = pagesDeploymentReportFixture(readback);
  const calls = [];
  let deploymentReads = 0;
  const result = await probePagesProductionSurface({
    sourceCommit: COMMIT,
    deploymentReport,
    sequence: 8,
    async fetchJson(url) {
      calls.push(url);
      return structuredClone(readback);
    },
    async fetchDeploymentStatus(report) {
      deploymentReads += 1;
      assert.equal(report.deployment_id, COMMIT);
      return { status: "succeed" };
    },
    now: () => NOW,
  });

  assert.equal(calls.length, 1);
  assert.equal(deploymentReads, 1);
  assert.match(calls[0], /clearra-build-identity\.json\?source=/u);
  assert.equal(result.identity.source_commit, COMMIT);
  assert.equal(result.identity.deployment_id, COMMIT);
  assert.equal(
    result.freshness.identity_readback_sha256,
    canonicalSha256(readback),
  );

  const mixed = pagesIdentityFixture();
  mixed.acceptedRunAttempt = "3";
  await assert.rejects(
    probePagesProductionSurface({
      sourceCommit: COMMIT,
      deploymentReport,
      sequence: 9,
      async fetchJson() { return mixed; },
      async fetchDeploymentStatus() { return { status: "succeed" }; },
      now: () => NOW,
    }),
    /differs from accepted deployment authority/u,
  );

  await assert.rejects(
    probePagesProductionSurface({
      sourceCommit: COMMIT,
      deploymentReport,
      sequence: 10,
      async fetchJson() { return pagesIdentityFixture(); },
      async fetchDeploymentStatus() { return { status: "failed" }; },
      now: () => NOW,
    }),
    /status is not succeed/u,
  );
});

function cloudServiceFixture() {
  return {
    metadata: {
      name: "clearra-current-job",
      annotations: {
        "run.googleapis.com/minScale": "0",
        "run.googleapis.com/maxScale": "4",
      },
    },
    spec: {
      template: {
        metadata: {
          annotations: { "run.googleapis.com/startup-cpu-boost": "true" },
        },
        spec: {
          containerConcurrency: 1,
          containers: [{
            image: `asia-northeast1-docker.pkg.dev/clearra-prod1/repo/image@${IMAGE_DIGEST}`,
            resources: { limits: { cpu: "8", memory: "16Gi" } },
          }],
        },
      },
    },
    status: {
      url: "https://clearra-current-job.example.run.app/",
      traffic: [
        { percent: 100, revisionName: "clearra-current-job-v080-1111111" },
        {
          percent: 0,
          revisionName: "clearra-current-job-v080-1111111",
          tag: "candidate-1111111",
          url: "https://candidate-1111111---clearra-current-job.example.run.app/",
        },
      ],
    },
  };
}

function cloudRevisionFixture() {
  return {
    metadata: {
      name: "clearra-current-job-v080-1111111",
      annotations: {
        "autoscaling.knative.dev/minScale": "0",
        "autoscaling.knative.dev/maxScale": "4",
        "run.googleapis.com/startup-cpu-boost": "true",
      },
    },
    spec: {
      containerConcurrency: 1,
      containers: [{
        image: `asia-northeast1-docker.pkg.dev/clearra-prod1/repo/image@${IMAGE_DIGEST}`,
        resources: { limits: { cpu: "8", memory: "16Gi" } },
      }],
    },
    status: {
      imageDigest: `asia-northeast1-docker.pkg.dev/clearra-prod1/repo/image@${IMAGE_DIGEST}`,
      conditions: [{ type: "Ready", status: "True" }],
    },
  };
}

function cloudHealthFixture() {
  return {
    status: "ok",
    activeJobs: 0,
    workerLimit: 8,
    runtime: {
      schema: "clearra.runtime.identity.v2",
      sourceCommit: COMMIT,
      engineBuildId: COMMIT,
      contractSchemaVersion: "clearra.search.contract.v2",
      supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
      artifactSchemaVersion: "clearra.solution-data.v1",
    },
  };
}

function cloudSmokeReportFixture() {
  return sealCanonicalReport({
    schema_id: "clearra.cloud.candidate-smoke.v1",
    source_commit: COMMIT,
    project_id: "clearra-prod1",
    region: "asia-northeast1",
    service_name: "clearra-current-job",
    candidate_revision: "clearra-current-job-v080-1111111",
    candidate_tag: "candidate-1111111",
    candidate_url: "https://candidate-1111111---clearra-current-job.example.run.app",
    image_digest: IMAGE_DIGEST,
    started_at: "2026-08-30T00:00:00.000Z",
    ended_at: "2026-08-30T00:00:01.000Z",
    smoke_job: "clearra-v080-candidate-smoke-1111111",
    execution_name: "clearra-v080-candidate-smoke-1111111-abcde",
    job_id: "candidate-smoke-111111111111-rs",
    zero_traffic_verified: true,
    service_readback_sha256: "6".repeat(64),
    revision_readback_sha256: "7".repeat(64),
    execution_readback_sha256: "8".repeat(64),
    solution_set_hash: "cts1:0000000000000000",
    status: "passed",
  });
}

function pagesIdentityFixture() {
  return {
    schema: "clearra.pages.identity.v2",
    sourceCommit: COMMIT,
    engineBuildId: COMMIT,
    contractSchemaVersion: "clearra.search.contract.v2",
    supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
    artifactSchemaVersion: "clearra.solution-data.v1",
    version: "0.8.0",
    acceptedRunId: "123456789",
    acceptedRunAttempt: "2",
    basePath: "/Clearra",
    files: [
      { path: "index.html", size: 128, sha256: "1".repeat(64) },
      { path: "wasm/clearra_wasm_bg.wasm", size: 256, sha256: "2".repeat(64) },
    ],
  };
}

function pagesDeploymentReportFixture(liveIdentity = pagesIdentityFixture()) {
  return sealCanonicalReport({
    schema_id: "clearra.pages.deployment-authority.v1",
    mode: "forward",
    repository: "daejunnom/Clearra",
    source_commit: COMMIT,
    workflow_source_commit: COMMIT,
    workflow_run_id: "22222",
    workflow_run_attempt: "1",
    workflow_path: ".github/workflows/pages.yml",
    accepted_run_id: "123456789",
    accepted_run_attempt: "2",
    artifact_id: "33333",
    artifact_name: "github-pages",
    artifact_digest: `sha256:${HASH}`,
    artifact_sha256: HASH,
    artifact_api_readback_sha256: "1".repeat(64),
    workflow_run_api_readback_sha256: "2".repeat(64),
    deployment_id: COMMIT,
    deployment_status: "succeed",
    deployment_api_readback_sha256: canonicalSha256({ status: "succeed" }),
    page_url: "https://daejunnom.github.io/Clearra/",
    base_path: "/Clearra",
    pages_configuration_api_readback_sha256: "4".repeat(64),
    live_identity_sha256: canonicalSha256(liveIdentity),
    status: "active",
  });
}

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  validateFinalSourceRevalidation,
  validateFinalSourceRevalidationFromStages,
} from "./validate-final-source-revalidation.mjs";
import {
  canonicalSha256,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";

const COMMIT = "1".repeat(40);
const HASH = "a".repeat(64);

test("direct execution fails closed because journal materialization is the sole production entrypoint", () => {
  const script = fileURLToPath(new URL(
    "./validate-final-source-revalidation.mjs",
    import.meta.url,
  ));
  const result = spawnSync(process.execPath, [script, "--manifest", "forged.json"], {
    encoding: "utf8",
    shell: false,
    windowsHide: true,
  });
  assert.equal(result.status, 2, result.stderr);
  assert.match(result.stderr, /library-only.*final-source-attempt-journal/u);
  assert.equal(result.stdout, "");
});

test("final-source validation keeps Discord runtime packages outside its import closure", () => {
  const catalogSource = readFileSync(
    new URL(
      "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs",
      import.meta.url,
    ),
    "utf8",
  );
  const firstExport = catalogSource.indexOf("export const DISCORD_CATALOG_SCHEMA_ID");
  assert.ok(firstExport > 0, "Discord catalog schema export must remain present");
  const staticImportPrefix = catalogSource.slice(0, firstExport);
  assert.doesNotMatch(staticImportPrefix, /\.\.\/src\/discord\//u);
  assert.doesNotMatch(staticImportPrefix, /(?:^|[^A-Za-z0-9_-])ctk3(?:[^A-Za-z0-9_-]|$)/u);
});

function evidence(id, extra = {}) {
  return { id, sha256: HASH, source_commit: COMMIT, ...extra };
}

function validBundle() {
  const discordCatalogSyncReport = sealCanonicalReport({
    schema_id: "clearra.discord.command-catalog-sync.v1",
    source_commit: COMMIT,
    application_id: "223456789012345678",
    started_at: "2026-08-27T00:00:00.000Z",
    ended_at: "2026-08-27T00:00:01.000Z",
    status: "synchronized",
    changed: true,
    command_count: 2,
    expected_catalog_sha256: HASH,
    accepted_run_id: "456",
    accepted_run_attempt: "1",
    accepted_ctk3_manifest_sha256: "1".repeat(64),
    canonical_acceptance_evidence_sha256: "2".repeat(64),
    canonical_acceptance_evidence_file_sha256: "3".repeat(64),
    command_catalog_file_sha256: "4".repeat(64),
    command_sync_authority_sha256: "5".repeat(64),
    command_sync_authority_file_sha256: "6".repeat(64),
    prior_catalog_sha256: "b".repeat(64),
    prior_snapshot_sha256: "8".repeat(64),
    current_before_sha256: "b".repeat(64),
    current_after_sha256: "c".repeat(64),
  });
  const productionObservationReport = productionObservation(
    discordCatalogSyncReport.report_sha256,
  );
  const manifest = {
    schema_id: "clearra.final-source-revalidation.v1",
    release: "v0.8.0",
    source: {
      commit: COMMIT,
      tree: "2".repeat(40),
      branch: "main",
      worktree_clean: true,
      engine_build_id: COMMIT,
    },
    contracts: {
      source_commit: COMMIT,
      product_registry_schema_id: "clearra.product-capability-registry.v1",
      product_registry_sha256: HASH,
      search_option_contract_sha256: HASH,
      legacy_alias_contract_sha256: HASH,
      ctk3_contract_sha256: HASH,
      readiness_open_count: 0,
    },
    toolchains: {
      source_commit: COMMIT,
      manifest_sha256: HASH,
      rust: "rustc 1.89.0",
      node: "v22.0.0",
      wasm_bindgen: "0.2.126",
    },
    drift_audits: [
      evidence("implementation-start", { phase: "implementation-start", status: "no-drift" }),
      evidence("release-freeze", { phase: "release-freeze", status: "no-drift" }),
    ],
    canonical_gate: evidence("release-acceptance", {
      status: "passed",
      readiness_open_count: 0,
    }),
    surface_reports: [
      evidence("desktop-report", { surface: "desktop", status: "passed" }),
      evidence("discord-report", { surface: "discord", status: "passed" }),
      evidence("native-report", { surface: "native", status: "passed" }),
      evidence("wasm-report", { surface: "wasm", status: "passed" }),
    ],
    release_artifacts: [
      { role: "linux-cli", name: "Clearra-CLI-v0.8.0-linux-x86_64", sha256: HASH, size_bytes: 1, source_commit: COMMIT },
      { role: "windows-cli", name: "Clearra-CLI-v0.8.0-windows-x86_64.exe", sha256: HASH, size_bytes: 2, source_commit: COMMIT },
      { role: "windows-gui", name: "Clearra-GUI-v0.8.0-windows-x86_64.exe", sha256: HASH, size_bytes: 3, source_commit: COMMIT },
    ],
    deployment: {
      pages: {
        source_commit: COMMIT,
        deployment_id: "pages-1",
        artifact_sha256: HASH,
        status: "active",
      },
      discord: {
        source_commit: COMMIT,
        application_id: "223456789012345678",
        image_digest: `sha256:${HASH}`,
        job_revision: "job-1",
        oracle_revision: "oracle-1",
        oracle_release_sha256: "d".repeat(64),
        oracle_settings_sha256: "e".repeat(64),
        traffic_percent: 100,
        command_catalog_sha256: HASH,
        command_catalog_prior_snapshot_sha256: "8".repeat(64),
        command_catalog_readback_sha256: "c".repeat(64),
        command_catalog_sync_report_sha256:
          discordCatalogSyncReport.report_sha256,
        catalog_synced: true,
        status: "active",
      },
      rollback_snapshot: evidence("rollback", { status: "captured" }),
    },
    observation: {
      report_schema_id: "clearra.production-observation.v1",
      source_commit: COMMIT,
      started_at: "2026-08-27T00:00:00.000Z",
      ended_at: "2026-08-27T00:20:00.000Z",
      duration_seconds: 1200,
      probe_spec_sha256: "6".repeat(64),
      status: "passed",
      report_sha256: productionObservationReport.report_sha256,
    },
    tag: {
      name: "v0.8.0",
      target_commit: COMMIT,
      annotated: true,
      remote_verified: true,
    },
    immutable_release: {
      tag: "v0.8.0",
      source_commit: COMMIT,
      workflow_run_id: "123",
      immutable: true,
      asset_count: 3,
      status: "published",
    },
  };
  return { manifest, discordCatalogSyncReport, productionObservationReport };
}

function validateBundle(bundle, options = {}) {
  return validateFinalSourceRevalidation(bundle.manifest, {
    discordCatalogSyncReport: bundle.discordCatalogSyncReport,
    productionObservationReport: bundle.productionObservationReport,
    ...options,
  });
}

function productionObservation(catalogSyncReportSha256) {
  const identities = {
    cloud: {
      source_commit: COMMIT,
      engine_build_id: COMMIT,
      revision: "job-1",
      image_digest: `sha256:${HASH}`,
      traffic_percent: 100,
      cpu: "8",
      memory: "16Gi",
      concurrency: 1,
      min_instances: 0,
      max_instances: 4,
      startup_cpu_boost: true,
      contract_schema_version: "clearra.search.contract.v2",
      supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
      artifact_schema_version: "clearra.solution-data.v1",
      job_smoke_report_sha256: "7".repeat(64),
      stable_url: "https://clearra-current-job.example.run.app/",
      tagged_url: "https://v080---clearra-current-job.example.run.app/",
      status: "active",
    },
    discord: {
      source_commit: COMMIT,
      application_id: "223456789012345678",
      command_catalog_sha256: HASH,
      command_catalog_prior_snapshot_sha256: "8".repeat(64),
      command_catalog_readback_sha256: "c".repeat(64),
      command_catalog_sync_report_sha256: catalogSyncReportSha256,
      accepted_run_id: "456",
      accepted_run_attempt: "1",
      accepted_ctk3_manifest_sha256: "1".repeat(64),
      canonical_acceptance_evidence_sha256: "2".repeat(64),
      canonical_acceptance_evidence_file_sha256: "3".repeat(64),
      command_catalog_file_sha256: "4".repeat(64),
      command_sync_authority_sha256: "5".repeat(64),
      command_sync_authority_file_sha256: "6".repeat(64),
      command_count: 2,
      command_names: ["1:help", "3:Get original GIF"],
      status: "active",
    },
    oracle: {
      source_commit: COMMIT,
      release_id: "oracle-1",
      release_tree_sha256: "d".repeat(64),
      settings_sha256: "e".repeat(64),
      candidate_revision: "job-1",
      candidate_url: "https://v080---clearra-current-job.example.run.app/",
      job_url: "https://v080---clearra-current-job.example.run.app/jobs",
      deployment_nonce: "9".repeat(64),
      gateway_pid: 1234,
      gateway_start_monotonic_usec: 123456789,
      boot_id: "12345678-1234-1234-1234-123456789abc",
      ready_record_observed: true,
      status: "active",
    },
    pages: {
      source_commit: COMMIT,
      engine_build_id: COMMIT,
      version: "0.8.0",
      deployment_id: "pages-1",
      artifact_sha256: HASH,
      base_path: "/Clearra",
      url: "https://daejunnom.github.io/Clearra/",
      status: "active",
    },
  };
  return sealCanonicalReport({
    schema_id: "clearra.production-observation.v1",
    source_commit: COMMIT,
    started_at: "2026-08-27T00:00:00.000Z",
    ended_at: "2026-08-27T00:20:00.000Z",
    duration_seconds: 1200,
    interval_seconds: 1200,
    probe_spec_sha256: "6".repeat(64),
    probe_adapters: ["cloud", "discord", "oracle", "pages"].map((surface) => ({
      surface,
      sha256: "5".repeat(64),
    })),
    status: "passed",
    surfaces: ["cloud", "discord", "oracle", "pages"].map((surface) => {
      const identity = identities[surface];
      const identitySha256 = canonicalSha256(identity);
      return {
        surface,
        identity,
        identity_sha256: identitySha256,
        observation_count: 2,
        observations: [0, 1].map((sequence) => ({
          sequence,
          observed_at: sequence === 0
            ? "2026-08-27T00:00:00.000Z"
            : "2026-08-27T00:20:00.000Z",
          identity_sha256: identitySha256,
          freshness: observationFreshness(surface, identity, sequence),
        })),
      };
    }),
  });
}

function observationFreshness(surface, identity, sequence) {
  const observedAt = sequence === 0
    ? "2026-08-27T00:00:00.000Z"
    : "2026-08-27T00:20:00.000Z";
  const probeId = (sequence + 1).toString(16).padStart(64, "0");
  if (surface === "oracle") {
    return {
      operation_marker: canonicalSha256({
        contract: "clearra.oracle.candidate-observation.v1",
        source_commit: identity.source_commit,
        candidate_revision: identity.candidate_revision,
        fresh_operation_at: observedAt,
        observed_at: observedAt,
      }),
      fresh_operation_at: observedAt,
      observed_at: observedAt,
    };
  }
  if (surface === "discord") {
    return { probe_id: probeId, readback_sha256: "c".repeat(64) };
  }
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

test("accepts exactly one fully observed v0.8.0 source identity", () => {
  const bundle = validBundle();
  assert.equal(
    validateBundle(bundle, { expectedSourceCommit: COMMIT }),
    true,
  );
});

test("rejects mixed source identities and nonzero readiness", () => {
  const mixed = validBundle();
  mixed.manifest.surface_reports[2].source_commit = "3".repeat(40);
  assert.throws(() => validateBundle(mixed), /source commit differs/u);

  const open = validBundle();
  open.manifest.contracts.readiness_open_count = 1;
  assert.throws(() => validateBundle(open), /zero readiness/u);
});

test("requires exactly three named release artifacts and twenty observed minutes", () => {
  const missingArtifact = validBundle();
  missingArtifact.manifest.release_artifacts.pop();
  assert.throws(() => validateBundle(missingArtifact), /exactly three/u);

  const renamedArtifact = validBundle();
  renamedArtifact.manifest.release_artifacts[2].name = "Clearra-GUI-v0.8.0-windows-x86_64.zip";
  assert.throws(
    () => validateBundle(renamedArtifact),
    /canonical release asset/u,
  );

  const shortObservation = validBundle();
  shortObservation.manifest.observation.ended_at = "2026-08-27T00:19:59.000Z";
  shortObservation.manifest.observation.duration_seconds = 1199;
  assert.throws(() => validateBundle(shortObservation), /at least 1200/u);
});

test("rejects prior release authority and secret-shaped fields recursively", () => {
  const stale = validBundle();
  stale.manifest.canonical_gate.id = "v0.7.5-release-acceptance";
  assert.throws(() => validateBundle(stale), /v0\.7\.5 authority/u);

  const secret = validBundle();
  secret.manifest.deployment.discord.api_token = "forbidden";
  assert.throws(() => validateBundle(secret), /forbidden secret material/u);
});

test("requires active deployments, catalog sync, exact tag, and immutable release", () => {
  const inactive = validBundle();
  inactive.manifest.deployment.discord.traffic_percent = 99;
  assert.throws(() => validateBundle(inactive), /100 percent/u);

  const tag = validBundle();
  tag.manifest.tag.target_commit = "4".repeat(40);
  assert.throws(() => validateBundle(tag), /source commit differs/u);

  const mutable = validBundle();
  mutable.manifest.immutable_release.immutable = false;
  assert.throws(() => validateBundle(mutable), /not recorded as immutable/u);
});

test("requires actual catalog and observation producer reports and exact bindings", () => {
  const missingCatalog = validBundle();
  assert.throws(
    () => validateFinalSourceRevalidation(missingCatalog.manifest, {
      productionObservationReport: missingCatalog.productionObservationReport,
    }),
    /actual Discord command catalog sync producer report is required/u,
  );

  const driftedCatalog = validBundle();
  driftedCatalog.manifest.deployment.discord.command_catalog_readback_sha256 =
    "9".repeat(64);
  assert.throws(
    () => validateBundle(driftedCatalog),
    /differs from the actual catalog sync producer report/u,
  );

  const driftedObservation = validBundle();
  driftedObservation.manifest.deployment.pages.deployment_id = "pages-drift";
  assert.throws(
    () => validateBundle(driftedObservation),
    /observed Pages identity differs/u,
  );
});

test("library validation requires the exact acceptance deployment and publication stages", () => {
  const bundle = validBundle();
  const stages = stageAuthorities(bundle);
  assert.equal(
    validateFinalSourceRevalidationFromStages(bundle.manifest, {
      expectedSourceCommit: COMMIT,
      ...stages.options,
    }),
    true,
  );

  const substituted = structuredClone(stages.options.deploymentStageEvidence);
  delete substituted.report_sha256;
  substituted.producer_inputs.find(({ role }) => role === "discord-catalog-sync")
    .file_sha256 = "9".repeat(64);
  const resealed = sealCanonicalReport(substituted);
  assert.throws(
    () => validateFinalSourceRevalidationFromStages(bundle.manifest, {
      expectedSourceCommit: COMMIT,
      ...stages.options,
      deploymentStageEvidence: resealed,
    }),
    /producer bytes differ/u,
  );

  const manuallyEdited = structuredClone(bundle.manifest);
  manuallyEdited.tag.remote_verified = false;
  assert.throws(
    () => validateFinalSourceRevalidationFromStages(manuallyEdited, {
      expectedSourceCommit: COMMIT,
      ...stages.options,
    }),
    /differs from the exact three producer stages/u,
  );
});

function stageAuthorities(bundle) {
  const syncFileSha256 = "d".repeat(64);
  const observationFileSha256 = "e".repeat(64);
  const makeStage = (stage, roles, events) => sealCanonicalReport({
    schema_id: "clearra.final-source-stage-evidence.v1",
    stage,
    source_commit: COMMIT,
    producer_inputs: roles.map((role) => ({
      role,
      schema_id: `clearra.test.${role}.v1`,
      evidence_sha256: role === "discord-catalog-sync"
        ? bundle.discordCatalogSyncReport.report_sha256
        : role === "production-observation"
          ? bundle.productionObservationReport.report_sha256
          : HASH,
      file_sha256: role === "discord-catalog-sync"
        ? syncFileSha256
        : role === "production-observation"
          ? observationFileSha256
          : HASH,
    })),
    events,
    status: "passed",
  });
  const manifest = bundle.manifest;
  const acceptance = makeStage("acceptance", [
    "canonical-acceptance",
    "implementation-start-audit",
    "legacy-alias-contract",
    "product-registry",
    "release-freeze-audit",
    "search-option-contract",
  ], [
    { kind: "source", payload: manifest.source },
    { kind: "contracts", payload: manifest.contracts },
    { kind: "toolchains", payload: manifest.toolchains },
    ...manifest.drift_audits.map((payload) => ({ kind: "drift-audit", payload })),
    { kind: "canonical-gate", payload: manifest.canonical_gate },
    ...manifest.surface_reports.map((payload) => ({ kind: "surface-report", payload })),
    ...manifest.release_artifacts.map((payload) => ({ kind: "release-artifact", payload })),
  ]);
  const deployment = makeStage("deployment", [
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
  ], [
    { kind: "deployment-pages", payload: manifest.deployment.pages },
    { kind: "deployment-discord", payload: manifest.deployment.discord },
    { kind: "rollback-snapshot", payload: manifest.deployment.rollback_snapshot },
    { kind: "observation", payload: manifest.observation },
  ]);
  const publication = makeStage("publication", [
    "release-publication",
    "release-publication-authority",
    "release-publication-receipt",
  ], [
    { kind: "tag", payload: manifest.tag },
    { kind: "immutable-release", payload: manifest.immutable_release },
  ]);
  return {
    options: {
      acceptanceStageEvidence: acceptance,
      acceptanceStageEvidenceFileSha256: "1".repeat(64),
      deploymentStageEvidence: deployment,
      deploymentStageEvidenceFileSha256: "2".repeat(64),
      publicationStageEvidence: publication,
      publicationStageEvidenceFileSha256: "3".repeat(64),
      discordCatalogSyncReport: bundle.discordCatalogSyncReport,
      discordCatalogSyncReportFileSha256: syncFileSha256,
      productionObservationReport: bundle.productionObservationReport,
      productionObservationReportFileSha256: observationFileSha256,
    },
  };
}

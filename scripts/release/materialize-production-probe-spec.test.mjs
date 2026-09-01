import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  createProductionProbeSpec,
  PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID,
  validateProductionProbeAuthority,
} from "./materialize-production-probe-spec.mjs";
import { validateProductionProbeSpec } from "./observe-production-surfaces.mjs";
import {
  canonicalJson,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";
import {
  createCanonicalDiscordCatalog,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";

const COMMIT = "1".repeat(40);

test("materializes three tracked Node adapters and one explicit Oracle owner boundary", async () => {
  await withAuthorityFiles(async ({ authority, sharedAdapterPath }) => {
    const spec = await createProductionProbeSpec(authority, { sharedAdapterPath });
    assert.equal(validateProductionProbeSpec(spec, COMMIT), spec);
    assert.deepEqual(spec.probes.map(({ surface, runtime }) => [surface, runtime]), [
      ["cloud", "node"],
      ["discord", "node"],
      ["oracle", "powershell"],
      ["pages", "node"],
    ]);
    assert.equal(new Set([
      spec.probes[0].sha256,
      spec.probes[1].sha256,
      spec.probes[3].sha256,
    ]).size, 1);
    assert.equal(spec.probes[2].sha256, authority.oracle.adapter_sha256);
    const discordProbe = spec.probes.find(({ surface }) => surface === "discord");
    assert.equal(
      discordProbe.arguments[discordProbe.arguments.indexOf("--sync-authority") + 1],
      authority.discord.sync_authority_path,
    );
    assert.equal(
      discordProbe.arguments[
        discordProbe.arguments.indexOf("--sync-authority-file-sha256") + 1
      ],
      authority.discord.sync_authority_file_sha256,
    );
    assert.deepEqual(spec.probes[2].arguments.slice(0, 2), [
      "-Operation",
      "observe-candidate",
    ]);
    assert.doesNotMatch(
      JSON.stringify(spec),
      /CLEARRA_ORACLE_IDENTITY_FILE|DISCORD_TOKEN|api[_-]?key|password/iu,
    );
  });
});

test("probe authority rejects hash drift, secret fields, and mixed source identity", async () => {
  await withAuthorityFiles(async ({ authority, sharedAdapterPath }) => {
    const wrongHash = structuredClone(authority);
    wrongHash.oracle.adapter_sha256 = "f".repeat(64);
    await assert.rejects(
      createProductionProbeSpec(wrongHash, { sharedAdapterPath }),
      /differs from its approved SHA-256/u,
    );

    const secret = structuredClone(authority);
    secret.discord.api_token = "forbidden";
    assert.throws(
      () => validateProductionProbeAuthority(secret),
      /fields differ from the closed schema/u,
    );

    const mixed = structuredClone(authority);
    mixed.oracle.candidate_revision = "clearra-current-job-v080-2222222";
    assert.throws(
      () => validateProductionProbeAuthority(mixed),
      /differs from the accepted source/u,
    );

    const shortInterval = structuredClone(authority);
    shortInterval.interval_seconds = 1199;
    assert.throws(
      () => validateProductionProbeAuthority(shortInterval),
      /interval must be exactly 1200 seconds/u,
    );

    await writeFile(authority.cloud.smoke_report_path, '{"drift":true}\n', "utf8");
    await assert.rejects(
      createProductionProbeSpec(authority, { sharedAdapterPath }),
      /Cloud candidate smoke evidence file SHA-256 changed/u,
    );
  });
});

test("Pages probe authority rejects a deployment report changed after approval", async () => {
  await withAuthorityFiles(async ({ authority, sharedAdapterPath }) => {
    await writeFile(authority.pages.deployment_report_path, '{"drift":true}\n', "utf8");
    await assert.rejects(
      createProductionProbeSpec(authority, { sharedAdapterPath }),
      /Pages deployment authority report file SHA-256 changed/u,
    );
  });
});

test("Discord probe authority rejects noncanonical sync-authority bytes even when rehashed", async () => {
  await withAuthorityFiles(async ({ authority, sharedAdapterPath, syncAuthority }) => {
    const noncanonicalBytes = `${JSON.stringify(syncAuthority, null, 2)}\n`;
    await writeFile(
      authority.discord.sync_authority_path,
      noncanonicalBytes,
      "utf8",
    );
    authority.discord.sync_authority_file_sha256 = fileSha256(noncanonicalBytes);
    await assert.rejects(
      createProductionProbeSpec(authority, { sharedAdapterPath }),
      /command sync authority bytes are not canonical JSON/u,
    );
  });
});

test("Discord probe authority rejects a re-sealed sync report bound to different authority bytes", async () => {
  await withAuthorityFiles(async ({
    authority,
    sharedAdapterPath,
    syncReport,
  }) => {
    const { report_sha256: _reportSha256, ...unsigned } = syncReport;
    const tampered = sealCanonicalReport({
      ...unsigned,
      command_sync_authority_file_sha256: "f".repeat(64),
    });
    const tamperedBytes = `${canonicalJson(tampered)}\n`;
    await writeFile(authority.discord.sync_report_path, tamperedBytes, "utf8");
    authority.discord.sync_report_file_sha256 = fileSha256(tamperedBytes);
    await assert.rejects(
      createProductionProbeSpec(authority, { sharedAdapterPath }),
      /differs from the command sync authority file bytes/u,
    );
  });
});

async function withAuthorityFiles(body) {
  const root = await mkdtemp(join(tmpdir(), "clearra-probe-spec-"));
  try {
    const oracleAdapter = resolve(root, "invoke-release-deploy-v080.ps1");
    const sharedAdapterPath = resolve(root, "production-surface-probe-adapter.mjs");
    const catalogPath = resolve(root, "discord-command-catalog.json");
    const syncAuthorityPath = resolve(root, "discord-command-sync-authority.json");
    const syncReportPath = resolve(root, "discord-command-catalog-sync.json");
    const smokeReportPath = resolve(root, "cloud-candidate-smoke.json");
    const pagesDeploymentReportPath = resolve(root, "pages-deployment-authority.json");
    await writeFile(oracleAdapter, "param()\n", "utf8");
    await writeFile(sharedAdapterPath, "export {};\n", "utf8");
    const catalog = createCanonicalDiscordCatalog({
      sourceCommit: COMMIT,
      commands: [{ name: "help", description: "Show help" }],
    });
    const catalogBytes = `${canonicalJson(catalog)}\n`;
    const catalogFileSha256 = fileSha256(catalogBytes);
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
    const syncAuthorityBytes = `${canonicalJson(syncAuthority)}\n`;
    const syncAuthorityFileSha256 = fileSha256(syncAuthorityBytes);
    const syncReport = sealCanonicalReport({
      schema_id: "clearra.discord.command-catalog-sync.v1",
      source_commit: COMMIT,
      application_id: "223456789012345678",
      started_at: "2026-08-30T00:00:00.000Z",
      ended_at: "2026-08-30T00:00:01.000Z",
      status: "synchronized",
      changed: true,
      command_count: catalog.command_count,
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
      current_after_sha256: "7".repeat(64),
    });
    const syncReportBytes = `${canonicalJson(syncReport)}\n`;
    await writeFile(catalogPath, catalogBytes, "utf8");
    await writeFile(syncAuthorityPath, syncAuthorityBytes, "utf8");
    await writeFile(syncReportPath, syncReportBytes, "utf8");
    await writeFile(smokeReportPath, "{}\n", "utf8");
    const pagesDeploymentReport = sealCanonicalReport({
      schema_id: "clearra.pages.deployment-authority.v2",
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
      artifact_digest: `sha256:${"a".repeat(64)}`,
      artifact_sha256: "a".repeat(64),
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
    const pagesDeploymentReportBytes = `${canonicalJson(pagesDeploymentReport)}\n`;
    await writeFile(pagesDeploymentReportPath, pagesDeploymentReportBytes, "utf8");
    const oracleBytes = await readFile(oracleAdapter);
    const emptyCanonicalFileSha256 = fileSha256("{}\n");
    const authority = {
      schema_id: PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID,
      source_commit: COMMIT,
      interval_seconds: 1200,
      discord: {
        application_id: "223456789012345678",
        catalog_path: catalogPath,
        catalog_file_sha256: catalogFileSha256,
        sync_authority_path: syncAuthorityPath,
        sync_authority_file_sha256: syncAuthorityFileSha256,
        sync_report_path: syncReportPath,
        sync_report_file_sha256: fileSha256(syncReportBytes),
        timeout_seconds: 30,
      },
      cloud: {
        project_id: "clearra-prod1",
        region: "asia-northeast1",
        service_name: "clearra-current-job",
        revision: "clearra-current-job-v080-1111111",
        tag: "candidate-1111111",
        image_digest: `sha256:${"f".repeat(64)}`,
        smoke_report_path: smokeReportPath,
        smoke_report_file_sha256: emptyCanonicalFileSha256,
        timeout_seconds: 30,
      },
      oracle: {
        adapter_path: oracleAdapter,
        adapter_sha256: createHash("sha256").update(oracleBytes).digest("hex"),
        script_release_id: "v0.8.0-1111111",
        script_release_sha256: "d".repeat(64),
        candidate_url: "https://v080---clearra-current-job.example.run.app/",
        candidate_revision: "clearra-current-job-v080-1111111",
        oracle_release_id: "v0.8.0-1111111",
        oracle_release_sha256: "d".repeat(64),
        oracle_settings_sha256: "e".repeat(64),
        deployment_nonce: "9".repeat(64),
        verified_after: "2026-08-30T00:00:00.000Z",
        timeout_seconds: 60,
      },
      pages: {
        deployment_report_path: pagesDeploymentReportPath,
        deployment_report_file_sha256: createHash("sha256")
          .update(pagesDeploymentReportBytes, "utf8")
          .digest("hex"),
        timeout_seconds: 30,
      },
    };
    await body({ authority, sharedAdapterPath, syncAuthority, syncReport });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

function fileSha256(bytes) {
  return createHash("sha256").update(bytes, "utf8").digest("hex");
}

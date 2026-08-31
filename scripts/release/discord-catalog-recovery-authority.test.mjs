import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { canonicalJson, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import {
  sealDiscordCatalogRecoveryAuthority,
  sealDiscordCatalogRecoveryDisposition,
  validateDiscordCatalogRecoveryDisposition,
  verifyDiscordCatalogRecoveryAuthority,
} from "./discord-catalog-recovery-authority.mjs";
import {
  captureDiscordCatalogSnapshot,
  createCanonicalDiscordCatalog,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";

const SOURCE = "a".repeat(40);
const APPLICATION = "123456789012345678";

test("catalog recovery authority binds the exact durable preimage and desired catalog bytes", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-catalog-recovery-"));
  try {
    const catalog = createCanonicalDiscordCatalog({
      sourceCommit: SOURCE,
      commands: [{ name: "path", description: "Path", type: 1 }],
    });
    const snapshot = await captureDiscordCatalogSnapshot({
      rest: { async getGlobalCommands() { return [{ name: "help", description: "Help", type: 1 }]; } },
      applicationId: APPLICATION,
      sourceCommit: SOURCE,
      observedAt: "2026-08-31T00:00:00.000Z",
    });
    const catalogPath = join(root, "discord-catalog.json");
    const snapshotPath = join(root, "discord-prior-catalog.json");
    await writeFile(catalogPath, `${canonicalJson(catalog)}\n`);
    await writeFile(snapshotPath, `${canonicalJson(snapshot)}\n`);
    const catalogFileSha256 = (await import("node:crypto")).createHash("sha256")
      .update(`${canonicalJson(catalog)}\n`).digest("hex");
    const syncAuthority = sealCanonicalReport({
      schema_id: "clearra.discord.command-sync-authority.v1",
      source_commit: SOURCE,
      repository: "daejunnom/Clearra",
      release_version: "0.8.0",
      pages_base_path: "/Clearra",
      accepted_run_id: "10",
      accepted_run_attempt: "1",
      accepted_ctk3_manifest_sha256: "b".repeat(64),
      canonical_acceptance_evidence_sha256: "c".repeat(64),
      canonical_acceptance_evidence_file_sha256: "d".repeat(64),
      command_catalog_sha256: catalog.catalog_sha256,
      command_catalog_file_sha256: catalogFileSha256,
    });
    const syncPath = join(root, "discord-sync-authority.json");
    await writeFile(syncPath, `${canonicalJson(syncAuthority)}\n`);
    const options = {
      repository: "daejunnom/Clearra",
      sourceCommit: SOURCE,
      workflowRunId: "20",
      workflowRunAttempt: "2",
      applicationId: APPLICATION,
      priorSnapshot: snapshotPath,
      desiredCatalog: catalogPath,
      syncAuthority: syncPath,
    };
    const report = await sealDiscordCatalogRecoveryAuthority(options);
    assert.equal(report.prior_catalog_sha256, snapshot.catalog_sha256);
    assert.equal(report.desired_catalog_sha256, catalog.catalog_sha256);
    const reportPath = join(root, "catalog-recovery-authority.json");
    await writeFile(reportPath, `${canonicalJson(report)}\n`);
    await verifyDiscordCatalogRecoveryAuthority(reportPath, options);

    const restore = sealCanonicalReport({
      schema_id: "clearra.discord.command-catalog-restore.v1",
      source_commit: SOURCE,
      application_id: APPLICATION,
      started_at: "2026-08-31T00:01:00.000Z",
      ended_at: "2026-08-31T00:01:01.000Z",
      status: "restored",
      changed: true,
      command_count: snapshot.command_count,
      expected_current_sha256: catalog.catalog_sha256,
      prior_snapshot_sha256: snapshot.snapshot_sha256,
      prior_catalog_sha256: snapshot.catalog_sha256,
      current_before_sha256: catalog.catalog_sha256,
      current_after_sha256: snapshot.catalog_sha256,
    });
    const restorePath = join(root, "discord-catalog-restore.json");
    await writeFile(restorePath, `${canonicalJson(restore)}\n`);
    const dispositionOptions = {
      ...options,
      originalWorkflowRunId: "20",
      originalWorkflowRunAttempt: "2",
      recoveryWorkflowRunId: "30",
      recoveryWorkflowRunAttempt: "1",
      artifactId: "77",
      artifactDigest: `sha256:${"7".repeat(64)}`,
      required: true,
      authorityReport: reportPath,
      restoreReport: restorePath,
    };
    const disposition = await sealDiscordCatalogRecoveryDisposition(dispositionOptions);
    assert.equal(disposition.recovery_required, true);
    assert.equal(disposition.current_before_sha256, catalog.catalog_sha256);
    assert.equal(disposition.current_after_sha256, snapshot.catalog_sha256);
    validateDiscordCatalogRecoveryDisposition(disposition, {
      repository: options.repository,
      sourceCommit: SOURCE,
      originalWorkflowRunId: "20",
      originalWorkflowRunAttempt: "2",
      recoveryWorkflowRunId: "30",
      recoveryWorkflowRunAttempt: "1",
      artifactId: "77",
      artifactDigest: dispositionOptions.artifactDigest,
      required: true,
    });
    const { report_sha256: ignoredDispositionSha256, ...unsignedDisposition } = disposition;
    void ignoredDispositionSha256;
    const wrongArtifact = sealCanonicalReport({
      ...unsignedDisposition,
      catalog_artifact_id: "78",
    });
    assert.throws(
      () => validateDiscordCatalogRecoveryDisposition(wrongArtifact, {
        artifactId: "77",
        artifactDigest: dispositionOptions.artifactDigest,
        required: true,
      }),
      /artifact authority differs/u,
    );
    assert.throws(
      () => validateDiscordCatalogRecoveryDisposition(disposition, { required: false }),
      /requirement differs/u,
    );
    await writeFile(catalogPath, `${canonicalJson({ ...catalog, catalog_sha256: "e".repeat(64) })}\n`);
    await assert.rejects(
      verifyDiscordCatalogRecoveryAuthority(reportPath, options),
      /SHA-256 differs|exact input files/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

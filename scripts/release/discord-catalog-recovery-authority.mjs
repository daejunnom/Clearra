#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, open, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  requireExactKeys,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import { validateDiscordCommandSyncAuthority } from "./discord-command-sync-authority.mjs";
import {
  validateCanonicalDiscordCatalog,
  validateDiscordCatalogRestoreReport,
  validateDiscordCatalogSnapshot,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";

export const DISCORD_CATALOG_RECOVERY_AUTHORITY_SCHEMA_ID =
  "clearra.discord-catalog-recovery-authority.v1";
export const DISCORD_CATALOG_RECOVERY_DISPOSITION_SCHEMA_ID =
  "clearra.discord-catalog-recovery-disposition.v1";

const SHA = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const DECIMAL = /^[1-9][0-9]*$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const SNOWFLAKE = /^[0-9]{17,20}$/u;

export async function sealDiscordCatalogRecoveryAuthority(options) {
  const sourceCommit = requirePattern(options?.sourceCommit, SHA, "source commit");
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const applicationId = requirePattern(options?.applicationId, SNOWFLAKE, "Discord application ID");
  const prior = await readCanonicalFile(options?.priorSnapshot, "Discord prior catalog snapshot");
  const catalog = await readCanonicalFile(options?.desiredCatalog, "Discord desired catalog");
  const sync = await readCanonicalFile(options?.syncAuthority, "Discord sync authority");
  validateDiscordCatalogSnapshot(prior.value, {
    expectedSourceCommit: sourceCommit,
    expectedApplicationId: applicationId,
  });
  validateCanonicalDiscordCatalog(catalog.value, sourceCommit);
  validateDiscordCommandSyncAuthority(sync.value, {
    sourceCommit,
    catalog: catalog.value,
    catalogFileSha256: catalog.fileSha256,
  });
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_CATALOG_RECOVERY_AUTHORITY_SCHEMA_ID,
    repository,
    source_commit: sourceCommit,
    workflow_run_id: requirePattern(options?.workflowRunId, DECIMAL, "workflow run ID"),
    workflow_run_attempt: requirePattern(
      options?.workflowRunAttempt,
      DECIMAL,
      "workflow run attempt",
    ),
    application_id: applicationId,
    prior_snapshot_sha256: prior.value.snapshot_sha256,
    prior_catalog_sha256: prior.value.catalog_sha256,
    prior_snapshot_file_sha256: prior.fileSha256,
    desired_catalog_sha256: catalog.value.catalog_sha256,
    desired_catalog_file_sha256: catalog.fileSha256,
    sync_authority_sha256: sync.value.report_sha256,
    sync_authority_file_sha256: sync.fileSha256,
  }));
}

export async function verifyDiscordCatalogRecoveryAuthority(path, options) {
  const input = await readCanonicalFile(path, "Discord catalog recovery authority");
  const report = input.value;
  requireExactKeys(report, [
    "schema_id", "repository", "source_commit", "workflow_run_id",
    "workflow_run_attempt", "application_id", "prior_snapshot_sha256",
    "prior_catalog_sha256", "prior_snapshot_file_sha256", "desired_catalog_sha256",
    "desired_catalog_file_sha256", "sync_authority_sha256",
    "sync_authority_file_sha256", "report_sha256",
  ], "Discord catalog recovery authority");
  verifyCanonicalReportHash(report, "Discord catalog recovery authority");
  const expected = await sealDiscordCatalogRecoveryAuthority(options);
  if (canonicalJson(report) !== canonicalJson(expected)) {
    throw new Error("Discord catalog recovery authority differs from its exact input files");
  }
  return Object.freeze({ report, fileSha256: input.fileSha256 });
}

export async function sealDiscordCatalogRecoveryDisposition(options) {
  const identity = {
    repository: requirePattern(options?.repository, REPOSITORY, "repository"),
    source_commit: requirePattern(options?.sourceCommit, SHA, "source commit"),
    original_workflow_run_id: requirePattern(
      options?.originalWorkflowRunId, DECIMAL, "original workflow run ID"),
    original_workflow_run_attempt: requirePattern(
      options?.originalWorkflowRunAttempt, DECIMAL, "original workflow run attempt"),
    recovery_workflow_run_id: requirePattern(
      options?.recoveryWorkflowRunId, DECIMAL, "recovery workflow run ID"),
    recovery_workflow_run_attempt: requirePattern(
      options?.recoveryWorkflowRunAttempt, DECIMAL, "recovery workflow run attempt"),
  };
  const required = options?.required === true || options?.required === "true";
  if (!required) {
    return Object.freeze(sealCanonicalReport({
      schema_id: DISCORD_CATALOG_RECOVERY_DISPOSITION_SCHEMA_ID,
      ...identity,
      recovery_required: false,
      status: "not-required",
      catalog_artifact_id: null,
      catalog_artifact_digest: null,
      catalog_authority_sha256: null,
      catalog_authority_file_sha256: null,
      prior_snapshot_sha256: null,
      prior_catalog_sha256: null,
      desired_catalog_sha256: null,
      restore_report_sha256: null,
      restore_report_file_sha256: null,
      current_before_sha256: null,
      current_after_sha256: null,
    }));
  }
  const authorityInput = await verifyDiscordCatalogRecoveryAuthority(
    options?.authorityReport,
    {
      repository: identity.repository,
      sourceCommit: identity.source_commit,
      workflowRunId: identity.original_workflow_run_id,
      workflowRunAttempt: identity.original_workflow_run_attempt,
      applicationId: options?.applicationId,
      priorSnapshot: options?.priorSnapshot,
      desiredCatalog: options?.desiredCatalog,
      syncAuthority: options?.syncAuthority,
    },
  );
  const restoreInput = await readCanonicalFile(
    options?.restoreReport,
    "Discord catalog restore report",
  );
  validateDiscordCatalogRestoreReport(restoreInput.value, {
    expectedSourceCommit: identity.source_commit,
    expectedApplicationId: options?.applicationId,
  });
  const authority = authorityInput.report;
  const restore = restoreInput.value;
  if (
    restore.prior_snapshot_sha256 !== authority.prior_snapshot_sha256 ||
    restore.prior_catalog_sha256 !== authority.prior_catalog_sha256 ||
    ![authority.desired_catalog_sha256, authority.prior_catalog_sha256]
      .includes(restore.current_before_sha256) ||
    restore.current_after_sha256 !== authority.prior_catalog_sha256
  ) throw new Error("Discord catalog recovery disposition is not an exact digest-guarded restore");
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_CATALOG_RECOVERY_DISPOSITION_SCHEMA_ID,
    ...identity,
    recovery_required: true,
    status: "restored-or-already-prior",
    catalog_artifact_id: requirePattern(options?.artifactId, DECIMAL, "catalog artifact ID"),
    catalog_artifact_digest: requirePattern(
      options?.artifactDigest,
      /^sha256:[0-9a-f]{64}$/u,
      "catalog artifact digest",
    ),
    catalog_authority_sha256: authority.report_sha256,
    catalog_authority_file_sha256: authorityInput.fileSha256,
    prior_snapshot_sha256: authority.prior_snapshot_sha256,
    prior_catalog_sha256: authority.prior_catalog_sha256,
    desired_catalog_sha256: authority.desired_catalog_sha256,
    restore_report_sha256: restore.report_sha256,
    restore_report_file_sha256: restoreInput.fileSha256,
    current_before_sha256: restore.current_before_sha256,
    current_after_sha256: restore.current_after_sha256,
  }));
}

export function validateDiscordCatalogRecoveryDisposition(report, expected = {}) {
  requireExactKeys(report, [
    "schema_id", "repository", "source_commit", "original_workflow_run_id",
    "original_workflow_run_attempt", "recovery_workflow_run_id",
    "recovery_workflow_run_attempt", "recovery_required", "status",
    "catalog_artifact_id", "catalog_artifact_digest", "catalog_authority_sha256",
    "catalog_authority_file_sha256", "prior_snapshot_sha256", "prior_catalog_sha256",
    "desired_catalog_sha256", "restore_report_sha256", "restore_report_file_sha256",
    "current_before_sha256", "current_after_sha256", "report_sha256",
  ], "Discord catalog recovery disposition");
  verifyCanonicalReportHash(report, "Discord catalog recovery disposition");
  if (report.schema_id !== DISCORD_CATALOG_RECOVERY_DISPOSITION_SCHEMA_ID) {
    throw new Error("Discord catalog recovery disposition schema is invalid");
  }
  for (const [field, value] of Object.entries({
    repository: expected.repository,
    source_commit: expected.sourceCommit,
    original_workflow_run_id: expected.originalWorkflowRunId,
    original_workflow_run_attempt: expected.originalWorkflowRunAttempt,
    recovery_workflow_run_id: expected.recoveryWorkflowRunId,
    recovery_workflow_run_attempt: expected.recoveryWorkflowRunAttempt,
  })) {
    if (value !== undefined && report[field] !== String(value)) {
      throw new Error(`Discord catalog recovery disposition ${field} differs`);
    }
  }
  const required = expected.required === true || expected.required === "true";
  if (report.recovery_required !== required) {
    throw new Error("Discord catalog recovery disposition requirement differs");
  }
  if (!required) {
    if (
      report.status !== "not-required" ||
      Object.entries(report).some(([key, value]) =>
        !["schema_id", "repository", "source_commit", "original_workflow_run_id",
          "original_workflow_run_attempt", "recovery_workflow_run_id",
          "recovery_workflow_run_attempt", "recovery_required", "status", "report_sha256"]
          .includes(key) && value !== null)
    ) throw new Error("Discord not-required catalog recovery disposition has extra authority");
    return report;
  }
  if (
    report.status !== "restored-or-already-prior" ||
    requirePattern(report.catalog_artifact_id, DECIMAL, "catalog artifact ID") !==
      String(expected.artifactId) ||
    requirePattern(report.catalog_artifact_digest, /^sha256:[0-9a-f]{64}$/u,
      "catalog artifact digest") !== expected.artifactDigest
  ) throw new Error("Discord catalog recovery disposition artifact authority differs");
  for (const field of [
    "catalog_authority_sha256", "catalog_authority_file_sha256", "prior_snapshot_sha256",
    "prior_catalog_sha256", "desired_catalog_sha256", "restore_report_sha256",
    "restore_report_file_sha256", "current_before_sha256", "current_after_sha256",
  ]) requirePattern(report[field], SHA256, `catalog disposition ${field}`);
  if (
    ![report.desired_catalog_sha256, report.prior_catalog_sha256]
      .includes(report.current_before_sha256) ||
    report.current_after_sha256 !== report.prior_catalog_sha256
  ) throw new Error("Discord catalog recovery disposition digest guard is invalid");
  return report;
}

async function readCanonicalFile(path, label) {
  const target = resolve(typeof path === "string" ? path : "");
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 2) {
    throw new Error(`${label} must be a nonempty regular non-link file`);
  }
  const raw = await readFile(target, "utf8");
  let value;
  try { value = JSON.parse(raw); } catch { throw new Error(`${label} is not valid JSON`); }
  if (raw !== `${canonicalJson(value)}\n`) throw new Error(`${label} bytes are not canonical JSON`);
  return Object.freeze({
    value,
    fileSha256: createHash("sha256").update(raw, "utf8").digest("hex"),
  });
}

async function writeCanonicalNew(path, value) {
  const target = resolve(typeof path === "string" ? path : "");
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(value)}\n`, "utf8");
    await handle.sync();
  } finally { await handle.close(); }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Discord catalog recovery path contains a link or non-directory");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requirePattern(value, pattern, label) {
  const text = typeof value === "number" && Number.isSafeInteger(value)
    ? String(value)
    : typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function parseCli() {
  return parseArgs({
    options: {
      repository: { type: "string" },
      "source-commit": { type: "string" },
      "workflow-run-id": { type: "string" },
      "workflow-run-attempt": { type: "string" },
      "application-id": { type: "string" },
      "prior-snapshot": { type: "string" },
      "desired-catalog": { type: "string" },
      "sync-authority": { type: "string" },
      "original-workflow-run-id": { type: "string" },
      "original-workflow-run-attempt": { type: "string" },
      "recovery-workflow-run-id": { type: "string" },
      "recovery-workflow-run-attempt": { type: "string" },
      "artifact-id": { type: "string" },
      "artifact-digest": { type: "string" },
      required: { type: "string" },
      "authority-report": { type: "string" },
      "restore-report": { type: "string" },
      report: { type: "string" },
      output: { type: "string" },
    },
    strict: true,
    allowPositionals: true,
  });
}

async function main() {
  const { values, positionals } = parseCli();
  if (positionals.length !== 1 || !["seal", "verify", "seal-disposition"].includes(positionals[0])) {
    throw new Error("Discord catalog recovery operation must be seal, verify, or seal-disposition");
  }
  const options = {
    repository: values.repository,
    sourceCommit: values["source-commit"],
    workflowRunId: values["workflow-run-id"],
    workflowRunAttempt: values["workflow-run-attempt"],
    applicationId: values["application-id"],
    priorSnapshot: values["prior-snapshot"],
    desiredCatalog: values["desired-catalog"],
    syncAuthority: values["sync-authority"],
  };
  if (positionals[0] === "seal") {
    await writeCanonicalNew(values.output, await sealDiscordCatalogRecoveryAuthority(options));
    process.stdout.write("discord_catalog_recovery=sealed\n");
  } else if (positionals[0] === "verify") {
    await verifyDiscordCatalogRecoveryAuthority(values.report, options);
    process.stdout.write("discord_catalog_recovery=verified\n");
  } else {
    const disposition = await sealDiscordCatalogRecoveryDisposition({
      ...options,
      originalWorkflowRunId: values["original-workflow-run-id"],
      originalWorkflowRunAttempt: values["original-workflow-run-attempt"],
      recoveryWorkflowRunId: values["recovery-workflow-run-id"],
      recoveryWorkflowRunAttempt: values["recovery-workflow-run-attempt"],
      artifactId: values["artifact-id"],
      artifactDigest: values["artifact-digest"],
      required: values.required,
      authorityReport: values["authority-report"],
      restoreReport: values["restore-report"],
    });
    await writeCanonicalNew(values.output, disposition);
    process.stdout.write("discord_catalog_recovery=disposition-sealed\n");
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try { await main(); } catch (error) {
    process.stderr.write(
      `discord_catalog_recovery=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}

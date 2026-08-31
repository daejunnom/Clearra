#!/usr/bin/env node

import { createHash } from "node:crypto";
import { link, lstat, open, readFile, unlink } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  requireExactKeys,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import { validateCanonicalAcceptanceEvidence } from
  "./canonical-acceptance-evidence.mjs";
import { validateDiscordCatalogSyncReport } from
  "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";
import { validateDiscordCommandSyncAuthority } from
  "./discord-command-sync-authority.mjs";
import {
  createDiscordSuccessfulDeploymentTopologyContract,
  validateDiscordCheckpointCandidatePrerequisites,
  validateDiscordCheckpointPrerequisiteProof,
  validateDiscordSuccessfulDeploymentTopologyContract,
} from "./discord-deployment-recovery.mjs";
import { DISCORD_RECOVERY_DEBT_CLEARANCE_SCHEMA_ID } from
  "./discord-recovery-debt.mjs";
import { verifyDiscordCatalogRecoveryAuthority } from
  "./discord-catalog-recovery-authority.mjs";
import {
  PRODUCTION_OBSERVATION_SECONDS,
  validateProductionObservationReport,
} from "./observe-production-surfaces.mjs";

export const DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_SCHEMA_ID =
  "clearra.discord-production-checkpoint-candidate.v1";
export const DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE =
  "discord-production-checkpoint-candidate.json";
export const DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_ARTIFACT_PREFIX =
  "discord-production-checkpoint-candidate";

const CATALOG_DISPOSITION_SCHEMA_ID =
  "clearra.discord-production-catalog-disposition.v1";
const REPOSITORY_ID = 1309293231;
const SHA = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const DIGEST = /^sha256:[0-9a-f]{64}$/u;
const DECIMAL = /^[1-9][0-9]*$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;

export function checkpointCandidateArtifactName(sourceCommit, runId, runAttempt) {
  return `${DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_ARTIFACT_PREFIX}-` +
    `${requirePattern(sourceCommit, SHA, "source commit")}-run-` +
    `${requirePattern(runId, DECIMAL, "workflow run ID")}-attempt-` +
    requirePattern(runAttempt, DECIMAL, "workflow run attempt");
}

export async function createDiscordProductionCheckpointCandidate(options) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(options?.sourceCommit, SHA, "source commit");
  const workflowRunId = requirePattern(options?.workflowRunId, DECIMAL, "workflow run ID");
  const workflowRunAttempt = requirePattern(
    options?.workflowRunAttempt,
    DECIMAL,
    "workflow run attempt",
  );
  const acceptedRunId = requirePattern(options?.acceptedRunId, DECIMAL, "accepted run ID");
  const acceptedRunAttempt = requirePattern(
    options?.acceptedRunAttempt,
    DECIMAL,
    "accepted run attempt",
  );
  const acceptance = await readCanonicalFile(
    options?.canonicalAcceptanceEvidence,
    "canonical acceptance evidence",
  );
  validateCanonicalAcceptanceEvidence(acceptance.value, {
    repository,
    version: options?.version,
    basePath: options?.basePath,
    sourceCommit,
    runId: acceptedRunId,
    runAttempt: acceptedRunAttempt,
  });

  const clearance = await readCanonicalFile(
    options?.recoveryDebtClearance,
    "Discord recovery-debt clearance",
  );
  validateRecoveryDebtClearance(clearance.value, {
    repository,
    sourceCommit,
    workflowRunId,
    workflowRunAttempt,
  });

  const prior = await readCanonicalFile(
    options?.priorCatalogSnapshot,
    "Discord prior catalog snapshot",
  );
  const desired = await readCanonicalFile(
    options?.desiredCatalog,
    "Discord desired catalog",
  );
  const syncAuthority = await readCanonicalFile(
    options?.syncAuthority,
    "Discord command sync authority",
  );
  const catalogAuthority = await verifyDiscordCatalogRecoveryAuthority(
    options?.catalogRecoveryAuthority,
    {
      repository,
      sourceCommit,
      workflowRunId,
      workflowRunAttempt,
      applicationId: options?.applicationId,
      priorSnapshot: options?.priorCatalogSnapshot,
      desiredCatalog: options?.desiredCatalog,
      syncAuthority: options?.syncAuthority,
    },
  );
  validateDiscordCommandSyncAuthority(syncAuthority.value, {
    sourceCommit,
    acceptedRunId,
    acceptedRunAttempt,
    catalog: desired.value,
    catalogFileSha256: desired.fileSha256,
  });
  if (
    syncAuthority.value.canonical_acceptance_evidence_sha256 !==
      acceptance.value.report_sha256 ||
    syncAuthority.value.canonical_acceptance_evidence_file_sha256 !==
      acceptance.fileSha256
  ) throw new Error("Discord checkpoint sync authority differs from accepted evidence bytes");

  const syncReport = await readCanonicalFile(
    options?.syncReport,
    "Discord command sync report",
  );
  validateDiscordCatalogSyncReport(syncReport.value, {
    expectedSourceCommit: sourceCommit,
    expectedApplicationId: options?.applicationId,
    expectedCatalog: desired.value,
    expectedCatalogFileSha256: desired.fileSha256,
    expectedSyncAuthority: syncAuthority.value,
    expectedSyncAuthorityFileSha256: syncAuthority.fileSha256,
  });
  if (
    syncReport.value.prior_snapshot_sha256 !== prior.value.snapshot_sha256 ||
    syncReport.value.prior_catalog_sha256 !== prior.value.catalog_sha256 ||
    syncReport.value.current_before_sha256 !== prior.value.catalog_sha256 ||
    syncReport.value.current_after_sha256 !== desired.value.catalog_sha256
  ) throw new Error("Discord checkpoint catalog disposition differs from its exact preimage");

  const observation = await readCanonicalFile(
    options?.productionObservation,
    "production observation",
  );
  validateProductionObservationReport(observation.value, {
    expectedSourceCommit: sourceCommit,
    expectedDurationSeconds: PRODUCTION_OBSERVATION_SECONDS,
    expectedIntervalSeconds: PRODUCTION_OBSERVATION_SECONDS,
    expectedObservationCount: 2,
  });
  const prerequisiteProof = validateDiscordCheckpointCandidatePrerequisites(
    options?.jobList,
    { repository, sourceCommit, workflowRunId, workflowRunAttempt },
  );
  const topologyContract = createDiscordSuccessfulDeploymentTopologyContract();

  const catalogDisposition = sealCanonicalReport({
    schema_id: CATALOG_DISPOSITION_SCHEMA_ID,
    application_id: requirePattern(options?.applicationId, /^[0-9]{17,20}$/u,
      "Discord application ID"),
    catalog_artifact_id: requirePattern(
      options?.catalogArtifactId,
      DECIMAL,
      "catalog recovery artifact ID",
    ),
    catalog_artifact_digest: requirePattern(
      options?.catalogArtifactDigest,
      DIGEST,
      "catalog recovery artifact digest",
    ),
    catalog_recovery_authority: catalogAuthority.report,
    catalog_recovery_authority_file_sha256: catalogAuthority.fileSha256,
    prior_snapshot_sha256: prior.value.snapshot_sha256,
    prior_catalog_sha256: prior.value.catalog_sha256,
    prior_snapshot_file_sha256: prior.fileSha256,
    desired_catalog_sha256: desired.value.catalog_sha256,
    desired_catalog_file_sha256: desired.fileSha256,
    discord_sync_authority: syncAuthority.value,
    discord_sync_authority_file_sha256: syncAuthority.fileSha256,
    discord_sync_report: syncReport.value,
    discord_sync_report_file_sha256: syncReport.fileSha256,
  });
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_SCHEMA_ID,
    repository,
    repository_id: REPOSITORY_ID,
    source_commit: sourceCommit,
    accepted_workflow_run_id: acceptedRunId,
    accepted_workflow_run_attempt: acceptedRunAttempt,
    discord_workflow_run_id: workflowRunId,
    discord_workflow_run_attempt: workflowRunAttempt,
    canonical_acceptance_evidence_sha256: acceptance.value.report_sha256,
    canonical_acceptance_evidence_file_sha256: acceptance.fileSha256,
    release_artifacts: acceptance.value.final_source_fragments.release_artifacts,
    deployment_topology_contract: topologyContract,
    deployment_topology_contract_sha256: topologyContract.report_sha256,
    deployment_prerequisite_job_proof: prerequisiteProof,
    deployment_prerequisite_job_proof_sha256: prerequisiteProof.report_sha256,
    recovery_debt_clearance: clearance.value,
    recovery_debt_clearance_sha256: clearance.value.report_sha256,
    recovery_debt_clearance_file_sha256: clearance.fileSha256,
    catalog_disposition: catalogDisposition,
    catalog_disposition_sha256: catalogDisposition.report_sha256,
    production_observation: observation.value,
    production_observation_sha256: observation.value.report_sha256,
    production_observation_file_sha256: observation.fileSha256,
    expected_artifact_name: checkpointCandidateArtifactName(
      sourceCommit,
      workflowRunId,
      workflowRunAttempt,
    ),
    expected_artifact_leaf: DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE,
  }));
}

export function validateDiscordProductionCheckpointCandidate(value, expected = {}) {
  requirePlainObject(value, "Discord production checkpoint candidate");
  requireExactKeys(value, [
    "schema_id", "repository", "repository_id", "source_commit",
    "accepted_workflow_run_id", "accepted_workflow_run_attempt",
    "discord_workflow_run_id", "discord_workflow_run_attempt",
    "canonical_acceptance_evidence_sha256",
    "canonical_acceptance_evidence_file_sha256", "release_artifacts",
    "deployment_topology_contract", "deployment_topology_contract_sha256",
    "deployment_prerequisite_job_proof", "deployment_prerequisite_job_proof_sha256",
    "recovery_debt_clearance", "recovery_debt_clearance_sha256",
    "recovery_debt_clearance_file_sha256", "catalog_disposition",
    "catalog_disposition_sha256", "production_observation",
    "production_observation_sha256", "production_observation_file_sha256",
    "expected_artifact_name", "expected_artifact_leaf", "report_sha256",
  ], "Discord production checkpoint candidate");
  verifyCanonicalReportHash(value, "Discord production checkpoint candidate");
  if (
    value.schema_id !== DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_SCHEMA_ID ||
    value.repository_id !== REPOSITORY_ID ||
    value.repository !== requirePattern(expected.repository ?? value.repository,
      REPOSITORY, "repository") ||
    value.source_commit !== requirePattern(expected.sourceCommit ?? value.source_commit,
      SHA, "source commit") ||
    value.accepted_workflow_run_id !== requirePattern(
      expected.acceptedRunId ?? value.accepted_workflow_run_id, DECIMAL, "accepted run ID") ||
    value.accepted_workflow_run_attempt !== requirePattern(
      expected.acceptedRunAttempt ?? value.accepted_workflow_run_attempt,
      DECIMAL, "accepted run attempt") ||
    value.discord_workflow_run_id !== requirePattern(
      expected.workflowRunId ?? value.discord_workflow_run_id,
      DECIMAL, "workflow run ID") ||
    value.discord_workflow_run_attempt !== requirePattern(
      expected.workflowRunAttempt ?? value.discord_workflow_run_attempt,
      DECIMAL, "workflow run attempt")
  ) throw new Error("Discord production checkpoint candidate identity differs");
  for (const field of [
    "canonical_acceptance_evidence_sha256",
    "canonical_acceptance_evidence_file_sha256",
    "deployment_topology_contract_sha256",
    "deployment_prerequisite_job_proof_sha256",
    "recovery_debt_clearance_sha256", "recovery_debt_clearance_file_sha256",
    "catalog_disposition_sha256", "production_observation_sha256",
    "production_observation_file_sha256",
  ]) requirePattern(value[field], SHA256, `checkpoint candidate ${field}`);
  if (
    value.deployment_topology_contract_sha256 !==
      value.deployment_topology_contract?.report_sha256 ||
    value.deployment_prerequisite_job_proof_sha256 !==
      value.deployment_prerequisite_job_proof?.report_sha256 ||
    value.recovery_debt_clearance_sha256 !== value.recovery_debt_clearance?.report_sha256 ||
    value.catalog_disposition_sha256 !== value.catalog_disposition?.report_sha256 ||
    value.production_observation_sha256 !== value.production_observation?.report_sha256 ||
    value.expected_artifact_name !== checkpointCandidateArtifactName(
      value.source_commit,
      value.discord_workflow_run_id,
      value.discord_workflow_run_attempt,
    ) || value.expected_artifact_leaf !== DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE
  ) throw new Error("Discord production checkpoint candidate nested authority differs");
  validateDiscordSuccessfulDeploymentTopologyContract(value.deployment_topology_contract);
  validateDiscordCheckpointPrerequisiteProof(
    value.deployment_prerequisite_job_proof,
    {
      repository: value.repository,
      sourceCommit: value.source_commit,
      workflowRunId: value.discord_workflow_run_id,
      workflowRunAttempt: value.discord_workflow_run_attempt,
    },
  );
  verifyCanonicalReportHash(value.recovery_debt_clearance, "checkpoint debt clearance");
  verifyCanonicalReportHash(value.catalog_disposition, "checkpoint catalog disposition");
  validateProductionObservationReport(value.production_observation, {
    expectedSourceCommit: value.source_commit,
    expectedDurationSeconds: PRODUCTION_OBSERVATION_SECONDS,
    expectedIntervalSeconds: PRODUCTION_OBSERVATION_SECONDS,
    expectedObservationCount: 2,
  });
  validateReleaseArtifacts(value.release_artifacts, value.source_commit);
  if (
    expected.canonicalAcceptanceEvidenceSha256 !== undefined &&
    value.canonical_acceptance_evidence_sha256 !==
      requirePattern(expected.canonicalAcceptanceEvidenceSha256, SHA256,
        "canonical acceptance evidence SHA-256")
  ) throw new Error("Discord checkpoint canonical acceptance report differs");
  if (
    expected.canonicalAcceptanceEvidenceFileSha256 !== undefined &&
    value.canonical_acceptance_evidence_file_sha256 !==
      requirePattern(expected.canonicalAcceptanceEvidenceFileSha256, SHA256,
        "canonical acceptance evidence file SHA-256")
  ) throw new Error("Discord checkpoint canonical acceptance bytes differ");
  if (
    expected.releaseArtifacts !== undefined &&
    canonicalJson(value.release_artifacts) !== canonicalJson(expected.releaseArtifacts)
  ) throw new Error("Discord checkpoint release artifacts differ from accepted evidence");
  validateRecoveryDebtClearance(value.recovery_debt_clearance, {
    repository: value.repository,
    sourceCommit: value.source_commit,
    workflowRunId: value.discord_workflow_run_id,
    workflowRunAttempt: value.discord_workflow_run_attempt,
  });
  validateCatalogDisposition(value.catalog_disposition, value);
  return value;
}

function validateReleaseArtifacts(value, sourceCommit) {
  const roles = ["linux-cli", "windows-cli", "windows-gui"];
  if (!Array.isArray(value) || value.length !== roles.length) {
    throw new Error("Discord production checkpoint release artifacts are not exact");
  }
  for (const [index, entry] of value.entries()) {
    requirePlainObject(entry, "Discord checkpoint release artifact");
    requireExactKeys(entry, [
      "role", "name", "sha256", "size_bytes", "source_commit",
    ], "Discord checkpoint release artifact");
    if (
      entry.role !== roles[index] || typeof entry.name !== "string" ||
      entry.name.length < 1 || entry.name.includes("/") || entry.name.includes("\\") ||
      !SHA256.test(entry.sha256 ?? "") || !Number.isSafeInteger(entry.size_bytes) ||
      entry.size_bytes < 1 || entry.source_commit !== sourceCommit
    ) throw new Error("Discord production checkpoint release artifacts are not exact");
  }
}

function validateRecoveryDebtClearance(value, expected) {
  requirePlainObject(value, "Discord recovery-debt clearance");
  requireExactKeys(value, [
    "schema_id", "repository", "current_workflow_run_id",
    "current_workflow_run_attempt", "current_source_commit", "checkpoint_sha256",
    "plan_sha256", "cleared_debts", "report_sha256",
  ], "Discord recovery-debt clearance");
  verifyCanonicalReportHash(value, "Discord recovery-debt clearance");
  if (
    value.schema_id !== DISCORD_RECOVERY_DEBT_CLEARANCE_SCHEMA_ID ||
    value.repository !== expected.repository ||
    value.current_source_commit !== expected.sourceCommit ||
    value.current_workflow_run_id !== expected.workflowRunId ||
    value.current_workflow_run_attempt !== expected.workflowRunAttempt ||
    !SHA256.test(value.checkpoint_sha256) || !SHA256.test(value.plan_sha256) ||
    !Array.isArray(value.cleared_debts)
  ) throw new Error("Discord recovery-debt clearance differs from checkpoint identity");
}

function validateCatalogDisposition(value, candidate) {
  requirePlainObject(value, "Discord production catalog disposition");
  requireExactKeys(value, [
    "schema_id", "application_id", "catalog_artifact_id",
    "catalog_artifact_digest", "catalog_recovery_authority",
    "catalog_recovery_authority_file_sha256", "prior_snapshot_sha256",
    "prior_catalog_sha256", "prior_snapshot_file_sha256", "desired_catalog_sha256",
    "desired_catalog_file_sha256", "discord_sync_authority",
    "discord_sync_authority_file_sha256", "discord_sync_report",
    "discord_sync_report_file_sha256", "report_sha256",
  ], "Discord production catalog disposition");
  verifyCanonicalReportHash(value, "Discord production catalog disposition");
  if (
    value.schema_id !== CATALOG_DISPOSITION_SCHEMA_ID ||
    !DECIMAL.test(value.catalog_artifact_id ?? "") ||
    !DIGEST.test(value.catalog_artifact_digest ?? "") ||
    !/^[0-9]{17,20}$/u.test(value.application_id ?? "") ||
    value.catalog_recovery_authority?.source_commit !== candidate.source_commit ||
    value.catalog_recovery_authority?.workflow_run_id !==
      candidate.discord_workflow_run_id ||
    value.catalog_recovery_authority?.workflow_run_attempt !==
      candidate.discord_workflow_run_attempt ||
    value.discord_sync_authority?.report_sha256 !==
      value.discord_sync_report?.command_sync_authority_sha256 ||
    value.discord_sync_report?.current_before_sha256 !== value.prior_catalog_sha256 ||
    value.discord_sync_report?.current_after_sha256 !== value.desired_catalog_sha256
  ) throw new Error("Discord production catalog disposition is inconsistent");
  for (const field of [
    "catalog_recovery_authority_file_sha256", "prior_snapshot_sha256",
    "prior_catalog_sha256", "prior_snapshot_file_sha256", "desired_catalog_sha256",
    "desired_catalog_file_sha256", "discord_sync_authority_file_sha256",
    "discord_sync_report_file_sha256",
  ]) requirePattern(value[field], SHA256, `catalog disposition ${field}`);
}

async function readCanonicalFile(path, label) {
  const target = resolve(String(path ?? ""));
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
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const temporary = `${target}.tmp-${process.pid}-${Date.now().toString(36)}`;
  const handle = await open(temporary, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(value)}\n`, "utf8");
    await handle.sync();
  } finally { await handle.close(); }
  try { await link(temporary, target); } finally {
    await unlink(temporary).catch((error) => {
      if (error?.code !== "ENOENT") throw error;
    });
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Discord checkpoint path contains a link or non-directory");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requirePlainObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
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
      "accepted-run-id": { type: "string" },
      "accepted-run-attempt": { type: "string" },
      version: { type: "string" },
      "base-path": { type: "string" },
      "application-id": { type: "string" },
      "catalog-artifact-id": { type: "string" },
      "catalog-artifact-digest": { type: "string" },
      "canonical-acceptance-evidence": { type: "string" },
      "recovery-debt-clearance": { type: "string" },
      "catalog-recovery-authority": { type: "string" },
      "prior-catalog-snapshot": { type: "string" },
      "desired-catalog": { type: "string" },
      "sync-authority": { type: "string" },
      "sync-report": { type: "string" },
      "production-observation": { type: "string" },
      "job-list": { type: "string" },
      report: { type: "string" },
      output: { type: "string" },
    },
    strict: true,
    allowPositionals: true,
  });
}

async function main() {
  const { values, positionals } = parseCli();
  try {
    if (
      positionals.length !== 1 ||
      !["seal-candidate", "verify-candidate", "verify-prerequisites"].includes(positionals[0])
    ) {
      throw new Error(
        "Discord checkpoint operation must be seal-candidate, verify-candidate, or verify-prerequisites",
      );
    }
    const identity = {
      repository: values.repository,
      sourceCommit: values["source-commit"],
      workflowRunId: values["workflow-run-id"],
      workflowRunAttempt: values["workflow-run-attempt"],
      acceptedRunId: values["accepted-run-id"],
      acceptedRunAttempt: values["accepted-run-attempt"],
    };
    if (positionals[0] === "verify-prerequisites") {
      const jobList = JSON.parse(await readFile(resolve(values["job-list"]), "utf8"));
      const proof = validateDiscordCheckpointCandidatePrerequisites(jobList, identity);
      process.stdout.write(
        `discord_checkpoint_prerequisites=verified sha256=${proof.report_sha256}\n`,
      );
    } else if (positionals[0] === "seal-candidate") {
      const candidate = await createDiscordProductionCheckpointCandidate({
        ...identity,
        version: values.version,
        basePath: values["base-path"],
        applicationId: values["application-id"],
        catalogArtifactId: values["catalog-artifact-id"],
        catalogArtifactDigest: values["catalog-artifact-digest"],
        canonicalAcceptanceEvidence: values["canonical-acceptance-evidence"],
        recoveryDebtClearance: values["recovery-debt-clearance"],
        catalogRecoveryAuthority: values["catalog-recovery-authority"],
        priorCatalogSnapshot: values["prior-catalog-snapshot"],
        desiredCatalog: values["desired-catalog"],
        syncAuthority: values["sync-authority"],
        syncReport: values["sync-report"],
        productionObservation: values["production-observation"],
        jobList: JSON.parse(await readFile(resolve(values["job-list"]), "utf8")),
      });
      await writeCanonicalNew(values.output, candidate);
      process.stdout.write(`discord_checkpoint_candidate=sealed sha256=${candidate.report_sha256}\n`);
    } else {
      const input = await readCanonicalFile(values.report, "Discord checkpoint candidate");
      validateDiscordProductionCheckpointCandidate(input.value, identity);
      process.stdout.write(`discord_checkpoint_candidate=verified sha256=${input.value.report_sha256}\n`);
    }
  } catch (error) {
    process.stderr.write(
      `discord_checkpoint_candidate=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();

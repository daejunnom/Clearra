#!/usr/bin/env node

import { createHash } from "node:crypto";
import { link, lstat, open, readFile, readdir, unlink } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  canonicalTimestamp,
  requireExactKeys,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  validateDiscordCatalogRecoveryDisposition,
  verifyDiscordCatalogRecoveryAuthority,
} from "./discord-catalog-recovery-authority.mjs";
import { validateDiscordCatalogRestoreReport } from
  "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";

export const DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID =
  "clearra.discord-deployment-recovery-authority.v1";
export const DISCORD_PRESTAGE_INTENT_SCHEMA_ID =
  "clearra.discord-deployment-prestage-intent.v1";
export const DISCORD_RECOVERY_RESULT_SCHEMA_ID =
  "clearra.discord-deployment-recovery-result.v1";
export const DISCORD_RUN_JOB_CATALOG_SCHEMA_ID =
  "clearra.discord-deployment-run-job-catalog.v1";
export const DISCORD_RUN_ATTEMPT_CATALOG_SCHEMA_ID =
  "clearra.discord-deployment-primary-attempt-catalog.v1";
export const DISCORD_RECOVERY_RESULT_FILE = "recovery-result.json";
export const DISCORD_PROTECTED_RECOVERY_AUTHORITY_FILE =
  "protected-recovery-authority.json";
export const DISCORD_CATALOG_RECOVERY_DISPOSITION_FILE =
  "discord-catalog-recovery-disposition.json";
const CATALOG_RECOVERY_EVIDENCE_FILES = Object.freeze([
  "discord-catalog-recovery-authority.json",
  "discord-catalog-restore.json",
  "discord-catalog.json",
  "discord-prior-catalog.json",
  "discord-sync-authority.json",
]);

const PRIMARY_WORKFLOW_NAME = "Deploy Discord Production";
const PRIMARY_WORKFLOW_PATH = ".github/workflows/discord-deploy.yml";
const CLEARRA_REPOSITORY_ID = 1309293231;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const SHA256_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const RECOVERABLE_CONCLUSIONS = new Set(["failure", "cancelled", "timed_out"]);
const PRIMARY_EVENTS = new Set(["workflow_dispatch", "workflow_run"]);
const JOB_CONCLUSIONS = new Set([
  "success",
  "failure",
  "cancelled",
  "skipped",
  "timed_out",
  "action_required",
  "neutral",
  "stale",
  "startup_failure",
]);
const PRESTAGE_UPLOAD_STEP =
  "Upload prestage authority before Oracle freeze or Cloud zero traffic";
const LIVE_UPLOAD_STEP =
  "Upload live-transition authority before Oracle activation or Cloud traffic";
const CATALOG_CAPTURE_STEP =
  "Capture and seal Discord catalog recovery authority before mutation";
const CATALOG_UPLOAD_STEP =
  "Upload Discord catalog recovery authority before global mutation";
const CATALOG_MUTATION_STEP =
  "Authority-bound global sync and sole canonical four-surface observation";
export const DISCORD_CHECKPOINT_JOB_CAPTURE_STEP =
  "Capture exact completed deployment prerequisites for the checkpoint candidate";
export const DISCORD_CHECKPOINT_CANDIDATE_SEAL_STEP =
  "Seal canonical Discord production checkpoint candidate";
export const DISCORD_CHECKPOINT_CANDIDATE_UPLOAD_STEP =
  "Upload canonical Discord production checkpoint candidate";
const RUNTIME_MUTATION_STEPS = Object.freeze([
  "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
  "Activate Oracle, verify the real path, then cut Cloud to 100 percent",
]);
const PRIMARY_JOB_NAMES = Object.freeze([
  "Prepare immutable Discord candidate inputs",
  "authority",
  "promote",
  "sync-observe",
].sort((left, right) => left.localeCompare(right, "en")));
const PRIMARY_PROMOTE_STEP_NAMES = new Set([
  "Set up job",
  "Check out the exact accepted source for protected promotion",
  "Set up Node.js for protected release validators",
  "Download the exact prepared state",
  "Authenticate the protected deployer identity",
  "Set up gcloud for protected promotion",
  "Materialize the protected-environment Oracle key for the real path gate",
  "Capture and seal prestage recovery authority",
  PRESTAGE_UPLOAD_STEP,
  RUNTIME_MUTATION_STEPS[0],
  LIVE_UPLOAD_STEP,
  RUNTIME_MUTATION_STEPS[1],
  "Upload sealed promoted state",
  "Compensate any protected-path failure after Oracle transition began",
  "Always remove the temporary Oracle key",
  "Post Authenticate the protected deployer identity",
  "Post Set up Node.js for protected release validators",
  "Post Check out the exact accepted source for protected promotion",
  "Complete job",
]);
const SUCCESSFUL_JOB_STEP_NAMES = Object.freeze({
  authority: new Set([
    "Set up job",
    "Check out main for exact authority resolution",
    "Resolve exact current main and one canonical acceptance",
    "Classify deployment impact from the last production tag",
    "Reject unresolved earlier Discord recovery debt before candidate work",
    "Upload exact recovery-debt clearance before candidate work",
    "Record explicit no-op for changes outside Discord",
    "Post Check out main for exact authority resolution",
    "Complete job",
  ]),
  "Prepare immutable Discord candidate inputs": new Set([
    "Set up job",
    "Check out the exact accepted source for candidate preparation",
    "Set up Node.js for immutable candidate preparation",
    "Download the exact pre-candidate recovery-debt clearance",
    "Authenticate the Cloud-Build-only identity",
    "Set up gcloud for Cloud-Build-only preparation",
    "Download canonical acceptance evidence without rebuilding products",
    "Download the already accepted CTK3 distribution",
    "Verify accepted evidence and package only runtime dependencies",
    "Build the exact source archive once in Cloud Build",
    "Seal the approval-free accepted-input and immutable-build state",
    "Upload sealed prepared state",
    "Post Authenticate the Cloud-Build-only identity",
    "Post Set up Node.js for immutable candidate preparation",
    "Post Check out the exact accepted source for candidate preparation",
    "Complete job",
  ]),
  promote: PRIMARY_PROMOTE_STEP_NAMES,
  "sync-observe": new Set([
    "Set up job",
    "Check out the exact accepted source for global synchronization",
    "Set up Node.js for global synchronization",
    "Install the frozen runtime dependencies for synchronization",
    "Download the exact protected promotion evidence",
    "Resolve one successful exact-SHA Pages deployment before global mutation",
    "Download the exact Pages deployment authority",
    "Verify Pages authority before Discord global mutation",
    "Authenticate command sync without Cloud mutation authority",
    "Set up gcloud for command synchronization",
    "Materialize the Oracle key for read-only observation",
    "Capture and seal Discord catalog recovery authority before mutation",
    "Upload Discord catalog recovery authority before global mutation",
    "Authority-bound global sync and sole canonical four-surface observation",
    "Upload durable sync and sole canonical observation evidence",
    DISCORD_CHECKPOINT_JOB_CAPTURE_STEP,
    DISCORD_CHECKPOINT_CANDIDATE_SEAL_STEP,
    DISCORD_CHECKPOINT_CANDIDATE_UPLOAD_STEP,
    "Compensate catalog mutation if any later sync job step failed",
    "Upload durable catalog compensation evidence",
    "Always remove the temporary Oracle key",
    "Post Authenticate command sync without Cloud mutation authority",
    "Post Set up Node.js for global synchronization",
    "Post Check out the exact accepted source for global synchronization",
    "Complete job",
  ]),
});

export const DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES = PRIMARY_JOB_NAMES;
export const DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_STEPS = Object.freeze(
  Object.fromEntries(PRIMARY_JOB_NAMES.map((name) => [
    name,
    Object.freeze([...SUCCESSFUL_JOB_STEP_NAMES[name]]),
  ])),
);

export function createDiscordSuccessfulDeploymentTopologyContract() {
  return Object.freeze(sealCanonicalReport({
    schema_id: "clearra.discord-successful-deployment-topology-contract.v1",
    workflow_path: PRIMARY_WORKFLOW_PATH,
    jobs: PRIMARY_JOB_NAMES.map((jobName) => Object.freeze({
      job_name: jobName,
      steps: [...SUCCESSFUL_JOB_STEP_NAMES[jobName]].map((name) => Object.freeze({
        name,
        expected_conclusion: (
          (jobName === "authority" && name === "Record explicit no-op for changes outside Discord") ||
          (jobName === "promote" &&
            name === "Compensate any protected-path failure after Oracle transition began") ||
          (jobName === "sync-observe" && [
            "Compensate catalog mutation if any later sync job step failed",
            "Upload durable catalog compensation evidence",
          ].includes(name))
        ) ? "skipped" : "success",
      })),
    })),
  }));
}

export function validateDiscordSuccessfulDeploymentTopologyContract(value) {
  const expected = createDiscordSuccessfulDeploymentTopologyContract();
  verifyCanonicalReportHash(value, "Discord successful deployment topology contract");
  if (canonicalJson(value) !== canonicalJson(expected)) {
    throw new Error("Discord successful deployment topology contract differs");
  }
  return value;
}
const RESULT_BINDINGS = Object.freeze({
  prestage: Object.freeze([
    "cloud_candidate_residue_readback",
    "cloud_cleanup_readback",
    "cloud_pre_mutation_readback",
    "intended_candidate_authority",
    "oracle_backup_cleanup",
    "oracle_inactive_cleanup",
    "oracle_rollback_capture",
    "prestage_state",
    "recovery_authority",
  ]),
  live: Object.freeze([
    "candidate_state",
    "cloud_candidate_residue_readback",
    "cloud_pre_mutation_classification",
    "cloud_prior_authority",
    "cloud_restore_readback",
    "oracle_pre_mutation_classification",
    "oracle_restore_attestation",
    "oracle_rollback_capture",
    "oracle_stage_manifest",
    "prestage_state",
    "recovery_authority",
  ]),
});

export function resolveDiscordRecoveryAuthority(run, artifactList, options) {
  requirePlainObject(run, "Discord recovery workflow run");
  requirePlainObject(artifactList, "Discord recovery artifact list");
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const expectedRunId = requireDecimalId(options?.workflowRunId, "workflow run ID");
  const expectedRunAttempt = requireDecimalId(
    options?.workflowRunAttempt,
    "workflow run attempt",
  );
  const recoveryWorkflowRunId = requireDecimalId(
    options?.recoveryWorkflowRunId,
    "recovery workflow run ID",
  );
  const recoveryWorkflowRunAttempt = requireDecimalId(
    options?.recoveryWorkflowRunAttempt,
    "recovery workflow run attempt",
  );
  const runId = requireDecimalId(run.id, "workflow run API ID");
  const runAttempt = requireDecimalId(run.run_attempt, "workflow run API attempt");
  const runNumber = requireDecimalId(run.run_number, "workflow run API number");
  const sourceCommit = requirePattern(run.head_sha, SOURCE_COMMIT, "workflow source commit");
  const runCreatedAt = requireGitHubTimestamp(run.created_at, "workflow run created-at");
  const runStartedAt = requireGitHubTimestamp(run.run_started_at, "workflow run started-at");
  const runUpdatedAt = requireGitHubTimestamp(run.updated_at, "workflow run updated-at");
  if (runCreatedAt > runStartedAt || runStartedAt > runUpdatedAt) {
    throw new Error("Discord recovery workflow run timestamps are internally inconsistent");
  }
  if (
    runId !== expectedRunId ||
    runAttempt !== expectedRunAttempt ||
    run.name !== PRIMARY_WORKFLOW_NAME ||
    run.path !== PRIMARY_WORKFLOW_PATH ||
    !PRIMARY_EVENTS.has(run.event) ||
    run.head_branch !== "main" ||
    run.repository?.id !== CLEARRA_REPOSITORY_ID ||
    run.repository?.full_name !== repository ||
    run.head_repository?.id !== CLEARRA_REPOSITORY_ID ||
    run.head_repository?.full_name !== repository ||
    run.status !== "completed" ||
    !RECOVERABLE_CONCLUSIONS.has(run.conclusion)
  ) {
    throw new Error("Discord recovery run differs from the exact completed primary authority");
  }
  const freshnessProof = validateDiscordRecoveryFreshness(options?.runList, {
    repository,
    sourceCommit,
    workflowRunId: runId,
    workflowRunAttempt: runAttempt,
    runNumber,
    runCreatedAt,
    runStartedAt,
    runUpdatedAt,
    runAttemptCatalog: options?.runAttemptCatalog,
    runJobCatalog: options?.runJobCatalog,
  });
  if (
    !Number.isSafeInteger(artifactList.total_count) ||
    artifactList.total_count < 0 ||
    !Array.isArray(artifactList.artifacts) ||
    artifactList.artifacts.length !== artifactList.total_count
  ) {
    throw new Error("Discord recovery artifact list must be complete and non-truncated");
  }
  const prestageName =
    `discord-prestage-recovery-authority-${sourceCommit}-run-${runId}-attempt-${runAttempt}`;
  const liveName =
    `discord-live-recovery-authority-${sourceCommit}-run-${runId}-attempt-${runAttempt}`;
  const catalogName =
    `discord-catalog-recovery-authority-${sourceCommit}-run-${runId}-attempt-${runAttempt}`;
  const prestageMatches = artifactList.artifacts.filter((artifact) => artifact?.name === prestageName);
  const liveMatches = artifactList.artifacts.filter((artifact) => artifact?.name === liveName);
  const catalogMatches = artifactList.artifacts.filter((artifact) => artifact?.name === catalogName);
  if (prestageMatches.length > 1 || liveMatches.length > 1 || catalogMatches.length > 1) {
    throw new Error("Discord recovery found ambiguous staged authority artifacts");
  }
  if (prestageMatches.length === 0 && liveMatches.length !== 0) {
    throw new Error("Discord live recovery authority exists without its prestage parent");
  }
  if (prestageMatches.length === 0) {
    const noMutationProof = validateNoPrestageArtifactAuthority(options?.jobList, {
      repository,
      sourceCommit,
      workflowRunId: runId,
      workflowRunAttempt: runAttempt,
    });
    const catalogRecovery = resolveCatalogRecoveryArtifactAuthority(
      catalogMatches,
      options?.jobList,
      { repository, sourceCommit, workflowRunId: runId, workflowRunAttempt: runAttempt,
        runStartedAt, runUpdatedAt, recoveryStage: "none", artifactName: catalogName },
    );
    const authority = Object.freeze(sealCanonicalReport({
      schema_id: DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID,
      repository,
      primary_workflow_name: PRIMARY_WORKFLOW_NAME,
      primary_workflow_path: PRIMARY_WORKFLOW_PATH,
      source_commit: sourceCommit,
      workflow_run_id: runId,
      workflow_run_attempt: runAttempt,
      recovery_workflow_run_id: recoveryWorkflowRunId,
      recovery_workflow_run_attempt: recoveryWorkflowRunAttempt,
      workflow_event: run.event,
      workflow_conclusion: run.conclusion,
      recovery_required: false,
      recovery_stage: "none",
      recovery_reason: "job-steps-prove-no-prestage-upload-or-runtime-mutation",
      artifact_id: null,
      artifact_name: prestageName,
      artifact_digest: null,
      artifact_size: null,
      artifact_created_at: null,
      freshness_proof: freshnessProof,
      no_mutation_job_step_proof: noMutationProof,
      prestage_only_job_step_proof: null,
      live_transition_job_step_proof: null,
      ...catalogRecovery,
    }));
    validateDiscordRecoveryAuthorityReport(authority, {
      repository,
      sourceCommit,
      workflowRunId: runId,
      workflowRunAttempt: runAttempt,
      recoveryWorkflowRunId,
      recoveryWorkflowRunAttempt,
      workflowEvent: run.event,
      workflowConclusion: run.conclusion,
    });
    return authority;
  }
  const recoveryStage = liveMatches.length === 1 ? "live" : "prestage";
  const validatedPrestage = validateRecoveryArtifact(prestageMatches[0], {
    runId,
    sourceCommit,
    runStartedAt,
    runUpdatedAt,
    label: "Discord prestage recovery artifact",
  });
  const artifact = recoveryStage === "live" ? liveMatches[0] : prestageMatches[0];
  const artifactName = recoveryStage === "live" ? liveName : prestageName;
  const validatedArtifact = validateRecoveryArtifact(artifact, {
    runId,
    sourceCommit,
    runStartedAt,
    runUpdatedAt,
    label: "Discord recovery artifact",
  });
  if (recoveryStage === "live" && validatedArtifact.createdAt < validatedPrestage.createdAt) {
    throw new Error("Discord live recovery artifact predates its prestage parent");
  }
  const prestageOnlyProof = recoveryStage === "prestage"
    ? validatePrestageOnlyArtifactAuthority(options?.jobList, {
      repository,
      sourceCommit,
      workflowRunId: runId,
      workflowRunAttempt: runAttempt,
    })
    : null;
  const liveTransitionProof = recoveryStage === "live"
    ? validateLiveArtifactAuthority(options?.jobList, {
      repository,
      sourceCommit,
      workflowRunId: runId,
      workflowRunAttempt: runAttempt,
    })
    : null;
  const catalogRecovery = resolveCatalogRecoveryArtifactAuthority(
    catalogMatches,
    options?.jobList,
    { repository, sourceCommit, workflowRunId: runId, workflowRunAttempt: runAttempt,
      runStartedAt, runUpdatedAt, recoveryStage, artifactName: catalogName },
  );
  const authority = Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID,
    repository,
    primary_workflow_name: PRIMARY_WORKFLOW_NAME,
    primary_workflow_path: PRIMARY_WORKFLOW_PATH,
    source_commit: sourceCommit,
    workflow_run_id: runId,
    workflow_run_attempt: runAttempt,
    recovery_workflow_run_id: recoveryWorkflowRunId,
    recovery_workflow_run_attempt: recoveryWorkflowRunAttempt,
    workflow_event: run.event,
    workflow_conclusion: run.conclusion,
    recovery_required: true,
    recovery_stage: recoveryStage,
    recovery_reason: recoveryStage === "live"
      ? "live-transition-authority-present"
      : "prestage-only-cleanup-required",
    artifact_id: validatedArtifact.id,
    artifact_name: artifactName,
    artifact_digest: validatedArtifact.digest,
    artifact_size: validatedArtifact.size,
    artifact_created_at: validatedArtifact.createdAtText,
    freshness_proof: freshnessProof,
    no_mutation_job_step_proof: null,
    prestage_only_job_step_proof: prestageOnlyProof,
    live_transition_job_step_proof: liveTransitionProof,
    ...catalogRecovery,
  }));
  validateDiscordRecoveryAuthorityReport(authority, {
    repository,
    sourceCommit,
    workflowRunId: runId,
    workflowRunAttempt: runAttempt,
    recoveryWorkflowRunId,
    recoveryWorkflowRunAttempt,
    workflowEvent: run.event,
    workflowConclusion: run.conclusion,
  });
  return authority;
}

export function validateDiscordRecoveryAuthorityReport(report, options) {
  requirePlainObject(report, "Discord recovery authority report");
  requireExactKeys(report, [
    "schema_id", "repository", "primary_workflow_name", "primary_workflow_path",
    "source_commit", "workflow_run_id", "workflow_run_attempt",
    "recovery_workflow_run_id", "recovery_workflow_run_attempt", "workflow_event",
    "workflow_conclusion", "recovery_required", "recovery_stage", "recovery_reason",
    "artifact_id", "artifact_name", "artifact_digest", "artifact_size",
    "artifact_created_at", "freshness_proof", "no_mutation_job_step_proof",
    "prestage_only_job_step_proof", "live_transition_job_step_proof",
    "catalog_recovery_required", "catalog_artifact_id", "catalog_artifact_name",
    "catalog_artifact_digest", "catalog_artifact_size", "catalog_artifact_created_at",
    "catalog_mutation_job_step_proof", "report_sha256",
  ], "Discord recovery authority report");
  verifyCanonicalReportHash(report, "Discord recovery authority report");
  const expected = Object.freeze({
    repository: requirePattern(options?.repository, REPOSITORY, "repository"),
    source_commit: requirePattern(options?.sourceCommit, SOURCE_COMMIT, "source commit"),
    workflow_run_id: requireDecimalId(options?.workflowRunId, "workflow run ID"),
    workflow_run_attempt: requireDecimalId(options?.workflowRunAttempt, "workflow run attempt"),
    recovery_workflow_run_id: requireDecimalId(
      options?.recoveryWorkflowRunId,
      "recovery workflow run ID",
    ),
    recovery_workflow_run_attempt: requireDecimalId(
      options?.recoveryWorkflowRunAttempt,
      "recovery workflow run attempt",
    ),
    workflow_event: String(options?.workflowEvent ?? ""),
    workflow_conclusion: String(options?.workflowConclusion ?? ""),
  });
  if (
    report.schema_id !== DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID ||
    report.primary_workflow_name !== PRIMARY_WORKFLOW_NAME ||
    report.primary_workflow_path !== PRIMARY_WORKFLOW_PATH ||
    !PRIMARY_EVENTS.has(expected.workflow_event) ||
    !RECOVERABLE_CONCLUSIONS.has(expected.workflow_conclusion) ||
    Object.entries(expected).some(([field, value]) => report[field] !== value)
  ) {
    throw new Error("Discord recovery authority differs from its exact workflow parent");
  }
  validateFreshnessProofReport(report.freshness_proof, expected);
  const prestageName =
    `discord-prestage-recovery-authority-${expected.source_commit}-run-${expected.workflow_run_id}-attempt-${expected.workflow_run_attempt}`;
  const liveName =
    `discord-live-recovery-authority-${expected.source_commit}-run-${expected.workflow_run_id}-attempt-${expected.workflow_run_attempt}`;
  const catalogName =
    `discord-catalog-recovery-authority-${expected.source_commit}-run-${expected.workflow_run_id}-attempt-${expected.workflow_run_attempt}`;
  validateCatalogRecoveryReportFields(report, catalogName);
  if (report.recovery_required === false) {
    if (
      report.recovery_stage !== "none" ||
      report.recovery_reason !== "job-steps-prove-no-prestage-upload-or-runtime-mutation" ||
      report.artifact_id !== null || report.artifact_name !== prestageName ||
      report.artifact_digest !== null || report.artifact_size !== null ||
      report.artifact_created_at !== null || report.prestage_only_job_step_proof !== null ||
      report.live_transition_job_step_proof !== null
    ) {
      throw new Error("Discord no-runtime-mutation recovery authority is incomplete");
    }
    validatePromoteStepProofReport(report.no_mutation_job_step_proof, "none");
    return Object.freeze(report);
  }
  if (
    report.recovery_required !== true ||
    !["prestage", "live"].includes(report.recovery_stage) ||
    report.recovery_reason !== (report.recovery_stage === "live"
      ? "live-transition-authority-present"
      : "prestage-only-cleanup-required") ||
    requireDecimalId(report.artifact_id, "recovery artifact ID") !== report.artifact_id ||
    requirePattern(report.artifact_digest, SHA256_DIGEST, "recovery artifact digest") !==
      report.artifact_digest ||
    !Number.isSafeInteger(report.artifact_size) || report.artifact_size < 1 ||
    requireGitHubTimestamp(report.artifact_created_at, "recovery artifact created-at") < 0 ||
    report.artifact_name !== (report.recovery_stage === "live" ? liveName : prestageName) ||
    report.no_mutation_job_step_proof !== null
  ) {
    throw new Error("Discord runtime recovery authority is incomplete");
  }
  if (report.recovery_stage === "prestage") {
    validatePromoteStepProofReport(report.prestage_only_job_step_proof, "prestage");
    if (report.live_transition_job_step_proof !== null) {
      throw new Error("Discord prestage recovery authority has unexpected live proof");
    }
  } else {
    if (report.prestage_only_job_step_proof !== null) {
      throw new Error("Discord live recovery authority has unexpected prestage-only proof");
    }
    validatePromoteStepProofReport(report.live_transition_job_step_proof, "live");
  }
  return Object.freeze(report);
}

function validateFreshnessProofReport(value, expected) {
  requirePlainObject(value, "Discord recovery freshness proof");
  requireExactKeys(value, [
    "schema_id", "original_workflow_run_id", "original_workflow_run_attempt",
    "original_source_commit", "potential_superseders",
  ], "Discord recovery freshness proof");
  if (
    value.schema_id !== "clearra.discord-deployment-recovery-freshness-proof.v1" ||
    value.original_workflow_run_id !== expected.workflow_run_id ||
    value.original_workflow_run_attempt !== expected.workflow_run_attempt ||
    value.original_source_commit !== expected.source_commit ||
    !Array.isArray(value.potential_superseders)
  ) throw new Error("Discord recovery freshness proof differs from its parent");
  let prior = null;
  const seen = new Set();
  for (const decision of value.potential_superseders) {
    requirePlainObject(decision, "Discord recovery freshness decision");
    requireExactKeys(decision, [
      "workflow_run_id", "workflow_run_attempt", "source_commit", "run_number",
      "status", "created_at", "run_started_at", "updated_at", "decision",
      "no_mutation_job_step_proof",
    ], "Discord recovery freshness decision");
    const id = requireDecimalId(decision.workflow_run_id, "freshness workflow run ID");
    const attempt = requireDecimalId(decision.workflow_run_attempt, "freshness workflow run attempt");
    requireDecimalId(decision.run_number, "freshness workflow run number");
    requirePattern(decision.source_commit, SOURCE_COMMIT, "freshness source commit");
    const createdAt = requireGitHubTimestamp(decision.created_at, "freshness created-at");
    const updatedAt = requireGitHubTimestamp(decision.updated_at, "freshness updated-at");
    const startedAt = decision.run_started_at === null
      ? null
      : requireGitHubTimestamp(decision.run_started_at, "freshness run-started-at");
    if (createdAt > updatedAt || (startedAt !== null && (createdAt > startedAt || startedAt > updatedAt))) {
      throw new Error("Discord recovery freshness decision timestamps are inconsistent");
    }
    if (decision.decision === "queued-behind-shared-group") {
      if (decision.status !== "queued" || decision.no_mutation_job_step_proof !== null) {
        throw new Error("Discord queued freshness decision is invalid");
      }
    } else if (decision.decision === "completed-exact-no-runtime-mutation") {
      if (decision.status !== "completed") {
        throw new Error("Discord completed freshness decision is invalid");
      }
      validatePromoteStepProofReport(decision.no_mutation_job_step_proof, "none");
    } else {
      throw new Error("Discord recovery freshness decision is invalid");
    }
    const key = `${id}:${attempt}`;
    if (seen.has(key) || (prior !== null && compareAuthorityKeys(prior, key) >= 0)) {
      throw new Error("Discord recovery freshness decisions are ambiguous or unordered");
    }
    seen.add(key);
    prior = key;
  }
}

function resolveCatalogRecoveryArtifactAuthority(matches, jobList, options) {
  const jobs = getExactPrimaryJobAuthority(jobList, {
    repository: options.repository,
    sourceCommit: options.sourceCommit,
    workflowRunId: options.workflowRunId,
    workflowRunAttempt: options.workflowRunAttempt,
  });
  const sync = jobs.get("sync-observe");
  const stepProof = (name) => {
    const step = sync.steps.find((candidate) => candidate.name === name);
    return step ? Object.freeze({
      name: step.name,
      number: step.number,
      status: step.status,
      conclusion: step.conclusion,
      started_at: step.started_at,
      completed_at: step.completed_at,
    }) : null;
  };
  const capture = stepProof(CATALOG_CAPTURE_STEP);
  const upload = stepProof(CATALOG_UPLOAD_STEP);
  const mutation = stepProof(CATALOG_MUTATION_STEP);
  const proof = Object.freeze({
    job_id: String(sync.id),
    job_name: sync.name,
    job_status: sync.status,
    job_conclusion: sync.conclusion,
    capture_step: capture,
    upload_step: upload,
    mutation_step: mutation,
  });
  if (matches.length === 0) {
    if (
      upload?.conclusion === "success" ||
      (mutation !== null && mutation.conclusion !== "skipped")
    ) throw new Error("Discord catalog recovery artifact is absent after upload or mutation began");
    return Object.freeze({
      catalog_recovery_required: false,
      catalog_artifact_id: null,
      catalog_artifact_name: options.artifactName,
      catalog_artifact_digest: null,
      catalog_artifact_size: null,
      catalog_artifact_created_at: null,
      catalog_mutation_job_step_proof: proof,
    });
  }
  if (capture?.conclusion !== "success" || upload?.conclusion !== "success") {
    throw new Error("Discord catalog recovery artifact lacks its exact capture-and-upload authority");
  }
  const artifact = validateRecoveryArtifact(matches[0], {
    runId: options.workflowRunId,
    sourceCommit: options.sourceCommit,
    runStartedAt: options.runStartedAt,
    runUpdatedAt: options.runUpdatedAt,
    label: "Discord catalog recovery artifact",
  });
  const captureCompletedAt = requireGitHubTimestamp(
    capture.completed_at,
    "catalog capture completed-at",
  );
  const uploadStartedAt = requireGitHubTimestamp(
    upload.started_at,
    "catalog upload started-at",
  );
  const uploadCompletedAt = requireGitHubTimestamp(
    upload.completed_at,
    "catalog upload completed-at",
  );
  if (
    capture.number >= upload.number || captureCompletedAt > uploadStartedAt ||
    uploadStartedAt > artifact.createdAt || artifact.createdAt > uploadCompletedAt
  ) {
    throw new Error("Discord catalog recovery artifact chronology is not pre-mutation durable");
  }
  if (mutation !== null) {
    const mutationStartedAt = requireGitHubTimestamp(
      mutation.started_at,
      "catalog mutation started-at",
    );
    if (upload.number >= mutation.number || uploadCompletedAt > mutationStartedAt) {
      throw new Error("Discord catalog mutation began before its durable recovery artifact");
    }
  }
  const required = mutation !== null && mutation.conclusion !== "skipped";
  if (required && options.recoveryStage !== "live") {
    throw new Error("Discord catalog mutation lacks the required live runtime recovery parent");
  }
  return Object.freeze({
    catalog_recovery_required: required,
    catalog_artifact_id: artifact.id,
    catalog_artifact_name: options.artifactName,
    catalog_artifact_digest: artifact.digest,
    catalog_artifact_size: artifact.size,
    catalog_artifact_created_at: artifact.createdAtText,
    catalog_mutation_job_step_proof: proof,
  });
}

function validateCatalogRecoveryReportFields(report, expectedName) {
  const proof = report.catalog_mutation_job_step_proof;
  requirePlainObject(proof, "Discord catalog mutation job-step proof");
  requireExactKeys(proof, [
    "job_id", "job_name", "job_status", "job_conclusion",
    "capture_step", "upload_step", "mutation_step",
  ], "Discord catalog mutation job-step proof");
  requireDecimalId(proof.job_id, "catalog mutation job ID");
  if (
    proof.job_name !== "sync-observe" || proof.job_status !== "completed" ||
    !JOB_CONCLUSIONS.has(proof.job_conclusion)
  ) throw new Error("Discord catalog mutation job-step proof is invalid");
  const capture = validateOptionalCatalogStepProof(proof.capture_step, CATALOG_CAPTURE_STEP);
  const upload = validateOptionalCatalogStepProof(proof.upload_step, CATALOG_UPLOAD_STEP);
  const mutation = validateOptionalCatalogStepProof(proof.mutation_step, CATALOG_MUTATION_STEP);
  if (report.catalog_artifact_id === null) {
    if (
      report.catalog_recovery_required !== false ||
      report.catalog_artifact_name !== expectedName ||
      report.catalog_artifact_digest !== null || report.catalog_artifact_size !== null ||
      report.catalog_artifact_created_at !== null || upload?.conclusion === "success" ||
      (mutation !== null && mutation.conclusion !== "skipped")
    ) throw new Error("Discord missing catalog recovery artifact authority is inconsistent");
    return;
  }
  requireDecimalId(report.catalog_artifact_id, "catalog recovery artifact ID");
  requirePattern(report.catalog_artifact_digest, SHA256_DIGEST, "catalog recovery artifact digest");
  const artifactCreatedAt = requireGitHubTimestamp(
    report.catalog_artifact_created_at,
    "catalog recovery artifact created-at",
  );
  if (
    report.catalog_artifact_name !== expectedName ||
    !Number.isSafeInteger(report.catalog_artifact_size) || report.catalog_artifact_size < 1 ||
    capture?.conclusion !== "success" || upload?.conclusion !== "success"
  ) throw new Error("Discord catalog recovery artifact authority is incomplete");
  const captureCompletedAt = requireGitHubTimestamp(
    capture.completed_at,
    "catalog capture completed-at",
  );
  const uploadStartedAt = requireGitHubTimestamp(
    upload.started_at,
    "catalog upload started-at",
  );
  const uploadCompletedAt = requireGitHubTimestamp(
    upload.completed_at,
    "catalog upload completed-at",
  );
  if (
    capture.number >= upload.number || captureCompletedAt > uploadStartedAt ||
    uploadStartedAt > artifactCreatedAt || artifactCreatedAt > uploadCompletedAt
  ) throw new Error("Discord catalog recovery artifact chronology is invalid");
  if (mutation !== null && (
    upload.number >= mutation.number ||
    uploadCompletedAt > requireGitHubTimestamp(
      mutation.started_at,
      "catalog mutation started-at",
    )
  )) throw new Error("Discord catalog recovery artifact is not durable before mutation");
  const required = mutation !== null && mutation.conclusion !== "skipped";
  if (
    report.catalog_recovery_required !== required ||
    (required && report.recovery_stage !== "live")
  ) throw new Error("Discord catalog recovery requirement differs from mutation authority");
}

function validateOptionalCatalogStepProof(value, expectedName) {
  if (value === null) return null;
  requirePlainObject(value, "Discord catalog recovery step proof");
  requireExactKeys(value, [
    "name", "number", "status", "conclusion", "started_at", "completed_at",
  ], "Discord catalog recovery step proof");
  if (
    value.name !== expectedName || !Number.isSafeInteger(value.number) ||
    value.number < 1 || value.status !== "completed" ||
    !JOB_CONCLUSIONS.has(value.conclusion)
  ) throw new Error("Discord catalog recovery step proof is invalid");
  const startedAt = requireGitHubTimestamp(value.started_at, `${expectedName} started-at`);
  const completedAt = requireGitHubTimestamp(value.completed_at, `${expectedName} completed-at`);
  if (startedAt > completedAt) {
    throw new Error("Discord catalog recovery step timestamps are reversed");
  }
  return value;
}

function validatePromoteStepProofReport(value, mode) {
  requirePlainObject(value, "Discord recovery promote-step proof");
  requireExactKeys(value, [
    "job_id", "job_name", "job_status", "job_conclusion", "prestage_upload_step",
    "live_upload_step", "runtime_mutation_steps", "primary_jobs",
  ], "Discord recovery promote-step proof");
  requireDecimalId(value.job_id, "promote proof job ID");
  if (
    value.job_name !== "promote" || value.job_status !== "completed" ||
    !JOB_CONCLUSIONS.has(value.job_conclusion) ||
    !Array.isArray(value.runtime_mutation_steps) ||
    value.runtime_mutation_steps.length !== RUNTIME_MUTATION_STEPS.length
  ) throw new Error("Discord recovery promote-step proof is invalid");
  const prestage = validateOptionalStepProof(value.prestage_upload_step, PRESTAGE_UPLOAD_STEP);
  const live = validateOptionalStepProof(value.live_upload_step, LIVE_UPLOAD_STEP);
  const mutations = value.runtime_mutation_steps.map((step, index) =>
    validateNamedStepProof(step, RUNTIME_MUTATION_STEPS[index]));
  const primaryJobs = validateSealedPrimaryJobs(value.primary_jobs);
  const promote = primaryJobs.get("promote");
  if (
    promote.job_id !== value.job_id || promote.job_status !== value.job_status ||
    promote.job_conclusion !== value.job_conclusion
  ) throw new Error("Discord promote proof differs from its sealed primary topology");
  if (mode === "none") {
    if (
      prestage?.conclusion === "success" || live?.conclusion === "success" ||
      mutations.some((step) => step.conclusion !== null && step.conclusion !== "skipped")
    ) throw new Error("Discord no-runtime-mutation promote proof shows mutation");
  } else if (mode === "prestage" && (
    prestage?.conclusion !== "success" || live?.conclusion === "success" ||
    (mutations[1].conclusion !== null && mutations[1].conclusion !== "skipped")
  )) {
    throw new Error("Discord prestage-only promote proof is invalid");
  } else if (mode === "live" && (
    prestage?.conclusion !== "success" || live?.conclusion !== "success" ||
    mutations[0].conclusion !== "success"
  )) {
    throw new Error("Discord live-transition promote proof is invalid");
  }
}

function validateSealedPrimaryJobs(value) {
  if (!Array.isArray(value) || value.length !== PRIMARY_JOB_NAMES.length) {
    throw new Error("Discord sealed primary job proof is incomplete");
  }
  const result = new Map();
  for (const [jobIndex, job] of value.entries()) {
    requirePlainObject(job, "Discord sealed primary job proof");
    requireExactKeys(job, [
      "job_id", "job_name", "job_status", "job_conclusion", "steps",
    ], "Discord sealed primary job proof");
    if (
      job.job_name !== PRIMARY_JOB_NAMES[jobIndex] ||
      requireDecimalId(job.job_id, "sealed primary job ID") !== job.job_id ||
      job.job_status !== "completed" || !JOB_CONCLUSIONS.has(job.job_conclusion) ||
      !Array.isArray(job.steps)
    ) throw new Error("Discord sealed primary job proof is invalid");
    const expected = [...SUCCESSFUL_JOB_STEP_NAMES[job.job_name]];
    let priorIndex = -1;
    let priorNumber = 0;
    for (const step of job.steps) {
      requirePlainObject(step, "Discord sealed primary step proof");
      requireExactKeys(
        step,
        ["name", "number", "status", "conclusion"],
        "Discord sealed primary step proof",
      );
      const expectedIndex = expected.indexOf(step.name);
      if (
        expectedIndex < 0 || expectedIndex <= priorIndex ||
        !Number.isSafeInteger(step.number) || step.number <= priorNumber ||
        step.status !== "completed" || !JOB_CONCLUSIONS.has(step.conclusion)
      ) throw new Error("Discord sealed primary step proof is foreign or unordered");
      priorIndex = expectedIndex;
      priorNumber = step.number;
    }
    result.set(job.job_name, job);
  }
  return result;
}

function validateOptionalStepProof(value, expectedName) {
  return value === null ? null : validateNamedStepProof(value, expectedName);
}

function validateNamedStepProof(value, expectedName) {
  requirePlainObject(value, "Discord recovery step proof");
  requireExactKeys(value, ["name", "number", "status", "conclusion"], "Discord recovery step proof");
  if (value.name !== expectedName) throw new Error("Discord recovery step proof name differs");
  if (value.number === null && value.status === null && value.conclusion === null) return value;
  if (
    !Number.isSafeInteger(value.number) || value.number < 1 ||
    value.status !== "completed" || !JOB_CONCLUSIONS.has(value.conclusion)
  ) throw new Error("Discord recovery step proof is invalid");
  return value;
}

function compareAuthorityKeys(left, right) {
  const [leftId, leftAttempt] = left.split(":").map(BigInt);
  const [rightId, rightAttempt] = right.split(":").map(BigInt);
  if (leftId !== rightId) return leftId < rightId ? -1 : 1;
  return leftAttempt < rightAttempt ? -1 : leftAttempt > rightAttempt ? 1 : 0;
}

export function validateNoPrestageArtifactAuthority(value, options) {
  const jobs = getExactPrimaryJobAuthority(value, options);
  const promote = jobs.get("promote");
  const sync = jobs.get("sync-observe");
  if (
    promote.conclusion === "success" || sync.conclusion !== "skipped" || sync.steps.length !== 0
  ) throw new Error("Discord no-prestage authority violates the closed dependency topology");
  const steps = promote.steps;
  const upload = steps.find((step) => step.name === PRESTAGE_UPLOAD_STEP);
  if (!upload && promote.conclusion !== "skipped") {
    throw new Error("Discord recovery promote job lacks the expected prestage upload step");
  }
  if (upload?.conclusion === "success") {
    throw new Error("Discord prestage artifact is absent after its upload step succeeded");
  }
  for (const name of [...RUNTIME_MUTATION_STEPS, LIVE_UPLOAD_STEP]) {
    const mutation = steps.find((step) => step.name === name);
    if (mutation && mutation.conclusion !== "skipped") {
      throw new Error("Discord prestage artifact is absent after runtime mutation began");
    }
    if (upload && mutation && mutation.number <= upload.number) {
      throw new Error("Discord recovery promote step order is invalid");
    }
  }
  return buildPromoteStepProof(promote, jobs);
}

export function validatePrestageOnlyArtifactAuthority(value, options) {
  const jobs = getExactPrimaryJobAuthority(value, options);
  const promote = jobs.get("promote");
  const sync = jobs.get("sync-observe");
  if (
    promote.conclusion === "success" || sync.conclusion !== "skipped" || sync.steps.length !== 0
  ) throw new Error("Discord prestage-only authority violates the closed dependency topology");
  const steps = promote.steps;
  const prestageUpload = steps.find((step) => step.name === PRESTAGE_UPLOAD_STEP);
  if (prestageUpload?.conclusion !== "success") {
    throw new Error("Discord prestage artifact exists without its successful upload step");
  }
  const liveUpload = steps.find((step) => step.name === LIVE_UPLOAD_STEP);
  if (liveUpload?.conclusion === "success") {
    throw new Error("Discord live artifact is absent after its upload step succeeded");
  }
  const activation = steps.find((step) => step.name === RUNTIME_MUTATION_STEPS[1]);
  if (activation && activation.conclusion !== "skipped") {
    throw new Error("Discord live artifact is absent after activation began");
  }
  if (
    (liveUpload && liveUpload.number <= prestageUpload.number) ||
    (activation && liveUpload && activation.number <= liveUpload.number)
  ) {
    throw new Error("Discord recovery promote step order is invalid");
  }
  return buildPromoteStepProof(promote, jobs);
}

export function validateLiveArtifactAuthority(value, options) {
  const jobs = getExactPrimaryJobAuthority(value, options);
  const promote = jobs.get("promote");
  const steps = promote.steps;
  const prestage = steps.find((step) => step.name === PRESTAGE_UPLOAD_STEP);
  const stage = steps.find((step) => step.name === RUNTIME_MUTATION_STEPS[0]);
  const live = steps.find((step) => step.name === LIVE_UPLOAD_STEP);
  if (
    prestage?.conclusion !== "success" || stage?.conclusion !== "success" ||
    live?.conclusion !== "success" ||
    !(prestage.number < stage.number && stage.number < live.number)
  ) throw new Error("Discord live artifact lacks exact successful transition-upload authority");
  return buildPromoteStepProof(promote, jobs);
}

function getExactPromoteJobAuthority(value, options) {
  return getExactPrimaryJobAuthority(value, options).get("promote");
}

function getExactPrimaryJobAuthority(value, options) {
  const pages = Array.isArray(value) ? value : [value];
  if (pages.length === 0) throw new Error("Discord recovery job-step authority is unavailable");
  let totalCount = null;
  const jobs = [];
  for (const [index, page] of pages.entries()) {
    requirePlainObject(page, `Discord recovery job page ${index}`);
    if (!Number.isSafeInteger(page.total_count) || page.total_count < 0 || !Array.isArray(page.jobs)) {
      throw new Error("Discord recovery job-step authority is invalid");
    }
    if (totalCount === null) totalCount = page.total_count;
    if (page.total_count !== totalCount) {
      throw new Error("Discord recovery job-step page totals differ");
    }
    jobs.push(...page.jobs);
  }
  if (jobs.length !== totalCount) {
    throw new Error("Discord recovery job-step authority must be complete and non-truncated");
  }
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(options?.sourceCommit, SOURCE_COMMIT, "source commit");
  const workflowRunId = requireDecimalId(options?.workflowRunId, "workflow run ID");
  const workflowRunAttempt = requireDecimalId(
    options?.workflowRunAttempt,
    "workflow run attempt",
  );
  const promote = [];
  const seenJobIds = new Set();
  const seenJobNames = new Set();
  for (const job of jobs) {
    requirePlainObject(job, "Discord recovery job");
    const jobId = requireDecimalId(job.id, "job ID");
    if (seenJobIds.has(jobId)) throw new Error("Discord recovery job-step authority has duplicate jobs");
    seenJobIds.add(jobId);
    if (
      requireDecimalId(job.run_id, "job run ID") !== workflowRunId ||
      requireDecimalId(job.run_attempt, "job run attempt") !== workflowRunAttempt ||
      job.head_sha !== sourceCommit ||
      job.head_branch !== "main" ||
      job.status !== "completed" ||
      !JOB_CONCLUSIONS.has(job.conclusion) ||
      typeof job.name !== "string" ||
      typeof job.html_url !== "string" ||
      !job.html_url.startsWith(`https://github.com/${repository}/actions/runs/${workflowRunId}/job/`) ||
      !Array.isArray(job.steps)
    ) {
      throw new Error("Discord recovery job differs from the exact original run attempt");
    }
    if (seenJobNames.has(job.name)) {
      throw new Error("Discord recovery job-step authority has duplicate job names");
    }
    seenJobNames.add(job.name);
    if (job.name === "promote") promote.push(job);
  }
  const actualJobNames = [...seenJobNames].sort((left, right) => left.localeCompare(right, "en"));
  if (
    actualJobNames.length !== PRIMARY_JOB_NAMES.length ||
    actualJobNames.some((name, index) => name !== PRIMARY_JOB_NAMES[index])
  ) {
    throw new Error("Discord recovery job-step authority differs from the closed primary topology");
  }
  if (promote.length > 1) throw new Error("Discord recovery promote job authority is ambiguous");
  if (promote.length === 0) {
    throw new Error("Discord recovery job-step authority lacks the exact promote job");
  }
  if (!JOB_CONCLUSIONS.has(promote[0].conclusion)) {
    throw new Error("Discord recovery promote job conclusion is invalid");
  }
  for (const job of jobs) {
    const expected = [...SUCCESSFUL_JOB_STEP_NAMES[job.name]];
    let priorIndex = -1;
    let priorNumber = 0;
    const names = new Set();
    for (const step of job.steps) {
      const expectedIndex = expected.indexOf(step.name);
      if (
        expectedIndex < 0 || expectedIndex <= priorIndex || names.has(step.name) ||
        !Number.isSafeInteger(step.number) || step.number <= priorNumber ||
        step.status !== "completed" || !JOB_CONCLUSIONS.has(step.conclusion)
      ) {
        throw new Error("Discord recovery job-step authority contains foreign or unordered topology");
      }
      names.add(step.name);
      priorIndex = expectedIndex;
      priorNumber = step.number;
    }
  }
  const steps = promote[0].steps;
  if (promote[0].conclusion === "skipped" && steps.length !== 0) {
    throw new Error("Discord recovery skipped promote job unexpectedly has steps");
  }
  const seenStepNames = new Set();
  for (const step of steps) {
    requirePlainObject(step, "Discord recovery promote step");
    if (
      typeof step.name !== "string" ||
      seenStepNames.has(step.name) ||
      !PRIMARY_PROMOTE_STEP_NAMES.has(step.name) ||
      !Number.isSafeInteger(step.number) ||
      step.number < 1 ||
      step.status !== "completed" ||
      !JOB_CONCLUSIONS.has(step.conclusion)
    ) {
      throw new Error("Discord recovery promote step authority is invalid");
    }
    seenStepNames.add(step.name);
  }
  return new Map(jobs.map((job) => [job.name, job]));
}

export function validateDiscordCheckpointCandidatePrerequisites(value, options) {
  const pages = Array.isArray(value) ? value : [value];
  let totalCount = null;
  const jobs = [];
  for (const page of pages) {
    requirePlainObject(page, "Discord checkpoint prerequisite job page");
    if (!Number.isSafeInteger(page.total_count) || !Array.isArray(page.jobs)) {
      throw new Error("Discord checkpoint prerequisite job authority is invalid");
    }
    if (totalCount === null) totalCount = page.total_count;
    if (page.total_count !== totalCount) {
      throw new Error("Discord checkpoint prerequisite job page totals differ");
    }
    jobs.push(...page.jobs);
  }
  if (jobs.length !== totalCount || jobs.length !== PRIMARY_JOB_NAMES.length) {
    throw new Error("Discord checkpoint prerequisite job authority is incomplete");
  }
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(options?.sourceCommit, SOURCE_COMMIT, "source commit");
  const workflowRunId = requireDecimalId(options?.workflowRunId, "workflow run ID");
  const workflowRunAttempt = requireDecimalId(
    options?.workflowRunAttempt,
    "workflow run attempt",
  );
  const byName = new Map();
  for (const job of jobs) {
    requirePlainObject(job, "Discord checkpoint prerequisite job");
    const name = String(job.name ?? "");
    if (
      !PRIMARY_JOB_NAMES.includes(name) || byName.has(name) ||
      requireDecimalId(job.id, "checkpoint prerequisite job ID") !== String(job.id) ||
      requireDecimalId(job.run_id, "checkpoint prerequisite run ID") !== workflowRunId ||
      requireDecimalId(job.run_attempt, "checkpoint prerequisite run attempt") !==
        workflowRunAttempt ||
      job.head_sha !== sourceCommit || job.head_branch !== "main" ||
      typeof job.html_url !== "string" ||
      !job.html_url.startsWith(
        `https://github.com/${repository}/actions/runs/${workflowRunId}/job/`,
      ) || !Array.isArray(job.steps)
    ) throw new Error("Discord checkpoint prerequisite job identity differs");
    byName.set(name, job);
  }

  const proofJobs = [];
  for (const name of PRIMARY_JOB_NAMES) {
    const job = byName.get(name);
    const expected = [...SUCCESSFUL_JOB_STEP_NAMES[name]];
    if (name !== "sync-observe") {
      if (job.status !== "completed" || job.conclusion !== "success") {
        throw new Error("Discord checkpoint prerequisite deployment job did not succeed");
      }
      validateExactStepSequence(job.steps, expected, { requireAll: true });
      validateSuccessfulStepConclusions(name, job.steps);
      proofJobs.push(sealCheckpointPrerequisiteJob(job, job.steps));
      continue;
    }
    if (job.status !== "in_progress" || job.conclusion !== null) {
      throw new Error("Discord checkpoint producer must run inside the exact active sync job");
    }
    const captureIndex = expected.indexOf(DISCORD_CHECKPOINT_JOB_CAPTURE_STEP);
    const completedPrefix = job.steps.filter((step) =>
      step.status === "completed" || step.status === "in_progress");
    validateExactStepSequence(completedPrefix, expected.slice(0, captureIndex + 1), {
      requireAll: true,
      finalInProgressName: DISCORD_CHECKPOINT_JOB_CAPTURE_STEP,
    });
    validateSuccessfulStepConclusions(name, completedPrefix, {
      finalInProgressName: DISCORD_CHECKPOINT_JOB_CAPTURE_STEP,
    });
    for (const step of job.steps.slice(completedPrefix.length)) {
      const index = expected.indexOf(step.name);
      if (
        index <= captureIndex || !["queued", "pending"].includes(step.status) ||
        step.conclusion !== null
      ) throw new Error("Discord checkpoint producer has foreign future step authority");
    }
    const durableUpload = completedPrefix.find((step) =>
      step.name === "Upload durable sync and sole canonical observation evidence");
    const observation = completedPrefix.find((step) => step.name === CATALOG_MUTATION_STEP);
    if (
      durableUpload?.conclusion !== "success" || observation?.conclusion !== "success" ||
      requireGitHubTimestamp(observation.completed_at, "observation completed-at") -
        requireGitHubTimestamp(observation.started_at, "observation started-at") < 1_200_000 ||
      requireGitHubTimestamp(observation.completed_at, "observation completed-at") >
        requireGitHubTimestamp(durableUpload.started_at, "observation upload started-at")
    ) throw new Error("Discord checkpoint candidate predates durable observation evidence");
    proofJobs.push(sealCheckpointPrerequisiteJob(job, completedPrefix));
  }
  const report = Object.freeze(sealCanonicalReport({
    schema_id: "clearra.discord-production-checkpoint-prerequisite-jobs.v1",
    repository,
    source_commit: sourceCommit,
    workflow_run_id: workflowRunId,
    workflow_run_attempt: workflowRunAttempt,
    jobs: proofJobs,
  }));
  validateDiscordCheckpointPrerequisiteProof(report, options);
  return report;
}

export function validateDiscordCheckpointPrerequisiteProof(value, options = {}) {
  requirePlainObject(value, "Discord checkpoint prerequisite proof");
  requireExactKeys(value, [
    "schema_id", "repository", "source_commit", "workflow_run_id",
    "workflow_run_attempt", "jobs", "report_sha256",
  ], "Discord checkpoint prerequisite proof");
  verifyCanonicalReportHash(value, "Discord checkpoint prerequisite proof");
  const repository = requirePattern(
    options.repository ?? value.repository,
    REPOSITORY,
    "repository",
  );
  const sourceCommit = requirePattern(
    options.sourceCommit ?? value.source_commit,
    SOURCE_COMMIT,
    "source commit",
  );
  const workflowRunId = requireDecimalId(
    options.workflowRunId ?? value.workflow_run_id,
    "workflow run ID",
  );
  const workflowRunAttempt = requireDecimalId(
    options.workflowRunAttempt ?? value.workflow_run_attempt,
    "workflow run attempt",
  );
  if (
    value.schema_id !== "clearra.discord-production-checkpoint-prerequisite-jobs.v1" ||
    value.repository !== repository || value.source_commit !== sourceCommit ||
    value.workflow_run_id !== workflowRunId ||
    value.workflow_run_attempt !== workflowRunAttempt ||
    !Array.isArray(value.jobs) || value.jobs.length !== PRIMARY_JOB_NAMES.length
  ) throw new Error("Discord checkpoint prerequisite proof identity differs");

  for (const [jobIndex, job] of value.jobs.entries()) {
    requirePlainObject(job, "Discord checkpoint prerequisite proof job");
    requireExactKeys(job, [
      "job_id", "job_name", "job_status", "job_conclusion", "steps",
    ], "Discord checkpoint prerequisite proof job");
    const expectedName = PRIMARY_JOB_NAMES[jobIndex];
    if (
      job.job_name !== expectedName ||
      requireDecimalId(job.job_id, "checkpoint prerequisite job ID") !== job.job_id ||
      !Array.isArray(job.steps)
    ) throw new Error("Discord checkpoint prerequisite proof job identity differs");
    const expectedSteps = [...SUCCESSFUL_JOB_STEP_NAMES[expectedName]];
    const isSync = expectedName === "sync-observe";
    const captureIndex = expectedSteps.indexOf(DISCORD_CHECKPOINT_JOB_CAPTURE_STEP);
    const expectedProofSteps = isSync
      ? expectedSteps.slice(0, captureIndex + 1)
      : expectedSteps;
    if (
      (!isSync && (job.job_status !== "completed" || job.job_conclusion !== "success")) ||
      (isSync && (job.job_status !== "in_progress" || job.job_conclusion !== null))
    ) throw new Error("Discord checkpoint prerequisite proof job state differs");
    validateExactStepSequence(job.steps, expectedProofSteps, {
      requireAll: true,
      finalInProgressName: isSync ? DISCORD_CHECKPOINT_JOB_CAPTURE_STEP : undefined,
    });
    validateSuccessfulStepConclusions(expectedName, job.steps, {
      finalInProgressName: isSync ? DISCORD_CHECKPOINT_JOB_CAPTURE_STEP : undefined,
    });
    for (const step of job.steps) {
      requirePlainObject(step, "Discord checkpoint prerequisite proof step");
      requireExactKeys(step, [
        "name", "number", "status", "conclusion", "started_at", "completed_at",
      ], "Discord checkpoint prerequisite proof step");
    }
    if (isSync) {
      const observation = job.steps.find((step) => step.name === CATALOG_MUTATION_STEP);
      const durableUpload = job.steps.find((step) =>
        step.name === "Upload durable sync and sole canonical observation evidence");
      if (
        observation?.conclusion !== "success" || durableUpload?.conclusion !== "success" ||
        requireGitHubTimestamp(observation.completed_at, "observation completed-at") -
          requireGitHubTimestamp(observation.started_at, "observation started-at") < 1_200_000 ||
        durableUpload.number <= observation.number ||
        requireGitHubTimestamp(durableUpload.started_at, "sync evidence upload started-at") <
          requireGitHubTimestamp(observation.completed_at, "observation completed-at")
      ) throw new Error("Discord checkpoint prerequisite proof lacks durable observation evidence");
    }
  }
  return value;
}

function validateSuccessfulStepConclusions(jobName, steps, options = {}) {
  const skipped = new Set([
    ...(jobName === "authority" ? ["Record explicit no-op for changes outside Discord"] : []),
    ...(jobName === "promote"
      ? ["Compensate any protected-path failure after Oracle transition began"]
      : []),
    ...(jobName === "sync-observe" ? [
      "Compensate catalog mutation if any later sync job step failed",
      "Upload durable catalog compensation evidence",
    ] : []),
  ]);
  for (const step of steps) {
    if (step.name === options.finalInProgressName) continue;
    const expectedConclusion = skipped.has(step.name) ? "skipped" : "success";
    if (step.conclusion !== expectedConclusion) {
      throw new Error("Discord checkpoint successful job step conclusion differs");
    }
  }
}

function validateExactStepSequence(steps, expected, options = {}) {
  if (!Array.isArray(steps) || (options.requireAll && steps.length !== expected.length)) {
    throw new Error("Discord checkpoint step sequence is incomplete");
  }
  let priorNumber = 0;
  for (const [index, step] of steps.entries()) {
    requirePlainObject(step, "Discord checkpoint step");
    if (
      step.name !== expected[index] || !Number.isSafeInteger(step.number) ||
      step.number <= priorNumber
    ) throw new Error("Discord checkpoint step sequence is foreign or unordered");
    priorNumber = step.number;
    if (step.name === options.finalInProgressName) {
      if (
        step.status !== "in_progress" || step.conclusion !== null ||
        requireGitHubTimestamp(step.started_at, "checkpoint producer started-at") < 0 ||
        step.completed_at !== null
      ) throw new Error("Discord checkpoint producer step state is invalid");
      continue;
    }
    if (
      step.status !== "completed" || !JOB_CONCLUSIONS.has(step.conclusion) ||
      (step.conclusion === "skipped"
        ? !((step.started_at === null && step.completed_at === null) ||
            requireGitHubTimestamp(step.started_at, "checkpoint step started-at") <=
              requireGitHubTimestamp(step.completed_at, "checkpoint step completed-at"))
        : requireGitHubTimestamp(step.started_at, "checkpoint step started-at") >
            requireGitHubTimestamp(step.completed_at, "checkpoint step completed-at"))
    ) throw new Error("Discord checkpoint completed step state is invalid");
  }
}

function sealCheckpointPrerequisiteJob(job, steps) {
  return Object.freeze({
    job_id: String(job.id),
    job_name: job.name,
    job_status: job.status,
    job_conclusion: job.conclusion,
    steps: steps.map((step) => Object.freeze({
      name: step.name,
      number: step.number,
      status: step.status,
      conclusion: step.conclusion,
      started_at: step.started_at,
      completed_at: step.completed_at,
    })),
  });
}

export function validateSuccessfulDiscordDeploymentAuthority(value, options) {
  const jobs = getExactPrimaryJobAuthority(value, options);
  for (const name of PRIMARY_JOB_NAMES) {
    const job = jobs.get(name);
    if (job?.conclusion !== "success") {
      throw new Error("Discord production checkpoint requires every exact primary job to succeed");
    }
    const expected = [...SUCCESSFUL_JOB_STEP_NAMES[name]];
    const actual = [...job.steps].sort((left, right) => left.number - right.number);
    if (
      actual.length !== expected.length ||
      actual.some((step, index) =>
        step.name !== expected[index] || step.status !== "completed" ||
        !JOB_CONCLUSIONS.has(step.conclusion) ||
        (step.conclusion === "skipped"
          ? !((step.started_at === null && step.completed_at === null) ||
              requireGitHubTimestamp(step.started_at, "checkpoint step started-at") <=
                requireGitHubTimestamp(step.completed_at, "checkpoint step completed-at"))
          : requireGitHubTimestamp(step.started_at, "checkpoint step started-at") >
              requireGitHubTimestamp(step.completed_at, "checkpoint step completed-at")))
    ) {
      throw new Error("Discord production checkpoint differs from the closed job-step topology");
    }
    validateSuccessfulStepConclusions(name, actual);
  }
  const requiredSteps = new Map([
    ["authority", [
      "Classify deployment impact from the last production tag",
      "Reject unresolved earlier Discord recovery debt before candidate work",
      "Upload exact recovery-debt clearance before candidate work",
    ]],
    ["Prepare immutable Discord candidate inputs", [
      "Download the exact pre-candidate recovery-debt clearance",
      "Upload sealed prepared state",
    ]],
    ["promote", [
      PRESTAGE_UPLOAD_STEP,
      RUNTIME_MUTATION_STEPS[0],
      LIVE_UPLOAD_STEP,
      RUNTIME_MUTATION_STEPS[1],
      "Upload sealed promoted state",
    ]],
    ["sync-observe", [
      "Resolve one successful exact-SHA Pages deployment before global mutation",
      "Verify Pages authority before Discord global mutation",
      "Authority-bound global sync and sole canonical four-surface observation",
      "Upload durable sync and sole canonical observation evidence",
      DISCORD_CHECKPOINT_JOB_CAPTURE_STEP,
      DISCORD_CHECKPOINT_CANDIDATE_SEAL_STEP,
      DISCORD_CHECKPOINT_CANDIDATE_UPLOAD_STEP,
    ]],
  ]);
  const proof = [];
  for (const [jobName, stepNames] of requiredSteps) {
    const job = jobs.get(jobName);
    const steps = [];
    for (const stepName of stepNames) {
      const matches = job.steps.filter((step) => step.name === stepName);
      if (
        matches.length !== 1 || matches[0].status !== "completed" ||
        matches[0].conclusion !== "success"
      ) {
        throw new Error("Discord production checkpoint lacks one exact successful authority step");
      }
      steps.push(Object.freeze({
        name: stepName,
        number: matches[0].number,
        conclusion: "success",
        started_at: matches[0].started_at,
        completed_at: matches[0].completed_at,
        duration_seconds: Math.floor((
          requireGitHubTimestamp(matches[0].completed_at, "checkpoint step completed-at") -
          requireGitHubTimestamp(matches[0].started_at, "checkpoint step started-at")
        ) / 1000),
      }));
    }
    proof.push(Object.freeze({
      job_id: String(job.id),
      job_name: jobName,
      job_conclusion: "success",
      required_steps: steps,
    }));
  }
  const syncSteps = jobs.get("sync-observe").steps;
  const observation = syncSteps.find((step) =>
    step.name === "Authority-bound global sync and sole canonical four-surface observation");
  const upload = syncSteps.find((step) =>
    step.name === "Upload durable sync and sole canonical observation evidence");
  const observationStartedAt = requireGitHubTimestamp(
    observation.started_at,
    "production observation started-at",
  );
  const observationCompletedAt = requireGitHubTimestamp(
    observation.completed_at,
    "production observation completed-at",
  );
  const uploadStartedAt = requireGitHubTimestamp(upload.started_at, "sync evidence upload started-at");
  if (
    observationCompletedAt - observationStartedAt < 1_200_000 ||
    upload.number <= observation.number || uploadStartedAt < observationCompletedAt
  ) {
    throw new Error("Discord production checkpoint lacks one completed 1200-second observation window");
  }
  return Object.freeze(proof);
}

function buildPromoteStepProof(promote, jobs) {
  const stepProof = (name) => {
    const step = promote.steps.find((candidate) => candidate.name === name);
    return step ? Object.freeze({
      name: step.name,
      number: step.number,
      status: step.status,
      conclusion: step.conclusion,
    }) : null;
  };
  return Object.freeze({
    job_id: String(promote.id),
    job_name: promote.name,
    job_status: promote.status,
    job_conclusion: promote.conclusion,
    prestage_upload_step: stepProof(PRESTAGE_UPLOAD_STEP),
    live_upload_step: stepProof(LIVE_UPLOAD_STEP),
    runtime_mutation_steps: RUNTIME_MUTATION_STEPS.map((name) => {
      const step = promote.steps.find((candidate) => candidate.name === name);
      return Object.freeze({
        name,
        number: step?.number ?? null,
        status: step?.status ?? null,
        conclusion: step?.conclusion ?? null,
      });
    }),
    primary_jobs: PRIMARY_JOB_NAMES.map((jobName) => {
      const job = jobs.get(jobName);
      return Object.freeze({
        job_id: String(job.id),
        job_name: job.name,
        job_status: job.status,
        job_conclusion: job.conclusion,
        steps: job.steps.map((step) => Object.freeze({
          name: step.name,
          number: step.number,
          status: step.status,
          conclusion: step.conclusion,
        })),
      });
    }),
  });
}

export function validateDiscordRecoveryFreshness(value, options) {
  const pages = Array.isArray(value) ? value : [value];
  if (pages.length === 0) throw new Error("Discord recovery run catalog is unavailable");
  let totalCount = null;
  const runs = [];
  for (const [index, page] of pages.entries()) {
    requirePlainObject(page, `Discord recovery run catalog page ${index}`);
    if (!Number.isSafeInteger(page.total_count) || page.total_count < 0 || !Array.isArray(page.workflow_runs)) {
      throw new Error("Discord recovery run catalog is invalid");
    }
    if (totalCount === null) totalCount = page.total_count;
    if (page.total_count !== totalCount) {
      throw new Error("Discord recovery run catalog page totals differ");
    }
    runs.push(...page.workflow_runs);
  }
  if (runs.length !== totalCount) {
    throw new Error("Discord recovery run catalog must be complete and non-truncated");
  }
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(options?.sourceCommit, SOURCE_COMMIT, "source commit");
  const workflowRunId = requireDecimalId(options?.workflowRunId, "workflow run ID");
  const workflowRunAttempt = requireDecimalId(
    options?.workflowRunAttempt,
    "workflow run attempt",
  );
  const latestById = new Map();
  const runJobCatalog = validateRunJobCatalog(options?.runJobCatalog);
  const freshnessDecisions = [];
  const original = {
    id: workflowRunId,
    attempt: workflowRunAttempt,
    runNumber: requireDecimalId(options?.runNumber, "original workflow run number"),
    createdAt: options?.runCreatedAt,
    runStartedAt: options?.runStartedAt,
    updatedAt: options?.runUpdatedAt,
    status: "completed",
    sourceCommit,
  };
  for (const entry of runs) {
    const normalized = normalizePrimaryAttempt(entry, repository, "catalog workflow");
    if (latestById.has(normalized.id)) {
      throw new Error("Discord recovery run catalog contains duplicate run IDs");
    }
    latestById.set(normalized.id, normalized);
  }
  const attempts = validatePrimaryRunAttemptCatalog(
    options?.runAttemptCatalog,
    latestById,
    repository,
  );
  const originalAttempt = attempts.get(`${workflowRunId}:${workflowRunAttempt}`);
  if (!originalAttempt ||
      originalAttempt.status !== "completed" ||
      originalAttempt.sourceCommit !== sourceCommit ||
      originalAttempt.runNumber !== original.runNumber ||
      originalAttempt.createdAt !== original.createdAt ||
      originalAttempt.runStartedAt !== original.runStartedAt ||
      originalAttempt.updatedAt !== original.updatedAt) {
    throw new Error("Discord recovery run attempt differs from its exact original authority");
  }
  const expectedJobKeys = new Set();
  const orderedAttempts = [...attempts.values()].sort(compareNormalizedAttempts);
  for (const ordering of orderedAttempts) {
    if (ordering.id === workflowRunId && ordering.attempt === workflowRunAttempt) continue;
    if (ordering.status === "queued") {
      freshnessDecisions.push(buildFreshnessDecision(
        ordering,
        "queued-behind-shared-group",
        null,
      ));
      continue;
    }
    if (ordering.status !== "completed") {
      throw new Error("Discord recovery has a concurrent in-progress deployment ambiguity");
    }
    const potentiallyLater =
      (ordering.id === workflowRunId && BigInt(ordering.attempt) > BigInt(workflowRunAttempt)) ||
      ordering.runStartedAt >= original.runStartedAt ||
      ordering.updatedAt >= original.updatedAt;
    if (!potentiallyLater) continue;
    const catalogKey = `${ordering.id}:${ordering.attempt}`;
    expectedJobKeys.add(catalogKey);
    const jobList = runJobCatalog.get(catalogKey);
    if (!jobList) {
      throw new Error("Discord recovery lacks exact job authority for a potential superseding run");
    }
    try {
      const noMutationProof = validateNoPrestageArtifactAuthority(jobList, {
        repository,
        sourceCommit: ordering.sourceCommit,
        workflowRunId: ordering.id,
        workflowRunAttempt: ordering.attempt,
      });
      freshnessDecisions.push(buildFreshnessDecision(
        ordering,
        "completed-exact-no-runtime-mutation",
        noMutationProof,
      ));
    } catch {
      throw new Error(
        "Discord recovery is stale or ambiguous relative to another runtime-mutation authority",
      );
    }
  }
  for (const key of runJobCatalog.keys()) {
    if (!expectedJobKeys.has(key)) {
      throw new Error("Discord recovery run-job catalog contains foreign authority");
    }
  }
  freshnessDecisions.sort((left, right) => {
    const byRun = BigInt(left.workflow_run_id) - BigInt(right.workflow_run_id);
    if (byRun !== 0n) return byRun < 0n ? -1 : 1;
    return BigInt(left.workflow_run_attempt) < BigInt(right.workflow_run_attempt) ? -1 : 1;
  });
  return Object.freeze({
    schema_id: "clearra.discord-deployment-recovery-freshness-proof.v1",
    original_workflow_run_id: workflowRunId,
    original_workflow_run_attempt: workflowRunAttempt,
    original_source_commit: sourceCommit,
    potential_superseders: freshnessDecisions,
  });
}

function normalizePrimaryAttempt(entry, repository, label) {
  requirePlainObject(entry, `${label} entry`);
  const id = requireDecimalId(entry.id, `${label} run ID`);
  const attempt = requireDecimalId(entry.run_attempt, `${label} run attempt`);
  const runNumber = requireDecimalId(entry.run_number, `${label} run number`);
  const createdAt = requireGitHubTimestamp(entry.created_at, `${label} created-at`);
  if (!["queued", "in_progress", "completed"].includes(entry.status)) {
    throw new Error(`Discord recovery ${label} status is invalid`);
  }
  if (
    (entry.status === "completed" && !JOB_CONCLUSIONS.has(entry.conclusion)) ||
    (entry.status !== "completed" && entry.conclusion !== null)
  ) throw new Error(`Discord recovery ${label} status/conclusion is inconsistent`);
  const runStartedAt = entry.status === "queued" && entry.run_started_at === null
    ? null
    : requireGitHubTimestamp(entry.run_started_at, `${label} run-started-at`);
  const updatedAt = requireGitHubTimestamp(entry.updated_at, `${label} updated-at`);
  if (
    createdAt > updatedAt ||
    (runStartedAt !== null && (createdAt > runStartedAt || runStartedAt > updatedAt))
  ) throw new Error(`Discord recovery ${label} timestamps are internally inconsistent`);
  if (
    entry.name !== PRIMARY_WORKFLOW_NAME ||
    entry.path !== PRIMARY_WORKFLOW_PATH ||
    entry.head_branch !== "main" ||
    entry.repository?.id !== CLEARRA_REPOSITORY_ID ||
    entry.repository?.full_name !== repository ||
    entry.head_repository?.id !== CLEARRA_REPOSITORY_ID ||
    entry.head_repository?.full_name !== repository ||
    !PRIMARY_EVENTS.has(entry.event) ||
    !SOURCE_COMMIT.test(entry.head_sha ?? "")
  ) throw new Error(`Discord recovery ${label} contains foreign authority`);
  return Object.freeze({
    id,
    attempt,
    runNumber,
    createdAt,
    runStartedAt,
    updatedAt,
    status: entry.status,
    conclusion: entry.conclusion,
    sourceCommit: entry.head_sha,
    createdAtText: entry.created_at,
    runStartedAtText: entry.run_started_at,
    updatedAtText: entry.updated_at,
  });
}

function validatePrimaryRunAttemptCatalog(value, latestById, repository) {
  requirePlainObject(value, "Discord recovery run-attempt catalog");
  requireExactKeys(
    value,
    ["schema_id", "attempts"],
    "Discord recovery run-attempt catalog",
  );
  if (
    value.schema_id !== DISCORD_RUN_ATTEMPT_CATALOG_SCHEMA_ID ||
    !Array.isArray(value.attempts)
  ) throw new Error("Discord recovery run-attempt catalog is invalid");
  const result = new Map();
  const attemptsById = new Map();
  for (const entry of value.attempts) {
    const normalized = normalizePrimaryAttempt(entry, repository, "run-attempt catalog");
    const key = `${normalized.id}:${normalized.attempt}`;
    if (result.has(key) || !latestById.has(normalized.id)) {
      throw new Error("Discord recovery run-attempt catalog is ambiguous or foreign");
    }
    result.set(key, normalized);
    const entries = attemptsById.get(normalized.id) ?? [];
    entries.push(normalized);
    attemptsById.set(normalized.id, entries);
  }
  for (const [id, latest] of latestById) {
    const entries = (attemptsById.get(id) ?? []).sort((left, right) =>
      Number(BigInt(left.attempt) - BigInt(right.attempt)));
    if (entries.length !== Number(latest.attempt)) {
      throw new Error("Discord recovery run-attempt catalog is incomplete");
    }
    for (const [index, entry] of entries.entries()) {
      if (
        entry.attempt !== String(index + 1) || entry.runNumber !== latest.runNumber ||
        entry.sourceCommit !== latest.sourceCommit || entry.createdAt !== latest.createdAt
      ) throw new Error("Discord recovery run-attempt catalog ordering authority is inconsistent");
    }
    const final = entries.at(-1);
    if (
      final.attempt !== latest.attempt || final.status !== latest.status ||
      final.conclusion !== latest.conclusion || final.runStartedAt !== latest.runStartedAt ||
      final.updatedAt !== latest.updatedAt
    ) throw new Error("Discord recovery latest run attempt differs from the complete catalog");
  }
  return result;
}

function compareNormalizedAttempts(left, right) {
  const byRun = BigInt(left.id) - BigInt(right.id);
  if (byRun !== 0n) return byRun < 0n ? -1 : 1;
  const byAttempt = BigInt(left.attempt) - BigInt(right.attempt);
  return byAttempt < 0n ? -1 : byAttempt > 0n ? 1 : 0;
}

function buildFreshnessDecision(ordering, decision, noMutationProof) {
  return Object.freeze({
    workflow_run_id: ordering.id,
    workflow_run_attempt: ordering.attempt,
    source_commit: ordering.sourceCommit,
    run_number: ordering.runNumber,
    status: ordering.status,
    created_at: ordering.createdAtText,
    run_started_at: ordering.runStartedAtText,
    updated_at: ordering.updatedAtText,
    decision,
    no_mutation_job_step_proof: noMutationProof,
  });
}

function validateRunJobCatalog(value) {
  requirePlainObject(value, "Discord recovery run-job catalog");
  requireExactKeys(
    value,
    ["schema_id", "runs"],
    "Discord recovery run-job catalog",
  );
  if (value.schema_id !== DISCORD_RUN_JOB_CATALOG_SCHEMA_ID || !Array.isArray(value.runs)) {
    throw new Error("Discord recovery run-job catalog is invalid");
  }
  const result = new Map();
  for (const entry of value.runs) {
    requirePlainObject(entry, "Discord recovery run-job catalog entry");
    requireExactKeys(
      entry,
      ["workflow_run_id", "workflow_run_attempt", "pages"],
      "Discord recovery run-job catalog entry",
    );
    const id = requireDecimalId(entry.workflow_run_id, "run-job catalog workflow run ID");
    const attempt = requireDecimalId(
      entry.workflow_run_attempt,
      "run-job catalog workflow run attempt",
    );
    if (!Array.isArray(entry.pages) || entry.pages.length === 0) {
      throw new Error("Discord recovery run-job catalog pages are unavailable");
    }
    const key = `${id}:${attempt}`;
    if (result.has(key)) throw new Error("Discord recovery run-job catalog is ambiguous");
    result.set(key, entry.pages);
  }
  return result;
}

export async function sealDiscordRecoveryResult(options) {
  const stage = requireRecoveryStage(options?.stage);
  const bindings = await materializeBindings(options?.bindings, stage);
  const catalogRecovery = await readJsonFile(
    options?.catalogDisposition,
    "Discord catalog recovery disposition",
  );
  validateDiscordCatalogRecoveryDisposition(catalogRecovery, {
    repository: options?.repository,
    sourceCommit: options?.sourceCommit,
    originalWorkflowRunId: options?.originalWorkflowRunId,
    originalWorkflowRunAttempt: options?.originalWorkflowRunAttempt,
    recoveryWorkflowRunId: options?.recoveryWorkflowRunId,
    recoveryWorkflowRunAttempt: options?.recoveryWorkflowRunAttempt,
    required: options?.catalogRecoveryRequired,
    artifactId: options?.catalogArtifactId,
    artifactDigest: options?.catalogArtifactDigest,
  });
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_RECOVERY_RESULT_SCHEMA_ID,
    recovery_stage: stage,
    repository: requirePattern(options?.repository, REPOSITORY, "repository"),
    source_commit: requirePattern(options?.sourceCommit, SOURCE_COMMIT, "source commit"),
    original_workflow_run_id: requireDecimalId(
      options?.originalWorkflowRunId,
      "original workflow run ID",
    ),
    original_workflow_run_attempt: requireDecimalId(
      options?.originalWorkflowRunAttempt,
      "original workflow run attempt",
    ),
    recovery_workflow_run_id: requireDecimalId(
      options?.recoveryWorkflowRunId,
      "recovery workflow run ID",
    ),
    recovery_workflow_run_attempt: requireDecimalId(
      options?.recoveryWorkflowRunAttempt,
      "recovery workflow run attempt",
    ),
    artifact_id: requireDecimalId(options?.artifactId, "artifact ID"),
    artifact_digest: requirePattern(
      options?.artifactDigest,
      SHA256_DIGEST,
      "artifact digest",
    ),
    recovered_at: canonicalTimestamp(options?.recoveredAt, "recovered-at"),
    catalog_recovery: catalogRecovery,
    bindings,
  }));
}

export async function verifyDiscordRecoveryResult(path, options) {
  const report = await readJsonFile(path, "Discord recovery result");
  requireExactKeys(report, [
    "schema_id",
    "recovery_stage",
    "repository",
    "source_commit",
    "original_workflow_run_id",
    "original_workflow_run_attempt",
    "recovery_workflow_run_id",
    "recovery_workflow_run_attempt",
    "artifact_id",
    "artifact_digest",
    "recovered_at",
    "catalog_recovery",
    "bindings",
    "report_sha256",
  ], "Discord recovery result");
  verifyCanonicalReportHash(report, "Discord recovery result");
  const stage = requireRecoveryStage(options?.stage);
  const expectedFields = {
    schema_id: DISCORD_RECOVERY_RESULT_SCHEMA_ID,
    recovery_stage: stage,
    repository: requirePattern(options?.repository, REPOSITORY, "repository"),
    source_commit: requirePattern(options?.sourceCommit, SOURCE_COMMIT, "source commit"),
    original_workflow_run_id: requireDecimalId(options?.originalWorkflowRunId, "original workflow run ID"),
    original_workflow_run_attempt: requireDecimalId(options?.originalWorkflowRunAttempt, "original workflow run attempt"),
    recovery_workflow_run_id: requireDecimalId(options?.recoveryWorkflowRunId, "recovery workflow run ID"),
    recovery_workflow_run_attempt: requireDecimalId(options?.recoveryWorkflowRunAttempt, "recovery workflow run attempt"),
    artifact_id: requireDecimalId(options?.artifactId, "artifact ID"),
    artifact_digest: requirePattern(options?.artifactDigest, SHA256_DIGEST, "artifact digest"),
  };
  for (const [field, expected] of Object.entries(expectedFields)) {
    if (report[field] !== expected) throw new Error(`Discord recovery result ${field} differs`);
  }
  canonicalTimestamp(report.recovered_at, "recovered-at");
  validateDiscordCatalogRecoveryDisposition(report.catalog_recovery, {
    repository: options?.repository,
    sourceCommit: options?.sourceCommit,
    originalWorkflowRunId: options?.originalWorkflowRunId,
    originalWorkflowRunAttempt: options?.originalWorkflowRunAttempt,
    recoveryWorkflowRunId: options?.recoveryWorkflowRunId,
    recoveryWorkflowRunAttempt: options?.recoveryWorkflowRunAttempt,
    required: options?.catalogRecoveryRequired,
    artifactId: options?.catalogArtifactId,
    artifactDigest: options?.catalogArtifactDigest,
  });
  validateResultBindings(report.bindings, stage);
  return Object.freeze(report);
}

export async function verifyDiscordRecoveryEvidence(root, options) {
  const evidenceRoot = resolve(String(root ?? ""));
  await assertSafeDirectoryChain(evidenceRoot);
  const rootMetadata = await lstat(evidenceRoot);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("Discord recovery evidence root must be a regular non-link directory");
  }
  const required = options?.catalogRecoveryRequired === true ||
    options?.catalogRecoveryRequired === "true";
  const expectedLeaves = [
    DISCORD_CATALOG_RECOVERY_DISPOSITION_FILE,
    DISCORD_PROTECTED_RECOVERY_AUTHORITY_FILE,
    DISCORD_RECOVERY_RESULT_FILE,
    ...(required ? CATALOG_RECOVERY_EVIDENCE_FILES : []),
  ].sort((left, right) => left.localeCompare(right, "en"));
  const entries = await readdir(evidenceRoot, { withFileTypes: true });
  const actualLeaves = entries.map((entry) => entry.name)
    .sort((left, right) => left.localeCompare(right, "en"));
  if (
    entries.some((entry) => !entry.isFile() || entry.isSymbolicLink()) ||
    canonicalJson(actualLeaves) !== canonicalJson(expectedLeaves)
  ) {
    throw new Error("Discord recovery evidence differs from the exact closed leaf set");
  }

  const result = await verifyDiscordRecoveryResult(
    resolve(evidenceRoot, DISCORD_RECOVERY_RESULT_FILE),
    options,
  );
  const protectedInput = await readCanonicalEvidenceFile(
    resolve(evidenceRoot, DISCORD_PROTECTED_RECOVERY_AUTHORITY_FILE),
    "protected Discord recovery authority",
  );
  const protectedAuthority = protectedInput.value;
  validateDiscordRecoveryAuthorityReport(protectedAuthority, {
    repository: options?.repository,
    sourceCommit: options?.sourceCommit,
    workflowRunId: options?.originalWorkflowRunId,
    workflowRunAttempt: options?.originalWorkflowRunAttempt,
    recoveryWorkflowRunId: options?.recoveryWorkflowRunId,
    recoveryWorkflowRunAttempt: options?.recoveryWorkflowRunAttempt,
    workflowEvent: options?.workflowEvent ?? protectedAuthority.workflow_event,
    workflowConclusion: options?.workflowConclusion ?? protectedAuthority.workflow_conclusion,
  });
  if (
    protectedAuthority.recovery_required !== true ||
    protectedAuthority.recovery_stage !== result.recovery_stage ||
    protectedAuthority.artifact_id !== result.artifact_id ||
    protectedAuthority.artifact_digest !== result.artifact_digest ||
    protectedAuthority.catalog_recovery_required !== required ||
    protectedAuthority.catalog_artifact_id !== (required
      ? String(options?.catalogArtifactId)
      : null) ||
    protectedAuthority.catalog_artifact_digest !== (required
      ? options?.catalogArtifactDigest
      : null)
  ) {
    throw new Error("protected Discord recovery authority differs from the terminal result");
  }
  const recoveryBinding = result.bindings.find((binding) =>
    binding.name === "recovery_authority");
  if (
    recoveryBinding?.file_sha256 !== protectedInput.fileSha256 ||
    recoveryBinding?.size !== protectedInput.size
  ) {
    throw new Error("terminal recovery result does not bind the protected authority bytes");
  }

  const dispositionInput = await readCanonicalEvidenceFile(
    resolve(evidenceRoot, DISCORD_CATALOG_RECOVERY_DISPOSITION_FILE),
    "Discord catalog recovery disposition",
  );
  validateDiscordCatalogRecoveryDisposition(dispositionInput.value, {
    repository: options?.repository,
    sourceCommit: options?.sourceCommit,
    originalWorkflowRunId: options?.originalWorkflowRunId,
    originalWorkflowRunAttempt: options?.originalWorkflowRunAttempt,
    recoveryWorkflowRunId: options?.recoveryWorkflowRunId,
    recoveryWorkflowRunAttempt: options?.recoveryWorkflowRunAttempt,
    required,
    artifactId: options?.catalogArtifactId,
    artifactDigest: options?.catalogArtifactDigest,
  });
  if (canonicalJson(dispositionInput.value) !== canonicalJson(result.catalog_recovery)) {
    throw new Error("terminal result embeds different Discord catalog recovery disposition bytes");
  }

  if (required) {
    const authorityPath = resolve(evidenceRoot, "discord-catalog-recovery-authority.json");
    const priorPath = resolve(evidenceRoot, "discord-prior-catalog.json");
    const catalogPath = resolve(evidenceRoot, "discord-catalog.json");
    const syncPath = resolve(evidenceRoot, "discord-sync-authority.json");
    const catalogAuthorityInput = await readCanonicalEvidenceFile(
      authorityPath,
      "Discord catalog recovery authority",
    );
    const catalogAuthority = await verifyDiscordCatalogRecoveryAuthority(authorityPath, {
      repository: options?.repository,
      sourceCommit: options?.sourceCommit,
      workflowRunId: options?.originalWorkflowRunId,
      workflowRunAttempt: options?.originalWorkflowRunAttempt,
      applicationId: catalogAuthorityInput.value.application_id,
      priorSnapshot: priorPath,
      desiredCatalog: catalogPath,
      syncAuthority: syncPath,
    });
    const restoreInput = await readCanonicalEvidenceFile(
      resolve(evidenceRoot, "discord-catalog-restore.json"),
      "Discord catalog restore report",
    );
    validateDiscordCatalogRestoreReport(restoreInput.value, {
      expectedSourceCommit: options?.sourceCommit,
      expectedApplicationId: catalogAuthority.report.application_id,
    });
    if (
      dispositionInput.value.catalog_authority_sha256 !==
        catalogAuthority.report.report_sha256 ||
      dispositionInput.value.catalog_authority_file_sha256 !==
        catalogAuthority.fileSha256 ||
      dispositionInput.value.restore_report_sha256 !== restoreInput.value.report_sha256 ||
      dispositionInput.value.restore_report_file_sha256 !== restoreInput.fileSha256
    ) {
      throw new Error("Discord catalog recovery preimages differ from the sealed disposition");
    }
  }
  return Object.freeze({ result, protectedAuthority });
}

export function sealDiscordPrestageIntent(options) {
  const sourceCommit = requirePattern(options?.sourceCommit, SOURCE_COMMIT, "source commit");
  const overlaySha256 = requirePattern(
    options?.remoteOverlaySha256,
    SHA256,
    "remote overlay SHA-256",
  );
  const remoteOverlayArchive = String(options?.remoteOverlayArchive ?? "");
  if (
    remoteOverlayArchive !==
      `/opt/clearra/sealed-release-inputs/private-overlay-no-config-${overlaySha256}.tar`
  ) {
    throw new Error("remote overlay archive is invalid");
  }
  const cloudImageDigest = String(options?.cloudImageDigest ?? "");
  if (
    cloudImageDigest.length < 72 ||
    cloudImageDigest.length > 2048 ||
    /\s/u.test(cloudImageDigest) ||
    !/@sha256:[0-9a-f]{64}$/u.test(cloudImageDigest)
  ) {
    throw new Error("Cloud image digest is invalid");
  }
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_PRESTAGE_INTENT_SCHEMA_ID,
    source_commit: sourceCommit,
    workflow_run_id: requireDecimalId(options?.workflowRunId, "workflow run ID"),
    workflow_run_attempt: requireDecimalId(
      options?.workflowRunAttempt,
      "workflow run attempt",
    ),
    deployment_nonce: requirePattern(options?.deploymentNonce, SHA256, "deployment nonce"),
    cloud_image_digest: cloudImageDigest,
    cloud_candidate_revision: `clearra-current-job-v080-${sourceCommit.slice(0, 7)}`,
    cloud_candidate_tag: `candidate-${sourceCommit.slice(0, 7)}`,
    oracle_candidate_release_id: `v0.8.0-${sourceCommit.slice(0, 7)}`,
    remote_overlay_archive: remoteOverlayArchive,
    remote_overlay_sha256: overlaySha256,
  }));
}

export async function verifyDiscordPrestageIntent(path, options) {
  const report = await readJsonFile(path, "Discord prestage intent");
  requireExactKeys(report, [
    "schema_id",
    "source_commit",
    "workflow_run_id",
    "workflow_run_attempt",
    "deployment_nonce",
    "cloud_image_digest",
    "cloud_candidate_revision",
    "cloud_candidate_tag",
    "oracle_candidate_release_id",
    "remote_overlay_archive",
    "remote_overlay_sha256",
    "report_sha256",
  ], "Discord prestage intent");
  verifyCanonicalReportHash(report, "Discord prestage intent");
  const expected = sealDiscordPrestageIntent({
    sourceCommit: options?.sourceCommit,
    workflowRunId: options?.workflowRunId,
    workflowRunAttempt: options?.workflowRunAttempt,
    deploymentNonce: options?.deploymentNonce,
    cloudImageDigest: report.cloud_image_digest,
    remoteOverlayArchive: report.remote_overlay_archive,
    remoteOverlaySha256: report.remote_overlay_sha256,
  });
  if (canonicalJson(report) !== canonicalJson(expected)) {
    throw new Error("Discord prestage intent differs from exact workflow authority");
  }
  return Object.freeze(report);
}

async function materializeBindings(values, stage) {
  if (!Array.isArray(values)) throw new Error("Discord recovery result bindings are required");
  const parsed = values.map(parseBinding).sort((a, b) => a.name.localeCompare(b.name, "en"));
  if (
    parsed.length !== RESULT_BINDINGS[stage].length ||
    parsed.some((entry, index) => entry.name !== RESULT_BINDINGS[stage][index])
  ) {
    throw new Error("Discord recovery result bindings differ from the closed set");
  }
  const result = [];
  for (const entry of parsed) {
    const target = resolve(entry.path);
    await assertSafeDirectoryChain(dirname(target));
    const metadata = await lstat(target);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 1) {
      throw new Error(`Discord recovery binding is not a nonempty regular file: ${entry.name}`);
    }
    const bytes = await readFile(target);
    result.push(Object.freeze({
      name: entry.name,
      file_sha256: createHash("sha256").update(bytes).digest("hex"),
      size: metadata.size,
    }));
  }
  return Object.freeze(result);
}

function validateResultBindings(bindings, stage) {
  if (!Array.isArray(bindings) || bindings.length !== RESULT_BINDINGS[stage].length) {
    throw new Error("Discord recovery result bindings differ from the closed set");
  }
  bindings.forEach((binding, index) => {
    requirePlainObject(binding, "Discord recovery result binding");
    requireExactKeys(binding, ["name", "file_sha256", "size"], "Discord recovery result binding");
    if (
      binding.name !== RESULT_BINDINGS[stage][index] ||
      !SHA256.test(binding.file_sha256) ||
      !Number.isSafeInteger(binding.size) ||
      binding.size < 1
    ) {
      throw new Error("Discord recovery result binding authority is invalid");
    }
  });
}

function requireRecoveryStage(value) {
  const stage = String(value ?? "");
  if (!Object.hasOwn(RESULT_BINDINGS, stage)) {
    throw new Error("Discord recovery result stage is invalid");
  }
  return stage;
}

function validateRecoveryArtifact(artifact, options) {
  requirePlainObject(artifact, options.label);
  const id = requireDecimalId(artifact.id, `${options.label} ID`);
  const digest = requirePattern(artifact.digest, SHA256_DIGEST, `${options.label} digest`);
  const createdAtText = typeof artifact.created_at === "string" ? artifact.created_at : "";
  const createdAt = requireGitHubTimestamp(createdAtText, `${options.label} created-at`);
  if (
    artifact.expired !== false ||
    !Number.isSafeInteger(artifact.size_in_bytes) ||
    artifact.size_in_bytes < 1 ||
    requireDecimalId(artifact.workflow_run?.id, "artifact workflow run ID") !== options.runId ||
    artifact.workflow_run?.repository_id !== CLEARRA_REPOSITORY_ID ||
    artifact.workflow_run?.head_repository_id !== CLEARRA_REPOSITORY_ID ||
    artifact.workflow_run?.head_sha !== options.sourceCommit ||
    artifact.workflow_run?.head_branch !== "main"
  ) {
    throw new Error(`${options.label} differs from the exact original run authority`);
  }
  if (createdAt < options.runStartedAt || createdAt > options.runUpdatedAt) {
    throw new Error(`${options.label} was not created within the exact run attempt window`);
  }
  return Object.freeze({ id, digest, size: artifact.size_in_bytes, createdAt, createdAtText });
}

function parseBinding(value) {
  if (typeof value !== "string") throw new Error("Discord recovery binding is invalid");
  const separator = value.indexOf("=");
  const name = value.slice(0, separator);
  const path = value.slice(separator + 1);
  if (separator < 1 || !/^[a-z][a-z0-9_]*$/u.test(name) || path.length === 0) {
    throw new Error("Discord recovery binding must be name=path");
  }
  return Object.freeze({ name, path });
}

async function readJsonFile(path, label) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  try {
    return JSON.parse(await readFile(target, "utf8"));
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
}

async function readCanonicalEvidenceFile(path, label) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 2) {
    throw new Error(`${label} must be a nonempty regular non-link file`);
  }
  const bytes = await readFile(target);
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (bytes.toString("utf8") !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  return Object.freeze({
    value,
    fileSha256: createHash("sha256").update(bytes).digest("hex"),
    size: metadata.size,
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
  } finally {
    await handle.close();
  }
  try {
    await link(temporary, target);
  } finally {
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
      throw new Error("Discord recovery path uses a link or non-directory");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requirePattern(value, pattern, label) {
  const text = typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function requireDecimalId(value, label) {
  const text = typeof value === "number" && Number.isSafeInteger(value)
    ? String(value)
    : typeof value === "string" ? value : "";
  if (!DECIMAL_ID.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function requireGitHubTimestamp(value, label) {
  const text = typeof value === "string" ? value : "";
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/u.test(text)) {
    throw new Error(`${label} is invalid`);
  }
  const milliseconds = Date.parse(text);
  if (!Number.isFinite(milliseconds)) throw new Error(`${label} is invalid`);
  const canonical = new Date(milliseconds).toISOString();
  if (text !== canonical && text !== canonical.replace(".000Z", "Z")) {
    throw new Error(`${label} is invalid`);
  }
  return milliseconds;
}

function requirePlainObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
}

function parseCli() {
  return parseArgs({
    options: {
      repository: { type: "string" },
      "run-metadata": { type: "string" },
      "artifact-list": { type: "string" },
      "run-list": { type: "string" },
      "run-attempt-catalog": { type: "string" },
      "job-list": { type: "string" },
      "run-job-catalog": { type: "string" },
      "workflow-run-id": { type: "string" },
      "workflow-run-attempt": { type: "string" },
      "source-commit": { type: "string" },
      "original-workflow-run-id": { type: "string" },
      "original-workflow-run-attempt": { type: "string" },
      "recovery-workflow-run-id": { type: "string" },
      "recovery-workflow-run-attempt": { type: "string" },
      "artifact-id": { type: "string" },
      "artifact-digest": { type: "string" },
      "cloud-image-digest": { type: "string" },
      "deployment-nonce": { type: "string" },
      "remote-overlay-archive": { type: "string" },
      "remote-overlay-sha256": { type: "string" },
      "recovered-at": { type: "string" },
      "catalog-disposition": { type: "string" },
      "catalog-recovery-required": { type: "string" },
      "catalog-artifact-id": { type: "string" },
      "catalog-artifact-digest": { type: "string" },
      "evidence-root": { type: "string" },
      stage: { type: "string" },
      report: { type: "string" },
      binding: { type: "string", multiple: true },
      output: { type: "string" },
      format: { type: "string", default: "summary" },
    },
    strict: true,
    allowPositionals: true,
  });
}

async function main() {
  const { values, positionals } = parseCli();
  try {
    if (positionals.length !== 1) throw new Error("one Discord recovery operation is required");
    if (positionals[0] === "resolve") {
      const authority = resolveDiscordRecoveryAuthority(
        await readJsonFile(values["run-metadata"], "workflow run metadata"),
        await readJsonFile(values["artifact-list"], "workflow artifact list"),
        {
          repository: values.repository,
          workflowRunId: values["workflow-run-id"],
          workflowRunAttempt: values["workflow-run-attempt"],
          recoveryWorkflowRunId: values["recovery-workflow-run-id"],
          recoveryWorkflowRunAttempt: values["recovery-workflow-run-attempt"],
          runList: await readJsonFile(values["run-list"], "workflow run catalog"),
          runAttemptCatalog: await readJsonFile(
            values["run-attempt-catalog"],
            "workflow run-attempt catalog",
          ),
          jobList: await readJsonFile(values["job-list"], "workflow job-step authority"),
          runJobCatalog: await readJsonFile(
            values["run-job-catalog"],
            "workflow run-job catalog",
          ),
        },
      );
      await writeCanonicalNew(values.output, authority);
      if (values.format === "github-output") {
        process.stdout.write([
          `source_commit=${authority.source_commit}`,
          `workflow_run_id=${authority.workflow_run_id}`,
          `workflow_run_attempt=${authority.workflow_run_attempt}`,
          `recovery_required=${authority.recovery_required}`,
          `recovery_stage=${authority.recovery_stage}`,
          `artifact_id=${authority.artifact_id ?? ""}`,
          `artifact_name=${authority.artifact_name}`,
          `artifact_digest=${authority.artifact_digest ?? ""}`,
          `catalog_recovery_required=${authority.catalog_recovery_required}`,
          `catalog_artifact_id=${authority.catalog_artifact_id ?? ""}`,
          `catalog_artifact_name=${authority.catalog_artifact_name}`,
          `catalog_artifact_digest=${authority.catalog_artifact_digest ?? ""}`,
        ].join("\n") + "\n");
      } else if (values.format === "summary") {
        process.stdout.write(
          `discord_recovery=resolved run_id=${authority.workflow_run_id} ` +
          `run_attempt=${authority.workflow_run_attempt}\n`,
        );
      } else {
        throw new Error("Discord recovery output format is invalid");
      }
    } else if (positionals[0] === "seal-intent") {
      const intent = sealDiscordPrestageIntent({
        sourceCommit: values["source-commit"],
        workflowRunId: values["workflow-run-id"],
        workflowRunAttempt: values["workflow-run-attempt"],
        deploymentNonce: values["deployment-nonce"],
        cloudImageDigest: values["cloud-image-digest"],
        remoteOverlayArchive: values["remote-overlay-archive"],
        remoteOverlaySha256: values["remote-overlay-sha256"],
      });
      await writeCanonicalNew(values.output, intent);
      process.stdout.write("discord_recovery=prestage-intent-sealed\n");
    } else if (positionals[0] === "verify-intent") {
      await verifyDiscordPrestageIntent(values.report, {
        sourceCommit: values["source-commit"],
        workflowRunId: values["workflow-run-id"],
        workflowRunAttempt: values["workflow-run-attempt"],
        deploymentNonce: values["deployment-nonce"],
      });
      process.stdout.write("discord_recovery=prestage-intent-verified\n");
    } else if (positionals[0] === "seal-result") {
      const result = await sealDiscordRecoveryResult({
        stage: values.stage,
        repository: values.repository,
        sourceCommit: values["source-commit"],
        originalWorkflowRunId: values["original-workflow-run-id"],
        originalWorkflowRunAttempt: values["original-workflow-run-attempt"],
        recoveryWorkflowRunId: values["recovery-workflow-run-id"],
        recoveryWorkflowRunAttempt: values["recovery-workflow-run-attempt"],
        artifactId: values["artifact-id"],
        artifactDigest: values["artifact-digest"],
        recoveredAt: values["recovered-at"],
        catalogDisposition: values["catalog-disposition"],
        catalogRecoveryRequired: values["catalog-recovery-required"],
        catalogArtifactId: values["catalog-artifact-id"],
        catalogArtifactDigest: values["catalog-artifact-digest"],
        bindings: values.binding,
      });
      await writeCanonicalNew(values.output, result);
      process.stdout.write("discord_recovery=result-sealed\n");
    } else if (positionals[0] === "verify-result") {
      await verifyDiscordRecoveryResult(values.report, {
        stage: values.stage,
        repository: values.repository,
        sourceCommit: values["source-commit"],
        originalWorkflowRunId: values["original-workflow-run-id"],
        originalWorkflowRunAttempt: values["original-workflow-run-attempt"],
        recoveryWorkflowRunId: values["recovery-workflow-run-id"],
        recoveryWorkflowRunAttempt: values["recovery-workflow-run-attempt"],
        artifactId: values["artifact-id"],
        artifactDigest: values["artifact-digest"],
        catalogRecoveryRequired: values["catalog-recovery-required"],
        catalogArtifactId: values["catalog-artifact-id"],
        catalogArtifactDigest: values["catalog-artifact-digest"],
      });
      process.stdout.write("discord_recovery=result-verified\n");
    } else if (positionals[0] === "verify-evidence") {
      await verifyDiscordRecoveryEvidence(values["evidence-root"], {
        stage: values.stage,
        repository: values.repository,
        sourceCommit: values["source-commit"],
        originalWorkflowRunId: values["original-workflow-run-id"],
        originalWorkflowRunAttempt: values["original-workflow-run-attempt"],
        recoveryWorkflowRunId: values["recovery-workflow-run-id"],
        recoveryWorkflowRunAttempt: values["recovery-workflow-run-attempt"],
        artifactId: values["artifact-id"],
        artifactDigest: values["artifact-digest"],
        catalogRecoveryRequired: values["catalog-recovery-required"],
        catalogArtifactId: values["catalog-artifact-id"],
        catalogArtifactDigest: values["catalog-artifact-digest"],
      });
      process.stdout.write("discord_recovery=evidence-verified\n");
    } else {
      throw new Error("Discord recovery operation must be resolve, seal-intent, verify-intent, seal-result, verify-result, or verify-evidence");
    }
  } catch (error) {
    process.stderr.write(
      `discord_recovery=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}

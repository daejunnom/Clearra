#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { link, lstat, open, readFile, unlink } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  canonicalSha256,
  requireExactKeys,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  validateDiscordRecoveryAuthorityReport,
  verifyDiscordRecoveryEvidence,
} from "./discord-deployment-recovery.mjs";
import {
  parseCanonicalReceiptBytes,
  validateDiscordProductionCheckpointReceipt,
  validateImmutableCheckpointReleaseReadback,
} from "./finalize-discord-production-checkpoint.mjs";

export const DISCORD_RECOVERY_DEBT_PLAN_SCHEMA_ID =
  "clearra.discord-recovery-debt-plan.v1";
export const DISCORD_RECOVERY_DEBT_CLEARANCE_SCHEMA_ID =
  "clearra.discord-recovery-debt-clearance.v1";
export const DISCORD_RECOVERY_DEBT_CHECKPOINT_SCHEMA_ID =
  "clearra.discord-recovery-debt-checkpoint.v1";

const PRIMARY_NAME = "Deploy Discord Production";
const PRIMARY_PATH = ".github/workflows/discord-deploy.yml";
const RECOVERY_NAME = "Recover Discord Production";
const RECOVERY_PATH = ".github/workflows/discord-deploy-recovery.yml";
const REPOSITORY_ID = 1309293231;
const SHA = /^[0-9a-f]{40}$/u;
const DECIMAL = /^[1-9][0-9]*$/u;
const DIGEST = /^sha256:[0-9a-f]{64}$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const RECOVERABLE = new Set(["failure", "cancelled", "timed_out"]);
const PRIMARY_EVENTS = new Set(["workflow_dispatch", "workflow_run"]);
const JOB_CONCLUSIONS = new Set([
  "success", "failure", "cancelled", "skipped", "timed_out", "action_required",
  "neutral", "stale", "startup_failure",
]);
const RECOVERY_JOB_STEPS = Object.freeze({
  authority: Object.freeze([
    "Set up job", "Check out trusted recovery source for authority resolution",
    "Set up Node.js for recovery authority resolution",
    "Resolve exact original run attempt, freshness, and staged artifacts",
    "Record the proven no-Oracle-or-Cloud-runtime-mutation terminal case",
    "Upload exact recovery resolution",
    "Post Set up Node.js for recovery authority resolution",
    "Post Check out trusted recovery source for authority resolution", "Complete job",
  ]),
  recover: Object.freeze([
    "Set up job", "Check out trusted recovery source for protected restore",
    "Set up Node.js for protected recovery validation",
    "Re-resolve exact recovery authority immediately before protected mutation",
    "Hard-verify the exact artifact ZIP digest from the REST authority",
    "Download the already hash-verified exact artifact ID into runner temp",
    "Hard-verify the exact Discord catalog recovery ZIP and closed leaves",
    "Download the hash-verified Discord catalog recovery artifact",
    "Authenticate the rollback-only identity through its separate provider",
    "Set up gcloud for protected rollback",
    "Materialize the reviewer-protected Oracle recovery key",
    "Restore exact prior Cloud and Oracle authorities before catalog recovery",
    "Authenticate command sync for reviewer-protected catalog recovery",
    "Restore the exact prior Discord catalog and seal its disposition",
    "Retry catalog recovery after an ordinary failure or cancellation",
    "Re-authenticate rollback-only identity after catalog recovery",
    "Restore exact prior live authorities and seal the canonical result",
    "Retry exact recovery after an ordinary step failure or cancellation",
    "Preserve and verify the exact terminal recovery evidence as the sole success authority",
    "Upload durable verified recovery result",
    "Always remove the temporary Oracle recovery key and ZIP",
    "Fail closed unless the canonical recovery result was verified and uploaded",
    "Post Re-authenticate rollback-only identity after catalog recovery",
    "Post Authenticate command sync for reviewer-protected catalog recovery",
    "Post Authenticate the rollback-only identity through its separate provider",
    "Post Set up Node.js for protected recovery validation",
    "Post Check out trusted recovery source for protected restore", "Complete job",
  ]),
});
const RESOLUTION_NAME =
  /^discord-recovery-resolution-run-([1-9][0-9]*)-attempt-([1-9][0-9]*)-recovery-run-([1-9][0-9]*)-attempt-([1-9][0-9]*)$/u;
const RESULT_NAME =
  /^discord-runtime-recovery-([0-9a-f]{40})-source-run-([1-9][0-9]*)-attempt-([1-9][0-9]*)-recovery-run-([1-9][0-9]*)-attempt-([1-9][0-9]*)$/u;
const SEMVER_TAG = /^v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/u;
const BOOTSTRAP_COMMIT = "b1a56bc15b8d6decd1bcfc1b49163e0542e36cd6";
const BOOTSTRAP_AT = "2026-08-30T17:12:23Z";
const BOOTSTRAP_EXPIRES_AT = "2026-09-06T17:12:23Z";

export function resolveDiscordRecoveryDebtCheckpoint(
  runList,
  attempts,
  tagAuthority,
  options,
) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const sourceCommit = requirePattern(options?.sourceCommit, SHA, "current source commit");
  const currentRunId = requireDecimal(options?.workflowRunId, "current workflow run ID");
  const currentRunAttempt = requireDecimal(
    options?.workflowRunAttempt,
    "current workflow run attempt",
  );
  const current = attempts.get(`${currentRunId}:${currentRunAttempt}`);
  if (!current || current.sourceCommit !== sourceCommit || current.status !== "in_progress" || current.startedAt === null) {
    throw new Error("Discord checkpoint differs from the exact current attempt");
  }
  requirePlainObject(tagAuthority, "Discord checkpoint tag authority");
  requireExactKeys(
    tagAuthority,
    ["schema_id", "tags"],
    "Discord checkpoint tag authority",
  );
  if (
    tagAuthority.schema_id !== "clearra.discord-production-tag-authority-catalog.v1" ||
    !Array.isArray(tagAuthority.tags)
  ) throw new Error("Discord checkpoint tag authority is invalid");
  const reachableTags = [...new Set(options?.reachableTags ?? [])]
    .map((tag) => requirePattern(tag, SEMVER_TAG, "reachable production tag"))
    .sort(compareSemverTags);
  const tagNames = tagAuthority.tags.map((entry) =>
    requirePattern(entry?.name, SEMVER_TAG, "production tag authority name"))
    .sort(compareSemverTags);
  if (canonicalJson(tagNames) !== canonicalJson(reachableTags)) {
    throw new Error("Discord checkpoint tag catalog differs from reachable production tags");
  }
  const checkpoints = [];
  const latestReachableTag = reachableTags.at(-1) ?? null;
  const latestRequiresDurableReceipt = latestReachableTag !== null &&
    compareSemverTags(latestReachableTag, "v0.8.0") >= 0;
  for (const entry of tagAuthority.tags) {
    let checkpoint;
    try {
      const tag = validateCheckpointTag(entry, repository);
      if (tag.taggerAt >= current.startedAt) {
        throw new Error("Discord production tag is not prior to the current deployment attempt");
      }
      const parsed = parseCanonicalReceiptBytes(Buffer.from(tag.message, "utf8"));
      const receipt = validateDiscordProductionCheckpointReceipt(parsed.value, {
        repository,
        sourceCommit: tag.targetCommit,
        tagger: tag.tagger,
      });
      if (receipt.tag?.name !== tag.name || receipt.tag?.target_commit !== tag.targetCommit) {
        throw new Error("Discord durable checkpoint receipt tag identity differs");
      }
      const release = validateImmutableCheckpointReleaseReadback(entry.release, {
        repository,
        sourceCommit: tag.targetCommit,
        tag: tag.name,
        taggerAt: tag.taggerAtText,
        acceptedArtifacts: receipt.checkpoint_candidate.release_artifacts,
      });
      const releasePublishedAtText = requireTimestampText(
        release.published_at,
        "production Release published-at",
      );
      if (requireTimestamp(releasePublishedAtText, "production Release published-at") >=
          current.startedAt) {
        throw new Error("Discord durable checkpoint Release is not prior to the current attempt");
      }
      const checkpointAtText = requireTimestampText(
        receipt.discord_workflow_completed_at,
        "Discord durable checkpoint completed-at",
      );
      checkpoint = Object.freeze({
        checkpoint_kind: "durable-annotated-receipt-immutable-release",
        checkpoint_at: checkpointAtText,
        source_commit: receipt.source_commit,
        workflow_run_id: receipt.discord_workflow_run_id,
        workflow_run_attempt: receipt.discord_workflow_run_attempt,
        workflow_run_completed_at: checkpointAtText,
        tag: tag.name,
        tag_object_sha: tag.objectSha,
        tag_target_commit: tag.targetCommit,
        tagger_at: tag.taggerAtText,
        release_id: String(release.id),
        release_published_at: releasePublishedAtText,
        release_assets: receipt.checkpoint_candidate.release_artifacts,
        release_api_readback_sha256: canonicalSha256(release),
        deployment_job_step_proof: receipt.completed_job_topology,
        run_catalog_api_readback_sha256: canonicalSha256(runList),
        checkpoint_receipt: receipt,
        checkpoint_receipt_sha256: receipt.report_sha256,
        checkpoint_receipt_file_sha256: createHash("sha256")
          .update(Buffer.from(tag.message, "utf8"))
          .digest("hex"),
      });
    } catch (error) {
      if (entry?.name === latestReachableTag && latestRequiresDurableReceipt) throw error;
      continue;
    }
    checkpoints.push(checkpoint);
  }
  if (checkpoints.length > 0) {
    checkpoints.sort((left, right) => {
      const byTime = requireTimestamp(left.checkpoint_at, "checkpoint-at") -
        requireTimestamp(right.checkpoint_at, "checkpoint-at");
      return byTime !== 0
        ? byTime
        : compareIds(left.workflow_run_id, right.workflow_run_id);
    });
    const selected = checkpoints.at(-1);
    if (
      checkpoints.length > 1 &&
      checkpoints.at(-2).checkpoint_at === selected.checkpoint_at
    ) throw new Error("Discord observed production checkpoint is ambiguous");
    return Object.freeze(sealCanonicalReport({
      schema_id: DISCORD_RECOVERY_DEBT_CHECKPOINT_SCHEMA_ID,
      repository,
      current_source_commit: sourceCommit,
      ...selected,
    }));
  }
  validateBootstrapProof(options?.bootstrapProof, sourceCommit);
  if (current.startedAt > requireTimestamp(BOOTSTRAP_EXPIRES_AT, "bootstrap expiry")) {
    throw new Error("Discord one-time recovery checkpoint bootstrap has expired");
  }
  const bootstrapAt = requireTimestamp(BOOTSTRAP_AT, "bootstrap-at");
  for (const run of flattenRunPages(runList, "Discord primary run catalog")) {
    const createdAt = requireTimestamp(run.created_at, "catalog run created-at");
    if (createdAt <= bootstrapAt) {
      throw new Error("Discord primary run catalog does not begin after the workflow bootstrap epoch");
    }
  }
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_RECOVERY_DEBT_CHECKPOINT_SCHEMA_ID,
    repository,
    current_source_commit: sourceCommit,
    checkpoint_kind: "code-bound-one-time-bootstrap",
    checkpoint_at: BOOTSTRAP_AT,
    source_commit: BOOTSTRAP_COMMIT,
    workflow_run_id: null,
    workflow_run_attempt: null,
    workflow_run_completed_at: null,
    tag: null,
    tag_object_sha: null,
    tag_target_commit: null,
    tagger_at: null,
    release_id: null,
    release_published_at: null,
    release_assets: null,
    release_api_readback_sha256: null,
    deployment_job_step_proof: null,
    run_catalog_api_readback_sha256: canonicalSha256(runList),
    checkpoint_receipt: null,
    checkpoint_receipt_sha256: null,
    checkpoint_receipt_file_sha256: null,
  }));
}

function validateCheckpointTag(entry, repository) {
  requirePlainObject(entry, "Discord production tag checkpoint candidate");
  requireExactKeys(
    entry,
    ["name", "local_tag_object_sha", "local_target_commit", "tag_ref", "tag_object", "release"],
    "Discord production tag checkpoint candidate",
  );
  const ref = entry.tag_ref;
  const object = entry.tag_object;
  requirePlainObject(ref, "Discord production tag ref");
  requirePlainObject(object, "Discord production annotated tag");
  const name = requirePattern(entry.name, SEMVER_TAG, "production tag");
  const objectSha = requirePattern(ref.object?.sha, SHA, "production tag object SHA");
  const targetCommit = requirePattern(object.object?.sha, SHA, "production tag target commit");
  const taggerAtText = requireTimestampText(object.tagger?.date, "production tagger-at");
  if (
    ref.ref !== `refs/tags/${name}` || ref.object?.type !== "tag" ||
    ref.url !== `https://api.github.com/repos/${repository}/git/refs/tags/${name}` ||
    object.sha !== objectSha || object.tag !== name || object.object?.type !== "commit" ||
    object.url !== `https://api.github.com/repos/${repository}/git/tags/${objectSha}` ||
    entry.local_tag_object_sha !== objectSha || entry.local_target_commit !== targetCommit ||
    typeof object.message !== "string"
  ) throw new Error("Discord production tag is not one exact annotated commit tag");
  return Object.freeze({
    name,
    objectSha,
    targetCommit,
    taggerAt: requireTimestamp(taggerAtText, "production tagger-at"),
    taggerAtText,
    tagger: object.tagger,
    message: object.message,
  });
}

function validateRecoveryJobCatalog(value) {
  requirePlainObject(value, "Discord recovery job catalog");
  requireExactKeys(value, ["schema_id", "runs"], "Discord recovery job catalog");
  if (value.schema_id !== "clearra.discord-recovery-job-catalog.v1" || !Array.isArray(value.runs)) {
    throw new Error("Discord recovery job catalog is invalid");
  }
  const result = new Map();
  for (const entry of value.runs) {
    requirePlainObject(entry, "Discord recovery job catalog entry");
    requireExactKeys(
      entry,
      ["workflow_run_id", "workflow_run_attempt", "pages"],
      "Discord recovery job catalog entry",
    );
    const key = `${requireDecimal(entry.workflow_run_id, "recovery job run ID")}:` +
      requireDecimal(entry.workflow_run_attempt, "recovery job run attempt");
    if (result.has(key) || !Array.isArray(entry.pages) || entry.pages.length === 0) {
      throw new Error("Discord recovery job catalog is ambiguous");
    }
    result.set(key, entry.pages);
  }
  return result;
}

function validateRecoveryJobAuthority(pages, recovery, candidate, repository) {
  let total = null;
  const jobs = [];
  for (const page of pages) {
    requirePlainObject(page, "Discord recovery job page");
    if (!Number.isSafeInteger(page.total_count) || page.total_count < 0 || !Array.isArray(page.jobs)) {
      throw new Error("Discord recovery job authority is invalid");
    }
    if (total === null) total = page.total_count;
    if (total !== page.total_count) throw new Error("Discord recovery job page totals differ");
    jobs.push(...page.jobs);
  }
  if (jobs.length !== total) throw new Error("Discord recovery job authority is incomplete");
  const byName = new Map();
  for (const job of jobs) {
    requirePlainObject(job, "Discord recovery job");
    if (
      requireDecimal(job.run_id, "recovery job run ID") !== recovery.runId ||
      requireDecimal(job.run_attempt, "recovery job run attempt") !== recovery.attempt ||
      job.head_sha !== recovery.sourceCommit || job.head_branch !== "main" ||
      job.status !== "completed" || !JOB_CONCLUSIONS.has(job.conclusion) ||
      !Object.hasOwn(RECOVERY_JOB_STEPS, job.name) || byName.has(job.name) ||
      typeof job.html_url !== "string" ||
      !job.html_url.startsWith(`https://github.com/${repository}/actions/runs/${recovery.runId}/job/`) ||
      !Array.isArray(job.steps)
    ) throw new Error("Discord recovery job differs from its exact workflow attempt");
    const expected = RECOVERY_JOB_STEPS[job.name];
    let priorIndex = -1;
    let priorNumber = 0;
    const names = new Set();
    for (const step of job.steps) {
      const index = expected.indexOf(step.name);
      if (
        index < 0 || index <= priorIndex || names.has(step.name) ||
        !Number.isSafeInteger(step.number) || step.number <= priorNumber ||
        step.status !== "completed" || !JOB_CONCLUSIONS.has(step.conclusion)
      ) throw new Error("Discord recovery job contains foreign or unordered step authority");
      names.add(step.name);
      priorIndex = index;
      priorNumber = step.number;
    }
    byName.set(job.name, job);
  }
  if (byName.size !== 2 || !byName.has("authority") || !byName.has("recover")) {
    throw new Error("Discord recovery jobs differ from the closed workflow topology");
  }
  const resolutionUpload = byName.get("authority").steps.find((step) =>
    step.name === "Upload exact recovery resolution");
  if (resolutionUpload?.conclusion !== "success") {
    throw new Error("Discord recovery resolution artifact lacks its successful upload authority");
  }
  const resolutionCreatedAt = requireTimestamp(
    candidate.resolution_artifact.artifact_created_at,
    "recovery resolution artifact created-at",
  );
  const resolutionUploadStartedAt = requireTimestamp(
    resolutionUpload.started_at,
    "recovery resolution upload started-at",
  );
  if (resolutionCreatedAt < resolutionUploadStartedAt) {
    throw new Error("Discord recovery resolution predates its upload step");
  }
  const proof = {
    authority_job_id: requireDecimal(byName.get("authority").id, "recovery authority job ID"),
    authority_job_conclusion: byName.get("authority").conclusion,
    authority_job_steps: byName.get("authority").steps.map(sealJobStepProof),
    resolution_upload_step: sealJobStepProof(resolutionUpload),
    recover_job_id: requireDecimal(byName.get("recover").id, "recovery restore job ID"),
    recover_job_conclusion: byName.get("recover").conclusion,
    recover_job_steps: byName.get("recover").steps.map(sealJobStepProof),
    runtime_restore_step: null,
    catalog_auth_step: null,
    catalog_restore_step: null,
    catalog_retry_step: null,
    rollback_reauth_step: null,
    restore_step: null,
    retry_step: null,
    result_verify_step: null,
    result_upload_step: null,
    terminal_fail_step: null,
  };
  if (candidate.result_artifact !== null) {
    const recoverSteps = byName.get("recover").steps;
    const runtimeRestore = recoverSteps.find((step) =>
      step.name === "Restore exact prior Cloud and Oracle authorities before catalog recovery");
    const catalogAuth = recoverSteps.find((step) =>
      step.name === "Authenticate command sync for reviewer-protected catalog recovery");
    const catalogRestore = recoverSteps.find((step) =>
      step.name === "Restore the exact prior Discord catalog and seal its disposition");
    const catalogRetry = recoverSteps.find((step) =>
      step.name === "Retry catalog recovery after an ordinary failure or cancellation");
    const rollbackReauth = recoverSteps.find((step) =>
      step.name === "Re-authenticate rollback-only identity after catalog recovery");
    const restore = recoverSteps.find((step) =>
      step.name === "Restore exact prior live authorities and seal the canonical result");
    const retry = byName.get("recover").steps.find((step) =>
      step.name === "Retry exact recovery after an ordinary step failure or cancellation");
    const verify = byName.get("recover").steps.find((step) =>
      step.name === "Preserve and verify the exact terminal recovery evidence as the sole success authority");
    const upload = byName.get("recover").steps.find((step) =>
      step.name === "Upload durable verified recovery result");
    const terminalFail = byName.get("recover").steps.find((step) =>
      step.name === "Fail closed unless the canonical recovery result was verified and uploaded");
    const catalogDirectSuccess = catalogRestore?.conclusion === "success" &&
      catalogRetry?.conclusion === "skipped";
    const catalogRetrySuccess = ["failure", "cancelled", "timed_out"].includes(
      catalogRestore?.conclusion,
    ) && catalogRetry?.conclusion === "success";
    const directSuccess = restore?.conclusion === "success" && retry?.conclusion === "skipped";
    const retrySuccess = ["failure", "cancelled", "timed_out"].includes(restore?.conclusion) &&
      retry?.conclusion === "success";
    if (
      !["success", "failure", "cancelled", "timed_out"].includes(
        runtimeRestore?.conclusion,
      ) || (!catalogDirectSuccess && !catalogRetrySuccess) ||
      rollbackReauth?.conclusion !== "success" ||
      (!directSuccess && !retrySuccess) || verify?.conclusion !== "success" ||
      upload?.conclusion !== "success" || terminalFail?.conclusion !== "skipped" ||
      catalogRestore.number <= runtimeRestore.number ||
      catalogRetry.number <= catalogRestore.number ||
      rollbackReauth.number <= catalogRetry.number ||
      restore.number <= rollbackReauth.number || retry.number <= restore.number ||
      verify.number <= retry.number ||
      upload.number <= verify.number || terminalFail.number <= upload.number
    ) {
      throw new Error("Discord recovery result lacks terminal verify-and-upload authority");
    }
    if (
      requireTimestamp(candidate.result_artifact.artifact_created_at, "recovery result created-at") <
      requireTimestamp(upload.started_at, "recovery result upload started-at")
    ) throw new Error("Discord recovery result predates its upload step");
    proof.runtime_restore_step = sealJobStepProof(runtimeRestore);
    proof.catalog_auth_step = sealJobStepProof(catalogAuth);
    proof.catalog_restore_step = sealJobStepProof(catalogRestore);
    proof.catalog_retry_step = sealJobStepProof(catalogRetry);
    proof.rollback_reauth_step = sealJobStepProof(rollbackReauth);
    proof.restore_step = sealJobStepProof(restore);
    proof.retry_step = sealJobStepProof(retry);
    proof.result_verify_step = sealJobStepProof(verify);
    proof.result_upload_step = sealJobStepProof(upload);
    proof.terminal_fail_step = sealJobStepProof(terminalFail);
  }
  return Object.freeze(proof);
}

function sealJobStepProof(step) {
  return Object.freeze({
    name: step.name,
    number: step.number,
    status: step.status,
    conclusion: step.conclusion,
    started_at: requireTimestampText(step.started_at, `${step.name} started-at`),
    completed_at: requireTimestampText(step.completed_at, `${step.name} completed-at`),
  });
}

function validateCatalogRecoveryJobProof(proof, required) {
  requirePlainObject(proof, "Discord recovery job-step proof");
  if (!Array.isArray(proof.recover_job_steps)) {
    throw new Error("Discord recovery job-step proof lacks the exact recover topology");
  }
  const byName = new Map(proof.recover_job_steps.map((step) => [step.name, step]));
  const conditional = [
    "Hard-verify the exact Discord catalog recovery ZIP and closed leaves",
    "Download the hash-verified Discord catalog recovery artifact",
    "Authenticate command sync for reviewer-protected catalog recovery",
  ];
  for (const name of conditional) {
    const step = byName.get(name);
    if (!step || step.conclusion !== (required ? "success" : "skipped")) {
      throw new Error("Discord catalog recovery protected step topology differs");
    }
  }
  const restore = byName.get(
    "Restore the exact prior Discord catalog and seal its disposition",
  );
  const retry = byName.get("Retry catalog recovery after an ordinary failure or cancellation");
  const direct = restore?.conclusion === "success" && retry?.conclusion === "skipped";
  const retried = ["failure", "cancelled", "timed_out"].includes(restore?.conclusion) &&
    retry?.conclusion === "success";
  if (!direct && !retried) {
    throw new Error("Discord catalog recovery did not produce one exact terminal success path");
  }
}

function validateBootstrapProof(value, sourceCommit) {
  requirePlainObject(value, "Discord one-time bootstrap proof");
  requireExactKeys(value, [
    "bootstrap_commit", "bootstrap_committed_at", "current_source_contains_bootstrap",
    "discord_deploy_workflow_absent",
  ], "Discord one-time bootstrap proof");
  if (
    value.bootstrap_commit !== BOOTSTRAP_COMMIT ||
    value.bootstrap_committed_at !== BOOTSTRAP_AT ||
    value.current_source_contains_bootstrap !== true ||
    value.discord_deploy_workflow_absent !== true ||
    sourceCommit === BOOTSTRAP_COMMIT
  ) throw new Error("Discord one-time bootstrap proof is invalid");
}

function compareSemverTags(left, right) {
  const a = left.slice(1).split(".").map(Number);
  const b = right.slice(1).split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return 0;
}

export function planDiscordRecoveryDebt(runList, primaryAttempts, recoveryAttempts, artifactPages, options) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const currentRunId = requireDecimal(options?.workflowRunId, "current workflow run ID");
  const currentRunAttempt = requireDecimal(
    options?.workflowRunAttempt,
    "current workflow run attempt",
  );
  const currentSource = requirePattern(options?.sourceCommit, SHA, "current source commit");
  const catalog = flattenRunPages(runList, "Discord primary run catalog");
  const attempts = validateAttemptEnvelope(primaryAttempts, "primary", repository);
  validatePrimaryAttemptCompleteness(catalog, attempts, repository);
  const current = attempts.get(`${currentRunId}:${currentRunAttempt}`);
  if (
    !current ||
    current.sourceCommit !== currentSource ||
    !["queued", "in_progress"].includes(current.status)
  ) {
    throw new Error("Discord recovery-debt gate differs from the exact current primary attempt");
  }
  const checkpoint = resolveDiscordRecoveryDebtCheckpoint(
    runList,
    attempts,
    options?.tagAuthority,
    { ...options, repository, sourceCommit: currentSource },
  );
  const checkpointAt = requireTimestamp(checkpoint.checkpoint_at, "Discord debt checkpoint-at");
  if (current.startedAt === null || current.startedAt <= checkpointAt) {
    throw new Error("Discord current attempt does not follow the exact debt checkpoint");
  }

  const debts = [...attempts.values()]
    .filter((entry) =>
      entry.key !== current.key &&
      entry.status === "completed" &&
      RECOVERABLE.has(entry.conclusion) &&
      entry.updatedAt > checkpointAt)
    .sort(compareAttempts);
  const recoveries = validateAttemptEnvelope(recoveryAttempts, "recovery", repository);
  const recoveryJobs = validateRecoveryJobCatalog(options?.recoveryJobCatalog);
  for (const key of recoveryJobs.keys()) {
    if (!recoveries.has(key)) throw new Error("Discord recovery job catalog contains foreign authority");
  }
  const artifacts = flattenArtifactPages(artifactPages);
  const candidatesByDebt = new Map(debts.map((debt) => [debt.key, new Map()]));

  for (const artifact of artifacts) {
    const parsed = parseRecoveryArtifact(artifact.name);
    if (!parsed) continue;
    const debtKey = `${parsed.primaryRunId}:${parsed.primaryRunAttempt}`;
    const candidateMap = candidatesByDebt.get(debtKey);
    if (!candidateMap) continue;
    const debt = attempts.get(debtKey);
    if (parsed.sourceCommit !== null && parsed.sourceCommit !== debt.sourceCommit) {
      throw new Error("Discord recovery result artifact source differs from its debt parent");
    }
    const recoveryKey = `${parsed.recoveryRunId}:${parsed.recoveryRunAttempt}`;
    const recovery = recoveries.get(recoveryKey);
    if (
      !recovery || recovery.status !== "completed" ||
      !["success", "failure", "cancelled", "timed_out"].includes(recovery.conclusion)
    ) {
      continue;
    }
    validateRecoveryArtifact(artifact, parsed, recovery, repository);
    const candidate = candidateMap.get(recoveryKey) ?? {
      recovery_workflow_run_id: recovery.runId,
      recovery_workflow_run_attempt: recovery.attempt,
      recovery_workflow_run_number: recovery.runNumber,
      recovery_run_started_at: recovery.startedAtText,
      recovery_run_updated_at: recovery.updatedAtText,
      resolution_artifact: null,
      result_artifact: null,
    };
    const field = parsed.kind === "resolution" ? "resolution_artifact" : "result_artifact";
    if (candidate[field] !== null) {
      throw new Error("Discord recovery-debt artifact authority is ambiguous");
    }
    candidate[field] = Object.freeze({
      artifact_id: artifact.id,
      artifact_name: artifact.name,
      artifact_digest: artifact.digest,
      artifact_created_at: artifact.createdAtText,
      report_name: parsed.kind === "resolution" ? "recovery-authority.json" : "recovery-result.json",
    });
    candidateMap.set(recoveryKey, candidate);
  }

  const plannedDebts = [];
  const downloads = new Map();
  for (const debt of debts) {
    const candidates = [...candidatesByDebt.get(debt.key).values()]
      .filter((candidate) => candidate.resolution_artifact !== null)
      .sort((left, right) => compareIds(
        left.recovery_workflow_run_id,
        right.recovery_workflow_run_id,
      ));
    if (candidates.length === 0) {
      throw new Error(
        `Discord recovery debt ${debt.runId}/${debt.attempt} lacks a successful parent-bound resolution`,
      );
    }
    for (const candidate of candidates) {
      const recoveryKey = `${candidate.recovery_workflow_run_id}:${candidate.recovery_workflow_run_attempt}`;
      const recovery = recoveries.get(recoveryKey);
      const pages = recoveryJobs.get(recoveryKey);
      if (!pages) throw new Error("Discord recovery clearance lacks exact recovery job authority");
      candidate.recovery_job_step_proof = validateRecoveryJobAuthority(
        pages,
        recovery,
        candidate,
        repository,
      );
    }
    for (const candidate of candidates) {
      for (const artifact of [candidate.resolution_artifact, candidate.result_artifact]) {
        if (!artifact) continue;
        downloads.set(artifact.artifact_id, artifact);
      }
    }
    plannedDebts.push(Object.freeze({
      primary_workflow_run_id: debt.runId,
      primary_workflow_run_attempt: debt.attempt,
      source_commit: debt.sourceCommit,
      workflow_event: debt.event,
      workflow_conclusion: debt.conclusion,
      candidates,
    }));
  }
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_RECOVERY_DEBT_PLAN_SCHEMA_ID,
    repository,
    current_workflow_run_id: currentRunId,
    current_workflow_run_attempt: currentRunAttempt,
    current_source_commit: currentSource,
    checkpoint,
    debts: plannedDebts,
    downloads: [...downloads.values()].sort((left, right) => compareIds(
      left.artifact_id,
      right.artifact_id,
    )),
  }));
}

export async function auditDiscordRecoveryDebt(planPath, reportRoot, options) {
  const plan = await readJsonFile(planPath, "Discord recovery-debt plan");
  validateDebtPlan(plan, options);
  const cleared = [];
  for (const debt of plan.debts) {
    const qualifying = [];
    for (const candidate of debt.candidates) {
      const resolution = await readArtifactReport(
        reportRoot,
        candidate.resolution_artifact,
        "Discord recovery resolution",
      );
      validateRecoveryAuthorityReport(resolution, debt, candidate, plan.repository);
      if (resolution.recovery_required === false) {
        if (candidate.recovery_job_step_proof.recover_job_conclusion !== "skipped") {
          throw new Error("Discord no-runtime-mutation resolution unexpectedly ran protected recovery");
        }
        qualifying.push({
          recovery_workflow_run_id: candidate.recovery_workflow_run_id,
          recovery_workflow_run_attempt: candidate.recovery_workflow_run_attempt,
          clearance_kind: "certified-no-runtime-mutation",
          resolution_report_sha256: resolution.report_sha256,
          result_report_sha256: null,
          recovered_stage: "none",
          recovered_artifact_id: null,
          recovered_artifact_digest: null,
        });
        continue;
      }
      validateCatalogRecoveryJobProof(
        candidate.recovery_job_step_proof,
        resolution.catalog_recovery_required,
      );
      if (!candidate.result_artifact) continue;
      const resultRoot = join(
        resolve(String(reportRoot ?? "")),
        requireDecimal(candidate.result_artifact.artifact_id, "result artifact ID"),
      );
      const verifiedEvidence = await verifyDiscordRecoveryEvidence(resultRoot, {
        stage: resolution.recovery_stage,
        repository: plan.repository,
        sourceCommit: debt.source_commit,
        originalWorkflowRunId: debt.primary_workflow_run_id,
        originalWorkflowRunAttempt: debt.primary_workflow_run_attempt,
        recoveryWorkflowRunId: candidate.recovery_workflow_run_id,
        recoveryWorkflowRunAttempt: candidate.recovery_workflow_run_attempt,
        artifactId: resolution.artifact_id,
        artifactDigest: resolution.artifact_digest,
        workflowEvent: debt.workflow_event,
        workflowConclusion: debt.workflow_conclusion,
        catalogRecoveryRequired: resolution.catalog_recovery_required,
        catalogArtifactId: resolution.catalog_artifact_id,
        catalogArtifactDigest: resolution.catalog_artifact_digest,
      });
      const result = verifiedEvidence.result;
      const resolutionCreatedAt = requireTimestamp(
        candidate.resolution_artifact.artifact_created_at,
        "recovery resolution artifact created-at",
      );
      const resultCreatedAt = requireTimestamp(
        candidate.result_artifact.artifact_created_at,
        "recovery result artifact created-at",
      );
      const recoveredAt = requireTimestamp(result.recovered_at, "recovered-at");
      const recoveryStartedAt = requireTimestamp(
        candidate.recovery_run_started_at,
        "recovery run started-at",
      );
      const recoveryUpdatedAt = requireTimestamp(
        candidate.recovery_run_updated_at,
        "recovery run updated-at",
      );
      if (
        resultCreatedAt < resolutionCreatedAt || recoveredAt < resolutionCreatedAt ||
        recoveredAt < recoveryStartedAt ||
        recoveredAt > resultCreatedAt || recoveredAt > recoveryUpdatedAt
      ) {
        throw new Error("Discord recovery result chronology differs from its exact run attempt");
      }
      qualifying.push({
        recovery_workflow_run_id: candidate.recovery_workflow_run_id,
        recovery_workflow_run_attempt: candidate.recovery_workflow_run_attempt,
        clearance_kind: `completed-${resolution.recovery_stage}-recovery`,
        resolution_report_sha256: resolution.report_sha256,
        result_report_sha256: result.report_sha256,
        recovered_stage: resolution.recovery_stage,
        recovered_artifact_id: resolution.artifact_id,
        recovered_artifact_digest: resolution.artifact_digest,
      });
    }
    if (qualifying.length === 0) {
      throw new Error(
        `Discord recovery debt ${debt.primary_workflow_run_id}/${debt.primary_workflow_run_attempt} has no exact clearance`,
      );
    }
    const clearanceIdentities = new Set(qualifying.map((entry) => JSON.stringify([
      entry.clearance_kind,
      entry.recovered_stage,
      entry.recovered_artifact_id,
      entry.recovered_artifact_digest,
    ])));
    if (clearanceIdentities.size !== 1) {
      throw new Error(
        `Discord recovery debt ${debt.primary_workflow_run_id}/${debt.primary_workflow_run_attempt} has inconsistent successful clearances`,
      );
    }
    qualifying.sort((left, right) => {
      const leftCandidate = debt.candidates.find((candidate) =>
        candidate.recovery_workflow_run_id === left.recovery_workflow_run_id &&
        candidate.recovery_workflow_run_attempt === left.recovery_workflow_run_attempt);
      const rightCandidate = debt.candidates.find((candidate) =>
        candidate.recovery_workflow_run_id === right.recovery_workflow_run_id &&
        candidate.recovery_workflow_run_attempt === right.recovery_workflow_run_attempt);
      const byNumber = compareIds(
        leftCandidate.recovery_workflow_run_number,
        rightCandidate.recovery_workflow_run_number,
      );
      return byNumber !== 0
        ? byNumber
        : compareIds(left.recovery_workflow_run_attempt, right.recovery_workflow_run_attempt);
    });
    const selected = qualifying.at(-1);
    cleared.push(Object.freeze({
      primary_workflow_run_id: debt.primary_workflow_run_id,
      primary_workflow_run_attempt: debt.primary_workflow_run_attempt,
      source_commit: debt.source_commit,
      recovery_workflow_run_id: selected.recovery_workflow_run_id,
      recovery_workflow_run_attempt: selected.recovery_workflow_run_attempt,
      clearance_kind: selected.clearance_kind,
      resolution_report_sha256: selected.resolution_report_sha256,
      result_report_sha256: selected.result_report_sha256,
      consistent_clearance_count: qualifying.length,
    }));
  }
  return Object.freeze(sealCanonicalReport({
    schema_id: DISCORD_RECOVERY_DEBT_CLEARANCE_SCHEMA_ID,
    repository: plan.repository,
    current_workflow_run_id: plan.current_workflow_run_id,
    current_workflow_run_attempt: plan.current_workflow_run_attempt,
    current_source_commit: plan.current_source_commit,
    checkpoint_sha256: plan.checkpoint.report_sha256,
    plan_sha256: plan.report_sha256,
    cleared_debts: cleared,
  }));
}

function validateDebtPlan(plan, options) {
  requireExactKeys(plan, [
    "schema_id", "repository", "current_workflow_run_id",
    "current_workflow_run_attempt", "current_source_commit", "checkpoint", "debts", "downloads",
    "report_sha256",
  ], "Discord recovery-debt plan");
  verifyCanonicalReportHash(plan, "Discord recovery-debt plan");
  if (
    plan.schema_id !== DISCORD_RECOVERY_DEBT_PLAN_SCHEMA_ID ||
    plan.repository !== requirePattern(options?.repository, REPOSITORY, "repository") ||
    plan.current_workflow_run_id !== requireDecimal(options?.workflowRunId, "workflow run ID") ||
    plan.current_workflow_run_attempt !== requireDecimal(
      options?.workflowRunAttempt,
      "workflow run attempt",
    ) ||
    plan.current_source_commit !== requirePattern(options?.sourceCommit, SHA, "source commit") ||
    plan.checkpoint?.schema_id !== DISCORD_RECOVERY_DEBT_CHECKPOINT_SCHEMA_ID ||
    plan.checkpoint?.repository !== plan.repository ||
    plan.checkpoint?.current_source_commit !== plan.current_source_commit ||
    !Array.isArray(plan.debts) ||
    !Array.isArray(plan.downloads)
  ) {
    throw new Error("Discord recovery-debt plan differs from the exact current authority");
  }
  verifyCanonicalReportHash(plan.checkpoint, "Discord recovery-debt checkpoint");
}

function validateRecoveryAuthorityReport(report, debt, candidate, repository) {
  validateDiscordRecoveryAuthorityReport(report, {
    repository,
    sourceCommit: debt.source_commit,
    workflowRunId: debt.primary_workflow_run_id,
    workflowRunAttempt: debt.primary_workflow_run_attempt,
    recoveryWorkflowRunId: candidate.recovery_workflow_run_id,
    recoveryWorkflowRunAttempt: candidate.recovery_workflow_run_attempt,
    workflowEvent: debt.workflow_event,
    workflowConclusion: debt.workflow_conclusion,
  });
}

function validatePrimaryAttemptCompleteness(catalog, attempts, repository) {
  const expected = new Set();
  for (const run of catalog) {
    validateRunIdentity(run, "primary", repository);
    const id = requireDecimal(run.id, "catalog run ID");
    const maxAttempt = Number(requireDecimal(run.run_attempt, "catalog run attempt"));
    for (let attempt = 1; attempt <= maxAttempt; attempt += 1) expected.add(`${id}:${attempt}`);
  }
  if (attempts.size !== expected.size || [...expected].some((key) => !attempts.has(key))) {
    throw new Error("Discord primary attempt catalog is incomplete");
  }
}

function validateAttemptEnvelope(value, kind, repository) {
  requirePlainObject(value, `Discord ${kind} attempt catalog`);
  requireExactKeys(value, ["schema_id", "attempts"], `Discord ${kind} attempt catalog`);
  if (
    value.schema_id !== `clearra.discord-${kind}-attempt-catalog.v1` ||
    !Array.isArray(value.attempts)
  ) throw new Error(`Discord ${kind} attempt catalog is invalid`);
  const map = new Map();
  for (const run of value.attempts) {
    const normalized = validateRunIdentity(run, kind, repository);
    if (map.has(normalized.key)) throw new Error(`Discord ${kind} attempt catalog is ambiguous`);
    map.set(normalized.key, normalized);
  }
  return map;
}

function validateRunIdentity(run, kind, expectedRepository) {
  requirePlainObject(run, `Discord ${kind} workflow run`);
  const repository = expectedRepository ?? run.repository?.full_name;
  const id = requireDecimal(run.id, `${kind} run ID`);
  const attempt = requireDecimal(run.run_attempt, `${kind} run attempt`);
  const runNumber = requireDecimal(run.run_number, `${kind} run number`);
  const sourceCommit = requirePattern(run.head_sha, SHA, `${kind} source commit`);
  const createdAt = requireTimestamp(run.created_at, `${kind} created-at`);
  const startedAt = run.status === "queued" && run.run_started_at === null
    ? null
    : requireTimestamp(run.run_started_at, `${kind} run-started-at`);
  const updatedAt = requireTimestamp(run.updated_at, `${kind} updated-at`);
  if (createdAt > updatedAt || (startedAt !== null && (createdAt > startedAt || startedAt > updatedAt))) {
    throw new Error(`Discord ${kind} workflow timestamps are inconsistent`);
  }
  const isPrimary = kind === "primary";
  if (
    run.name !== (isPrimary ? PRIMARY_NAME : RECOVERY_NAME) ||
    run.path !== (isPrimary ? PRIMARY_PATH : RECOVERY_PATH) ||
    (isPrimary ? !PRIMARY_EVENTS.has(run.event) : run.event !== "workflow_run") ||
    run.head_branch !== "main" ||
    run.repository?.id !== REPOSITORY_ID ||
    run.repository?.full_name !== repository ||
    run.head_repository?.id !== REPOSITORY_ID ||
    run.head_repository?.full_name !== repository ||
    !["queued", "in_progress", "completed"].includes(run.status) ||
    (run.status === "completed" ? typeof run.conclusion !== "string" : run.conclusion !== null)
  ) throw new Error(`Discord ${kind} workflow run contains foreign authority`);
  return Object.freeze({
    key: `${id}:${attempt}`,
    runId: id,
    attempt,
    runNumber,
    sourceCommit,
    event: run.event,
    status: run.status,
    conclusion: run.conclusion,
    createdAt,
    startedAt,
    updatedAt,
    createdAtText: run.created_at,
    updatedAtText: run.updated_at,
    startedAtText: run.run_started_at,
  });
}

function flattenRunPages(value, label) {
  const pages = Array.isArray(value) ? value : [value];
  let total = null;
  const runs = [];
  for (const page of pages) {
    requirePlainObject(page, label);
    if (!Number.isSafeInteger(page.total_count) || page.total_count < 0 || !Array.isArray(page.workflow_runs)) {
      throw new Error(`${label} is invalid`);
    }
    if (total === null) total = page.total_count;
    if (total !== page.total_count) throw new Error(`${label} page totals differ`);
    runs.push(...page.workflow_runs);
  }
  if (runs.length !== total) throw new Error(`${label} is incomplete`);
  return runs;
}

function flattenArtifactPages(value) {
  const pages = Array.isArray(value) ? value : [value];
  let total = null;
  const artifacts = [];
  for (const page of pages) {
    requirePlainObject(page, "Discord repository artifact page");
    if (!Number.isSafeInteger(page.total_count) || page.total_count < 0 || !Array.isArray(page.artifacts)) {
      throw new Error("Discord repository artifact catalog is invalid");
    }
    if (total === null) total = page.total_count;
    if (total !== page.total_count) throw new Error("Discord artifact page totals differ");
    artifacts.push(...page.artifacts);
  }
  if (artifacts.length !== total) throw new Error("Discord artifact catalog is incomplete");
  const normalized = artifacts.flatMap((artifact) => {
    if (parseRecoveryArtifact(String(artifact?.name ?? "")) === null) return [];
    requirePlainObject(artifact, "Discord recovery artifact");
    if (artifact.expired === true) return [];
    if (artifact.expired !== false) {
      throw new Error("Discord recovery artifact expiry authority is invalid");
    }
    return [{
      raw: artifact,
      id: requireDecimal(artifact.id, "artifact ID"),
      name: String(artifact.name ?? ""),
      digest: requirePattern(artifact.digest, DIGEST, "artifact digest"),
      createdAt: requireTimestamp(artifact.created_at, "artifact created-at"),
      createdAtText: artifact.created_at,
    }];
  });
  const ids = new Set();
  for (const artifact of normalized) {
    if (ids.has(artifact.id)) throw new Error("Discord recovery artifact catalog is ambiguous");
    ids.add(artifact.id);
  }
  return normalized;
}

function parseRecoveryArtifact(name) {
  let match = RESOLUTION_NAME.exec(name);
  if (match) return {
    kind: "resolution", sourceCommit: null, primaryRunId: match[1],
    primaryRunAttempt: match[2], recoveryRunId: match[3], recoveryRunAttempt: match[4],
  };
  match = RESULT_NAME.exec(name);
  if (match) return {
    kind: "result", sourceCommit: match[1], primaryRunId: match[2],
    primaryRunAttempt: match[3], recoveryRunId: match[4], recoveryRunAttempt: match[5],
  };
  return null;
}

function validateRecoveryArtifact(artifact, parsed, recovery, repository) {
  const raw = artifact.raw;
  if (
    raw.expired !== false ||
    !Number.isSafeInteger(raw.size_in_bytes) || raw.size_in_bytes < 1 ||
    requireDecimal(raw.workflow_run?.id, "artifact workflow run ID") !== parsed.recoveryRunId ||
    raw.workflow_run?.repository_id !== REPOSITORY_ID ||
    raw.workflow_run?.head_repository_id !== REPOSITORY_ID ||
    raw.workflow_run?.head_sha !== recovery.sourceCommit ||
    raw.workflow_run?.head_branch !== "main" ||
    artifact.createdAt < recovery.startedAt || artifact.createdAt > recovery.updatedAt
  ) throw new Error("Discord recovery artifact differs from its exact successful run attempt");
  if (recovery.runId !== parsed.recoveryRunId || recovery.attempt !== parsed.recoveryRunAttempt) {
    throw new Error("Discord recovery artifact name differs from its run authority");
  }
  if (raw.workflow_run?.repository_id !== REPOSITORY_ID || repository.length === 0) {
    throw new Error("Discord recovery artifact repository authority is invalid");
  }
}

function compareAttempts(left, right) {
  const byRun = compareIds(left.runId, right.runId);
  return byRun !== 0 ? byRun : compareIds(left.attempt, right.attempt);
}

function compareIds(left, right) {
  const a = BigInt(left);
  const b = BigInt(right);
  return a < b ? -1 : a > b ? 1 : 0;
}

async function readArtifactReport(root, artifact, label) {
  if (!artifact) throw new Error(`${label} artifact is unavailable`);
  return readJsonFile(
    reportPath(root, artifact.artifact_id, artifact.report_name),
    label,
  );
}

function reportPath(root, artifactId, leaf) {
  const name = String(leaf ?? "");
  if (!["recovery-authority.json", "recovery-result.json"].includes(name)) {
    throw new Error("Discord recovery report leaf is invalid");
  }
  return join(
    resolve(String(root ?? "")),
    requireDecimal(artifactId, "artifact ID"),
    name,
  );
}

async function readJsonFile(path, label) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  let metadata;
  try {
    metadata = await lstat(target);
  } catch {
    throw new Error(`${label} must be a regular file`);
  }
  if (!metadata.isFile() || metadata.isSymbolicLink()) throw new Error(`${label} must be a regular file`);
  try {
    return JSON.parse(await readFile(target, "utf8"));
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
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
      throw new Error("Discord recovery-debt path uses a link or non-directory");
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
  const text = typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function requireDecimal(value, label) {
  const text = typeof value === "number" && Number.isSafeInteger(value)
    ? String(value)
    : typeof value === "string" ? value : "";
  if (!DECIMAL.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function requireTimestamp(value, label) {
  const text = typeof value === "string" ? value : "";
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/u.test(text)) {
    throw new Error(`${label} is invalid`);
  }
  const milliseconds = Date.parse(text);
  if (!Number.isFinite(milliseconds)) throw new Error(`${label} is invalid`);
  return milliseconds;
}

function requireTimestampText(value, label) {
  requireTimestamp(value, label);
  return value;
}

function gitOutput(arguments_) {
  const result = spawnSync("git", arguments_, {
    encoding: "utf8",
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) {
    throw new Error("Discord recovery-debt git authority query failed");
  }
  return result.stdout.trim();
}

function buildBootstrapProof(sourceCommit) {
  const ancestry = spawnSync("git", [
    "merge-base", "--is-ancestor", BOOTSTRAP_COMMIT, sourceCommit,
  ], { shell: false, stdio: "ignore" });
  if (ancestry.error || ancestry.status !== 0) {
    throw new Error("Discord source does not contain the code-bound bootstrap commit");
  }
  const committedAt = new Date(Number(gitOutput([
    "show", "-s", "--format=%ct", BOOTSTRAP_COMMIT,
  ])) * 1000).toISOString().replace(".000Z", "Z");
  const workflow = spawnSync("git", [
    "cat-file", "-e", `${BOOTSTRAP_COMMIT}:${PRIMARY_PATH}`,
  ], { shell: false, stdio: "ignore" });
  if (workflow.error || ![0, 128].includes(workflow.status)) {
    throw new Error("Discord bootstrap workflow-absence query failed");
  }
  return Object.freeze({
    bootstrap_commit: BOOTSTRAP_COMMIT,
    bootstrap_committed_at: committedAt,
    current_source_contains_bootstrap: true,
    discord_deploy_workflow_absent: workflow.status !== 0,
  });
}

function parseCli() {
  return parseArgs({
    options: {
      repository: { type: "string" },
      "workflow-run-id": { type: "string" },
      "workflow-run-attempt": { type: "string" },
      "source-commit": { type: "string" },
      "run-list": { type: "string" },
      "primary-attempts": { type: "string" },
      "recovery-attempts": { type: "string" },
      "recovery-job-catalog": { type: "string" },
      "artifact-list": { type: "string" },
      "tag-authority": { type: "string" },
      plan: { type: "string" },
      "report-root": { type: "string" },
      output: { type: "string" },
    },
    strict: true,
    allowPositionals: true,
  });
}

async function main() {
  const { values, positionals } = parseCli();
  try {
    if (positionals.length !== 1) throw new Error("one recovery-debt operation is required");
    const identity = {
      repository: values.repository,
      workflowRunId: values["workflow-run-id"],
      workflowRunAttempt: values["workflow-run-attempt"],
      sourceCommit: values["source-commit"],
    };
    if (positionals[0] === "plan") {
      const sourceCommit = requirePattern(values["source-commit"], SHA, "source commit");
      const plan = planDiscordRecoveryDebt(
        await readJsonFile(values["run-list"], "primary run catalog"),
        await readJsonFile(values["primary-attempts"], "primary attempt catalog"),
        await readJsonFile(values["recovery-attempts"], "recovery attempt catalog"),
        await readJsonFile(values["artifact-list"], "artifact catalog"),
        {
          ...identity,
          tagAuthority: await readJsonFile(
            values["tag-authority"],
            "production tag authority catalog",
          ),
          recoveryJobCatalog: await readJsonFile(
            values["recovery-job-catalog"],
            "recovery job catalog",
          ),
          reachableTags: gitOutput([
            "tag", "--merged", sourceCommit, "--list", "v[0-9]*.[0-9]*.[0-9]*",
          ]).split(/\r?\n/u).filter(Boolean),
          bootstrapProof: buildBootstrapProof(sourceCommit),
        },
      );
      await writeCanonicalNew(values.output, plan);
      for (const artifact of plan.downloads) {
        process.stdout.write(
          `${artifact.artifact_id}\t${artifact.artifact_digest}\t${artifact.report_name}\n`,
        );
      }
    } else if (positionals[0] === "audit") {
      const clearance = await auditDiscordRecoveryDebt(values.plan, values["report-root"], identity);
      await writeCanonicalNew(values.output, clearance);
      process.stdout.write(`discord_recovery_debt=clear count=${clearance.cleared_debts.length}\n`);
    } else {
      throw new Error("recovery-debt operation must be plan or audit");
    }
  } catch (error) {
    process.stderr.write(
      `discord_recovery_debt=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();

#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  canonicalSha256,
  requireExactKeys,
  requireNonEmptyString,
  requirePlainObject,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  validateCanonicalAcceptanceEvidence,
} from "./canonical-acceptance-evidence.mjs";
import {
  extractClosedCanonicalJsonArtifactZip,
} from "./release-publication-evidence.mjs";
import {
  DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_ARTIFACT_PREFIX,
  DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE,
  checkpointCandidateArtifactName,
  validateDiscordProductionCheckpointCandidate,
} from "./discord-production-checkpoint-receipt.mjs";
import {
  DISCORD_CHECKPOINT_CANDIDATE_UPLOAD_STEP,
  DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES,
  DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_STEPS,
  createDiscordSuccessfulDeploymentTopologyContract,
  validateSuccessfulDiscordDeploymentAuthority,
} from "./discord-deployment-recovery.mjs";

export const DISCORD_PRODUCTION_CHECKPOINT_RECEIPT_SCHEMA_ID =
  "clearra.discord-production-checkpoint-receipt.v1";

const RELEASE = "v0.8.0";
const VERSION = "0.8.0";
const REPOSITORY_ID = "1309293231";
const DISCORD_WORKFLOW_PATH = ".github/workflows/discord-deploy.yml";
const ACCEPTANCE_WORKFLOW_PATH = ".github/workflows/release-cli.yml";
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const ARTIFACT_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const SAFE_REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const SAFE_TAGGER_EMAIL = /^[^<>\u0000-\u001f\u007f]+$/u;
const SECOND_TIMESTAMP =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/u;
const MAXIMUM_GH_API_JSON_BYTES = 8 * 1024 * 1024;
const MAXIMUM_CANDIDATE_ARCHIVE_BYTES = 2 * 1024 * 1024;
const GITHUB_ACTIONS_BOT_ID = "41898282";
const DISCORD_PRODUCTION_OBSERVATION_STEP =
  "Authority-bound global sync and sole canonical four-surface observation";
const DISCORD_OBSERVATION_EVIDENCE_UPLOAD_STEP =
  "Upload durable sync and sole canonical observation evidence";

export async function materializeDiscordProductionCheckpointReceipt(
  options,
  dependencies = {},
) {
  const identity = validateReceiptAuthority(options);
  const apiGet = dependencies.apiGet ?? createGithubApiGet(identity.repository);
  const downloadArtifact = dependencies.downloadArtifact ??
    createGithubArtifactDownload(identity.repository);
  const [run, artifact, jobs, artifactCatalog] = await Promise.all([
    apiGet(
      `/actions/runs/${identity.discordWorkflowRunId}/attempts/${identity.discordWorkflowRunAttempt}`,
      "Discord checkpoint workflow attempt",
    ),
    apiGet(
      `/actions/artifacts/${identity.artifactId}`,
      "Discord checkpoint candidate artifact",
    ),
    apiGet(
      `/actions/runs/${identity.discordWorkflowRunId}/attempts/${identity.discordWorkflowRunAttempt}/jobs?filter=all&per_page=100&page=1`,
      "Discord checkpoint completed job topology",
    ),
    readCompleteRunArtifactCatalog(
      apiGet,
      identity.discordWorkflowRunId,
    ),
  ]);
  const runAuthority = validateCompletedDiscordRun(run, identity);
  validateSuccessfulDiscordDeploymentAuthority(jobs, {
    repository: identity.repository,
    sourceCommit: identity.sourceCommit,
    workflowRunId: identity.discordWorkflowRunId,
    workflowRunAttempt: identity.discordWorkflowRunAttempt,
  });
  const jobTopology = materializeCompletedJobTopology(jobs, runAuthority);
  const artifactAuthority = validateCandidateArtifact(artifact, identity, runAuthority);
  validateCandidateArtifactUploadWindow(
    jobTopology,
    artifactAuthority.createdAt,
    runAuthority,
  );
  validateExactCandidateArtifactCatalog(
    artifactCatalog,
    artifact,
    identity,
  );
  const archive = await downloadArtifact(
    artifactAuthority.archiveDownloadUrl,
    "Discord checkpoint candidate artifact",
  );
  const extracted = extractClosedCanonicalJsonArtifactZip(
    archive,
    artifactAuthority.digest,
    [DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE],
    "Discord production checkpoint candidate",
  );
  const entry = extracted.entries.get(
    DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE,
  );
  const candidate = validateDiscordProductionCheckpointCandidate(entry.value, {
    repository: identity.repository,
    sourceCommit: identity.sourceCommit,
    acceptedRunId: identity.acceptedWorkflowRunId,
    acceptedRunAttempt: identity.acceptedWorkflowRunAttempt,
    workflowRunId: identity.discordWorkflowRunId,
    workflowRunAttempt: identity.discordWorkflowRunAttempt,
  });
  validateProductionObservationJobWindow(
    jobTopology,
    candidate.production_observation,
  );
  validateCheckpointChronology({
    taggerAt: parseSecondTimestamp(identity.tagger.date, "checkpoint tagger date"),
    discordCompletedAt: runAuthority.completedAt,
    artifactCreatedAt: artifactAuthority.createdAt,
    observationEndedAt: parseApiTimestamp(
      candidate.production_observation?.ended_at,
      "checkpoint observation end time",
    ),
  });
  const receipt = sealCanonicalReport({
    schema_id: DISCORD_PRODUCTION_CHECKPOINT_RECEIPT_SCHEMA_ID,
    repository: identity.repository,
    repository_id: REPOSITORY_ID,
    release: RELEASE,
    version: VERSION,
    source_commit: identity.sourceCommit,
    accepted_workflow_path: ACCEPTANCE_WORKFLOW_PATH,
    accepted_workflow_run_id: identity.acceptedWorkflowRunId,
    accepted_workflow_run_attempt: identity.acceptedWorkflowRunAttempt,
    discord_workflow_path: DISCORD_WORKFLOW_PATH,
    discord_workflow_run_id: identity.discordWorkflowRunId,
    discord_workflow_run_attempt: identity.discordWorkflowRunAttempt,
    discord_workflow_started_at: runAuthority.startedAtText,
    discord_workflow_completed_at: runAuthority.completedAtText,
    checkpoint_candidate_artifact: {
      artifact_id: artifactAuthority.id,
      artifact_name: artifactAuthority.name,
      artifact_digest: artifactAuthority.digest,
      artifact_created_at: artifactAuthority.createdAtText,
      archive_sha256: extracted.archiveSha256,
      file_name: DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE,
      file_sha256: entry.fileSha256,
      candidate_report_sha256: candidate.report_sha256,
    },
    checkpoint_candidate: candidate,
    completed_job_topology: jobTopology,
    completed_job_topology_sha256: canonicalSha256(jobTopology),
    tag: {
      name: RELEASE,
      target_commit: identity.sourceCommit,
      annotated: true,
      message_contract: "exact-canonical-receipt-bytes",
      tagger: identity.tagger,
    },
    github_release_contract: {
      tag: RELEASE,
      title: `Clearra ${RELEASE}`,
      source_commit: identity.sourceCommit,
      draft: false,
      prerelease: false,
      immutable: true,
      asset_count: 3,
      canonical_acceptance_evidence_sha256:
        candidate.canonical_acceptance_evidence_sha256,
    },
    status: "ready-for-annotated-tag-and-immutable-release",
  });
  validateDiscordProductionCheckpointReceipt(receipt, identity);
  const bytes = Buffer.from(`${canonicalJson(receipt)}\n`, "utf8");
  return Object.freeze({
    receipt,
    bytes,
    receiptFileSha256: parsedReceiptFileSha256(bytes),
    runAuthority,
    artifactAuthority,
  });
}

export function validateDiscordProductionCheckpointReceipt(value, expected = {}) {
  requireExactKeys(value, [
    "schema_id", "repository", "repository_id", "release", "version",
    "source_commit", "accepted_workflow_path", "accepted_workflow_run_id",
    "accepted_workflow_run_attempt", "discord_workflow_path",
    "discord_workflow_run_id", "discord_workflow_run_attempt",
    "discord_workflow_started_at", "discord_workflow_completed_at",
    "checkpoint_candidate_artifact",
    "checkpoint_candidate", "completed_job_topology",
    "completed_job_topology_sha256", "tag",
    "github_release_contract", "status",
    "report_sha256",
  ], "Discord production checkpoint receipt");
  if (value.schema_id !== DISCORD_PRODUCTION_CHECKPOINT_RECEIPT_SCHEMA_ID) {
    throw new Error("Discord production checkpoint receipt schema is invalid");
  }
  verifyCanonicalReportHash(value, "Discord production checkpoint receipt");
  const identity = validateReceiptAuthority({
    repository: expected.repository ?? value.repository,
    sourceCommit: expected.sourceCommit ?? value.source_commit,
    acceptedWorkflowRunId:
      expected.acceptedWorkflowRunId ?? value.accepted_workflow_run_id,
    acceptedWorkflowRunAttempt:
      expected.acceptedWorkflowRunAttempt ?? value.accepted_workflow_run_attempt,
    discordWorkflowRunId:
      expected.discordWorkflowRunId ?? value.discord_workflow_run_id,
    discordWorkflowRunAttempt:
      expected.discordWorkflowRunAttempt ?? value.discord_workflow_run_attempt,
    artifactId:
      expected.artifactId ?? value.checkpoint_candidate_artifact?.artifact_id,
    artifactDigest:
      expected.artifactDigest ?? value.checkpoint_candidate_artifact?.artifact_digest,
    tagger: expected.tagger ?? value.tag?.tagger,
  });
  if (
    value.repository !== identity.repository ||
    value.repository_id !== REPOSITORY_ID ||
    value.release !== RELEASE || value.version !== VERSION ||
    value.source_commit !== identity.sourceCommit ||
    value.accepted_workflow_path !== ACCEPTANCE_WORKFLOW_PATH ||
    value.accepted_workflow_run_id !== identity.acceptedWorkflowRunId ||
    value.accepted_workflow_run_attempt !== identity.acceptedWorkflowRunAttempt ||
    value.discord_workflow_path !== DISCORD_WORKFLOW_PATH ||
    value.discord_workflow_run_id !== identity.discordWorkflowRunId ||
    value.discord_workflow_run_attempt !== identity.discordWorkflowRunAttempt ||
    value.status !== "ready-for-annotated-tag-and-immutable-release"
  ) {
    throw new Error("Discord production checkpoint receipt authority differs");
  }
  const workflowStartedAt = parseGithubTimestamp(
    value.discord_workflow_started_at,
    "Discord workflow start time",
  );
  const workflowCompletedAt = parseGithubTimestamp(
    value.discord_workflow_completed_at,
    "Discord workflow completion time",
  );
  if (workflowStartedAt > workflowCompletedAt) {
    throw new Error("Discord checkpoint workflow completion predates its start");
  }
  validateCandidateArtifactReceipt(
    value.checkpoint_candidate_artifact,
    identity,
    value.checkpoint_candidate,
  );
  const candidate = validateDiscordProductionCheckpointCandidate(
    value.checkpoint_candidate,
    {
      repository: identity.repository,
      sourceCommit: identity.sourceCommit,
      acceptedRunId: identity.acceptedWorkflowRunId,
      acceptedRunAttempt: identity.acceptedWorkflowRunAttempt,
      workflowRunId: identity.discordWorkflowRunId,
      workflowRunAttempt: identity.discordWorkflowRunAttempt,
    },
  );
  validateCompletedJobTopologyReceipt(
    value.completed_job_topology,
    value.completed_job_topology_sha256,
    candidate.deployment_topology_contract,
    workflowStartedAt,
    workflowCompletedAt,
  );
  validateProductionObservationJobWindow(
    value.completed_job_topology,
    candidate.production_observation,
  );
  validateCandidateArtifactUploadWindow(
    value.completed_job_topology,
    parseGithubTimestamp(
      value.checkpoint_candidate_artifact.artifact_created_at,
      "candidate artifact creation time",
    ),
    {
      completedAt: parseGithubTimestamp(
        value.discord_workflow_completed_at,
        "Discord workflow completion time",
      ),
    },
  );
  validateCheckpointChronology({
    taggerAt: parseSecondTimestamp(value.tag.tagger.date, "checkpoint tagger date"),
    discordCompletedAt: parseGithubTimestamp(
      value.discord_workflow_completed_at,
      "Discord workflow completion time",
    ),
    artifactCreatedAt: parseGithubTimestamp(
      value.checkpoint_candidate_artifact.artifact_created_at,
      "candidate artifact creation time",
    ),
    observationEndedAt: parseApiTimestamp(
      candidate.production_observation?.ended_at,
      "checkpoint observation end time",
    ),
  });
  validateTagContract(value.tag, identity);
  validateReleaseContract(value.github_release_contract, identity, candidate);
  return value;
}

export async function verifyRemoteDiscordProductionCheckpointTag(
  options,
  dependencies = {},
) {
  const repository = requirePattern(options?.repository, SAFE_REPOSITORY, "repository");
  const sourceCommit = requireSourceCommit(options?.sourceCommit);
  const tag = requireReleaseTag(options?.tag);
  const apiGet = dependencies.apiGet ?? createGithubApiGet(repository);
  const encodedTag = encodeURIComponent(tag);
  const tagRef = await apiGet(`/git/ref/tags/${encodedTag}`, "checkpoint tag ref");
  const tagObjectSha = validateRemoteTagRef(tagRef, tag);
  const tagObject = await apiGet(
    `/git/tags/${tagObjectSha}`,
    "checkpoint annotated tag object",
  );
  const message = validateRemoteTagObject(tagObject, {
    tag,
    sourceCommit,
    tagObjectSha,
  });
  const parsed = parseCanonicalReceiptBytes(Buffer.from(message, "utf8"));
  validateDiscordProductionCheckpointReceipt(parsed.value, {
    repository,
    sourceCommit,
    acceptedWorkflowRunId: options.acceptedWorkflowRunId,
    acceptedWorkflowRunAttempt: options.acceptedWorkflowRunAttempt,
    tagger: tagObject.tagger,
  });
  const rematerialized = await materializeDiscordProductionCheckpointReceipt({
    repository,
    sourceCommit,
    acceptedWorkflowRunId: parsed.value.accepted_workflow_run_id,
    acceptedWorkflowRunAttempt: parsed.value.accepted_workflow_run_attempt,
    discordWorkflowRunId: parsed.value.discord_workflow_run_id,
    discordWorkflowRunAttempt: parsed.value.discord_workflow_run_attempt,
    artifactId: parsed.value.checkpoint_candidate_artifact.artifact_id,
    artifactDigest: parsed.value.checkpoint_candidate_artifact.artifact_digest,
    tagger: parsed.value.tag.tagger,
  }, {
    apiGet,
    downloadArtifact: dependencies.downloadArtifact,
  });
  if (!rematerialized.bytes.equals(parsed.bytes)) {
    throw new Error("remote annotated tag message differs from recomputed checkpoint receipt");
  }
  if (options.acceptanceEvidencePath !== undefined) {
    await verifyReceiptAgainstAcceptanceEvidence(
      parsed.value,
      options.acceptanceEvidencePath,
    );
  }
  return Object.freeze({
    tag,
    tagObjectSha,
    sourceCommit,
    receipt: parsed.value,
    receiptFileSha256: parsed.fileSha256,
  });
}

export async function verifyImmutableDiscordCheckpointRelease(
  options,
  dependencies = {},
) {
  const verifiedTag = await verifyRemoteDiscordProductionCheckpointTag(
    options,
    dependencies,
  );
  if (options.acceptanceEvidencePath === undefined) {
    throw new Error("immutable checkpoint Release verification requires acceptance evidence");
  }
  const accepted = await verifyReceiptAgainstAcceptanceEvidence(
    verifiedTag.receipt,
    options.acceptanceEvidencePath,
  );
  const apiGet = dependencies.apiGet ?? createGithubApiGet(verifiedTag.receipt.repository);
  const release = await apiGet(
    `/releases/tags/${encodeURIComponent(verifiedTag.tag)}`,
    "immutable checkpoint GitHub Release",
  );
  validateImmutableCheckpointReleaseReadback(release, {
    repository: verifiedTag.receipt.repository,
    sourceCommit: verifiedTag.sourceCommit,
    tag: verifiedTag.tag,
    taggerAt: verifiedTag.receipt.tag.tagger.date,
    acceptedArtifacts: accepted.evidence.final_source_fragments.release_artifacts,
  });
  return Object.freeze({
    tag: verifiedTag.tag,
    sourceCommit: verifiedTag.sourceCommit,
    releaseId: String(release.id),
    receiptSha256: verifiedTag.receipt.report_sha256,
  });
}

export async function finalizeDiscordProductionCheckpointTag(
  options,
  dependencies = {},
) {
  const repository = requirePattern(options?.repository, SAFE_REPOSITORY, "repository");
  const sourceCommit = requireSourceCommit(options?.sourceCommit);
  const tag = requireReleaseTag(options?.tag);
  const runGit = dependencies.runGit ?? createGitRunner(options?.cwd);
  requireCleanExactMainCheckout(runGit, { repository, sourceCommit, tag });
  const tagger = Object.freeze({
    name: validateTaggerName(runGit.text(["config", "--get", "user.name"])),
    email: validateTaggerEmail(runGit.text(["config", "--get", "user.email"])),
    date: dependencies.taggerDate ?? currentSecondTimestamp(),
  });
  validateTagger(tagger);
  const materialized = await materializeDiscordProductionCheckpointReceipt({
    ...options,
    repository,
    sourceCommit,
    tagger,
  }, dependencies);
  assertRemoteMainAndAbsentTag(runGit, sourceCommit, tag);
  let local = null;
  let createdTagObjectSha = null;
  let pushed = false;
  try {
    runGit.bytes([
      "tag", "-a", "--cleanup=verbatim", "-F", "-", tag, sourceCommit,
    ], {
      input: materialized.bytes,
      env: {
        GIT_COMMITTER_NAME: tagger.name,
        GIT_COMMITTER_EMAIL: tagger.email,
        GIT_COMMITTER_DATE: tagger.date,
      },
    });
    createdTagObjectSha = requireSourceCommit(
      runGit.text(["rev-parse", `refs/tags/${tag}`]).trim(),
      "created tag object SHA",
    );
    local = readLocalTag(runGit, tag, materialized.bytes, {
      sourceCommit,
      tagger,
    });
    assertRemoteMainAndAbsentTag(runGit, sourceCommit, tag);
    runGit.bytes([
      "push", "origin", `refs/tags/${tag}:refs/tags/${tag}`,
    ]);
    pushed = true;
    const remote = validateRemoteLsRemote(
      runGit.text([
        "ls-remote", "origin", `refs/tags/${tag}`, `refs/tags/${tag}^{}`,
      ]),
      tag,
      sourceCommit,
    );
    if (remote.tagObjectSha !== local.tagObjectSha) {
      throw new Error("remote checkpoint tag object differs from the locally verified object");
    }
    const verified = await verifyRemoteDiscordProductionCheckpointTag({
      repository,
      sourceCommit,
      tag,
      acceptedWorkflowRunId: materialized.receipt.accepted_workflow_run_id,
      acceptedWorkflowRunAttempt: materialized.receipt.accepted_workflow_run_attempt,
    }, dependencies);
    if (
      verified.tagObjectSha !== local.tagObjectSha ||
      verified.receiptFileSha256 !== materialized.receiptFileSha256
    ) {
      throw new Error("remote checkpoint tag readback differs from its local tag authority");
    }
    return Object.freeze({
      tag,
      sourceCommit,
      tagObjectSha: local.tagObjectSha,
      receiptSha256: materialized.receipt.report_sha256,
      receiptFileSha256: materialized.receiptFileSha256,
    });
  } catch (error) {
    if (createdTagObjectSha !== null && !pushed) {
      const removed = runGit.optional([
        "update-ref", "-d", `refs/tags/${tag}`, createdTagObjectSha,
      ]);
      if (removed.status !== 0) {
        throw new AggregateError(
          [error, new Error("created local checkpoint tag could not be cleaned safely")],
          "checkpoint tag finalization failed before push",
        );
      }
    }
    throw error;
  }
}

export async function verifyReceiptAgainstAcceptanceEvidence(receipt, path) {
  validateDiscordProductionCheckpointReceipt(receipt);
  const input = await readCanonicalJsonFile(path, "canonical acceptance evidence");
  const evidence = input.value;
  validateCanonicalAcceptanceEvidence(evidence, {
    repository: receipt.repository,
    version: VERSION,
    basePath: evidence.pages_base_path,
    sourceCommit: receipt.source_commit,
    runId: receipt.accepted_workflow_run_id,
    runAttempt: receipt.accepted_workflow_run_attempt,
  });
  const candidate = receipt.checkpoint_candidate;
  validateDiscordProductionCheckpointCandidate(candidate, {
    repository: receipt.repository,
    sourceCommit: receipt.source_commit,
    acceptedRunId: receipt.accepted_workflow_run_id,
    acceptedRunAttempt: receipt.accepted_workflow_run_attempt,
    workflowRunId: receipt.discord_workflow_run_id,
    workflowRunAttempt: receipt.discord_workflow_run_attempt,
    canonicalAcceptanceEvidenceSha256: evidence.report_sha256,
    canonicalAcceptanceEvidenceFileSha256: input.fileSha256,
    releaseArtifacts: evidence.final_source_fragments.release_artifacts,
  });
  validateAcceptedReleaseArtifactBinding(
    candidate.release_artifacts,
    evidence.final_source_fragments.release_artifacts,
  );
  return Object.freeze({ evidence, fileSha256: input.fileSha256 });
}

export function validateAcceptedReleaseArtifactBinding(
  candidateArtifacts,
  acceptedArtifacts,
) {
  if (canonicalJson(candidateArtifacts) !== canonicalJson(acceptedArtifacts)) {
    throw new Error("checkpoint candidate release artifacts differ from canonical acceptance");
  }
  return candidateArtifacts;
}

function validateReceiptAuthority(options) {
  return Object.freeze({
    repository: requirePattern(options?.repository, SAFE_REPOSITORY, "repository"),
    sourceCommit: requireSourceCommit(options?.sourceCommit),
    acceptedWorkflowRunId: requireDecimal(
      options?.acceptedWorkflowRunId,
      "accepted workflow run ID",
    ),
    acceptedWorkflowRunAttempt: requireDecimal(
      options?.acceptedWorkflowRunAttempt,
      "accepted workflow run attempt",
    ),
    discordWorkflowRunId: requireDecimal(
      options?.discordWorkflowRunId,
      "Discord workflow run ID",
    ),
    discordWorkflowRunAttempt: requireDecimal(
      options?.discordWorkflowRunAttempt,
      "Discord workflow run attempt",
    ),
    artifactId: requireDecimal(options?.artifactId, "candidate artifact ID"),
    artifactDigest: requirePattern(
      options?.artifactDigest,
      ARTIFACT_DIGEST,
      "candidate artifact digest",
    ),
    tagger: validateTagger(options?.tagger),
  });
}

export function validateCompletedDiscordRun(run, identity) {
  requirePlainObject(run, "Discord checkpoint workflow attempt");
  if (
    String(run.id ?? "") !== identity.discordWorkflowRunId ||
    String(run.run_attempt ?? "") !== identity.discordWorkflowRunAttempt ||
    run.name !== "Deploy Discord Production" ||
    run.path !== DISCORD_WORKFLOW_PATH || run.event !== "workflow_run" ||
    run.head_branch !== "main" || run.head_sha !== identity.sourceCommit ||
    run.status !== "completed" || run.conclusion !== "success" ||
    String(run.repository?.id ?? "") !== REPOSITORY_ID ||
    run.repository?.full_name !== identity.repository ||
    String(run.head_repository?.id ?? "") !== REPOSITORY_ID ||
    run.head_repository?.full_name !== identity.repository
  ) {
    throw new Error("Discord checkpoint workflow attempt is not exact completed success");
  }
  const startedAt = parseGithubTimestamp(
    run.run_started_at,
    "Discord workflow start time",
  );
  const completedAt = parseGithubTimestamp(
    run.updated_at,
    "Discord workflow completion time",
  );
  if (completedAt < startedAt) {
    throw new Error("Discord workflow completion predates its start");
  }
  return Object.freeze({
    startedAt,
    completedAt,
    startedAtText: run.run_started_at,
    completedAtText: run.updated_at,
  });
}

function validateCandidateArtifact(artifact, identity, runAuthority) {
  requirePlainObject(artifact, "Discord checkpoint candidate artifact");
  const expectedName = checkpointCandidateArtifactName(
    identity.sourceCommit,
    identity.discordWorkflowRunId,
    identity.discordWorkflowRunAttempt,
  );
  const createdAt = parseGithubTimestamp(
    artifact.created_at,
    "candidate artifact creation time",
  );
  if (
    String(artifact.id ?? "") !== identity.artifactId ||
    artifact.name !== expectedName || artifact.expired !== false ||
    artifact.digest !== identity.artifactDigest ||
    !Number.isSafeInteger(artifact.size_in_bytes) || artifact.size_in_bytes < 22 ||
    String(artifact.workflow_run?.id ?? "") !== identity.discordWorkflowRunId ||
    String(artifact.workflow_run?.repository_id ?? "") !== REPOSITORY_ID ||
    String(artifact.workflow_run?.head_repository_id ?? "") !== REPOSITORY_ID ||
    artifact.workflow_run?.head_branch !== "main" ||
    artifact.workflow_run?.head_sha !== identity.sourceCommit ||
    createdAt < runAuthority.startedAt || createdAt > runAuthority.completedAt
  ) {
    throw new Error("Discord checkpoint candidate artifact differs from its exact run");
  }
  const url = validateArtifactDownloadUrl(
    artifact.archive_download_url,
    identity.repository,
    identity.artifactId,
  );
  return Object.freeze({
    id: identity.artifactId,
    name: expectedName,
    digest: identity.artifactDigest,
    createdAt,
    createdAtText: artifact.created_at,
    archiveDownloadUrl: url,
  });
}

async function readCompleteRunArtifactCatalog(apiGet, workflowRunId) {
  const pages = [];
  let totalCount = null;
  for (let page = 1; ; page += 1) {
    const value = await apiGet(
      `/actions/runs/${workflowRunId}/artifacts?per_page=100&page=${page}`,
      `Discord checkpoint artifact catalog page ${page}`,
    );
    requirePlainObject(value, "Discord checkpoint artifact catalog page");
    if (
      !Number.isSafeInteger(value.total_count) || value.total_count < 0 ||
      !Array.isArray(value.artifacts) || value.artifacts.length > 100
    ) {
      throw new Error("Discord checkpoint artifact catalog page is invalid");
    }
    if (totalCount === null) totalCount = value.total_count;
    if (value.total_count !== totalCount) {
      throw new Error("Discord checkpoint artifact catalog totals changed during pagination");
    }
    pages.push(...value.artifacts);
    if (pages.length >= totalCount) break;
    if (value.artifacts.length !== 100 || page >= 1000) {
      throw new Error("Discord checkpoint artifact catalog is incomplete");
    }
  }
  if (pages.length !== totalCount) {
    throw new Error("Discord checkpoint artifact catalog is truncated or over-complete");
  }
  const ids = new Set();
  for (const artifact of pages) {
    const id = requireDecimal(artifact?.id, "catalog artifact ID");
    if (ids.has(id)) throw new Error("Discord checkpoint artifact catalog has duplicate IDs");
    ids.add(id);
  }
  return Object.freeze(pages);
}

export function validateExactCandidateArtifactCatalog(catalog, selected, identity) {
  const expectedName = checkpointCandidateArtifactName(
    identity.sourceCommit,
    identity.discordWorkflowRunId,
    identity.discordWorkflowRunAttempt,
  );
  const checkpointArtifacts = catalog.filter((artifact) =>
    typeof artifact?.name === "string" &&
    artifact.name.startsWith(`${DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_ARTIFACT_PREFIX}-`));
  const matches = checkpointArtifacts.filter((artifact) => artifact.name === expectedName);
  if (checkpointArtifacts.length !== 1 || matches.length !== 1) {
    throw new Error("Discord checkpoint candidate artifact is missing, duplicate, or foreign");
  }
  const listed = matches[0];
  if (
    String(listed.id ?? "") !== identity.artifactId ||
    listed.digest !== identity.artifactDigest || listed.expired !== false ||
    listed.created_at !== selected.created_at ||
    listed.size_in_bytes !== selected.size_in_bytes ||
    listed.archive_download_url !== selected.archive_download_url ||
    String(listed.workflow_run?.id ?? "") !== identity.discordWorkflowRunId ||
    listed.workflow_run?.head_sha !== identity.sourceCommit
  ) {
    throw new Error("selected candidate artifact differs from the exact-one run catalog");
  }
}

export function materializeCompletedJobTopology(value, runAuthority) {
  requirePlainObject(value, "Discord checkpoint completed job topology response");
  if (
    value.total_count !== DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES.length ||
    !Array.isArray(value.jobs) || value.jobs.length !== value.total_count ||
    !Number.isFinite(runAuthority?.completedAt)
  ) {
    throw new Error("Discord checkpoint completed job topology is incomplete");
  }
  const contract = createDiscordSuccessfulDeploymentTopologyContract();
  const contractByName = new Map(contract.jobs.map((job) => [job.job_name, job]));
  const jobsByName = new Map();
  const jobIds = new Set();
  for (const job of value.jobs) {
    const name = typeof job?.name === "string" ? job.name : "";
    const id = requireDecimal(job?.id, "completed Discord job ID");
    if (jobsByName.has(name) || jobIds.has(id)) {
      throw new Error("Discord checkpoint completed job topology is ambiguous");
    }
    jobsByName.set(name, job);
    jobIds.add(id);
  }
  return Object.freeze(DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES.map((jobName) => {
    const job = jobsByName.get(jobName);
    const expected = contractByName.get(jobName);
    if (!job || !expected || !Array.isArray(job.steps)) {
      throw new Error("Discord checkpoint completed job topology has a foreign job set");
    }
    const startedAt = parseGithubTimestamp(job.started_at, `${jobName} job started-at`);
    const completedAt = parseGithubTimestamp(job.completed_at, `${jobName} job completed-at`);
    if (
      job.status !== "completed" || job.conclusion !== "success" ||
      startedAt > completedAt || completedAt > runAuthority.completedAt
    ) {
      throw new Error("Discord checkpoint completed job authority is invalid");
    }
    const steps = [...job.steps].sort((left, right) => left.number - right.number);
    if (
      steps.length !== expected.steps.length ||
      steps.length !== DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_STEPS[jobName].length
    ) {
      throw new Error("Discord checkpoint completed step topology is incomplete");
    }
    let priorNumber = 0;
    const sealedSteps = steps.map((step, index) => {
      const expectedStep = expected.steps[index];
      if (
        step?.name !== expectedStep.name || step.status !== "completed" ||
        step.conclusion !== expectedStep.expected_conclusion ||
        !Number.isSafeInteger(step.number) || step.number <= priorNumber
      ) {
        throw new Error("Discord checkpoint completed step topology is foreign");
      }
      priorNumber = step.number;
      let stepStartedAt = null;
      let stepCompletedAt = null;
      if (step.started_at === null || step.completed_at === null) {
        if (
          step.conclusion !== "skipped" ||
          step.started_at !== null || step.completed_at !== null
        ) {
          throw new Error("Discord checkpoint completed step timestamps are incomplete");
        }
      } else {
        stepStartedAt = parseGithubTimestamp(
          step.started_at,
          `${jobName}/${step.name} started-at`,
        );
        stepCompletedAt = parseGithubTimestamp(
          step.completed_at,
          `${jobName}/${step.name} completed-at`,
        );
        if (
          stepStartedAt > stepCompletedAt || stepStartedAt < startedAt ||
          stepCompletedAt > completedAt
        ) {
          throw new Error("Discord checkpoint completed step timestamps leave their job");
        }
      }
      return Object.freeze({
        name: step.name,
        number: step.number,
        status: step.status,
        conclusion: step.conclusion,
        started_at: step.started_at,
        completed_at: step.completed_at,
      });
    });
    return Object.freeze({
      job_id: requireDecimal(job.id, "completed Discord job ID"),
      job_name: jobName,
      status: "completed",
      conclusion: "success",
      started_at: job.started_at,
      completed_at: job.completed_at,
      steps: Object.freeze(sealedSteps),
    });
  }));
}

export function validateCandidateArtifactUploadWindow(
  completedJobTopology,
  artifactCreatedAt,
  runAuthority,
) {
  if (!Array.isArray(completedJobTopology) || !Number.isFinite(artifactCreatedAt)) {
    throw new Error("Discord checkpoint candidate upload authority is invalid");
  }
  const syncJobs = completedJobTopology.filter((job) => job?.job_name === "sync-observe");
  if (syncJobs.length !== 1 || !Array.isArray(syncJobs[0].steps)) {
    throw new Error("Discord checkpoint candidate upload job authority is not exact");
  }
  const findExactStep = (name) => {
    const matches = syncJobs[0].steps.filter((step) => step?.name === name);
    if (
      matches.length !== 1 || matches[0].conclusion !== "success" ||
      !Number.isSafeInteger(matches[0].number) || matches[0].number < 1
    ) {
      throw new Error("Discord checkpoint candidate upload step is not exact success");
    }
    return Object.freeze({
      number: matches[0].number,
      startedAt: parseGithubTimestamp(
        matches[0].started_at,
        `${name} started-at`,
      ),
      completedAt: parseGithubTimestamp(
        matches[0].completed_at,
        `${name} completed-at`,
      ),
    });
  };
  const observationUpload = findExactStep(DISCORD_OBSERVATION_EVIDENCE_UPLOAD_STEP);
  const candidateUpload = findExactStep(DISCORD_CHECKPOINT_CANDIDATE_UPLOAD_STEP);
  const workflowStartedAt = runAuthority?.startedAt;
  const workflowCompletedAt = runAuthority?.completedAt;
  if (
    !Number.isFinite(workflowCompletedAt) ||
    observationUpload.startedAt > observationUpload.completedAt ||
    candidateUpload.startedAt > candidateUpload.completedAt ||
    observationUpload.number >= candidateUpload.number ||
    observationUpload.completedAt > candidateUpload.startedAt ||
    artifactCreatedAt < candidateUpload.startedAt ||
    artifactCreatedAt > candidateUpload.completedAt ||
    candidateUpload.completedAt > workflowCompletedAt ||
    (workflowStartedAt !== undefined &&
      (!Number.isFinite(workflowStartedAt) ||
       observationUpload.startedAt < workflowStartedAt))
  ) {
    throw new Error("Discord checkpoint candidate artifact is outside its exact upload window");
  }
  return Object.freeze({ observationUpload, candidateUpload });
}

export function validateProductionObservationJobWindow(
  completedJobTopology,
  productionObservation,
) {
  if (!Array.isArray(completedJobTopology)) {
    throw new Error("Discord production observation job authority is invalid");
  }
  const syncJobs = completedJobTopology.filter((job) => job?.job_name === "sync-observe");
  if (syncJobs.length !== 1 || !Array.isArray(syncJobs[0].steps)) {
    throw new Error("Discord production observation job authority is not exact");
  }
  const findExactStep = (name) => {
    const matches = syncJobs[0].steps.filter((step) => step?.name === name);
    if (
      matches.length !== 1 || matches[0].conclusion !== "success" ||
      !Number.isSafeInteger(matches[0].number) || matches[0].number < 1
    ) {
      throw new Error("Discord production observation step is not exact success");
    }
    return Object.freeze({
      number: matches[0].number,
      startedAt: parseGithubTimestamp(matches[0].started_at, `${name} started-at`),
      completedAt: parseGithubTimestamp(matches[0].completed_at, `${name} completed-at`),
    });
  };
  const observationStep = findExactStep(DISCORD_PRODUCTION_OBSERVATION_STEP);
  const evidenceUpload = findExactStep(DISCORD_OBSERVATION_EVIDENCE_UPLOAD_STEP);
  const reportStartedAt = parseApiTimestamp(
    productionObservation?.started_at,
    "production observation report started-at",
  );
  const reportEndedAt = parseApiTimestamp(
    productionObservation?.ended_at,
    "production observation report ended-at",
  );
  if (
    observationStep.startedAt > observationStep.completedAt ||
    evidenceUpload.startedAt > evidenceUpload.completedAt ||
    observationStep.startedAt > reportStartedAt ||
    reportEndedAt > observationStep.completedAt ||
    reportEndedAt - reportStartedAt < 1_200_000 ||
    observationStep.number >= evidenceUpload.number ||
    observationStep.completedAt > evidenceUpload.startedAt
  ) {
    throw new Error("production observation report is outside its exact completed job window");
  }
  return Object.freeze({
    observationStep,
    evidenceUpload,
    reportStartedAt,
    reportEndedAt,
  });
}

function validateCandidateArtifactReceipt(value, identity, candidate) {
  requireExactKeys(value, [
    "artifact_id", "artifact_name", "artifact_digest", "artifact_created_at",
    "archive_sha256", "file_name", "file_sha256", "candidate_report_sha256",
  ], "checkpoint candidate artifact receipt");
  if (
    value.artifact_id !== identity.artifactId ||
    value.artifact_name !== checkpointCandidateArtifactName(
      identity.sourceCommit,
      identity.discordWorkflowRunId,
      identity.discordWorkflowRunAttempt,
    ) ||
    value.artifact_digest !== identity.artifactDigest ||
    value.archive_sha256 !== identity.artifactDigest.slice("sha256:".length) ||
    value.file_name !== DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE ||
    value.candidate_report_sha256 !== candidate?.report_sha256
  ) {
    throw new Error("checkpoint candidate artifact receipt authority differs");
  }
  parseGithubTimestamp(value.artifact_created_at, "candidate artifact creation time");
  requireSha256(value.file_sha256, "checkpoint candidate file SHA-256");
  requireSha256(value.candidate_report_sha256, "checkpoint candidate report SHA-256");
}

function validateTagContract(value, identity) {
  requireExactKeys(value, [
    "name", "target_commit", "annotated", "message_contract", "tagger",
  ], "checkpoint annotated tag contract");
  if (
    value.name !== RELEASE || value.target_commit !== identity.sourceCommit ||
    value.annotated !== true ||
    value.message_contract !== "exact-canonical-receipt-bytes"
  ) {
    throw new Error("checkpoint annotated tag contract differs");
  }
  if (canonicalJson(validateTagger(value.tagger)) !== canonicalJson(identity.tagger)) {
    throw new Error("checkpoint tagger differs from its receipt authority");
  }
}

function validateReleaseContract(value, identity, candidate) {
  requireExactKeys(value, [
    "tag", "title", "source_commit", "draft", "prerelease", "immutable",
    "asset_count", "canonical_acceptance_evidence_sha256",
  ], "checkpoint GitHub Release contract");
  if (
    value.tag !== RELEASE || value.title !== `Clearra ${RELEASE}` ||
    value.source_commit !== identity.sourceCommit || value.draft !== false ||
    value.prerelease !== false || value.immutable !== true || value.asset_count !== 3 ||
    value.canonical_acceptance_evidence_sha256 !==
      candidate.canonical_acceptance_evidence_sha256
  ) {
    throw new Error("checkpoint GitHub Release contract differs");
  }
  requireSha256(
    value.canonical_acceptance_evidence_sha256,
    "checkpoint canonical acceptance evidence SHA-256",
  );
}

export function validateImmutableCheckpointReleaseReadback(release, options) {
  requirePlainObject(release, "immutable checkpoint GitHub Release");
  const repository = requirePattern(options?.repository, SAFE_REPOSITORY, "repository");
  const sourceCommit = requireSourceCommit(options?.sourceCommit);
  const tag = requireReleaseTag(options?.tag);
  const releaseId = requireDecimal(release.id, "GitHub Release ID");
  const taggerAt = parseSecondTimestamp(options?.taggerAt, "checkpoint tagger date");
  const publishedAt = parseGithubTimestamp(
    release.published_at,
    "GitHub Release publication time",
  );
  if (
    release.tag_name !== tag || release.target_commitish !== sourceCommit ||
    release.name !== `Clearra ${tag}` || release.draft !== false ||
    release.prerelease !== false || release.immutable !== true ||
    publishedAt <= taggerAt ||
    release.url !== `https://api.github.com/repos/${repository}/releases/${releaseId}` ||
    release.html_url !== `https://github.com/${repository}/releases/tag/${tag}` ||
    release.assets_url !==
      `https://api.github.com/repos/${repository}/releases/${releaseId}/assets` ||
    release.upload_url !==
      `https://uploads.github.com/repos/${repository}/releases/${releaseId}/assets{?name,label}`
  ) {
    throw new Error("immutable checkpoint GitHub Release identity is invalid");
  }
  validateGithubActionsBot(release.author, "GitHub Release author");
  const accepted = options?.acceptedArtifacts;
  if (!Array.isArray(accepted) || accepted.length !== 3 ||
      !Array.isArray(release.assets) || release.assets.length !== 3) {
    throw new Error("immutable checkpoint GitHub Release must contain exactly three assets");
  }
  const expectedByName = new Map(accepted.map((asset) => [asset.name, asset]));
  const names = new Set();
  for (const asset of release.assets) {
    const expected = expectedByName.get(asset?.name);
    if (
      expected === undefined || names.has(asset.name) ||
      !Number.isSafeInteger(asset.id) || asset.id <= 0 ||
      asset.state !== "uploaded" || asset.size !== expected.size_bytes ||
      asset.digest !== `sha256:${expected.sha256}` ||
      asset.url !==
        `https://api.github.com/repos/${repository}/releases/assets/${asset.id}` ||
      asset.browser_download_url !==
        `https://github.com/${repository}/releases/download/${tag}/${asset.name}`
    ) {
      throw new Error("immutable checkpoint GitHub Release asset differs from accepted bytes");
    }
    validateGithubActionsBot(asset.uploader, "GitHub Release asset uploader");
    names.add(asset.name);
  }
  if (names.size !== expectedByName.size) {
    throw new Error("immutable checkpoint GitHub Release asset set is incomplete");
  }
  return release;
}

function validateGithubActionsBot(value, label) {
  requirePlainObject(value, label);
  if (
    value.login !== "github-actions[bot]" || String(value.id ?? "") !== GITHUB_ACTIONS_BOT_ID ||
    value.type !== "Bot" || value.site_admin !== false ||
    value.url !== "https://api.github.com/users/github-actions%5Bbot%5D" ||
    value.html_url !== "https://github.com/apps/github-actions"
  ) {
    throw new Error(`${label} is not the stable GitHub Actions bot authority`);
  }
}

function validateCompletedJobTopologyReceipt(
  value,
  expectedSha256,
  topologyContract,
  workflowStartedAt,
  workflowCompletedAt,
) {
  if (
    !Array.isArray(value) ||
    value.length !== DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES.length ||
    !Array.isArray(topologyContract?.jobs) ||
    topologyContract.jobs.length !== value.length ||
    !Number.isFinite(workflowStartedAt) ||
    !Number.isFinite(workflowCompletedAt)
  ) {
    throw new Error("checkpoint receipt requires the exact four-job topology");
  }
  if (
    canonicalSha256(value) !== requireSha256(
      expectedSha256,
      "completed job topology SHA-256",
    )
  ) {
    throw new Error("completed job topology differs from its receipt SHA-256");
  }
  const jobIds = new Set();
  for (const [jobIndex, job] of value.entries()) {
    requirePlainObject(job, "completed Discord job receipt");
    requireExactKeys(job, [
      "job_id", "job_name", "status", "conclusion", "started_at",
      "completed_at", "steps",
    ], "completed Discord job receipt");
    const expectedJob = topologyContract.jobs[jobIndex];
    const jobId = requireDecimal(job.job_id, "completed Discord job receipt ID");
    const startedAt = parseGithubTimestamp(
      job.started_at,
      `${job.job_name} receipt job started-at`,
    );
    const completedAt = parseGithubTimestamp(
      job.completed_at,
      `${job.job_name} receipt job completed-at`,
    );
    if (
      jobIds.has(jobId) || job.job_name !== expectedJob?.job_name ||
      job.job_name !== DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES[jobIndex] ||
      job.status !== "completed" || job.conclusion !== "success" ||
      startedAt > completedAt || startedAt < workflowStartedAt ||
      completedAt > workflowCompletedAt ||
      !Array.isArray(job.steps) || job.steps.length !== expectedJob.steps?.length
    ) {
      throw new Error(`completed Discord job receipt differs from the topology contract: ${job.job_name}`);
    }
    jobIds.add(jobId);
    let priorNumber = 0;
    for (const [stepIndex, step] of job.steps.entries()) {
      requirePlainObject(step, "completed Discord step receipt");
      requireExactKeys(step, [
        "name", "number", "status", "conclusion", "started_at", "completed_at",
      ], "completed Discord step receipt");
      const expectedStep = expectedJob.steps[stepIndex];
      if (
        step.name !== expectedStep?.name || step.status !== "completed" ||
        step.conclusion !== expectedStep.expected_conclusion ||
        !Number.isSafeInteger(step.number) || step.number <= priorNumber
      ) {
        throw new Error("completed Discord step receipt differs from the topology contract");
      }
      priorNumber = step.number;
      if (step.started_at === null || step.completed_at === null) {
        if (
          step.conclusion !== "skipped" ||
          step.started_at !== null || step.completed_at !== null
        ) {
          throw new Error("completed Discord step receipt timestamps are incomplete");
        }
        continue;
      }
      const stepStartedAt = parseGithubTimestamp(
        step.started_at,
        `${job.job_name}/${step.name} receipt started-at`,
      );
      const stepCompletedAt = parseGithubTimestamp(
        step.completed_at,
        `${job.job_name}/${step.name} receipt completed-at`,
      );
      if (
        stepStartedAt > stepCompletedAt || stepStartedAt < startedAt ||
        stepCompletedAt > completedAt
      ) {
        throw new Error("completed Discord step receipt timestamps leave their job");
      }
    }
  }
}

export function validateCheckpointChronology({
  taggerAt,
  discordCompletedAt,
  artifactCreatedAt,
  observationEndedAt,
}) {
  if (
    taggerAt <= discordCompletedAt || taggerAt < artifactCreatedAt ||
    taggerAt <= observationEndedAt || artifactCreatedAt > discordCompletedAt ||
    observationEndedAt > artifactCreatedAt
  ) {
    throw new Error("checkpoint tag/artifact/observation chronology is invalid");
  }
}

function requireCleanExactMainCheckout(runGit, { repository, sourceCommit, tag }) {
  const remoteUrl = runGit.text(["remote", "get-url", "origin"]).trim();
  if (repositoryFromRemoteUrl(remoteUrl) !== repository) {
    throw new Error("origin remote differs from the checkpoint repository");
  }
  if (runGit.text(["status", "--porcelain=v1", "--untracked-files=normal"]).length !== 0) {
    throw new Error("checkpoint tag finalizer requires a clean worktree");
  }
  if (runGit.text(["rev-parse", "HEAD"]).trim() !== sourceCommit) {
    throw new Error("checkpoint tag finalizer HEAD differs from the release source");
  }
  if (runGit.text(["cat-file", "-t", sourceCommit]).trim() !== "commit") {
    throw new Error("checkpoint source is not a commit");
  }
  const localTag = runGit.optional(["show-ref", "--verify", `refs/tags/${tag}`]);
  if (localTag.status === 0) throw new Error("checkpoint release tag already exists locally");
  if (localTag.status !== 1) throw new Error("local checkpoint tag query failed");
  runGit.bytes(["fetch", "--no-tags", "--depth=1", "origin", "main"]);
  if (runGit.text(["rev-parse", "refs/remotes/origin/main"]).trim() !== sourceCommit) {
    throw new Error("fetched origin/main differs from the release source");
  }
}

function assertRemoteMainAndAbsentTag(runGit, sourceCommit, tag) {
  const main = runGit.text(["ls-remote", "origin", "refs/heads/main"]);
  if (main !== `${sourceCommit}\trefs/heads/main\n`) {
    throw new Error("remote main is missing, ambiguous, or differs from the release source");
  }
  const tagOutput = runGit.text([
    "ls-remote", "origin", `refs/tags/${tag}`, `refs/tags/${tag}^{}`,
  ]);
  if (tagOutput.length !== 0) {
    throw new Error("checkpoint release tag already exists remotely");
  }
}

function readLocalTag(runGit, tag, expectedMessage, { sourceCommit, tagger }) {
  const tagObjectSha = requireSourceCommit(
    runGit.text(["rev-parse", `refs/tags/${tag}`]).trim(),
    "local tag object SHA",
  );
  const object = parseRawTagObject(runGit.bytes(["cat-file", "tag", tagObjectSha]));
  if (
    object.targetCommit !== sourceCommit || object.type !== "commit" ||
    object.tag !== tag || object.tagger.name !== tagger.name ||
    object.tagger.email !== tagger.email || object.tagger.date !== tagger.date ||
    !object.message.equals(expectedMessage)
  ) {
    throw new Error("local annotated checkpoint tag differs from exact receipt bytes");
  }
  return Object.freeze({ tagObjectSha, object });
}

export function parseRawTagObject(bytes) {
  const raw = Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes ?? []);
  const separator = raw.indexOf(Buffer.from("\n\n", "utf8"));
  if (separator < 1) throw new Error("annotated tag object has no message boundary");
  const header = raw.subarray(0, separator).toString("utf8");
  if (!Buffer.from(header, "utf8").equals(raw.subarray(0, separator))) {
    throw new Error("annotated tag header is not UTF-8");
  }
  const lines = header.split("\n");
  if (lines.length !== 4) throw new Error("annotated tag header is not closed");
  const object = /^object ([0-9a-f]{40})$/u.exec(lines[0]);
  const type = /^type (commit)$/u.exec(lines[1]);
  const tag = /^tag (v[0-9][0-9A-Za-z.+-]*)$/u.exec(lines[2]);
  const tagger = /^tagger (.+) <([^<>]+)> ([0-9]+) (\+0000)$/u.exec(lines[3]);
  if (!object || !type || !tag || !tagger) {
    throw new Error("annotated tag header authority is invalid");
  }
  const milliseconds = Number(tagger[3]) * 1000;
  if (!Number.isSafeInteger(milliseconds) || milliseconds < 0) {
    throw new Error("annotated tagger epoch is invalid");
  }
  return Object.freeze({
    targetCommit: object[1],
    type: type[1],
    tag: tag[1],
    tagger: Object.freeze({
      name: tagger[1],
      email: tagger[2],
      date: toSecondTimestamp(milliseconds),
    }),
    message: Buffer.from(raw.subarray(separator + 2)),
  });
}

export function validateRemoteLsRemote(output, tag, sourceCommit) {
  const records = new Map();
  for (const line of output.split(/\r?\n/u)) {
    if (line.length === 0) continue;
    const match = /^([0-9a-f]{40})\t(\S+)$/u.exec(line);
    if (!match || records.has(match[2])) {
      throw new Error("remote checkpoint tag response is malformed or ambiguous");
    }
    records.set(match[2], match[1]);
  }
  const ref = `refs/tags/${tag}`;
  if (
    records.size !== 2 || !records.has(ref) ||
    records.get(`${ref}^{}`) !== sourceCommit ||
    records.get(ref) === sourceCommit
  ) {
    throw new Error("remote checkpoint tag is not the exact annotated release tag");
  }
  return Object.freeze({
    tagObjectSha: records.get(ref),
    targetCommit: records.get(`${ref}^{}`),
  });
}

function validateRemoteTagRef(value, tag) {
  requirePlainObject(value, "checkpoint tag ref");
  if (
    value.ref !== `refs/tags/${tag}` || value.object?.type !== "tag" ||
    !/^[0-9a-f]{40}$/u.test(value.object?.sha ?? "")
  ) {
    throw new Error("remote checkpoint tag ref is not one annotated tag object");
  }
  return value.object.sha;
}

function validateRemoteTagObject(value, { tag, sourceCommit, tagObjectSha }) {
  requirePlainObject(value, "checkpoint annotated tag object");
  if (
    value.sha !== tagObjectSha || value.tag !== tag ||
    value.object?.type !== "commit" || value.object?.sha !== sourceCommit ||
    typeof value.message !== "string"
  ) {
    throw new Error("remote checkpoint annotated tag object differs from its authority");
  }
  validateTagger(value.tagger);
  return value.message;
}

export function parseCanonicalReceiptBytes(bytes) {
  const raw = Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes ?? []);
  const text = raw.toString("utf8");
  if (!Buffer.from(text, "utf8").equals(raw)) {
    throw new Error("checkpoint receipt tag message is not UTF-8");
  }
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("checkpoint receipt tag message is not JSON");
  }
  if (text !== `${canonicalJson(value)}\n`) {
    throw new Error("checkpoint receipt tag message is not exact canonical JSON bytes");
  }
  return Object.freeze({
    value,
    bytes: raw,
    fileSha256: parsedReceiptFileSha256(raw),
  });
}

function parsedReceiptFileSha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function readCanonicalJsonFile(path, label) {
  const target = resolve(requireNonEmptyString(path, `${label} path`));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const raw = await readFile(target, "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  return Object.freeze({
    value,
    fileSha256: createHash("sha256").update(raw, "utf8").digest("hex"),
  });
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("checkpoint evidence path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function createGithubApiGet(repository) {
  return async (path, label) => {
    if (typeof path !== "string" || !path.startsWith("/") || /[\r\n]/u.test(path)) {
      throw new Error(`${label} GitHub API path is invalid`);
    }
    const raw = runCommand(
      "gh",
      [
        "api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2026-03-10",
        `/repos/${repository}${path}`,
      ],
      { maximumBytes: MAXIMUM_GH_API_JSON_BYTES },
    );
    try {
      return JSON.parse(raw.toString("utf8"));
    } catch {
      throw new Error(`${label} GitHub API response is not JSON`);
    }
  };
}

function createGithubArtifactDownload(repository) {
  return async (url, label) => {
    const target = new URL(requireNonEmptyString(url, `${label} URL`));
    const expected = new RegExp(
      `^/repos/${escapeRegExp(repository)}/actions/artifacts/[1-9][0-9]*/zip$`,
      "u",
    );
    if (
      target.protocol !== "https:" || target.hostname !== "api.github.com" ||
      target.username || target.password || target.search || target.hash ||
      !expected.test(target.pathname)
    ) {
      throw new Error(`${label} URL is outside the exact GitHub artifact endpoint`);
    }
    return runCommand(
      "gh",
      [
        "api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2026-03-10",
        target.pathname,
      ],
      { maximumBytes: MAXIMUM_CANDIDATE_ARCHIVE_BYTES },
    );
  };
}

function createGitRunner(cwd) {
  const root = resolve(cwd ?? process.cwd());
  const invoke = (args, options = {}, allowFailure = false) => {
    if (!Array.isArray(args) || args.some((value) => typeof value !== "string")) {
      throw new Error("git argument vector is invalid");
    }
    const result = spawnSync("git", args, {
      cwd: root,
      encoding: null,
      input: options.input,
      env: options.env ? { ...process.env, ...options.env } : process.env,
      maxBuffer: 8 * 1024 * 1024,
      windowsHide: true,
      shell: false,
    });
    if (result.error || result.signal || (!allowFailure && result.status !== 0)) {
      throw new Error(`git ${args[0] ?? "command"} failed`);
    }
    return Object.freeze({
      status: result.status,
      stdout: Buffer.from(result.stdout ?? []),
    });
  };
  return Object.freeze({
    bytes(args, options) {
      return invoke(args, options).stdout;
    },
    text(args, options) {
      return invoke(args, options).stdout.toString("utf8");
    },
    optional(args, options) {
      return invoke(args, options, true);
    },
  });
}

function runCommand(command, args, { maximumBytes }) {
  const result = spawnSync(command, args, {
    encoding: null,
    maxBuffer: maximumBytes,
    windowsHide: true,
    shell: false,
  });
  if (result.error || result.signal || result.status !== 0) {
    throw new Error(`${command} command failed`);
  }
  const bytes = Buffer.from(result.stdout ?? []);
  if (bytes.length > maximumBytes) throw new Error(`${command} output is too large`);
  return bytes;
}

function validateArtifactDownloadUrl(value, repository, artifactId) {
  let target;
  try {
    target = new URL(requireNonEmptyString(value, "artifact download URL"));
  } catch {
    throw new Error("artifact download URL is invalid");
  }
  const expectedPath = `/repos/${repository}/actions/artifacts/${artifactId}/zip`;
  if (
    target.protocol !== "https:" || target.hostname !== "api.github.com" ||
    target.username || target.password || target.search || target.hash ||
    target.pathname !== expectedPath
  ) {
    throw new Error("artifact download URL differs from the exact REST artifact ID");
  }
  return target.toString();
}

function validateTagger(value) {
  requirePlainObject(value, "checkpoint tagger");
  requireExactKeys(value, ["name", "email", "date"], "checkpoint tagger");
  return Object.freeze({
    name: validateTaggerName(value.name),
    email: validateTaggerEmail(value.email),
    date: requireSecondTimestamp(value.date, "checkpoint tagger date"),
  });
}

function validateTaggerName(value) {
  const name = requireNonEmptyString(String(value ?? "").trim(), "checkpoint tagger name");
  if (/[<>\u0000-\u001f\u007f]/u.test(name)) {
    throw new Error("checkpoint tagger name contains forbidden tag-header bytes");
  }
  return name;
}

function validateTaggerEmail(value) {
  const email = requireNonEmptyString(String(value ?? "").trim(), "checkpoint tagger email");
  if (!SAFE_TAGGER_EMAIL.test(email)) {
    throw new Error("checkpoint tagger email contains forbidden tag-header bytes");
  }
  return email;
}

function currentSecondTimestamp() {
  return toSecondTimestamp(Math.floor(Date.now() / 1000) * 1000);
}

function requireSecondTimestamp(value, label) {
  if (typeof value !== "string" || !SECOND_TIMESTAMP.test(value)) {
    throw new Error(`${label} is not a UTC second timestamp`);
  }
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds) || toSecondTimestamp(milliseconds) !== value) {
    throw new Error(`${label} is not canonical`);
  }
  return value;
}

function parseSecondTimestamp(value, label) {
  return Date.parse(requireSecondTimestamp(value, label));
}

function parseApiTimestamp(value, label) {
  if (typeof value !== "string") throw new Error(`${label} is invalid`);
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds)) throw new Error(`${label} is invalid`);
  return milliseconds;
}

function parseGithubTimestamp(value, label) {
  if (
    typeof value !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/u.test(value)
  ) {
    throw new Error(`${label} is invalid`);
  }
  const milliseconds = Date.parse(value);
  const canonical = Number.isFinite(milliseconds)
    ? new Date(milliseconds).toISOString()
    : "";
  if (value !== canonical && value !== canonical.replace(".000Z", "Z")) {
    throw new Error(`${label} is invalid`);
  }
  return milliseconds;
}

function toSecondTimestamp(milliseconds) {
  return new Date(milliseconds).toISOString().replace(".000Z", "Z");
}

function requireDecimal(value, label) {
  return requirePattern(String(value ?? ""), DECIMAL_ID, label);
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function requireReleaseTag(value) {
  if (value !== RELEASE) throw new Error("checkpoint finalizer accepts only v0.8.0");
  return value;
}

function repositoryFromRemoteUrl(value) {
  const https = /^https:\/\/github\.com\/([^/]+\/[^/]+?)(?:\.git)?\/?$/u.exec(value);
  const scp = /^git@github\.com:([^/]+\/[^/]+?)(?:\.git)?$/u.exec(value);
  const ssh = /^ssh:\/\/git@github\.com\/([^/]+\/[^/]+?)(?:\.git)?\/?$/u.exec(value);
  return (https ?? scp ?? ssh)?.[1] ?? null;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function parseCliArguments(argv) {
  if (
    !Array.isArray(argv) || argv.length < 1 ||
    !["tag", "verify-tag", "verify-release"].includes(argv[0])
  ) {
    throw new Error("usage: finalize-discord-production-checkpoint.mjs (tag|verify-tag|verify-release) [closed options]");
  }
  const command = argv[0];
  const common = {
    repository: { type: "string" },
    tag: { type: "string" },
    "source-commit": { type: "string" },
    "accepted-run-id": { type: "string" },
    "accepted-run-attempt": { type: "string" },
  };
  const options = command === "tag"
    ? {
        ...common,
        "discord-run-id": { type: "string" },
        "discord-run-attempt": { type: "string" },
        "artifact-id": { type: "string" },
        "artifact-digest": { type: "string" },
      }
    : {
        ...common,
        "acceptance-evidence": { type: "string" },
      };
  const { values, positionals } = parseArgs({
    args: argv.slice(1),
    options,
    strict: true,
    allowPositionals: true,
  });
  if (positionals.length !== 0 || Object.keys(values).length !== Object.keys(options).length) {
    throw new Error(`${command} requires its exact closed option set`);
  }
  return Object.freeze({ command, values });
}

async function main() {
  const { command, values } = parseCliArguments(process.argv.slice(2));
  const common = {
    repository: values.repository,
    tag: values.tag,
    sourceCommit: values["source-commit"],
    acceptedWorkflowRunId: values["accepted-run-id"],
    acceptedWorkflowRunAttempt: values["accepted-run-attempt"],
  };
  if (command === "verify-tag" || command === "verify-release") {
    const verifyOptions = {
      ...common,
      acceptanceEvidencePath: values["acceptance-evidence"],
    };
    const verified = command === "verify-tag"
      ? await verifyRemoteDiscordProductionCheckpointTag(verifyOptions)
      : await verifyImmutableDiscordCheckpointRelease(verifyOptions);
    process.stdout.write(
      command === "verify-tag"
        ? `discord_checkpoint_tag=${verified.tag} receipt_sha256=${verified.receipt.report_sha256}\n`
        : `discord_checkpoint_release=${verified.tag} release_id=${verified.releaseId} receipt_sha256=${verified.receiptSha256}\n`,
    );
    return;
  }
  const finalized = await finalizeDiscordProductionCheckpointTag({
    ...common,
    discordWorkflowRunId: values["discord-run-id"],
    discordWorkflowRunAttempt: values["discord-run-attempt"],
    artifactId: values["artifact-id"],
    artifactDigest: values["artifact-digest"],
  });
  process.stdout.write(
    `discord_checkpoint_tag=${finalized.tag} tag_object=${finalized.tagObjectSha} receipt_sha256=${finalized.receiptSha256}\n`,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  });
}

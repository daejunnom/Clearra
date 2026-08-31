// SRP rationale: publication provenance stays cohesive here because the behavior-level change reason is to bind draft recovery, receipt capture, finalization, artifact resolution, and verification to one closed release authority.
import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { lstat, mkdir, open, readFile, rename, rmdir, unlink } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { inflateRawSync } from "node:zlib";

import {
  canonicalJson,
  canonicalSha256,
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  validateCanonicalAcceptanceEvidence,
  verifyCanonicalAcceptanceEvidence,
} from "./canonical-acceptance-evidence.mjs";
import {
  validateFinalSourceEventPayload,
} from "./final-source-event-contract.mjs";

export const RELEASE_PUBLICATION_EVIDENCE_SCHEMA_ID =
  "clearra.release-publication-evidence.v1";
export const RELEASE_PUBLICATION_RECEIPT_SCHEMA_ID =
  "clearra.release-publication-receipt.v1";
export const RELEASE_PUBLICATION_FINAL_AUTHORITY_SCHEMA_ID =
  "clearra.release-publication-final-authority.v1";

const RELEASE = "v0.8.0";
const VERSION = "0.8.0";
const WORKFLOW_PATH = ".github/workflows/release-cli.yml";
const FINALIZER_WORKFLOW_PATH = ".github/workflows/finalize-release-publication.yml";
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const GIT_OBJECT_ID = /^[0-9a-f]{40}$/u;
const EXPECTED_ASSET_ROLES = Object.freeze([
  "linux-cli",
  "windows-cli",
  "windows-gui",
]);
const ARTIFACT_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const RECEIPT_RETENTION_DAYS = 90;
const MINIMUM_RECEIPT_RETENTION_SECONDS = 89 * 24 * 60 * 60;
const RECEIPT_FILE_NAME = "clearra-release-publication-receipt.v1.json";
const FINAL_EVIDENCE_FILE_NAME = "clearra-release-publication-evidence.v1.json";
const FINAL_AUTHORITY_FILE_NAME =
  "clearra-release-publication-final-authority.v1.json";
const MAXIMUM_RECEIPT_ARCHIVE_BYTES = 2 * 1024 * 1024;
const MAXIMUM_GH_API_JSON_BYTES = 8 * 1024 * 1024;

export function expectedReleasePublicationReceiptArtifactName({
  sourceCommit,
  workflowRunId,
  workflowRunAttempt,
}) {
  const source = requireSourceCommit(sourceCommit, "receipt source commit");
  const runId = requirePattern(String(workflowRunId ?? ""), DECIMAL_ID, "receipt run ID");
  const attempt = requirePattern(
    String(workflowRunAttempt ?? ""),
    DECIMAL_ID,
    "receipt run attempt",
  );
  return `release-publication-receipt-${source}-run-${runId}-attempt-${attempt}`;
}

export function expectedReleasePublicationEvidenceArtifactName({
  sourceCommit,
  workflowRunId,
  workflowRunAttempt,
  finalizerWorkflowRunId,
  finalizerWorkflowRunAttempt,
}) {
  const receiptName = expectedReleasePublicationReceiptArtifactName({
    sourceCommit,
    workflowRunId,
    workflowRunAttempt,
  }).replace("release-publication-receipt-", "release-publication-evidence-");
  const finalizerRun = requirePattern(
    String(finalizerWorkflowRunId ?? ""),
    DECIMAL_ID,
    "finalizer workflow run ID",
  );
  const finalizerAttempt = requirePattern(
    String(finalizerWorkflowRunAttempt ?? ""),
    DECIMAL_ID,
    "finalizer workflow run attempt",
  );
  return `${receiptName}-finalizer-${finalizerRun}-attempt-${finalizerAttempt}`;
}

export async function recoverReleasePublication(options, dependencies = {}) {
  const authority = validateAuthority(options);
  if (Number(authority.workflowRunAttempt) <= 1) {
    throw new Error("release recovery is forbidden on the first publication attempt");
  }
  const acceptanceEvidence = options.acceptanceEvidence;
  validateCanonicalAcceptanceEvidence(acceptanceEvidence, {
    repository: authority.repository,
    version: VERSION,
    basePath: acceptanceEvidence?.pages_base_path,
    sourceCommit: authority.sourceCommit,
    runId: acceptanceEvidence?.run_id,
    runAttempt: acceptanceEvidence?.run_attempt,
  });
  const verifyAcceptance = dependencies.verifyAcceptanceEvidence ??
    verifyCanonicalAcceptanceEvidence;
  await verifyAcceptance(acceptanceEvidence, {
    repository: authority.repository,
    version: VERSION,
    basePath: acceptanceEvidence.pages_base_path,
    sourceCommit: authority.sourceCommit,
    runId: acceptanceEvidence.run_id,
    runAttempt: acceptanceEvidence.run_attempt,
    productsDirectory: options.productsDirectory,
  });
  const apiGet = dependencies.apiGet ?? githubApiClient({
    repository: authority.repository,
    apiUrl: dependencies.apiUrl,
    token: dependencies.token,
  });
  const encodedTag = encodeURIComponent(authority.tag);
  const [priorAttempts, run, tagRef, release] = await Promise.all([
    readPriorPublicationAttempts(apiGet, authority),
    apiGet(
      `/actions/runs/${authority.workflowRunId}/attempts/${authority.workflowRunAttempt}`,
      "active publication recovery workflow run",
    ),
    apiGet(`/git/ref/tags/${encodedTag}`, "release recovery annotated tag"),
    apiGet(`/releases/tags/${encodedTag}`, "release recovery readback"),
  ]);
  if (priorAttempts.length !== Number(authority.workflowRunAttempt) - 1) {
    throw new Error("release recovery prior attempt history is incomplete");
  }
  validatePublicationRun(run, authority, { active: true });
  const tagObjectSha = validateAnnotatedTagRef(tagRef, authority.tag);
  const tagObject = await apiGet(
    `/git/tags/${tagObjectSha}`,
    "release recovery annotated tag object",
  );
  validateAnnotatedTagObject(tagObject, authority, tagObjectSha);
  const acceptedArtifacts = acceptanceEvidence.final_source_fragments.release_artifacts;
  let plan = planReleasePublicationRecovery(
    release,
    authority,
    acceptedArtifacts,
  );
  if (plan.status === "already-published") return plan;

  const uploadAssets = dependencies.uploadAssets ?? (async (paths) => {
    await runGhApi(
      ["release", "upload", authority.tag, ...paths, "--repo", authority.repository],
      "release recovery missing asset upload",
      1024 * 1024,
    );
  });
  const publishDraft = dependencies.publishDraft ?? (async () => {
    await runGhApi(
      ["release", "edit", authority.tag, "--draft=false", "--repo", authority.repository],
      "release recovery draft publication",
      1024 * 1024,
    );
  });
  if (plan.missing_artifacts.length > 0) {
    const productRoot = resolve(
      requireNonEmptyString(options.productsDirectory, "release recovery product directory"),
    );
    await uploadAssets(plan.missing_artifacts.map((artifact) =>
      resolve(productRoot, artifact.name)));
  }
  const completeDraft = await apiGet(
    `/releases/tags/${encodedTag}`,
    "release recovery complete draft readback",
  );
  plan = planReleasePublicationRecovery(
    completeDraft,
    authority,
    acceptedArtifacts,
  );
  if (plan.status !== "draft-ready" || plan.missing_artifacts.length !== 0) {
    throw new Error("release recovery draft is not the exact accepted three-asset set");
  }
  await publishDraft();
  const published = await apiGet(
    `/releases/tags/${encodedTag}`,
    "release recovery immutable publication readback",
  );
  const assets = validateReleaseReadback(published, authority, acceptedArtifacts);
  return Object.freeze({
    status: "published",
    release_id: String(published.id),
    recovered_asset_count: assets.length,
    missing_artifacts: Object.freeze([]),
  });
}

export function planReleasePublicationRecovery(
  release,
  options,
  acceptedArtifacts,
) {
  const authority = validateAuthority(options);
  const accepted = validateAcceptedReleaseArtifacts(
    acceptedArtifacts,
    authority.sourceCommit,
  );
  if (release?.draft !== true) {
    const assets = validateReleaseReadback(release, authority, accepted);
    return Object.freeze({
      status: "already-published",
      release_id: String(release.id),
      recovered_asset_count: assets.length,
      missing_artifacts: Object.freeze([]),
    });
  }
  if (
    !Number.isSafeInteger(release?.id) || release.id <= 0 ||
    release.tag_name !== authority.tag || release.prerelease !== false ||
    release.immutable !== false || release.published_at !== null ||
    !Array.isArray(release.assets) || release.assets.length > accepted.length
  ) {
    throw new Error("release recovery found a non-canonical partial draft");
  }
  const acceptedByName = new Map(accepted.map((artifact) => [artifact.name, artifact]));
  const present = new Set();
  for (const asset of release.assets) {
    const canonical = acceptedByName.get(asset?.name);
    const digest = /^sha256:([0-9a-f]{64})$/u.exec(String(asset?.digest ?? ""));
    if (
      canonical === undefined || present.has(asset.name) ||
      !Number.isSafeInteger(asset?.id) || asset.id <= 0 ||
      asset.state !== "uploaded" || asset.size !== canonical.size_bytes ||
      digest?.[1] !== canonical.sha256
    ) {
      throw new Error("release recovery draft asset differs from accepted product bytes");
    }
    present.add(asset.name);
  }
  return Object.freeze({
    status: "draft-ready",
    release_id: String(release.id),
    recovered_asset_count: present.size,
    missing_artifacts: Object.freeze(
      accepted.filter((artifact) => !present.has(artifact.name)),
    ),
  });
}

export async function collectReleasePublicationReceipt(options, dependencies = {}) {
  const authority = validateAuthority(options);
  const acceptanceEvidence = options.acceptanceEvidence;
  validateCanonicalAcceptanceEvidence(acceptanceEvidence, {
    repository: authority.repository,
    version: VERSION,
    basePath: acceptanceEvidence.pages_base_path,
    sourceCommit: authority.sourceCommit,
    runId: acceptanceEvidence.run_id,
    runAttempt: acceptanceEvidence.run_attempt,
  });
  if (typeof options.productsDirectory === "string") {
    await verifyCanonicalAcceptanceEvidence(acceptanceEvidence, {
      repository: authority.repository,
      version: VERSION,
      basePath: acceptanceEvidence.pages_base_path,
      sourceCommit: authority.sourceCommit,
      runId: acceptanceEvidence.run_id,
      runAttempt: acceptanceEvidence.run_attempt,
      productsDirectory: options.productsDirectory,
    });
  }
  const apiGet = dependencies.apiGet ?? githubApiClient({
    repository: authority.repository,
    apiUrl: dependencies.apiUrl,
    token: dependencies.token,
  });
  const encodedTag = encodeURIComponent(authority.tag);
  const priorAttempts = await readPriorPublicationAttempts(apiGet, authority);
  const [run, tagRef, release] = await Promise.all([
    apiGet(
      `/actions/runs/${authority.workflowRunId}/attempts/${authority.workflowRunAttempt}`,
      "publication workflow run",
    ),
    apiGet(`/git/ref/tags/${encodedTag}`, "remote annotated release tag"),
    apiGet(`/releases/tags/${encodedTag}`, "immutable GitHub Release"),
  ]);
  validatePublicationRun(run, authority, { active: true });
  const tagObjectSha = validateAnnotatedTagRef(tagRef, authority.tag);
  const annotatedTag = await apiGet(`/git/tags/${tagObjectSha}`, "annotated release tag object");
  validateAnnotatedTagObject(annotatedTag, authority, tagObjectSha);
  const assets = validateReleaseReadback(
    release,
    authority,
    acceptanceEvidence.final_source_fragments.release_artifacts,
  );
  const receipt = sealCanonicalReport({
    schema_id: RELEASE_PUBLICATION_RECEIPT_SCHEMA_ID,
    repository: authority.repository,
    release: authority.tag,
    source_commit: authority.sourceCommit,
    workflow_path: WORKFLOW_PATH,
    workflow_run_id: authority.workflowRunId,
    workflow_run_attempt: authority.workflowRunAttempt,
    accepted_run_id: acceptanceEvidence.run_id,
    accepted_run_attempt: acceptanceEvidence.run_attempt,
    acceptance_evidence_sha256: acceptanceEvidence.report_sha256,
    canonical_acceptance_evidence: acceptanceEvidence,
    receipt_artifact_name: expectedReleasePublicationReceiptArtifactName(authority),
    receipt_retention_days: RECEIPT_RETENTION_DAYS,
    prior_attempts: {
      count: priorAttempts.length,
      all_completed_non_success: true,
      api_readback_sha256: canonicalSha256(priorAttempts),
    },
    workflow_run: {
      status: "in_progress",
      conclusion: null,
      api_readback_sha256: canonicalSha256(run),
    },
    remote_tag: {
      tag_object_sha: tagObjectSha,
      target_commit: authority.sourceCommit,
      annotated: true,
      remote_verified: true,
      ref_api_readback_sha256: canonicalSha256(tagRef),
      tag_api_readback_sha256: canonicalSha256(annotatedTag),
    },
    github_release: {
      release_id: String(release.id),
      published_at: canonicalTimestamp(release.published_at, "GitHub Release publication time"),
      immutable: true,
      draft: false,
      prerelease: false,
      asset_count: assets.length,
      api_readback_sha256: canonicalSha256(release),
    },
    assets,
    status: "captured",
  });
  validateReleasePublicationReceipt(receipt, {
    expectedRepository: authority.repository,
    expectedSourceCommit: authority.sourceCommit,
    expectedWorkflowRunId: authority.workflowRunId,
    expectedWorkflowRunAttempt: authority.workflowRunAttempt,
    acceptanceEvidence,
  });
  return receipt;
}

export function validateReleasePublicationReceipt(
  value,
  {
    expectedRepository,
    expectedSourceCommit,
    expectedWorkflowRunId,
    expectedWorkflowRunAttempt,
    acceptanceEvidence,
  } = {},
) {
  requireExactKeys(value, [
    "schema_id", "repository", "release", "source_commit", "workflow_path",
    "workflow_run_id", "workflow_run_attempt", "accepted_run_id",
    "accepted_run_attempt", "acceptance_evidence_sha256",
    "canonical_acceptance_evidence",
    "receipt_artifact_name", "receipt_retention_days", "prior_attempts", "workflow_run",
    "remote_tag", "github_release", "assets", "status", "report_sha256",
  ], "release publication receipt");
  if (value.schema_id !== RELEASE_PUBLICATION_RECEIPT_SCHEMA_ID) {
    throw new Error("release publication receipt schema is invalid");
  }
  verifyCanonicalReportHash(value, "release publication receipt");
  validatePublicationIdentity(value, {
    expectedRepository,
    expectedSourceCommit,
    expectedWorkflowRunId,
    expectedWorkflowRunAttempt,
    expectedStatus: "captured",
  });
  if (
    value.receipt_artifact_name !== expectedReleasePublicationReceiptArtifactName({
      sourceCommit: value.source_commit,
      workflowRunId: value.workflow_run_id,
      workflowRunAttempt: value.workflow_run_attempt,
    }) ||
    value.receipt_retention_days !== RECEIPT_RETENTION_DAYS
  ) {
    throw new Error("publication receipt artifact identity or retention is invalid");
  }
  requireExactKeys(value.prior_attempts, [
    "count", "all_completed_non_success", "api_readback_sha256",
  ], "publication receipt prior attempts");
  if (
    value.prior_attempts.count !== Number(value.workflow_run_attempt) - 1 ||
    value.prior_attempts.all_completed_non_success !== true
  ) {
    throw new Error("publication receipt prior attempt history is incomplete or successful");
  }
  requireSha256(
    value.prior_attempts.api_readback_sha256,
    "publication prior attempts API readback SHA-256",
  );
  requireExactKeys(value.workflow_run, [
    "status", "conclusion", "api_readback_sha256",
  ], "publication receipt workflow run");
  if (value.workflow_run.status !== "in_progress" || value.workflow_run.conclusion !== null) {
    throw new Error("publication receipt was not captured from its active tag run");
  }
  requireSha256(value.workflow_run.api_readback_sha256, "receipt workflow API readback SHA-256");
  validateRemoteTag(value.remote_tag, value.source_commit, { requireReadbackHashes: true });
  validateGithubRelease(value.github_release, { requireReadbackHash: true });
  validatePublicationAssets(value.assets, value.source_commit);
  const embeddedAcceptance = value.canonical_acceptance_evidence;
  validateAcceptanceBinding(value, embeddedAcceptance);
  if (
    acceptanceEvidence !== undefined &&
    canonicalJson(acceptanceEvidence) !== canonicalJson(embeddedAcceptance)
  ) {
    throw new Error("publication receipt embedded acceptance evidence differs from its caller authority");
  }
  rejectSecretMaterial(value, "release publication receipt");
  return value;
}

export async function inspectReleasePublicationFinalizerAttempt(
  options,
  dependencies = {},
) {
  const authority = validateAuthority(options);
  const finalizer = validateFinalizerAuthority(options);
  const apiGet = dependencies.apiGet ?? githubApiClient({
    repository: authority.repository,
    apiUrl: dependencies.apiUrl,
    token: dependencies.token,
  });
  const attempt = Number(finalizer.workflowRunAttempt);
  if (!Number.isSafeInteger(attempt) || attempt < 1 || attempt > 100) {
    throw new Error("finalizer workflow run attempt is outside the bounded history contract");
  }
  const [currentRun, ...priorRuns] = await Promise.all([
    apiGet(
      `/actions/runs/${finalizer.workflowRunId}/attempts/${finalizer.workflowRunAttempt}`,
      "current publication finalizer workflow attempt",
    ),
    ...Array.from({ length: attempt - 1 }, (_, index) => {
      const priorAttempt = String(index + 1);
      return apiGet(
        `/actions/runs/${finalizer.workflowRunId}/attempts/${priorAttempt}`,
        `prior publication finalizer attempt ${priorAttempt}`,
      );
    }),
  ]);
  validateFinalizerRun(currentRun, finalizer, finalizer.workflowRunAttempt, {
    active: true,
  });
  let priorSuccessCount = 0;
  priorRuns.forEach((run, index) => {
    validateFinalizerRun(run, finalizer, String(index + 1), { active: false });
    if (run.conclusion === "success") priorSuccessCount += 1;
  });
  if (priorSuccessCount > 1) {
    throw new Error("publication finalizer has ambiguous multiple successful prior attempts");
  }
  return Object.freeze({
    finalizer,
    currentRun,
    priorRuns: Object.freeze(priorRuns),
    skipBecausePriorSuccess: priorSuccessCount === 1,
    currentRunApiReadbackSha256: canonicalSha256(currentRun),
    priorRunsApiReadbackSha256: canonicalSha256(priorRuns),
  });
}

export async function collectReleasePublicationEvidence(options, dependencies = {}) {
  return (await collectReleasePublicationEvidenceBundle(options, dependencies)).report;
}

export async function collectReleasePublicationEvidenceBundle(options, dependencies = {}) {
  const authority = validateAuthority(options);
  const apiGet = dependencies.apiGet ?? githubApiClient({
    repository: authority.repository,
    apiUrl: dependencies.apiUrl,
    token: dependencies.token,
  });
  const downloadArtifact = dependencies.downloadArtifact ?? githubArtifactDownloader({
    apiUrl: dependencies.apiUrl,
    token: dependencies.token,
  });
  const finalizerAttempt = dependencies.finalizerAttemptAuthority ??
    await inspectReleasePublicationFinalizerAttempt(options, { apiGet });
  if (finalizerAttempt.skipBecausePriorSuccess) {
    throw new Error("publication finalizer already has a successful prior attempt");
  }
  const encodedTag = encodeURIComponent(authority.tag);
  const [run, tagRef, release, receiptArtifacts, priorAttempts] = await Promise.all([
    apiGet(
      `/actions/runs/${authority.workflowRunId}/attempts/${authority.workflowRunAttempt}`,
      "publication workflow run",
    ),
    apiGet(`/git/ref/tags/${encodedTag}`, "remote annotated release tag"),
    apiGet(`/releases/tags/${encodedTag}`, "immutable GitHub Release"),
    apiGet(
      `/actions/runs/${authority.workflowRunId}/artifacts?per_page=100`,
      "publication receipt artifact list",
    ),
    readPriorPublicationAttempts(apiGet, authority),
  ]);
  validatePublicationRun(run, authority, { active: false });
  const tagObjectSha = validateAnnotatedTagRef(tagRef, authority.tag);
  const annotatedTag = await apiGet(
    `/git/tags/${tagObjectSha}`,
    "annotated release tag object",
  );
  validateAnnotatedTagObject(annotatedTag, authority, tagObjectSha);
  const receiptArtifactListing = resolveReceiptArtifact(
    receiptArtifacts,
    authority,
  );
  const receiptArtifact = await apiGet(
    `/actions/artifacts/${receiptArtifactListing.artifactId}`,
    "publication receipt artifact",
  );
  const receiptArtifactAuthority = validateReceiptArtifact(
    receiptArtifact,
    authority,
    receiptArtifactListing.artifactId,
    receiptArtifactListing.artifactDigest,
  );
  const receiptArchive = await downloadArtifact(
    receiptArtifact.archive_download_url,
    "publication receipt artifact archive",
  );
  const receiptInput = extractReleasePublicationReceiptFromArtifactZip(
    receiptArchive,
    receiptArtifactListing.artifactDigest,
  );
  const receipt = validateReleasePublicationReceipt(receiptInput.receipt, {
    expectedRepository: authority.repository,
    expectedSourceCommit: authority.sourceCommit,
    expectedWorkflowRunId: authority.workflowRunId,
    expectedWorkflowRunAttempt: authority.workflowRunAttempt,
  });
  const acceptanceEvidence = receipt.canonical_acceptance_evidence;
  const assets = validateReleaseReadback(
    release,
    authority,
    acceptanceEvidence.final_source_fragments.release_artifacts,
  );
  const receiptFileSha256 = receiptInput.fileSha256;
  if (
    receipt.prior_attempts.api_readback_sha256 !== canonicalSha256(priorAttempts) ||
    receipt.remote_tag.ref_api_readback_sha256 !== canonicalSha256(tagRef) ||
    receipt.remote_tag.tag_api_readback_sha256 !== canonicalSha256(annotatedTag)
  ) {
    throw new Error("publication retry or tag readback differs from the captured receipt");
  }

  const tagPayload = Object.freeze({
    name: authority.tag,
    target_commit: authority.sourceCommit,
    annotated: true,
    remote_verified: true,
  });
  const immutableReleasePayload = Object.freeze({
    tag: authority.tag,
    source_commit: authority.sourceCommit,
    workflow_run_id: authority.workflowRunId,
    immutable: true,
    asset_count: assets.length,
    status: "published",
  });
  validateFinalSourceEventPayload("tag", tagPayload, authority.sourceCommit);
  validateFinalSourceEventPayload(
    "immutable-release",
    immutableReleasePayload,
    authority.sourceCommit,
  );

  const report = sealCanonicalReport({
    schema_id: RELEASE_PUBLICATION_EVIDENCE_SCHEMA_ID,
    repository: authority.repository,
    release: authority.tag,
    source_commit: authority.sourceCommit,
    workflow_path: WORKFLOW_PATH,
    workflow_run_id: authority.workflowRunId,
    workflow_run_attempt: authority.workflowRunAttempt,
    accepted_run_id: acceptanceEvidence.run_id,
    accepted_run_attempt: acceptanceEvidence.run_attempt,
    acceptance_evidence_sha256: acceptanceEvidence.report_sha256,
    finalizer_workflow: {
      path: FINALIZER_WORKFLOW_PATH,
      run_id: finalizerAttempt.finalizer.workflowRunId,
      run_attempt: finalizerAttempt.finalizer.workflowRunAttempt,
      current_status: "in_progress",
      current_api_readback_sha256: finalizerAttempt.currentRunApiReadbackSha256,
      prior_attempt_count: finalizerAttempt.priorRuns.length,
      prior_attempts_api_readback_sha256:
        finalizerAttempt.priorRunsApiReadbackSha256,
    },
    publication_receipt: {
      schema_id: receipt.schema_id,
      report_sha256: receipt.report_sha256,
      file_sha256: receiptFileSha256,
      artifact_id: receiptArtifactListing.artifactId,
      artifact_name: receipt.receipt_artifact_name,
      artifact_digest: receiptArtifactListing.artifactDigest,
      retention_seconds: receiptArtifactAuthority.retentionSeconds,
      artifact_list_api_readback_sha256: canonicalSha256(receiptArtifacts),
      artifact_api_readback_sha256: canonicalSha256(receiptArtifact),
      archive_sha256: receiptInput.archiveSha256,
    },
    workflow_run: {
      status: "completed",
      conclusion: "success",
      api_readback_sha256: canonicalSha256(run),
    },
    remote_tag: {
      tag_object_sha: tagObjectSha,
      target_commit: authority.sourceCommit,
      annotated: true,
      remote_verified: true,
      ref_api_readback_sha256: canonicalSha256(tagRef),
      tag_api_readback_sha256: canonicalSha256(annotatedTag),
    },
    github_release: {
      release_id: String(release.id),
      published_at: canonicalTimestamp(
        release.published_at,
        "GitHub Release publication time",
      ),
      immutable: true,
      draft: false,
      prerelease: false,
      asset_count: assets.length,
      api_readback_sha256: canonicalSha256(release),
    },
    assets,
    final_source_fragments: {
      tag: tagPayload,
      immutable_release: immutableReleasePayload,
    },
    status: "published",
  });
  validateReleasePublicationEvidence(report, {
    expectedRepository: authority.repository,
    expectedSourceCommit: authority.sourceCommit,
    expectedWorkflowRunId: authority.workflowRunId,
    expectedWorkflowRunAttempt: authority.workflowRunAttempt,
    expectedFinalizerWorkflowRunId: finalizerAttempt.finalizer.workflowRunId,
    expectedFinalizerWorkflowRunAttempt:
      finalizerAttempt.finalizer.workflowRunAttempt,
    acceptanceEvidence,
    receipt,
    receiptFileSha256,
  });
  return Object.freeze({
    report,
    receipt,
    receiptFileSha256,
    receiptArchiveSha256: receiptInput.archiveSha256,
  });
}

export async function resolveReleasePublicationFinalAuthority(
  options,
  dependencies = {},
) {
  const authority = validateAuthority(options);
  const apiGet = dependencies.apiGet ?? githubApiClient({
    repository: authority.repository,
    apiUrl: dependencies.apiUrl,
    token: dependencies.token,
  });
  const downloadArtifact = dependencies.downloadArtifact ?? githubArtifactDownloader({
    apiUrl: dependencies.apiUrl,
    token: dependencies.token,
  });
  const artifactPages = [];
  const artifacts = [];
  let expectedTotal;
  for (let page = 1; page <= 10; page += 1) {
    const listing = await apiGet(
      `/actions/artifacts?per_page=100&page=${page}`,
      `publication final evidence artifact page ${page}`,
    );
    if (
      !Number.isSafeInteger(listing?.total_count) || listing.total_count < 0 ||
      listing.total_count > 1000 || !Array.isArray(listing?.artifacts) ||
      listing.artifacts.length > 100 ||
      (expectedTotal !== undefined && listing.total_count !== expectedTotal)
    ) {
      throw new Error("publication final evidence artifact listing is invalid or ambiguous");
    }
    expectedTotal ??= listing.total_count;
    artifactPages.push(listing);
    artifacts.push(...listing.artifacts);
    if (artifacts.length >= expectedTotal) break;
    if (listing.artifacts.length === 0) {
      throw new Error("publication final evidence artifact listing is truncated");
    }
  }
  if (artifacts.length !== expectedTotal) {
    throw new Error("publication final evidence artifact listing is truncated");
  }
  const prefix = expectedReleasePublicationReceiptArtifactName(authority)
    .replace("release-publication-receipt-", "release-publication-evidence-") +
    "-finalizer-";
  const candidates = artifacts.filter((artifact) =>
    typeof artifact?.name === "string" && artifact.name.startsWith(prefix));
  if (candidates.length === 0) {
    throw new Error("publication final evidence artifact is missing");
  }
  const qualified = [];
  for (const candidate of candidates) {
    const match = new RegExp(
      `^${escapeRegExp(prefix)}([1-9][0-9]*)-attempt-([1-9][0-9]*)$`,
      "u",
    ).exec(candidate.name);
    if (!match) {
      throw new Error("publication final evidence artifact name is malformed");
    }
    const [, finalizerRunId, finalizerRunAttempt] = match;
    if (String(candidate?.workflow_run?.id ?? "") !== finalizerRunId) {
      throw new Error("publication final artifact listing has wrong workflow provenance");
    }
    const finalizerRun = await apiGet(
      `/actions/runs/${finalizerRunId}/attempts/${finalizerRunAttempt}`,
      "publication finalizer completed attempt",
    );
    validateFinalizerRun(
      finalizerRun,
      { workflowRunId: finalizerRunId, workflowRunAttempt: finalizerRunAttempt },
      finalizerRunAttempt,
      { active: false },
    );
    if (finalizerRun.conclusion !== "success") continue;
    const artifactId = requirePattern(
      String(candidate?.id ?? ""),
      DECIMAL_ID,
      "publication final artifact ID",
    );
    const artifactDigest = requirePattern(
      candidate?.digest,
      ARTIFACT_DIGEST,
      "publication final artifact digest",
    );
    const artifact = await apiGet(
      `/actions/artifacts/${artifactId}`,
      "publication final evidence artifact",
    );
    const artifactAuthority = validateFinalEvidenceArtifact(artifact, {
      artifactId,
      artifactName: candidate.name,
      artifactDigest,
      finalizerRunId,
    });
    const archive = await downloadArtifact(
      artifact.archive_download_url,
      "publication final evidence artifact archive",
    );
    const extracted = extractReleasePublicationEvidenceFromArtifactZip(
      archive,
      artifactDigest,
    );
    const receipt = validateReleasePublicationReceipt(extracted.receipt, {
      expectedRepository: authority.repository,
      expectedSourceCommit: authority.sourceCommit,
      expectedWorkflowRunId: authority.workflowRunId,
      expectedWorkflowRunAttempt: authority.workflowRunAttempt,
    });
    const report = validateReleasePublicationEvidence(extracted.report, {
      expectedRepository: authority.repository,
      expectedSourceCommit: authority.sourceCommit,
      expectedWorkflowRunId: authority.workflowRunId,
      expectedWorkflowRunAttempt: authority.workflowRunAttempt,
      expectedFinalizerWorkflowRunId: finalizerRunId,
      expectedFinalizerWorkflowRunAttempt: finalizerRunAttempt,
      acceptanceEvidence: receipt.canonical_acceptance_evidence,
      receipt,
      receiptFileSha256: extracted.receiptFileSha256,
    });
    qualified.push({
      artifact,
      artifactAuthority,
      artifactDigest,
      artifactId,
      extracted,
      finalizerRun,
      finalizerRunId,
      finalizerRunAttempt,
      report,
      receipt,
    });
  }
  if (qualified.length !== 1) {
    throw new Error("publication final evidence must have exactly one successful authority");
  }
  const selected = qualified[0];
  const finalAuthority = sealCanonicalReport({
    schema_id: RELEASE_PUBLICATION_FINAL_AUTHORITY_SCHEMA_ID,
    repository: authority.repository,
    release: authority.tag,
    source_commit: authority.sourceCommit,
    publication_workflow_path: WORKFLOW_PATH,
    publication_workflow_run_id: authority.workflowRunId,
    publication_workflow_run_attempt: authority.workflowRunAttempt,
    finalizer_workflow_path: FINALIZER_WORKFLOW_PATH,
    finalizer_workflow_run_id: selected.finalizerRunId,
    finalizer_workflow_run_attempt: selected.finalizerRunAttempt,
    artifact_id: selected.artifactId,
    artifact_name: selected.artifact.name,
    artifact_digest: selected.artifactDigest,
    artifact_retention_seconds: selected.artifactAuthority.retentionSeconds,
    artifact_list_api_readback_sha256: canonicalSha256(artifactPages),
    artifact_api_readback_sha256: canonicalSha256(selected.artifact),
    finalizer_run_api_readback_sha256: canonicalSha256(selected.finalizerRun),
    archive_sha256: selected.extracted.archiveSha256,
    publication_evidence_sha256: selected.report.report_sha256,
    publication_evidence_file_sha256: selected.extracted.reportFileSha256,
    publication_receipt_sha256: selected.receipt.report_sha256,
    publication_receipt_file_sha256: selected.extracted.receiptFileSha256,
    status: "resolved",
  });
  validateReleasePublicationFinalAuthority(finalAuthority, {
    expectedRepository: authority.repository,
    expectedSourceCommit: authority.sourceCommit,
    expectedWorkflowRunId: authority.workflowRunId,
    expectedWorkflowRunAttempt: authority.workflowRunAttempt,
    publicationEvidence: selected.report,
    publicationEvidenceFileSha256: selected.extracted.reportFileSha256,
    publicationReceipt: selected.receipt,
    publicationReceiptFileSha256: selected.extracted.receiptFileSha256,
  });
  return Object.freeze({
    authority: finalAuthority,
    report: selected.report,
    receipt: selected.receipt,
    reportFileSha256: selected.extracted.reportFileSha256,
    receiptFileSha256: selected.extracted.receiptFileSha256,
  });
}

export function validateReleasePublicationFinalAuthority(value, {
  expectedRepository,
  expectedSourceCommit,
  expectedWorkflowRunId,
  expectedWorkflowRunAttempt,
  publicationEvidence,
  publicationEvidenceFileSha256,
  publicationReceipt,
  publicationReceiptFileSha256,
} = {}) {
  requireExactKeys(value, [
    "schema_id", "repository", "release", "source_commit",
    "publication_workflow_path", "publication_workflow_run_id",
    "publication_workflow_run_attempt", "finalizer_workflow_path",
    "finalizer_workflow_run_id", "finalizer_workflow_run_attempt",
    "artifact_id", "artifact_name", "artifact_digest",
    "artifact_retention_seconds", "artifact_list_api_readback_sha256",
    "artifact_api_readback_sha256", "finalizer_run_api_readback_sha256",
    "archive_sha256", "publication_evidence_sha256",
    "publication_evidence_file_sha256", "publication_receipt_sha256",
    "publication_receipt_file_sha256", "status", "report_sha256",
  ], "release publication final authority");
  if (value.schema_id !== RELEASE_PUBLICATION_FINAL_AUTHORITY_SCHEMA_ID) {
    throw new Error("release publication final authority schema is invalid");
  }
  verifyCanonicalReportHash(value, "release publication final authority");
  if (
    value.release !== RELEASE || value.publication_workflow_path !== WORKFLOW_PATH ||
    value.finalizer_workflow_path !== FINALIZER_WORKFLOW_PATH ||
    value.status !== "resolved"
  ) {
    throw new Error("release publication final authority identity is invalid");
  }
  requirePattern(value.repository, REPOSITORY, "publication final repository");
  requireSourceCommit(value.source_commit, "publication final source commit");
  for (const [field, label] of [
    ["publication_workflow_run_id", "publication run ID"],
    ["publication_workflow_run_attempt", "publication run attempt"],
    ["finalizer_workflow_run_id", "finalizer run ID"],
    ["finalizer_workflow_run_attempt", "finalizer run attempt"],
    ["artifact_id", "final artifact ID"],
  ]) requirePattern(value[field], DECIMAL_ID, label);
  if (
    value.artifact_name !== expectedReleasePublicationEvidenceArtifactName({
      sourceCommit: value.source_commit,
      workflowRunId: value.publication_workflow_run_id,
      workflowRunAttempt: value.publication_workflow_run_attempt,
      finalizerWorkflowRunId: value.finalizer_workflow_run_id,
      finalizerWorkflowRunAttempt: value.finalizer_workflow_run_attempt,
    }) ||
    !Number.isSafeInteger(value.artifact_retention_seconds) ||
    value.artifact_retention_seconds < MINIMUM_RECEIPT_RETENTION_SECONDS ||
    value.artifact_digest !== `sha256:${value.archive_sha256}`
  ) {
    throw new Error("release publication final artifact authority is invalid");
  }
  requirePattern(value.artifact_digest, ARTIFACT_DIGEST, "final artifact digest");
  for (const field of [
    "artifact_list_api_readback_sha256", "artifact_api_readback_sha256",
    "finalizer_run_api_readback_sha256", "archive_sha256",
    "publication_evidence_sha256", "publication_evidence_file_sha256",
    "publication_receipt_sha256", "publication_receipt_file_sha256",
  ]) requireSha256(value[field], `publication final authority ${field}`);
  for (const [actual, expected, label] of [
    [value.repository, expectedRepository, "repository"],
    [value.source_commit, expectedSourceCommit, "source commit"],
    [value.publication_workflow_run_id, expectedWorkflowRunId, "publication run ID"],
    [value.publication_workflow_run_attempt, expectedWorkflowRunAttempt, "publication run attempt"],
  ]) {
    if (expected !== undefined && actual !== String(expected)) {
      throw new Error(`release publication final authority ${label} differs`);
    }
  }
  if (publicationEvidence !== undefined && (
    value.publication_evidence_sha256 !== publicationEvidence.report_sha256 ||
    value.publication_evidence_file_sha256 !== publicationEvidenceFileSha256 ||
    publicationEvidence.finalizer_workflow.run_id !== value.finalizer_workflow_run_id ||
    publicationEvidence.finalizer_workflow.run_attempt !==
      value.finalizer_workflow_run_attempt
  )) {
    throw new Error("release publication evidence differs from its final artifact authority");
  }
  if (publicationReceipt !== undefined && (
    value.publication_receipt_sha256 !== publicationReceipt.report_sha256 ||
    value.publication_receipt_file_sha256 !== publicationReceiptFileSha256
  )) {
    throw new Error("release publication receipt differs from its final artifact authority");
  }
  rejectSecretMaterial(value, "release publication final authority");
  return value;
}

export function validateReleasePublicationEvidence(
  value,
  {
    expectedRepository,
    expectedSourceCommit,
    expectedWorkflowRunId,
    expectedWorkflowRunAttempt,
    expectedFinalizerWorkflowRunId,
    expectedFinalizerWorkflowRunAttempt,
    acceptanceEvidence,
    receipt,
    receiptFileSha256,
  } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "repository",
    "release",
    "source_commit",
    "workflow_path",
    "workflow_run_id",
    "workflow_run_attempt",
    "accepted_run_id",
    "accepted_run_attempt",
    "acceptance_evidence_sha256",
    "finalizer_workflow",
    "publication_receipt",
    "workflow_run",
    "remote_tag",
    "github_release",
    "assets",
    "final_source_fragments",
    "status",
    "report_sha256",
  ], "release publication evidence");
  if (value.schema_id !== RELEASE_PUBLICATION_EVIDENCE_SCHEMA_ID) {
    throw new Error("release publication evidence schema is invalid");
  }
  verifyCanonicalReportHash(value, "release publication evidence");
  validatePublicationIdentity(value, {
    expectedRepository,
    expectedSourceCommit,
    expectedWorkflowRunId,
    expectedWorkflowRunAttempt,
    expectedStatus: "published",
  });
  validatePublicationReceiptReference(value.publication_receipt, value);
  validateFinalizerWorkflowReference(value.finalizer_workflow, {
    expectedRunId: expectedFinalizerWorkflowRunId,
    expectedRunAttempt: expectedFinalizerWorkflowRunAttempt,
  });
  requireExactKeys(value.workflow_run, [
    "status", "conclusion", "api_readback_sha256",
  ], "publication evidence workflow run");
  if (
    value.workflow_run.status !== "completed" ||
    value.workflow_run.conclusion !== "success"
  ) {
    throw new Error("publication evidence workflow run did not complete successfully");
  }
  requireSha256(value.workflow_run.api_readback_sha256, "completed workflow API readback SHA-256");
  validateRemoteTag(value.remote_tag, value.source_commit, { requireReadbackHashes: true });
  validateGithubRelease(value.github_release, { requireReadbackHash: true });
  validatePublicationAssets(value.assets, value.source_commit);

  requireExactKeys(value.final_source_fragments, [
    "tag",
    "immutable_release",
  ], "publication final-source fragments");
  validateFinalSourceEventPayload(
    "tag",
    value.final_source_fragments.tag,
    value.source_commit,
  );
  validateFinalSourceEventPayload(
    "immutable-release",
    value.final_source_fragments.immutable_release,
    value.source_commit,
  );
  if (
    value.final_source_fragments.immutable_release.workflow_run_id !==
      value.workflow_run_id ||
    value.final_source_fragments.immutable_release.asset_count !==
      value.github_release.asset_count
  ) {
    throw new Error("publication final-source fragments differ from the release readback");
  }

  const boundAcceptance = acceptanceEvidence ?? receipt?.canonical_acceptance_evidence;
  if (boundAcceptance === undefined) {
    throw new Error("final publication evidence requires its canonical acceptance authority");
  }
  validateAcceptanceBinding(value, boundAcceptance);
  if (receipt !== undefined) {
    validateReleasePublicationReceipt(receipt, {
      expectedRepository: value.repository,
      expectedSourceCommit: value.source_commit,
      expectedWorkflowRunId: value.workflow_run_id,
      expectedWorkflowRunAttempt: value.workflow_run_attempt,
      acceptanceEvidence: boundAcceptance,
    });
    if (
      value.publication_receipt.report_sha256 !== receipt.report_sha256 ||
      value.publication_receipt.schema_id !== receipt.schema_id ||
      value.publication_receipt.artifact_name !== receipt.receipt_artifact_name ||
      (receiptFileSha256 !== undefined &&
        value.publication_receipt.file_sha256 !== requireSha256(
          receiptFileSha256,
          "expected publication receipt raw SHA-256",
        ))
    ) {
      throw new Error("final publication evidence differs from its captured receipt");
    }
  }
  rejectSecretMaterial(value, "release publication evidence");
  return value;
}

function validatePublicationIdentity(value, {
  expectedRepository,
  expectedSourceCommit,
  expectedWorkflowRunId,
  expectedWorkflowRunAttempt,
  expectedStatus,
}) {
  requirePattern(value.repository, REPOSITORY, "publication repository");
  requireSourceCommit(value.source_commit, "publication source commit");
  requirePattern(value.workflow_run_id, DECIMAL_ID, "publication workflow run ID");
  requirePattern(value.workflow_run_attempt, DECIMAL_ID, "publication workflow run attempt");
  requirePattern(value.accepted_run_id, DECIMAL_ID, "accepted run ID");
  requirePattern(value.accepted_run_attempt, DECIMAL_ID, "accepted run attempt");
  requireSha256(value.acceptance_evidence_sha256, "canonical acceptance evidence SHA-256");
  if (
    value.release !== RELEASE || value.workflow_path !== WORKFLOW_PATH ||
    value.status !== expectedStatus
  ) {
    throw new Error("release publication identity is invalid");
  }
  for (const [actual, expected, label] of [
    [value.repository, expectedRepository, "repository"],
    [value.source_commit, expectedSourceCommit, "source commit"],
    [value.workflow_run_id, expectedWorkflowRunId, "workflow run ID"],
    [value.workflow_run_attempt, expectedWorkflowRunAttempt, "workflow run attempt"],
  ]) {
    if (expected !== undefined && actual !== String(expected)) {
      throw new Error(`release publication ${label} differs from its authority`);
    }
  }
}

function validateRemoteTag(value, sourceCommit, { requireReadbackHashes }) {
  requireExactKeys(value, [
    "tag_object_sha", "target_commit", "annotated", "remote_verified",
    "ref_api_readback_sha256", "tag_api_readback_sha256",
  ], "release publication remote tag");
  requirePattern(value.tag_object_sha, GIT_OBJECT_ID, "remote tag object SHA");
  if (
    value.target_commit !== sourceCommit || value.annotated !== true ||
    value.remote_verified !== true || value.tag_object_sha === sourceCommit
  ) {
    throw new Error("release publication remote tag is not an exact annotated tag");
  }
  if (requireReadbackHashes) {
    requireSha256(value.ref_api_readback_sha256, "tag-ref API readback SHA-256");
    requireSha256(value.tag_api_readback_sha256, "annotated-tag API readback SHA-256");
  }
}

function validateGithubRelease(value, { requireReadbackHash }) {
  requireExactKeys(value, [
    "release_id", "published_at", "immutable", "draft", "prerelease",
    "asset_count", "api_readback_sha256",
  ], "release publication GitHub Release");
  requirePattern(value.release_id, DECIMAL_ID, "GitHub Release ID");
  canonicalTimestamp(value.published_at, "GitHub Release publication time");
  if (
    value.immutable !== true || value.draft !== false ||
    value.prerelease !== false || value.asset_count !== 3
  ) {
    throw new Error("GitHub Release is not the immutable published three-asset release");
  }
  if (requireReadbackHash) {
    requireSha256(value.api_readback_sha256, "GitHub Release API readback SHA-256");
  }
}

function validateAcceptanceBinding(value, acceptanceEvidence) {
  if (acceptanceEvidence === undefined) return;
  validateCanonicalAcceptanceEvidence(acceptanceEvidence, {
    repository: value.repository,
    version: VERSION,
    basePath: acceptanceEvidence.pages_base_path,
    sourceCommit: value.source_commit,
    runId: value.accepted_run_id,
    runAttempt: value.accepted_run_attempt,
  });
  if (
    value.acceptance_evidence_sha256 !== acceptanceEvidence.report_sha256 ||
    canonicalJson(value.assets.map(({ asset_id: _assetId, ...asset }) => asset)) !==
      canonicalJson(acceptanceEvidence.final_source_fragments.release_artifacts)
  ) {
    throw new Error("publication assets differ from canonical acceptance evidence");
  }
}

function validatePublicationReceiptReference(value, publication) {
  requireExactKeys(value, [
    "schema_id", "report_sha256", "file_sha256", "artifact_id",
    "artifact_name", "artifact_digest", "retention_seconds",
    "artifact_list_api_readback_sha256", "artifact_api_readback_sha256",
    "archive_sha256",
  ], "publication receipt reference");
  if (
    value.schema_id !== RELEASE_PUBLICATION_RECEIPT_SCHEMA_ID ||
    value.artifact_name !== expectedReleasePublicationReceiptArtifactName({
      sourceCommit: publication.source_commit,
      workflowRunId: publication.workflow_run_id,
      workflowRunAttempt: publication.workflow_run_attempt,
    }) ||
    !Number.isSafeInteger(value.retention_seconds) ||
    value.retention_seconds < MINIMUM_RECEIPT_RETENTION_SECONDS
  ) {
    throw new Error("publication receipt reference identity or retention is invalid");
  }
  requireSha256(value.report_sha256, "publication receipt report SHA-256");
  requireSha256(value.file_sha256, "publication receipt raw file SHA-256");
  requirePattern(value.artifact_id, DECIMAL_ID, "publication receipt artifact ID");
  requirePattern(value.artifact_digest, ARTIFACT_DIGEST, "publication receipt artifact digest");
  requireSha256(value.artifact_list_api_readback_sha256, "receipt artifact-list API readback SHA-256");
  requireSha256(value.artifact_api_readback_sha256, "receipt artifact API readback SHA-256");
  requireSha256(value.archive_sha256, "receipt artifact archive SHA-256");
  if (value.artifact_digest !== `sha256:${value.archive_sha256}`) {
    throw new Error("publication receipt archive differs from the artifact API digest");
  }
}

function validateFinalizerWorkflowReference(value, {
  expectedRunId,
  expectedRunAttempt,
} = {}) {
  requireExactKeys(value, [
    "path",
    "run_id",
    "run_attempt",
    "current_status",
    "current_api_readback_sha256",
    "prior_attempt_count",
    "prior_attempts_api_readback_sha256",
  ], "publication finalizer workflow reference");
  if (
    value.path !== FINALIZER_WORKFLOW_PATH ||
    value.current_status !== "in_progress" ||
    !Number.isSafeInteger(value.prior_attempt_count) ||
    value.prior_attempt_count !== Number(value.run_attempt) - 1
  ) {
    throw new Error("publication finalizer workflow reference is invalid");
  }
  requirePattern(value.run_id, DECIMAL_ID, "publication finalizer run ID");
  requirePattern(value.run_attempt, DECIMAL_ID, "publication finalizer run attempt");
  requireSha256(
    value.current_api_readback_sha256,
    "publication finalizer current API readback SHA-256",
  );
  requireSha256(
    value.prior_attempts_api_readback_sha256,
    "publication finalizer prior attempts API readback SHA-256",
  );
  if (
    (expectedRunId !== undefined && value.run_id !== String(expectedRunId)) ||
    (expectedRunAttempt !== undefined &&
      value.run_attempt !== String(expectedRunAttempt))
  ) {
    throw new Error("publication evidence differs from its finalizer workflow attempt");
  }
}

function resolveReceiptArtifact(list, authority) {
  if (
    !Number.isSafeInteger(list?.total_count) ||
    !Array.isArray(list?.artifacts) ||
    list.total_count !== list.artifacts.length ||
    list.total_count > 100
  ) {
    throw new Error("publication receipt artifact list is invalid or truncated");
  }
  const artifactName = expectedReleasePublicationReceiptArtifactName(authority);
  const matches = list.artifacts.filter((artifact) => artifact?.name === artifactName);
  if (matches.length !== 1) {
    throw new Error("publication receipt artifact must resolve exactly once in its tag run");
  }
  const artifact = matches[0];
  const artifactId = requirePattern(
    String(artifact?.id ?? ""),
    DECIMAL_ID,
    "publication receipt artifact ID",
  );
  const artifactDigest = requirePattern(
    artifact?.digest,
    ARTIFACT_DIGEST,
    "publication receipt artifact digest",
  );
  if (
    artifact.expired !== false ||
    String(artifact?.workflow_run?.id ?? "") !== authority.workflowRunId ||
    artifact?.workflow_run?.head_branch !== authority.tag ||
    artifact?.workflow_run?.head_sha !== authority.sourceCommit
  ) {
    throw new Error("publication receipt artifact listing differs from the exact tag run");
  }
  return Object.freeze({ artifactId, artifactDigest });
}

function validateReceiptArtifact(
  artifact,
  authority,
  expectedArtifactId,
  expectedArtifactDigest,
) {
  const created = Date.parse(String(artifact?.created_at ?? ""));
  const expires = Date.parse(String(artifact?.expires_at ?? ""));
  const retentionSeconds = (expires - created) / 1000;
  if (
    String(artifact?.id ?? "") !== expectedArtifactId ||
    artifact?.name !== expectedReleasePublicationReceiptArtifactName(authority) ||
    artifact?.digest !== expectedArtifactDigest ||
    typeof artifact?.archive_download_url !== "string" ||
    !artifact.archive_download_url.startsWith("https://") ||
    artifact?.expired !== false ||
    String(artifact?.workflow_run?.id ?? "") !== authority.workflowRunId ||
    artifact?.workflow_run?.head_branch !== authority.tag ||
    artifact?.workflow_run?.head_sha !== authority.sourceCommit ||
    !Number.isSafeInteger(retentionSeconds) ||
    retentionSeconds < MINIMUM_RECEIPT_RETENTION_SECONDS
  ) {
    throw new Error("publication receipt artifact differs from the exact tag run authority");
  }
  return Object.freeze({ retentionSeconds });
}

function validateFinalEvidenceArtifact(artifact, {
  artifactId,
  artifactName,
  artifactDigest,
  finalizerRunId,
}) {
  const created = Date.parse(String(artifact?.created_at ?? ""));
  const expires = Date.parse(String(artifact?.expires_at ?? ""));
  const retentionSeconds = (expires - created) / 1000;
  if (
    String(artifact?.id ?? "") !== artifactId ||
    artifact?.name !== artifactName || artifact?.digest !== artifactDigest ||
    typeof artifact?.archive_download_url !== "string" ||
    !artifact.archive_download_url.startsWith("https://") ||
    artifact?.expired !== false ||
    String(artifact?.workflow_run?.id ?? "") !== finalizerRunId ||
    !Number.isSafeInteger(retentionSeconds) ||
    retentionSeconds < MINIMUM_RECEIPT_RETENTION_SECONDS
  ) {
    throw new Error("publication final artifact differs from its finalizer authority");
  }
  return Object.freeze({ retentionSeconds });
}

async function readPriorPublicationAttempts(apiGet, authority) {
  const attempt = Number(authority.workflowRunAttempt);
  if (!Number.isSafeInteger(attempt) || attempt < 1 || attempt > 100) {
    throw new Error("publication workflow run attempt is outside the bounded history contract");
  }
  const attempts = await Promise.all(
    Array.from({ length: attempt - 1 }, (_, index) => {
      const priorAttempt = String(index + 1);
      return apiGet(
        `/actions/runs/${authority.workflowRunId}/attempts/${priorAttempt}`,
        `prior publication workflow attempt ${priorAttempt}`,
      );
    }),
  );
  attempts.forEach((run, index) => {
    const priorAttempt = String(index + 1);
    if (
      String(run?.id ?? "") !== authority.workflowRunId ||
      String(run?.run_attempt ?? "") !== priorAttempt ||
      run?.event !== "push" ||
      run?.head_branch !== authority.tag ||
      run?.head_sha !== authority.sourceCommit ||
      run?.path !== WORKFLOW_PATH ||
      run?.status !== "completed" ||
      typeof run?.conclusion !== "string" ||
      run.conclusion.length === 0 ||
      run.conclusion === "success"
    ) {
      throw new Error(
        "publication retry has an incomplete, ambiguous, or successful prior attempt",
      );
    }
  });
  return Object.freeze(attempts);
}

function validatePublicationRun(run, authority, { active }) {
  const statusIsValid = active
    ? run?.status === "in_progress" && run?.conclusion === null
    : run?.status === "completed" && run?.conclusion === "success";
  if (
    String(run?.id ?? "") !== authority.workflowRunId ||
    String(run?.run_attempt ?? "") !== authority.workflowRunAttempt ||
    run?.event !== "push" ||
    run?.head_branch !== authority.tag ||
    run?.head_sha !== authority.sourceCommit ||
    run?.path !== WORKFLOW_PATH ||
    !statusIsValid
  ) {
    throw new Error("publication workflow run differs from the exact tag authority");
  }
}

function validateAnnotatedTagRef(value, tag) {
  if (
    value?.ref !== `refs/tags/${tag}` ||
    value?.object?.type !== "tag" ||
    !GIT_OBJECT_ID.test(value?.object?.sha ?? "")
  ) {
    throw new Error("remote release tag is missing, lightweight, or malformed");
  }
  return value.object.sha;
}

function validateAnnotatedTagObject(value, authority, tagObjectSha) {
  if (
    value?.sha !== tagObjectSha ||
    value?.tag !== authority.tag ||
    value?.object?.type !== "commit" ||
    value?.object?.sha !== authority.sourceCommit
  ) {
    throw new Error("annotated release tag object resolves to a different source");
  }
}

function validateReleaseReadback(release, authority, acceptedArtifacts) {
  if (
    !Number.isSafeInteger(release?.id) ||
    release.id <= 0 ||
    release.tag_name !== authority.tag ||
    release.draft !== false ||
    release.prerelease !== false ||
    release.immutable !== true ||
    !Array.isArray(release.assets) ||
    release.assets.length !== 3
  ) {
    throw new Error("GitHub Release readback is not one immutable three-asset release");
  }
  canonicalTimestamp(release.published_at, "GitHub Release publication time");
  const acceptedByName = new Map(acceptedArtifacts.map((artifact) => [artifact.name, artifact]));
  const assets = release.assets.map((asset) => {
    const accepted = acceptedByName.get(asset?.name);
    const digest = /^sha256:([0-9a-f]{64})$/u.exec(String(asset?.digest ?? ""));
    if (
      accepted === undefined ||
      !Number.isSafeInteger(asset?.id) ||
      asset.id <= 0 ||
      asset.state !== "uploaded" ||
      asset.size !== accepted.size_bytes ||
      digest?.[1] !== accepted.sha256
    ) {
      throw new Error("GitHub Release asset differs from the accepted product bytes");
    }
    return Object.freeze({
      role: accepted.role,
      name: accepted.name,
      sha256: accepted.sha256,
      size_bytes: accepted.size_bytes,
      source_commit: accepted.source_commit,
      asset_id: String(asset.id),
    });
  });
  assets.sort((left, right) => left.role.localeCompare(right.role, "en"));
  validatePublicationAssets(assets, authority.sourceCommit);
  return Object.freeze(assets);
}

function validateAcceptedReleaseArtifacts(artifacts, sourceCommit) {
  if (!Array.isArray(artifacts) || artifacts.length !== 3) {
    throw new Error("release recovery requires exactly three accepted artifacts");
  }
  const roles = [];
  const names = new Set();
  for (const artifact of artifacts) {
    requireExactKeys(artifact, [
      "role", "name", "sha256", "size_bytes", "source_commit",
    ], "release recovery accepted artifact");
    requireNonEmptyString(artifact.name, "release recovery accepted artifact name");
    requireSha256(artifact.sha256, "release recovery accepted artifact SHA-256");
    if (
      artifact.source_commit !== sourceCommit ||
      !Number.isSafeInteger(artifact.size_bytes) || artifact.size_bytes <= 0 ||
      names.has(artifact.name)
    ) {
      throw new Error("release recovery accepted artifact identity is invalid");
    }
    names.add(artifact.name);
    roles.push(artifact.role);
  }
  if (canonicalJson(roles) !== canonicalJson(EXPECTED_ASSET_ROLES)) {
    throw new Error("release recovery accepted artifact roles are not canonical");
  }
  return artifacts;
}

function validatePublicationAssets(assets, sourceCommit) {
  if (!Array.isArray(assets) || assets.length !== 3) {
    throw new Error("publication evidence must contain exactly three assets");
  }
  const roles = [];
  const names = new Set();
  for (const asset of assets) {
    requireExactKeys(asset, [
      "role",
      "name",
      "sha256",
      "size_bytes",
      "source_commit",
      "asset_id",
    ], "publication asset");
    requirePattern(asset.asset_id, DECIMAL_ID, "publication asset ID");
    requireSha256(asset.sha256, "publication asset SHA-256");
    if (
      asset.source_commit !== sourceCommit ||
      !Number.isSafeInteger(asset.size_bytes) ||
      asset.size_bytes <= 0 ||
      names.has(asset.name)
    ) {
      throw new Error("publication asset identity is invalid or duplicated");
    }
    names.add(asset.name);
    roles.push(asset.role);
  }
  if (canonicalJson(roles) !== canonicalJson(EXPECTED_ASSET_ROLES)) {
    throw new Error("publication asset roles are not the canonical ordered set");
  }
}

function validateAuthority(options) {
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const tag = requireNonEmptyString(options?.tag, "release tag");
  if (tag !== RELEASE) throw new Error(`release tag must be ${RELEASE}`);
  const sourceCommit = requireSourceCommit(options?.sourceCommit, "publication source commit");
  const workflowRunId = requirePattern(
    String(options?.workflowRunId ?? ""),
    DECIMAL_ID,
    "publication workflow run ID",
  );
  const workflowRunAttempt = requirePattern(
    String(options?.workflowRunAttempt ?? ""),
    DECIMAL_ID,
    "publication workflow run attempt",
  );
  return Object.freeze({
    repository,
    tag,
    sourceCommit,
    workflowRunId,
    workflowRunAttempt,
  });
}

function validateFinalizerAuthority(options) {
  return Object.freeze({
    workflowRunId: requirePattern(
      String(options?.finalizerWorkflowRunId ?? ""),
      DECIMAL_ID,
      "finalizer workflow run ID",
    ),
    workflowRunAttempt: requirePattern(
      String(options?.finalizerWorkflowRunAttempt ?? ""),
      DECIMAL_ID,
      "finalizer workflow run attempt",
    ),
  });
}

function validateFinalizerRun(run, finalizer, expectedAttempt, { active }) {
  const statusValid = active
    ? run?.status === "in_progress" && run?.conclusion === null
    : run?.status === "completed" && typeof run?.conclusion === "string" &&
      run.conclusion.length > 0;
  if (
    String(run?.id ?? "") !== finalizer.workflowRunId ||
    String(run?.run_attempt ?? "") !== expectedAttempt ||
    run?.event !== "workflow_run" ||
    run?.path !== FINALIZER_WORKFLOW_PATH ||
    !statusValid
  ) {
    throw new Error("publication finalizer attempt differs from its workflow authority");
  }
}

function githubApiClient({ repository, apiUrl, token }) {
  const baseUrl = requireNonEmptyString(apiUrl, "GitHub API URL").replace(/\/$/u, "");
  const bearer = requireNonEmptyString(token, "GH_TOKEN");
  return async (path, label) => {
    const response = await fetch(`${baseUrl}/repos/${repository}${path}`, {
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: `Bearer ${bearer}`,
        "X-GitHub-Api-Version": "2026-03-10",
      },
    });
    const body = await response.text();
    if (!response.ok) {
      throw new Error(`${label} request failed with HTTP ${response.status}`);
    }
    try {
      return JSON.parse(body);
    } catch {
      throw new Error(`${label} response is not JSON`);
    }
  };
}

function githubArtifactDownloader({ apiUrl, token }) {
  const apiOrigin = new URL(requireNonEmptyString(apiUrl, "GitHub API URL")).origin;
  const bearer = requireNonEmptyString(token, "GH_TOKEN");
  return async (url, label) => {
    let current = new URL(requireNonEmptyString(url, `${label} URL`));
    if (current.protocol !== "https:" || current.username || current.password ||
        current.origin !== apiOrigin) {
      throw new Error(`${label} URL is outside the approved GitHub API origin`);
    }
    for (let redirect = 0; redirect <= 5; redirect += 1) {
      const response = await fetch(current, {
        redirect: "manual",
        headers: current.origin === apiOrigin
          ? {
              Accept: "application/vnd.github+json",
              Authorization: `Bearer ${bearer}`,
              "X-GitHub-Api-Version": "2026-03-10",
            }
          : {},
      });
      if (response.status >= 300 && response.status < 400) {
        const location = response.headers.get("location");
        if (!location) throw new Error(`${label} redirect has no location`);
        current = new URL(location, current);
        if (current.protocol !== "https:" || current.username || current.password) {
          throw new Error(`${label} redirect is not credential-free HTTPS`);
        }
        continue;
      }
      if (!response.ok) {
        throw new Error(`${label} download failed with HTTP ${response.status}`);
      }
      return Buffer.from(await response.arrayBuffer());
    }
    throw new Error(`${label} exceeded the bounded redirect count`);
  };
}

export function createGithubCliPublicationDependencies({
  repository,
  runGh = runGhApi,
} = {}) {
  const canonicalRepository = requirePattern(
    repository,
    REPOSITORY,
    "publication resolver repository",
  );
  if (typeof runGh !== "function") {
    throw new Error("publication resolver gh runner is invalid");
  }
  return Object.freeze({
    apiGet: async (path, label) => {
      if (
        typeof path !== "string" || !path.startsWith("/actions/") ||
        /[\r\n]/u.test(path)
      ) {
        throw new Error(`${label} GitHub API path is invalid`);
      }
      const raw = await runGh(
        [
          "api", "--method", "GET", "-H",
          "X-GitHub-Api-Version: 2026-03-10",
          `/repos/${canonicalRepository}${path}`,
        ],
        label,
        MAXIMUM_GH_API_JSON_BYTES,
      );
      try {
        return JSON.parse(Buffer.from(raw).toString("utf8"));
      } catch {
        throw new Error(`${label} gh api response is not JSON`);
      }
    },
    downloadArtifact: async (url, label) => {
      let target;
      try {
        target = new URL(requireNonEmptyString(url, `${label} URL`));
      } catch {
        throw new Error(`${label} URL is invalid`);
      }
      const expectedPath = new RegExp(
        `^/repos/${escapeRegExp(canonicalRepository)}/actions/artifacts/[1-9][0-9]*/zip$`,
        "u",
      );
      if (
        target.protocol !== "https:" || target.username || target.password ||
        target.search || target.hash || !expectedPath.test(target.pathname)
      ) {
        throw new Error(`${label} URL is outside the closed artifact download path`);
      }
      const raw = Buffer.from(await runGh(
        [
          "api", "--method", "GET", "-H",
          "X-GitHub-Api-Version: 2026-03-10",
          target.pathname,
        ],
        label,
        MAXIMUM_RECEIPT_ARCHIVE_BYTES,
      ));
      if (raw.length > MAXIMUM_RECEIPT_ARCHIVE_BYTES) {
        throw new Error(`${label} exceeds the closed artifact size limit`);
      }
      return raw;
    },
  });
}

function runGhApi(args, label, maximumBytes) {
  return new Promise((resolvePromise, rejectPromise) => {
    execFile("gh", args, {
      encoding: null,
      maxBuffer: maximumBytes,
      shell: false,
      timeout: 60_000,
      windowsHide: true,
    }, (error, stdout) => {
      if (error) {
        rejectPromise(new Error(`${label} gh api command failed`));
        return;
      }
      resolvePromise(Buffer.from(stdout));
    });
  });
}

export function extractReleasePublicationReceiptFromArtifactZip(
  archive,
  expectedArtifactDigest,
) {
  const bytes = Buffer.isBuffer(archive) ? archive : Buffer.from(archive ?? []);
  if (bytes.length < 22 || bytes.length > MAXIMUM_RECEIPT_ARCHIVE_BYTES) {
    throw new Error("publication receipt artifact ZIP size is invalid");
  }
  const archiveSha256 = createHash("sha256").update(bytes).digest("hex");
  if (
    requirePattern(
      expectedArtifactDigest,
      ARTIFACT_DIGEST,
      "publication receipt artifact digest",
    ) !== `sha256:${archiveSha256}`
  ) {
    throw new Error("downloaded publication receipt ZIP differs from the artifact API digest");
  }
  const eocdOffset = findEndOfCentralDirectory(bytes);
  const disk = bytes.readUInt16LE(eocdOffset + 4);
  const centralDisk = bytes.readUInt16LE(eocdOffset + 6);
  const diskEntries = bytes.readUInt16LE(eocdOffset + 8);
  const totalEntries = bytes.readUInt16LE(eocdOffset + 10);
  const centralSize = bytes.readUInt32LE(eocdOffset + 12);
  const centralOffset = bytes.readUInt32LE(eocdOffset + 16);
  const commentLength = bytes.readUInt16LE(eocdOffset + 20);
  if (
    disk !== 0 || centralDisk !== 0 || diskEntries !== 1 || totalEntries !== 1 ||
    eocdOffset + 22 + commentLength !== bytes.length ||
    centralOffset + centralSize !== eocdOffset || centralSize < 46 ||
    bytes.readUInt32LE(centralOffset) !== 0x02014b50
  ) {
    throw new Error("publication receipt artifact ZIP central directory is not closed");
  }
  const flags = bytes.readUInt16LE(centralOffset + 8);
  const method = bytes.readUInt16LE(centralOffset + 10);
  const expectedCrc32 = bytes.readUInt32LE(centralOffset + 16);
  const compressedSize = bytes.readUInt32LE(centralOffset + 20);
  const uncompressedSize = bytes.readUInt32LE(centralOffset + 24);
  const nameLength = bytes.readUInt16LE(centralOffset + 28);
  const extraLength = bytes.readUInt16LE(centralOffset + 30);
  const entryCommentLength = bytes.readUInt16LE(centralOffset + 32);
  const diskStart = bytes.readUInt16LE(centralOffset + 34);
  const externalAttributes = bytes.readUInt32LE(centralOffset + 38);
  const localOffset = bytes.readUInt32LE(centralOffset + 42);
  const nameStart = centralOffset + 46;
  const centralEnd = nameStart + nameLength + extraLength + entryCommentLength;
  const name = bytes.subarray(nameStart, nameStart + nameLength).toString("utf8");
  const unixMode = externalAttributes >>> 16;
  if (
    centralEnd !== eocdOffset || diskStart !== 0 || (flags & 0x1) !== 0 ||
    !new Set([0, 8]).has(method) || name !== RECEIPT_FILE_NAME ||
    (unixMode & 0o170000) === 0o120000 ||
    uncompressedSize < 2 || uncompressedSize > 1024 * 1024 ||
    localOffset + 30 > centralOffset ||
    bytes.readUInt32LE(localOffset) !== 0x04034b50
  ) {
    throw new Error("publication receipt artifact ZIP entry is invalid");
  }
  const localFlags = bytes.readUInt16LE(localOffset + 6);
  const localMethod = bytes.readUInt16LE(localOffset + 8);
  const localNameLength = bytes.readUInt16LE(localOffset + 26);
  const localExtraLength = bytes.readUInt16LE(localOffset + 28);
  const localNameStart = localOffset + 30;
  const localName = bytes.subarray(
    localNameStart,
    localNameStart + localNameLength,
  ).toString("utf8");
  const dataStart = localNameStart + localNameLength + localExtraLength;
  const dataEnd = dataStart + compressedSize;
  if (
    localFlags !== flags || localMethod !== method || localName !== name ||
    dataEnd > centralOffset
  ) {
    throw new Error("publication receipt artifact ZIP local entry differs from its directory");
  }
  const compressed = bytes.subarray(dataStart, dataEnd);
  const payload = method === 0 ? Buffer.from(compressed) : inflateRawSync(compressed);
  if (
    payload.length !== uncompressedSize ||
    crc32(payload) !== expectedCrc32
  ) {
    throw new Error("publication receipt artifact ZIP payload integrity is invalid");
  }
  const raw = payload.toString("utf8");
  if (!Buffer.from(raw, "utf8").equals(payload)) {
    throw new Error("publication receipt artifact is not canonical UTF-8");
  }
  let receipt;
  try {
    receipt = JSON.parse(raw);
  } catch {
    throw new Error("publication receipt artifact is not valid JSON");
  }
  if (raw !== `${canonicalJson(receipt)}\n`) {
    throw new Error("publication receipt artifact bytes are not canonical JSON");
  }
  return Object.freeze({
    receipt,
    fileSha256: createHash("sha256").update(payload).digest("hex"),
    archiveSha256,
  });
}

export function extractReleasePublicationEvidenceFromArtifactZip(
  archive,
  expectedArtifactDigest,
) {
  const extracted = extractClosedCanonicalJsonArtifactZip(archive, expectedArtifactDigest, [
    FINAL_EVIDENCE_FILE_NAME,
    RECEIPT_FILE_NAME,
  ], "publication final evidence");
  const report = extracted.entries.get(FINAL_EVIDENCE_FILE_NAME);
  const receipt = extracted.entries.get(RECEIPT_FILE_NAME);
  return Object.freeze({
    report: report.value,
    reportFileSha256: report.fileSha256,
    receipt: receipt.value,
    receiptFileSha256: receipt.fileSha256,
    archiveSha256: extracted.archiveSha256,
  });
}

export function extractClosedCanonicalJsonArtifactZip(
  archive,
  expectedArtifactDigest,
  expectedNames,
  label,
) {
  const bytes = Buffer.isBuffer(archive) ? archive : Buffer.from(archive ?? []);
  if (bytes.length < 22 || bytes.length > MAXIMUM_RECEIPT_ARCHIVE_BYTES) {
    throw new Error(`${label} artifact ZIP size is invalid`);
  }
  const archiveSha256 = createHash("sha256").update(bytes).digest("hex");
  if (
    requirePattern(expectedArtifactDigest, ARTIFACT_DIGEST, `${label} artifact digest`) !==
    `sha256:${archiveSha256}`
  ) {
    throw new Error(`downloaded ${label} ZIP differs from the artifact API digest`);
  }
  const eocdOffset = findEndOfCentralDirectory(bytes);
  const entryCount = expectedNames.length;
  const centralSize = bytes.readUInt32LE(eocdOffset + 12);
  const centralOffset = bytes.readUInt32LE(eocdOffset + 16);
  const commentLength = bytes.readUInt16LE(eocdOffset + 20);
  if (
    bytes.readUInt16LE(eocdOffset + 4) !== 0 ||
    bytes.readUInt16LE(eocdOffset + 6) !== 0 ||
    bytes.readUInt16LE(eocdOffset + 8) !== entryCount ||
    bytes.readUInt16LE(eocdOffset + 10) !== entryCount ||
    eocdOffset + 22 + commentLength !== bytes.length ||
    centralOffset + centralSize !== eocdOffset
  ) {
    throw new Error(`${label} artifact ZIP central directory is not closed`);
  }
  const expected = new Set(expectedNames);
  const entries = new Map();
  const ranges = [];
  let centralCursor = centralOffset;
  for (let index = 0; index < entryCount; index += 1) {
    if (
      centralCursor + 46 > eocdOffset ||
      bytes.readUInt32LE(centralCursor) !== 0x02014b50
    ) {
      throw new Error(`${label} artifact ZIP central entry is invalid`);
    }
    const flags = bytes.readUInt16LE(centralCursor + 8);
    const method = bytes.readUInt16LE(centralCursor + 10);
    const expectedCrc32 = bytes.readUInt32LE(centralCursor + 16);
    const compressedSize = bytes.readUInt32LE(centralCursor + 20);
    const uncompressedSize = bytes.readUInt32LE(centralCursor + 24);
    const nameLength = bytes.readUInt16LE(centralCursor + 28);
    const extraLength = bytes.readUInt16LE(centralCursor + 30);
    const entryCommentLength = bytes.readUInt16LE(centralCursor + 32);
    const diskStart = bytes.readUInt16LE(centralCursor + 34);
    const externalAttributes = bytes.readUInt32LE(centralCursor + 38);
    const localOffset = bytes.readUInt32LE(centralCursor + 42);
    const nameStart = centralCursor + 46;
    const centralEnd = nameStart + nameLength + extraLength + entryCommentLength;
    const name = bytes.subarray(nameStart, nameStart + nameLength).toString("utf8");
    const unixMode = externalAttributes >>> 16;
    if (
      centralEnd > eocdOffset || diskStart !== 0 || (flags & 0x1) !== 0 ||
      (flags & ~(0x800 | 0x8)) !== 0 || !new Set([0, 8]).has(method) ||
      !expected.has(name) || entries.has(name) ||
      (unixMode & 0o170000) === 0o120000 ||
      uncompressedSize < 2 || uncompressedSize > 1024 * 1024 ||
      localOffset + 30 > centralOffset ||
      bytes.readUInt32LE(localOffset) !== 0x04034b50
    ) {
      throw new Error(`${label} artifact ZIP entry is invalid`);
    }
    const localFlags = bytes.readUInt16LE(localOffset + 6);
    const localMethod = bytes.readUInt16LE(localOffset + 8);
    const localCrc32 = bytes.readUInt32LE(localOffset + 14);
    const localCompressedSize = bytes.readUInt32LE(localOffset + 18);
    const localUncompressedSize = bytes.readUInt32LE(localOffset + 22);
    const localNameLength = bytes.readUInt16LE(localOffset + 26);
    const localExtraLength = bytes.readUInt16LE(localOffset + 28);
    const localNameStart = localOffset + 30;
    const localName = bytes.subarray(
      localNameStart,
      localNameStart + localNameLength,
    ).toString("utf8");
    const dataStart = localNameStart + localNameLength + localExtraLength;
    const dataEnd = dataStart + compressedSize;
    if (
      localFlags !== flags || localMethod !== method || localName !== name ||
      dataEnd > centralOffset ||
      ((flags & 0x8) === 0 &&
        (localCrc32 !== expectedCrc32 ||
          localCompressedSize !== compressedSize ||
          localUncompressedSize !== uncompressedSize))
    ) {
      throw new Error(`${label} artifact ZIP local entry differs from its directory`);
    }
    let rangeEnd = dataEnd;
    if ((flags & 0x8) !== 0) {
      const hasSignature = dataEnd + 4 <= centralOffset &&
        bytes.readUInt32LE(dataEnd) === 0x08074b50;
      const descriptor = dataEnd + (hasSignature ? 4 : 0);
      if (
        descriptor + 12 > centralOffset ||
        bytes.readUInt32LE(descriptor) !== expectedCrc32 ||
        bytes.readUInt32LE(descriptor + 4) !== compressedSize ||
        bytes.readUInt32LE(descriptor + 8) !== uncompressedSize
      ) {
        throw new Error(`${label} artifact ZIP data descriptor is invalid`);
      }
      rangeEnd = descriptor + 12;
    }
    const compressed = bytes.subarray(dataStart, dataEnd);
    const payload = method === 0 ? Buffer.from(compressed) : inflateRawSync(compressed);
    if (payload.length !== uncompressedSize || crc32(payload) !== expectedCrc32) {
      throw new Error(`${label} artifact ZIP payload integrity is invalid`);
    }
    const raw = payload.toString("utf8");
    if (!Buffer.from(raw, "utf8").equals(payload)) {
      throw new Error(`${label} artifact entry is not canonical UTF-8`);
    }
    let value;
    try {
      value = JSON.parse(raw);
    } catch {
      throw new Error(`${label} artifact entry is not valid JSON`);
    }
    if (raw !== `${canonicalJson(value)}\n`) {
      throw new Error(`${label} artifact entry bytes are not canonical JSON`);
    }
    entries.set(name, Object.freeze({
      value,
      fileSha256: createHash("sha256").update(payload).digest("hex"),
    }));
    ranges.push([localOffset, rangeEnd]);
    centralCursor = centralEnd;
  }
  ranges.sort((left, right) => left[0] - right[0]);
  let localCursor = 0;
  for (const [start, end] of ranges) {
    if (start !== localCursor || end < start) {
      throw new Error(`${label} artifact ZIP contains unreferenced local bytes`);
    }
    localCursor = end;
  }
  if (
    centralCursor !== eocdOffset || localCursor !== centralOffset ||
    entries.size !== expected.size
  ) {
    throw new Error(`${label} artifact ZIP regular-file set is incomplete`);
  }
  return Object.freeze({ entries, archiveSha256 });
}

function findEndOfCentralDirectory(bytes) {
  const minimum = Math.max(0, bytes.length - 65_557);
  for (let offset = bytes.length - 22; offset >= minimum; offset -= 1) {
    if (bytes.readUInt32LE(offset) === 0x06054b50) return offset;
  }
  throw new Error("publication receipt artifact is not a bounded ZIP archive");
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ ((crc & 1) === 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

async function readCanonicalJsonFile(path, label, expectedFileSha256) {
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
    throw new Error(`${label} is not valid JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  const fileSha256 = createHash("sha256").update(raw, "utf8").digest("hex");
  if (
    expectedFileSha256 !== undefined &&
    fileSha256 !== requireSha256(expectedFileSha256, `${label} raw file SHA-256`)
  ) {
    throw new Error(`${label} raw file SHA-256 differs from its authority`);
  }
  return Object.freeze({ value, fileSha256 });
}

async function writeCanonicalJsonNew(path, value) {
  const target = resolve(requireNonEmptyString(path, "publication evidence output path"));
  await assertSafeDirectoryChain(dirname(target));
  const raw = `${canonicalJson(value)}\n`;
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(raw, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  return createHash("sha256").update(raw, "utf8").digest("hex");
}

async function writePublicationEvidenceBundleNew(directory, bundle) {
  const target = resolve(
    requireNonEmptyString(directory, "publication evidence output directory"),
  );
  const parent = dirname(target);
  await assertSafeDirectoryChain(parent);
  try {
    await lstat(target);
    throw new Error("publication evidence output directory already exists");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  const temporary = `${target}.next-${process.pid}-${randomUUID()}`;
  const receiptPath = resolve(temporary, RECEIPT_FILE_NAME);
  const reportPath = resolve(temporary, FINAL_EVIDENCE_FILE_NAME);
  const authorityPath = resolve(temporary, FINAL_AUTHORITY_FILE_NAME);
  try {
    await mkdir(temporary, { mode: 0o700 });
    const receiptFileSha256 = await writeCanonicalJsonNew(receiptPath, bundle.receipt);
    if (receiptFileSha256 !== bundle.receiptFileSha256) {
      throw new Error("persisted publication receipt differs from the downloaded artifact bytes");
    }
    const reportFileSha256 = await writeCanonicalJsonNew(reportPath, bundle.report);
    const authorityFileSha256 = bundle.authority === undefined
      ? undefined
      : await writeCanonicalJsonNew(authorityPath, bundle.authority);
    await rename(temporary, target);
    return Object.freeze({
      receiptFileSha256,
      reportFileSha256,
      authorityFileSha256,
    });
  } catch (error) {
    await unlink(receiptPath).catch(() => undefined);
    await unlink(reportPath).catch(() => undefined);
    await unlink(authorityPath).catch(() => undefined);
    await rmdir(temporary).catch(() => undefined);
    throw error;
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("publication evidence path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function requireExactCliOptions(values, mode, expected) {
  const actual = Object.keys(values).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((option, index) => option !== wanted[index]) ||
    wanted.some((option) => typeof values[option] !== "string" || values[option].length === 0)
  ) {
    throw new Error(`${mode} options differ from the closed publication CLI contract`);
  }
}

function githubActionsDependencies(expectedEvent) {
  if (
    process.env.GITHUB_ACTIONS !== "true" ||
    (expectedEvent !== undefined && process.env.GITHUB_EVENT_NAME !== expectedEvent)
  ) {
    throw new Error(
      `${expectedEvent ?? "release"} publication evidence collection is restricted to GitHub Actions`,
    );
  }
  return Object.freeze({
    apiUrl: requireNonEmptyString(process.env.GITHUB_API_URL, "GitHub API URL"),
    token: requireNonEmptyString(process.env.GH_TOKEN, "GH_TOKEN"),
  });
}

async function main() {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      repository: { type: "string" },
      tag: { type: "string" },
      "source-commit": { type: "string" },
      "workflow-run-id": { type: "string" },
      "workflow-run-attempt": { type: "string" },
      "finalizer-workflow-run-id": { type: "string" },
      "finalizer-workflow-run-attempt": { type: "string" },
      "acceptance-evidence": { type: "string" },
      products: { type: "string" },
      receipt: { type: "string" },
      "receipt-file-sha256": { type: "string" },
      report: { type: "string" },
      "output-directory": { type: "string" },
      output: { type: "string" },
    },
    strict: true,
  });
  const mode = positionals[0];
  if (positionals.length !== 1 || !new Set(["capture", "recover", "finalize", "resolve", "verify"]).has(mode)) {
    throw new Error("usage: release-publication-evidence.mjs (capture|recover|finalize|resolve|verify) [closed options]");
  }
  const common = {
    repository: values.repository,
    tag: values.tag,
    sourceCommit: values["source-commit"],
    workflowRunId: values["workflow-run-id"],
    workflowRunAttempt: values["workflow-run-attempt"],
    finalizerWorkflowRunId: values["finalizer-workflow-run-id"],
    finalizerWorkflowRunAttempt: values["finalizer-workflow-run-attempt"],
  };
  validateAuthority(common);
  if (mode === "recover") {
    requireExactCliOptions(values, "recover", [
      "repository", "tag", "source-commit", "workflow-run-id",
      "workflow-run-attempt", "acceptance-evidence", "products",
    ]);
    const acceptanceInput = await readCanonicalJsonFile(
      values["acceptance-evidence"],
      "canonical acceptance evidence",
    );
    const result = await recoverReleasePublication({
      ...common,
      acceptanceEvidence: acceptanceInput.value,
      productsDirectory: values.products,
    }, githubActionsDependencies("push"));
    process.stdout.write(
      `release_publication_recovery=${result.status} asset_count=${result.recovered_asset_count}\n`,
    );
    return;
  }
  if (mode === "capture") {
    requireExactCliOptions(values, "capture", [
      "repository", "tag", "source-commit", "workflow-run-id",
      "workflow-run-attempt", "acceptance-evidence", "products", "output",
    ]);
    const acceptanceInput = await readCanonicalJsonFile(
      values["acceptance-evidence"],
      "canonical acceptance evidence",
    );
    const receipt = await collectReleasePublicationReceipt({
      ...common,
      acceptanceEvidence: acceptanceInput.value,
      productsDirectory: values.products,
    }, githubActionsDependencies("push"));
    const fileSha256 = await writeCanonicalJsonNew(values.output, receipt);
    process.stdout.write(
      `${RELEASE_PUBLICATION_RECEIPT_SCHEMA_ID} ${receipt.report_sha256} ${fileSha256}\n`,
    );
    return;
  }
  if (mode === "finalize") {
    requireExactCliOptions(values, "finalize", [
      "repository", "tag", "source-commit", "workflow-run-id",
      "workflow-run-attempt", "finalizer-workflow-run-id",
      "finalizer-workflow-run-attempt", "output-directory",
    ]);
    const actionDependencies = githubActionsDependencies("workflow_run");
    const finalizerAttempt = await inspectReleasePublicationFinalizerAttempt(
      common,
      actionDependencies,
    );
    if (finalizerAttempt.skipBecausePriorSuccess) {
      process.stdout.write("upload_required=false\n");
      return;
    }
    const bundle = await collectReleasePublicationEvidenceBundle({
      ...common,
    }, {
      ...actionDependencies,
      finalizerAttemptAuthority: finalizerAttempt,
    });
    const hashes = await writePublicationEvidenceBundleNew(
      values["output-directory"],
      bundle,
    );
    process.stdout.write(
      [
        "upload_required=true",
        `artifact_name=${expectedReleasePublicationEvidenceArtifactName(common)}`,
        `report_sha256=${bundle.report.report_sha256}`,
        `report_file_sha256=${hashes.reportFileSha256}`,
        `receipt_file_sha256=${hashes.receiptFileSha256}`,
      ].join("\n") + "\n",
    );
    return;
  }
  if (mode === "resolve") {
    requireExactCliOptions(values, "resolve", [
      "repository", "tag", "source-commit", "workflow-run-id",
      "workflow-run-attempt", "output-directory",
    ]);
    const bundle = await resolveReleasePublicationFinalAuthority(
      common,
      createGithubCliPublicationDependencies({ repository: common.repository }),
    );
    const hashes = await writePublicationEvidenceBundleNew(
      values["output-directory"],
      bundle,
    );
    process.stdout.write(
      [
        RELEASE_PUBLICATION_FINAL_AUTHORITY_SCHEMA_ID,
        bundle.authority.report_sha256,
        hashes.authorityFileSha256,
        hashes.reportFileSha256,
        hashes.receiptFileSha256,
      ].join(" ") + "\n",
    );
    return;
  }
  const receiptInput = await readCanonicalJsonFile(
    values.receipt,
    "release publication receipt",
    values["receipt-file-sha256"],
  );
  requireExactCliOptions(values, "verify", [
    "repository", "tag", "source-commit", "workflow-run-id",
    "workflow-run-attempt", "acceptance-evidence", "receipt",
    "receipt-file-sha256", "report",
  ]);
  const acceptanceInput = await readCanonicalJsonFile(
    values["acceptance-evidence"],
    "canonical acceptance evidence",
  );
  const reportInput = await readCanonicalJsonFile(
    values.report,
    "release publication evidence",
  );
  validateReleasePublicationEvidence(reportInput.value, {
    expectedRepository: common.repository,
    expectedSourceCommit: common.sourceCommit,
    expectedWorkflowRunId: common.workflowRunId,
    expectedWorkflowRunAttempt: common.workflowRunAttempt,
    acceptanceEvidence: acceptanceInput.value,
    receipt: receiptInput.value,
    receiptFileSha256: receiptInput.fileSha256,
  });
  process.stdout.write(
    `${RELEASE_PUBLICATION_EVIDENCE_SCHEMA_ID} ${reportInput.value.report_sha256}\n`,
  );
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `release_publication_evidence=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}

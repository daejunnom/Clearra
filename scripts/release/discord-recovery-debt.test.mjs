import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { canonicalJson, canonicalSha256, sealCanonicalReport } from "./canonical-release-evidence.mjs";
import {
  DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID,
  DISCORD_RECOVERY_RESULT_SCHEMA_ID,
  validateDiscordCheckpointCandidatePrerequisites,
} from "./discord-deployment-recovery.mjs";
import {
  ACCEPTED_RUN_ATTEMPT as CHECKPOINT_ACCEPTED_ATTEMPT,
  ACCEPTED_RUN_ID as CHECKPOINT_ACCEPTED_RUN_ID,
  candidateFixture,
  checkpointJobList,
  completedCheckpointJobList,
  identity as checkpointCandidateIdentity,
  RUN_ATTEMPT as CHECKPOINT_RUN_ATTEMPT,
  RUN_ID as CHECKPOINT_RUN_ID,
  SOURCE as CHECKPOINT_SOURCE,
} from "./discord-production-checkpoint-receipt.test.mjs";
import {
  auditDiscordRecoveryDebt,
  DISCORD_RECOVERY_DEBT_CLEARANCE_SCHEMA_ID,
  planDiscordRecoveryDebt,
} from "./discord-recovery-debt.mjs";
import {
  DISCORD_PRODUCTION_CHECKPOINT_RECEIPT_SCHEMA_ID,
  materializeCompletedJobTopology,
  validateDiscordProductionCheckpointReceipt,
} from "./finalize-discord-production-checkpoint.mjs";

const REPOSITORY = "daejunnom/Clearra";
const FAILED_SOURCE = "a".repeat(40);
const CURRENT_SOURCE = "b".repeat(40);
const RECOVERY_SOURCE = "c".repeat(40);
const PRESTAGE_UPLOAD = "Upload prestage authority before Oracle freeze or Cloud zero traffic";
const LIVE_UPLOAD = "Upload live-transition authority before Oracle activation or Cloud traffic";
const MUTATIONS = [
  "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
  "Activate Oracle, verify the real path, then cut Cloud to 100 percent",
];
const CATALOG_CAPTURE = "Capture and seal Discord catalog recovery authority before mutation";
const CATALOG_UPLOAD = "Upload Discord catalog recovery authority before global mutation";
const CATALOG_MUTATION =
  "Authority-bound global sync and sole canonical four-surface observation";

function primaryRun(overrides = {}) {
  return {
    id: 100,
    run_attempt: 1,
    run_number: 10,
    name: "Deploy Discord Production",
    path: ".github/workflows/discord-deploy.yml",
    event: "workflow_dispatch",
    head_branch: "main",
    head_sha: FAILED_SOURCE,
    status: "completed",
    conclusion: "failure",
    created_at: "2026-08-31T00:00:00Z",
    run_started_at: "2026-08-31T00:00:01Z",
    updated_at: "2026-08-31T00:00:10Z",
    repository: { id: 1309293231, full_name: REPOSITORY },
    head_repository: { id: 1309293231, full_name: REPOSITORY },
    ...overrides,
  };
}

function currentRun(overrides = {}) {
  return primaryRun({
    id: 200,
    run_number: 20,
    head_sha: CURRENT_SOURCE,
    status: "in_progress",
    conclusion: null,
    created_at: "2026-08-31T01:00:00Z",
    run_started_at: "2026-08-31T01:00:01Z",
    updated_at: "2026-08-31T01:00:02Z",
    ...overrides,
  });
}

function recoveryRun(overrides = {}) {
  return {
    id: 300,
    run_attempt: 1,
    run_number: 30,
    name: "Recover Discord Production",
    path: ".github/workflows/discord-deploy-recovery.yml",
    event: "workflow_run",
    head_branch: "main",
    head_sha: RECOVERY_SOURCE,
    status: "completed",
    conclusion: "success",
    created_at: "2026-08-31T00:01:00Z",
    run_started_at: "2026-08-31T00:01:01Z",
    updated_at: "2026-08-31T00:01:20Z",
    repository: { id: 1309293231, full_name: REPOSITORY },
    head_repository: { id: 1309293231, full_name: REPOSITORY },
    ...overrides,
  };
}

function resolutionArtifact(overrides = {}) {
  return {
    id: 400,
    name: "discord-recovery-resolution-run-100-attempt-1-recovery-run-300-attempt-1",
    digest: `sha256:${"d".repeat(64)}`,
    created_at: "2026-08-31T00:01:10Z",
    expired: false,
    size_in_bytes: 1024,
    workflow_run: {
      id: 300,
      repository_id: 1309293231,
      head_repository_id: 1309293231,
      head_sha: RECOVERY_SOURCE,
      head_branch: "main",
    },
    ...overrides,
  };
}

function resultArtifact(overrides = {}) {
  return resolutionArtifact({
    id: 401,
    name: `discord-runtime-recovery-${FAILED_SOURCE}-source-run-100-attempt-1-recovery-run-300-attempt-1`,
    digest: `sha256:${"e".repeat(64)}`,
    created_at: "2026-08-31T00:01:18Z",
    ...overrides,
  });
}

function notRequiredCatalogDisposition({ recoveryRunId = "300", recoveryAttempt = "1" } = {}) {
  return sealCanonicalReport({
    schema_id: "clearra.discord-catalog-recovery-disposition.v1",
    repository: REPOSITORY,
    source_commit: FAILED_SOURCE,
    original_workflow_run_id: "100",
    original_workflow_run_attempt: "1",
    recovery_workflow_run_id: recoveryRunId,
    recovery_workflow_run_attempt: recoveryAttempt,
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
  });
}

async function writeReportArtifact(root, artifactId, leaf, report) {
  const directory = join(root, String(artifactId));
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, leaf), `${canonicalJson(report)}\n`, { mode: 0o600 });
}

function catalogs(artifacts = [resolutionArtifact()]) {
  return {
    runList: { total_count: 2, workflow_runs: [currentRun(), primaryRun()] },
    primaryAttempts: {
      schema_id: "clearra.discord-primary-attempt-catalog.v1",
      attempts: [primaryRun(), currentRun()],
    },
    recoveryAttempts: {
      schema_id: "clearra.discord-recovery-attempt-catalog.v1",
      attempts: [recoveryRun()],
    },
    artifactPages: { total_count: artifacts.length, artifacts },
  };
}

const identity = {
  repository: REPOSITORY,
  workflowRunId: "200",
  workflowRunAttempt: "1",
  sourceCommit: CURRENT_SOURCE,
  reachableTags: [],
  tagAuthority: {
    schema_id: "clearra.discord-production-tag-authority-catalog.v1",
    tags: [],
  },
  recoveryJobCatalog: {
    schema_id: "clearra.discord-recovery-job-catalog.v1",
    runs: [],
  },
  bootstrapProof: {
    bootstrap_commit: "b1a56bc15b8d6decd1bcfc1b49163e0542e36cd6",
    bootstrap_committed_at: "2026-08-30T17:12:23Z",
    current_source_contains_bootstrap: true,
    discord_deploy_workflow_absent: true,
  },
};

function recoveryJobCatalog(runtime, overrides = {}) {
  const authoritySteps = [
    "Set up job", "Check out trusted recovery source for authority resolution",
    "Set up Node.js for recovery authority resolution",
    "Resolve exact original run attempt, freshness, and staged artifacts",
    "Record the proven no-Oracle-or-Cloud-runtime-mutation terminal case",
    "Upload exact recovery resolution",
    "Post Set up Node.js for recovery authority resolution",
    "Post Check out trusted recovery source for authority resolution", "Complete job",
  ];
  const recoverSteps = [
    "Set up job", "Check out trusted recovery source for protected restore",
    "Set up Node.js for protected recovery validation",
    "Re-resolve exact recovery authority immediately before protected mutation",
    "Hard-verify the exact artifact ZIP digest from the REST authority",
    "Download the already hash-verified exact artifact ID into runner temp",
    "Hard-verify the exact Discord catalog recovery ZIP and closed leaves",
    "Download the hash-verified Discord catalog recovery artifact",
    "Authenticate the rollback-only identity through its separate provider",
    "Set up gcloud for protected rollback", "Materialize the reviewer-protected Oracle recovery key",
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
  ];
  const makeSteps = (names, recover = false) => names.map((name, index) => ({
    name,
    number: index + 1,
    status: "completed",
    conclusion: [
      ...(runtime ? ["Record the proven no-Oracle-or-Cloud-runtime-mutation terminal case"] : []),
      ...(recover ? [
        "Hard-verify the exact Discord catalog recovery ZIP and closed leaves",
        "Download the hash-verified Discord catalog recovery artifact",
        "Authenticate command sync for reviewer-protected catalog recovery",
        "Post Authenticate command sync for reviewer-protected catalog recovery",
        "Retry catalog recovery after an ordinary failure or cancellation",
        "Retry exact recovery after an ordinary step failure or cancellation",
        "Fail closed unless the canonical recovery result was verified and uploaded",
      ] : []),
    ].includes(name) ? "skipped" : "success",
    started_at: new Date(Date.parse("2026-08-31T00:01:01Z") + 100 + index * 400)
      .toISOString(),
    completed_at: new Date(Date.parse("2026-08-31T00:01:01Z") + 200 + index * 400)
      .toISOString(),
  }));
  const jobs = [
    {
      id: 910, run_id: 300, run_attempt: 1, head_sha: RECOVERY_SOURCE, head_branch: "main",
      status: "completed", conclusion: "success", name: "authority",
      html_url: `https://github.com/${REPOSITORY}/actions/runs/300/job/910`,
      steps: makeSteps(authoritySteps),
    },
    {
      id: 911, run_id: 300, run_attempt: 1, head_sha: RECOVERY_SOURCE, head_branch: "main",
      status: "completed", conclusion: runtime ? "success" : "skipped", name: "recover",
      html_url: `https://github.com/${REPOSITORY}/actions/runs/300/job/911`,
      steps: runtime ? makeSteps(recoverSteps, true) : [],
    },
  ];
  return {
    schema_id: "clearra.discord-recovery-job-catalog.v1",
    runs: [{ workflow_run_id: "300", workflow_run_attempt: "1", pages: [{ total_count: 2, jobs }] }],
    ...overrides,
  };
}

function debtIdentity(runtime = false, overrides = {}) {
  return { ...identity, recoveryJobCatalog: recoveryJobCatalog(runtime), ...overrides };
}

function noOpResolution(overrides = {}) {
  return sealCanonicalReport({
    schema_id: DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID,
    repository: REPOSITORY,
    primary_workflow_name: "Deploy Discord Production",
    primary_workflow_path: ".github/workflows/discord-deploy.yml",
    source_commit: FAILED_SOURCE,
    workflow_run_id: "100",
    workflow_run_attempt: "1",
    recovery_workflow_run_id: "300",
    recovery_workflow_run_attempt: "1",
    workflow_event: "workflow_dispatch",
    workflow_conclusion: "failure",
    recovery_required: false,
    recovery_stage: "none",
    recovery_reason: "job-steps-prove-no-prestage-upload-or-runtime-mutation",
    artifact_id: null,
    artifact_name: `discord-prestage-recovery-authority-${FAILED_SOURCE}-run-100-attempt-1`,
    artifact_digest: null,
    artifact_size: null,
    artifact_created_at: null,
    freshness_proof: {
      schema_id: "clearra.discord-deployment-recovery-freshness-proof.v1",
      original_workflow_run_id: "100",
      original_workflow_run_attempt: "1",
      original_source_commit: FAILED_SOURCE,
      potential_superseders: [],
    },
    no_mutation_job_step_proof: noMutationProof(),
    prestage_only_job_step_proof: null,
    live_transition_job_step_proof: null,
    catalog_recovery_required: false,
    catalog_artifact_id: null,
    catalog_artifact_name:
      `discord-catalog-recovery-authority-${FAILED_SOURCE}-run-100-attempt-1`,
    catalog_artifact_digest: null,
    catalog_artifact_size: null,
    catalog_artifact_created_at: null,
    catalog_mutation_job_step_proof: {
      job_id: "903",
      job_name: "sync-observe",
      job_status: "completed",
      job_conclusion: "skipped",
      capture_step: null,
      upload_step: null,
      mutation_step: null,
    },
    ...overrides,
  });
}

function noMutationProof() {
  const prestage = {
    name: PRESTAGE_UPLOAD, number: 9, status: "completed", conclusion: "failure",
  };
  return {
    job_id: "900",
    job_name: "promote",
    job_status: "completed",
    job_conclusion: "failure",
    prestage_upload_step: prestage,
    live_upload_step: null,
    runtime_mutation_steps: MUTATIONS.map((name) => ({
      name, number: null, status: null, conclusion: null,
    })),
    primary_jobs: [
      { job_id: "902", job_name: "authority", job_status: "completed", job_conclusion: "success", steps: [] },
      { job_id: "901", job_name: "Prepare immutable Discord candidate inputs", job_status: "completed", job_conclusion: "success", steps: [] },
      { job_id: "900", job_name: "promote", job_status: "completed", job_conclusion: "failure", steps: [prestage] },
      { job_id: "903", job_name: "sync-observe", job_status: "completed", job_conclusion: "skipped", steps: [] },
    ],
  };
}

function liveTransitionProof() {
  const proof = noMutationProof();
  const prestage = { name: PRESTAGE_UPLOAD, number: 9, status: "completed", conclusion: "success" };
  const stage = { name: MUTATIONS[0], number: 10, status: "completed", conclusion: "success" };
  const live = { name: LIVE_UPLOAD, number: 11, status: "completed", conclusion: "success" };
  proof.prestage_upload_step = prestage;
  proof.live_upload_step = live;
  proof.runtime_mutation_steps = [
    stage,
    { name: MUTATIONS[1], number: null, status: null, conclusion: null },
  ];
  proof.primary_jobs.find((job) => job.job_name === "promote").steps = [prestage, stage, live];
  return proof;
}

function resultBindings(stage) {
  const names = stage === "live" ? [
    "candidate_state", "cloud_candidate_residue_readback",
    "cloud_pre_mutation_classification", "cloud_prior_authority",
    "cloud_restore_readback", "oracle_pre_mutation_classification",
    "oracle_restore_attestation", "oracle_rollback_capture", "oracle_stage_manifest",
    "prestage_state", "recovery_authority",
  ] : [
    "cloud_candidate_residue_readback", "cloud_cleanup_readback",
    "cloud_pre_mutation_readback", "intended_candidate_authority",
    "oracle_backup_cleanup", "oracle_inactive_cleanup", "oracle_rollback_capture",
    "prestage_state", "recovery_authority",
  ];
  return names.map((name) => ({ name, file_sha256: "0".repeat(64), size: 1 }));
}

const JOB_STEPS = {
  authority: [
    "Set up job", "Check out main for exact authority resolution",
    "Resolve exact current main and one canonical acceptance",
    "Classify deployment impact from the last production tag",
    "Reject unresolved earlier Discord recovery debt before candidate work",
    "Upload exact recovery-debt clearance before candidate work",
    "Record explicit no-op for changes outside Discord",
    "Post Check out main for exact authority resolution", "Complete job",
  ],
  "Prepare immutable Discord candidate inputs": [
    "Set up job", "Check out the exact accepted source for candidate preparation",
    "Set up Node.js for immutable candidate preparation",
    "Download the exact pre-candidate recovery-debt clearance",
    "Authenticate the Cloud-Build-only identity",
    "Set up gcloud for Cloud-Build-only preparation",
    "Download canonical acceptance evidence without rebuilding products",
    "Download the already accepted CTK3 distribution",
    "Verify accepted evidence and package only runtime dependencies",
    "Build the exact source archive once in Cloud Build",
    "Seal the approval-free accepted-input and immutable-build state",
    "Upload sealed prepared state", "Post Authenticate the Cloud-Build-only identity",
    "Post Set up Node.js for immutable candidate preparation",
    "Post Check out the exact accepted source for candidate preparation", "Complete job",
  ],
  promote: [
    "Set up job", "Check out the exact accepted source for protected promotion",
    "Set up Node.js for protected release validators", "Download the exact prepared state",
    "Authenticate the protected deployer identity", "Set up gcloud for protected promotion",
    "Materialize the protected-environment Oracle key for the real path gate",
    "Capture and seal prestage recovery authority", PRESTAGE_UPLOAD, MUTATIONS[0],
    LIVE_UPLOAD, MUTATIONS[1], "Upload sealed promoted state",
    "Compensate any protected-path failure after Oracle transition began",
    "Always remove the temporary Oracle key",
    "Post Authenticate the protected deployer identity",
    "Post Set up Node.js for protected release validators",
    "Post Check out the exact accepted source for protected promotion", "Complete job",
  ],
  "sync-observe": [
    "Set up job", "Check out the exact accepted source for global synchronization",
    "Set up Node.js for global synchronization",
    "Install the frozen runtime dependencies for synchronization",
    "Download the exact protected promotion evidence",
    "Resolve one successful exact-SHA Pages deployment before global mutation",
    "Download the exact Pages deployment authority",
    "Verify Pages authority before Discord global mutation",
    "Authenticate command sync without Cloud mutation authority",
    "Set up gcloud for command synchronization",
    "Materialize the Oracle key for read-only observation",
    CATALOG_CAPTURE,
    CATALOG_UPLOAD,
    "Authority-bound global sync and sole canonical four-surface observation",
    "Upload durable sync and sole canonical observation evidence",
    "Capture exact completed deployment prerequisites for the checkpoint candidate",
    "Seal canonical Discord production checkpoint candidate",
    "Upload canonical Discord production checkpoint candidate",
    "Compensate catalog mutation if any later sync job step failed",
    "Upload durable catalog compensation evidence", "Always remove the temporary Oracle key",
    "Post Authenticate command sync without Cloud mutation authority",
    "Post Set up Node.js for global synchronization",
    "Post Check out the exact accepted source for global synchronization", "Complete job",
  ],
};

const CHECKPOINT_ARTIFACT_ID = "9901";
const CHECKPOINT_ARTIFACT_DIGEST = `sha256:${"6".repeat(64)}`;
const CHECKPOINT_TAG_OBJECT_SHA = "e".repeat(40);
const CHECKPOINT_TAGGER = Object.freeze({
  name: "github-actions[bot]",
  email: "41898282+github-actions[bot]@users.noreply.github.com",
  date: "2026-08-31T02:51:00Z",
});
const GITHUB_ACTIONS_BOT = Object.freeze({
  login: "github-actions[bot]",
  id: 41898282,
  type: "Bot",
  site_admin: false,
  url: "https://api.github.com/users/github-actions%5Bbot%5D",
  html_url: "https://github.com/apps/github-actions",
});

const CHECKPOINT_PREREQUISITE_PROOF = validateDiscordCheckpointCandidatePrerequisites(
  checkpointJobList(),
  checkpointCandidateIdentity(),
);
const CHECKPOINT_CANDIDATE = await candidateFixture(CHECKPOINT_PREREQUISITE_PROOF);
const CHECKPOINT_COMPLETED_JOB_API = completedCheckpointJobList();
for (const job of CHECKPOINT_COMPLETED_JOB_API.jobs) {
  const timestamps = job.steps.flatMap((step) => [step.started_at, step.completed_at])
    .filter((value) => value !== null)
    .map(Date.parse);
  job.started_at = new Date(Math.min(...timestamps)).toISOString();
  job.completed_at = new Date(Math.max(...timestamps)).toISOString();
}
const CHECKPOINT_WORKFLOW_STARTED_AT = new Date(
  Math.min(...CHECKPOINT_COMPLETED_JOB_API.jobs.map((job) => Date.parse(job.started_at))) - 1_000,
).toISOString();
const CHECKPOINT_WORKFLOW_COMPLETED_AT = new Date(
  Math.max(...CHECKPOINT_COMPLETED_JOB_API.jobs.map((job) => Date.parse(job.completed_at))) + 1_000,
).toISOString();
const CHECKPOINT_COMPLETED_TOPOLOGY = materializeCompletedJobTopology(
  CHECKPOINT_COMPLETED_JOB_API,
  {
    startedAt: Date.parse(CHECKPOINT_WORKFLOW_STARTED_AT),
    completedAt: Date.parse(CHECKPOINT_WORKFLOW_COMPLETED_AT),
  },
);
const CHECKPOINT_CANDIDATE_UPLOAD = CHECKPOINT_COMPLETED_JOB_API.jobs
  .find((job) => job.name === "sync-observe").steps
  .find((step) => step.name === "Upload canonical Discord production checkpoint candidate");
const CHECKPOINT_ARTIFACT_CREATED_AT = new Date(
  Date.parse(CHECKPOINT_CANDIDATE_UPLOAD.started_at) + 500,
).toISOString();
const CHECKPOINT_CANDIDATE_FILE_SHA256 = createHash("sha256")
  .update(`${canonicalJson(CHECKPOINT_CANDIDATE)}\n`, "utf8")
  .digest("hex");
const CHECKPOINT_RECEIPT = sealCanonicalReport({
  schema_id: DISCORD_PRODUCTION_CHECKPOINT_RECEIPT_SCHEMA_ID,
  repository: REPOSITORY,
  repository_id: "1309293231",
  release: "v0.8.0",
  version: "0.8.0",
  source_commit: CHECKPOINT_SOURCE,
  accepted_workflow_path: ".github/workflows/release-cli.yml",
  accepted_workflow_run_id: CHECKPOINT_ACCEPTED_RUN_ID,
  accepted_workflow_run_attempt: CHECKPOINT_ACCEPTED_ATTEMPT,
  discord_workflow_path: ".github/workflows/discord-deploy.yml",
  discord_workflow_run_id: CHECKPOINT_RUN_ID,
  discord_workflow_run_attempt: CHECKPOINT_RUN_ATTEMPT,
  discord_workflow_started_at: CHECKPOINT_WORKFLOW_STARTED_AT,
  discord_workflow_completed_at: CHECKPOINT_WORKFLOW_COMPLETED_AT,
  checkpoint_candidate_artifact: {
    artifact_id: CHECKPOINT_ARTIFACT_ID,
    artifact_name: CHECKPOINT_CANDIDATE.expected_artifact_name,
    artifact_digest: CHECKPOINT_ARTIFACT_DIGEST,
    artifact_created_at: CHECKPOINT_ARTIFACT_CREATED_AT,
    archive_sha256: CHECKPOINT_ARTIFACT_DIGEST.slice("sha256:".length),
    file_name: CHECKPOINT_CANDIDATE.expected_artifact_leaf,
    file_sha256: CHECKPOINT_CANDIDATE_FILE_SHA256,
    candidate_report_sha256: CHECKPOINT_CANDIDATE.report_sha256,
  },
  checkpoint_candidate: CHECKPOINT_CANDIDATE,
  completed_job_topology: CHECKPOINT_COMPLETED_TOPOLOGY,
  completed_job_topology_sha256: canonicalSha256(CHECKPOINT_COMPLETED_TOPOLOGY),
  tag: {
    name: "v0.8.0",
    target_commit: CHECKPOINT_SOURCE,
    annotated: true,
    message_contract: "exact-canonical-receipt-bytes",
    tagger: CHECKPOINT_TAGGER,
  },
  github_release_contract: {
    tag: "v0.8.0",
    title: "Clearra v0.8.0",
    source_commit: CHECKPOINT_SOURCE,
    draft: false,
    prerelease: false,
    immutable: true,
    asset_count: 3,
    canonical_acceptance_evidence_sha256:
      CHECKPOINT_CANDIDATE.canonical_acceptance_evidence_sha256,
  },
  status: "ready-for-annotated-tag-and-immutable-release",
});
validateDiscordProductionCheckpointReceipt(CHECKPOINT_RECEIPT, {
  repository: REPOSITORY,
  sourceCommit: CHECKPOINT_SOURCE,
  acceptedWorkflowRunId: CHECKPOINT_ACCEPTED_RUN_ID,
  acceptedWorkflowRunAttempt: CHECKPOINT_ACCEPTED_ATTEMPT,
  discordWorkflowRunId: CHECKPOINT_RUN_ID,
  discordWorkflowRunAttempt: CHECKPOINT_RUN_ATTEMPT,
  artifactId: CHECKPOINT_ARTIFACT_ID,
  artifactDigest: CHECKPOINT_ARTIFACT_DIGEST,
  tagger: CHECKPOINT_TAGGER,
});

function checkpointRelease() {
  return {
    id: 700,
    tag_name: "v0.8.0",
    target_commitish: CHECKPOINT_SOURCE,
    name: "Clearra v0.8.0",
    draft: false,
    prerelease: false,
    immutable: true,
    published_at: "2026-08-31T02:52:00Z",
    url: `https://api.github.com/repos/${REPOSITORY}/releases/700`,
    html_url: `https://github.com/${REPOSITORY}/releases/tag/v0.8.0`,
    assets_url: `https://api.github.com/repos/${REPOSITORY}/releases/700/assets`,
    upload_url: `https://uploads.github.com/repos/${REPOSITORY}/releases/700/assets{?name,label}`,
    author: structuredClone(GITHUB_ACTIONS_BOT),
    assets: CHECKPOINT_CANDIDATE.release_artifacts.map((artifact, index) => ({
      id: 800 + index,
      name: artifact.name,
      state: "uploaded",
      size: artifact.size_bytes,
      digest: `sha256:${artifact.sha256}`,
      url: `https://api.github.com/repos/${REPOSITORY}/releases/assets/${800 + index}`,
      browser_download_url:
        `https://github.com/${REPOSITORY}/releases/download/v0.8.0/${artifact.name}`,
      uploader: structuredClone(GITHUB_ACTIONS_BOT),
    })),
  };
}

function checkpointTagAuthority() {
  return {
    schema_id: "clearra.discord-production-tag-authority-catalog.v1",
    tags: [{
      name: "v0.8.0",
      local_tag_object_sha: CHECKPOINT_TAG_OBJECT_SHA,
      local_target_commit: CHECKPOINT_SOURCE,
      tag_ref: {
        ref: "refs/tags/v0.8.0",
        url: `https://api.github.com/repos/${REPOSITORY}/git/refs/tags/v0.8.0`,
        object: { type: "tag", sha: CHECKPOINT_TAG_OBJECT_SHA },
      },
      tag_object: {
        sha: CHECKPOINT_TAG_OBJECT_SHA,
        tag: "v0.8.0",
        message: `${canonicalJson(CHECKPOINT_RECEIPT)}\n`,
        object: { type: "commit", sha: CHECKPOINT_SOURCE },
        tagger: structuredClone(CHECKPOINT_TAGGER),
        url: `https://api.github.com/repos/${REPOSITORY}/git/tags/${CHECKPOINT_TAG_OBJECT_SHA}`,
      },
      release: checkpointRelease(),
    }],
  };
}

function checkpointIdentity(overrides = {}) {
  return {
    ...identity,
    reachableTags: ["v0.8.0"],
    tagAuthority: checkpointTagAuthority(),
    ...overrides,
  };
}

function checkpointAuthorityForReceipt(receipt) {
  const authority = checkpointTagAuthority();
  authority.tags[0].tag_object.message = `${canonicalJson(receipt)}\n`;
  authority.tags[0].tag_object.tagger = structuredClone(receipt.tag.tagger);
  return authority;
}

function reseal(value) {
  const { report_sha256: ignored, ...unsigned } = value;
  void ignored;
  return sealCanonicalReport(unsigned);
}

test("debt gate seals an empty clearance for the first exact deployment attempt", async () => {
  const current = currentRun();
  const plan = planDiscordRecoveryDebt(
    { total_count: 1, workflow_runs: [current] },
    { schema_id: "clearra.discord-primary-attempt-catalog.v1", attempts: [current] },
    { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
    { total_count: 0, artifacts: [] },
    identity,
  );
  assert.deepEqual(plan.debts, []);
  const root = await mkdtemp(join(tmpdir(), "clearra-debt-empty-"));
  try {
    const planPath = join(root, "plan.json");
    await writeFile(planPath, `${JSON.stringify(plan)}\n`);
    const clearance = await auditDiscordRecoveryDebt(planPath, root, identity);
    assert.equal(clearance.schema_id, DISCORD_RECOVERY_DEBT_CLEARANCE_SCHEMA_ID);
    assert.deepEqual(clearance.cleared_debts, []);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("one successful parent-bound no-op resolution clears exact prior debt", async () => {
  const input = catalogs();
  const plan = planDiscordRecoveryDebt(
    input.runList,
    input.primaryAttempts,
    input.recoveryAttempts,
    input.artifactPages,
    debtIdentity(false),
  );
  assert.equal(plan.debts.length, 1);
  assert.equal(plan.downloads.length, 1);
  const root = await mkdtemp(join(tmpdir(), "clearra-debt-noop-"));
  try {
    const planPath = join(root, "plan.json");
    await writeFile(planPath, `${JSON.stringify(plan)}\n`);
    await writeReportArtifact(root, "400", "recovery-authority.json", noOpResolution());
    const clearance = await auditDiscordRecoveryDebt(planPath, root, identity);
    assert.equal(clearance.cleared_debts[0].clearance_kind, "certified-no-runtime-mutation");
    assert.equal(clearance.cleared_debts[0].primary_workflow_run_id, "100");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("missing, expired, ambiguous, or parent-mismatched recovery evidence remains debt", async () => {
  const missing = catalogs([]);
  assert.throws(
    () => planDiscordRecoveryDebt(
      missing.runList,
      missing.primaryAttempts,
      missing.recoveryAttempts,
      missing.artifactPages,
      debtIdentity(false),
    ),
    /lacks a successful parent-bound resolution/u,
  );
  const expired = catalogs([resolutionArtifact({ expired: true })]);
  assert.throws(
    () => planDiscordRecoveryDebt(
      expired.runList,
      expired.primaryAttempts,
      expired.recoveryAttempts,
      expired.artifactPages,
      identity,
    ),
    /lacks a successful parent-bound resolution/u,
  );
  const ambiguous = catalogs([resolutionArtifact(), resolutionArtifact({ id: 402 })]);
  assert.throws(
    () => planDiscordRecoveryDebt(
      ambiguous.runList,
      ambiguous.primaryAttempts,
      ambiguous.recoveryAttempts,
      ambiguous.artifactPages,
      identity,
    ),
    /ambiguous/u,
  );
  const root = await mkdtemp(join(tmpdir(), "clearra-debt-mismatch-"));
  try {
    const input = catalogs();
    const plan = planDiscordRecoveryDebt(
      input.runList,
      input.primaryAttempts,
      input.recoveryAttempts,
      input.artifactPages,
      debtIdentity(false),
    );
    const planPath = join(root, "plan.json");
    await writeFile(planPath, `${JSON.stringify(plan)}\n`);
    await writeReportArtifact(
      root,
      "400",
      "recovery-authority.json",
      noOpResolution({ workflow_run_attempt: "2" }),
    );
    await assert.rejects(
      auditDiscordRecoveryDebt(planPath, root, identity),
      /differs from its exact workflow parent/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("runtime debt requires the exact durable recovery result", async () => {
  const input = catalogs([resolutionArtifact(), resultArtifact()]);
  const plan = planDiscordRecoveryDebt(
    input.runList,
    input.primaryAttempts,
    input.recoveryAttempts,
    input.artifactPages,
    { ...identity, recoveryJobCatalog: recoveryJobCatalog(true) },
  );
  const resolution = noOpResolution({
    recovery_required: true,
    recovery_stage: "live",
    recovery_reason: "live-transition-authority-present",
    artifact_id: "500",
    artifact_name: `discord-live-recovery-authority-${FAILED_SOURCE}-run-100-attempt-1`,
    artifact_digest: `sha256:${"f".repeat(64)}`,
    artifact_size: 2048,
    artifact_created_at: "2026-08-31T00:00:05Z",
    no_mutation_job_step_proof: null,
    live_transition_job_step_proof: liveTransitionProof(),
  });
  const protectedAuthorityBytes = Buffer.from(`${canonicalJson(resolution)}\n`, "utf8");
  const bindings = resultBindings("live");
  const recoveryAuthorityBinding = bindings.find((entry) => entry.name === "recovery_authority");
  recoveryAuthorityBinding.file_sha256 = createHash("sha256")
    .update(protectedAuthorityBytes)
    .digest("hex");
  recoveryAuthorityBinding.size = protectedAuthorityBytes.length;
  const catalogDisposition = notRequiredCatalogDisposition();
  const result = sealCanonicalReport({
    schema_id: DISCORD_RECOVERY_RESULT_SCHEMA_ID,
    recovery_stage: "live",
    repository: REPOSITORY,
    source_commit: FAILED_SOURCE,
    original_workflow_run_id: "100",
    original_workflow_run_attempt: "1",
    recovery_workflow_run_id: "300",
    recovery_workflow_run_attempt: "1",
    artifact_id: "500",
    artifact_digest: `sha256:${"f".repeat(64)}`,
    recovered_at: "2026-08-31T00:01:15.000Z",
    catalog_recovery: catalogDisposition,
    bindings,
  });
  const root = await mkdtemp(join(tmpdir(), "clearra-debt-runtime-"));
  try {
    const planPath = join(root, "plan.json");
    await writeFile(planPath, `${JSON.stringify(plan)}\n`);
    await writeReportArtifact(root, "400", "recovery-authority.json", resolution);
    await assert.rejects(
      auditDiscordRecoveryDebt(planPath, root, identity),
      /ENOENT|regular file/u,
    );
    await writeReportArtifact(root, "401", "recovery-result.json", result);
    await writeReportArtifact(
      root,
      "401",
      "protected-recovery-authority.json",
      resolution,
    );
    await writeReportArtifact(
      root,
      "401",
      "discord-catalog-recovery-disposition.json",
      catalogDisposition,
    );
    const clearance = await auditDiscordRecoveryDebt(planPath, root, identity);
    assert.equal(clearance.cleared_debts[0].clearance_kind, "completed-live-recovery");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("attempt inventory must be complete and current identity must bind exactly", () => {
  const input = catalogs();
  assert.throws(
    () => planDiscordRecoveryDebt(
      input.runList,
      { ...input.primaryAttempts, attempts: [currentRun()] },
      input.recoveryAttempts,
      input.artifactPages,
      identity,
    ),
    /attempt catalog is incomplete/u,
  );
  assert.throws(
    () => planDiscordRecoveryDebt(
      input.runList,
      input.primaryAttempts,
      input.recoveryAttempts,
      input.artifactPages,
      { ...identity, sourceCommit: "f".repeat(40) },
    ),
    /exact current primary attempt/u,
  );
});

test("one-time bootstrap scans every completed debt regardless run-number ordering", () => {
  const deferredCurrent = currentRun({ id: 50, run_number: 5 });
  const newerNumberDebt = primaryRun({ id: 100, run_number: 10 });
  const options = {
    ...identity,
    workflowRunId: "50",
  };
  assert.throws(
    () => planDiscordRecoveryDebt(
      { total_count: 2, workflow_runs: [deferredCurrent, newerNumberDebt] },
      {
        schema_id: "clearra.discord-primary-attempt-catalog.v1",
        attempts: [deferredCurrent, newerNumberDebt],
      },
      { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
      { total_count: 0, artifacts: [] },
      options,
    ),
    /lacks a successful parent-bound resolution/u,
  );
});

test("one-time bootstrap cutoff is bound to the exact current attempt start, not queue creation", () => {
  const queuedBeforeButStartedAfter = currentRun({
    created_at: "2026-09-06T17:12:22Z",
    run_started_at: "2026-09-06T17:12:24Z",
    updated_at: "2026-09-06T17:12:25Z",
  });
  assert.throws(
    () => planDiscordRecoveryDebt(
      { total_count: 1, workflow_runs: [queuedBeforeButStartedAfter] },
      {
        schema_id: "clearra.discord-primary-attempt-catalog.v1",
        attempts: [queuedBeforeButStartedAfter],
      },
      { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
      { total_count: 0, artifacts: [] },
      identity,
    ),
    /bootstrap has expired/u,
  );

  const exactBoundary = currentRun({
    created_at: "2026-09-06T17:12:22Z",
    run_started_at: "2026-09-06T17:12:23Z",
    updated_at: "2026-09-06T17:12:24Z",
  });
  const accepted = planDiscordRecoveryDebt(
    { total_count: 1, workflow_runs: [exactBoundary] },
    { schema_id: "clearra.discord-primary-attempt-catalog.v1", attempts: [exactBoundary] },
    { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
    { total_count: 0, artifacts: [] },
    identity,
  );
  assert.equal(accepted.checkpoint.checkpoint_kind, "code-bound-one-time-bootstrap");
});

test("durable annotated receipt folds expired Actions history only before Discord completion", () => {
  const current = currentRun({
    created_at: "2026-09-07T00:00:00Z",
    run_started_at: "2026-09-07T00:00:01Z",
    updated_at: "2026-09-07T00:00:02Z",
  });
  const oldExpiredDebt = primaryRun({ updated_at: "2026-08-31T00:00:10Z" });
  const plan = planDiscordRecoveryDebt(
    { total_count: 2, workflow_runs: [current, oldExpiredDebt] },
    {
      schema_id: "clearra.discord-primary-attempt-catalog.v1",
      attempts: [current, oldExpiredDebt],
    },
    { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
    { total_count: 1, artifacts: [{ id: 999, name: "unrelated-old-artifact", digest: null }] },
    checkpointIdentity(),
  );
  assert.equal(plan.checkpoint.checkpoint_kind, "durable-annotated-receipt-immutable-release");
  assert.equal(plan.checkpoint.workflow_run_id, CHECKPOINT_RUN_ID);
  assert.equal(plan.checkpoint.checkpoint_receipt_sha256, CHECKPOINT_RECEIPT.report_sha256);
  assert.deepEqual(plan.debts, []);

  const afterCheckpoint = primaryRun({
    id: 110,
    run_number: 11,
    created_at: "2026-08-31T02:51:10Z",
    run_started_at: "2026-08-31T02:51:11Z",
    updated_at: "2026-08-31T02:51:20Z",
  });
  assert.throws(
    () => planDiscordRecoveryDebt(
      { total_count: 3, workflow_runs: [current, oldExpiredDebt, afterCheckpoint] },
      {
        schema_id: "clearra.discord-primary-attempt-catalog.v1",
        attempts: [current, oldExpiredDebt, afterCheckpoint],
      },
      { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
      { total_count: 0, artifacts: [] },
      checkpointIdentity(),
    ),
    /110\/1 lacks a successful parent-bound resolution/u,
  );
});

test("bootstrap handles legacy tags but expires; fake or mutable v0.8 receipts never checkpoint", () => {
  const current = currentRun({
    created_at: "2026-09-07T00:00:00Z",
    run_started_at: "2026-09-07T00:00:01Z",
    updated_at: "2026-09-07T00:00:02Z",
  });
  const base = {
    runList: { total_count: 1, workflow_runs: [current] },
    attempts: {
      schema_id: "clearra.discord-primary-attempt-catalog.v1",
      attempts: [current],
    },
  };
  const invalidRelease = checkpointTagAuthority();
  invalidRelease.tags[0].release = { ...invalidRelease.tags[0].release, immutable: false };
  assert.throws(
    () => planDiscordRecoveryDebt(
      base.runList,
      base.attempts,
      { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
      { total_count: 0, artifacts: [] },
      checkpointIdentity({ tagAuthority: invalidRelease }),
    ),
    /immutable checkpoint GitHub Release identity is invalid/u,
  );
  const noTag = checkpointIdentity({
    reachableTags: [],
    tagAuthority: { schema_id: "clearra.discord-production-tag-authority-catalog.v1", tags: [] },
  });
  assert.throws(
    () => planDiscordRecoveryDebt(
      base.runList,
      base.attempts,
      { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
      { total_count: 0, artifacts: [] },
      noTag,
    ),
    /bootstrap has expired/u,
  );
  const forgedReceipt = structuredClone(CHECKPOINT_RECEIPT);
  forgedReceipt.checkpoint_candidate.release_artifacts[0].sha256 = "0".repeat(64);
  const forgedAuthority = checkpointTagAuthority();
  forgedAuthority.tags[0].tag_object.message = `${canonicalJson(reseal(forgedReceipt))}\n`;
  assert.throws(
    () => planDiscordRecoveryDebt(
      base.runList,
      base.attempts,
      { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
      { total_count: 0, artifacts: [] },
      checkpointIdentity({
        tagAuthority: forgedAuthority,
      }),
    ),
    /release artifacts|canonical checkpoint candidate|candidate SHA|report SHA/u,
  );

  const legacyCurrent = currentRun();
  const legacyAuthority = {
    schema_id: "clearra.discord-production-tag-authority-catalog.v1",
    tags: [{
      name: "v0.7.4",
      local_tag_object_sha: null,
      local_target_commit: "9".repeat(40),
      tag_ref: { ref: "refs/tags/v0.7.4", object: { type: "commit", sha: "9".repeat(40) } },
      tag_object: null,
      release: null,
    }],
  };
  const legacy = planDiscordRecoveryDebt(
    { total_count: 1, workflow_runs: [legacyCurrent] },
    { schema_id: "clearra.discord-primary-attempt-catalog.v1", attempts: [legacyCurrent] },
    { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
    { total_count: 0, artifacts: [] },
    {
      ...identity,
      reachableTags: ["v0.7.4"],
      tagAuthority: legacyAuthority,
    },
  );
  assert.equal(legacy.checkpoint.checkpoint_kind, "code-bound-one-time-bootstrap");
});

test("multiple consistent successful recovery reruns select the latest exact attempt", async () => {
  const secondRecovery = recoveryRun({
    id: 301,
    run_number: 31,
    head_sha: "f".repeat(40),
    created_at: "2026-08-31T00:02:00Z",
    run_started_at: "2026-08-31T00:02:01Z",
    updated_at: "2026-08-31T00:02:20Z",
  });
  const secondArtifact = resolutionArtifact({
    id: 402,
    name: "discord-recovery-resolution-run-100-attempt-1-recovery-run-301-attempt-1",
    created_at: "2026-08-31T00:02:10Z",
    workflow_run: {
      id: 301,
      repository_id: 1309293231,
      head_repository_id: 1309293231,
      head_sha: "f".repeat(40),
      head_branch: "main",
    },
  });
  const input = catalogs([resolutionArtifact(), secondArtifact]);
  input.recoveryAttempts.attempts.push(secondRecovery);
  const jobs = recoveryJobCatalog(false);
  const secondJobPages = structuredClone(jobs.runs[0].pages);
  for (const [index, job] of secondJobPages[0].jobs.entries()) {
    job.id = 920 + index;
    job.run_id = 301;
    job.head_sha = "f".repeat(40);
    job.html_url = `https://github.com/${REPOSITORY}/actions/runs/301/job/${920 + index}`;
  }
  jobs.runs.push({ workflow_run_id: "301", workflow_run_attempt: "1", pages: secondJobPages });
  const plan = planDiscordRecoveryDebt(
    input.runList,
    input.primaryAttempts,
    input.recoveryAttempts,
    input.artifactPages,
    { ...identity, recoveryJobCatalog: jobs },
  );
  const root = await mkdtemp(join(tmpdir(), "clearra-debt-rerun-"));
  try {
    const planPath = join(root, "plan.json");
    await writeFile(planPath, `${JSON.stringify(plan)}\n`);
    await writeReportArtifact(root, "400", "recovery-authority.json", noOpResolution());
    await writeReportArtifact(root, "402", "recovery-authority.json", noOpResolution({
      recovery_workflow_run_id: "301",
      recovery_workflow_run_attempt: "1",
    }));
    const clearance = await auditDiscordRecoveryDebt(planPath, root, identity);
    assert.equal(clearance.cleared_debts[0].recovery_workflow_run_id, "301");
    assert.equal(clearance.cleared_debts[0].consistent_clearance_count, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("durable receipt rejects 1199-second proof, foreign topology, moved refs, and future tags", () => {
  const current = currentRun({
    created_at: "2026-09-07T00:00:00Z", run_started_at: "2026-09-07T00:00:01Z",
    updated_at: "2026-09-07T00:00:02Z",
  });
  const runList = { total_count: 1, workflow_runs: [current] };
  const attempts = {
    schema_id: "clearra.discord-primary-attempt-catalog.v1", attempts: [current],
  };
  const execute = (options) => planDiscordRecoveryDebt(
    runList,
    attempts,
    { schema_id: "clearra.discord-recovery-attempt-catalog.v1", attempts: [] },
    { total_count: 0, artifacts: [] },
    options,
  );

  const shortReceipt = structuredClone(CHECKPOINT_RECEIPT);
  const observation = shortReceipt.completed_job_topology
    .find((job) => job.job_name === "sync-observe").steps.find(
    (step) => step.name === "Authority-bound global sync and sole canonical four-surface observation",
  );
  observation.completed_at = new Date(Date.parse(observation.started_at) + 1_199_000)
    .toISOString();
  shortReceipt.completed_job_topology_sha256 = canonicalSha256(
    shortReceipt.completed_job_topology,
  );
  const shortAuthority = checkpointAuthorityForReceipt(reseal(shortReceipt));
  assert.throws(
    () => execute(checkpointIdentity({
      tagAuthority: shortAuthority,
    })),
    /observation|1200|job window/u,
  );

  const foreignReceipt = structuredClone(CHECKPOINT_RECEIPT);
  const sync = foreignReceipt.completed_job_topology.find((job) => job.job_name === "sync-observe");
  sync.steps.splice(-1, 0, {
    name: "Foreign late runtime mutation", number: sync.steps.at(-1).number - 1,
    status: "completed", conclusion: "success",
    started_at: sync.steps.at(-2).completed_at,
    completed_at: sync.steps.at(-2).completed_at,
  });
  foreignReceipt.completed_job_topology_sha256 = canonicalSha256(
    foreignReceipt.completed_job_topology,
  );
  assert.throws(
    () => execute(checkpointIdentity({
      tagAuthority: checkpointAuthorityForReceipt(reseal(foreignReceipt)),
    })),
    /topology|step receipt/u,
  );

  const moved = checkpointTagAuthority();
  moved.tags[0].tag_ref.object.sha = "9".repeat(40);
  assert.throws(
    () => execute(checkpointIdentity({ tagAuthority: moved })),
    /not one exact annotated commit tag/u,
  );
  const future = checkpointTagAuthority();
  future.tags[0].tag_object.tagger.date = "2026-09-08T00:00:00Z";
  assert.throws(
    () => execute(checkpointIdentity({ tagAuthority: future })),
    /not prior to the current deployment attempt/u,
  );
});

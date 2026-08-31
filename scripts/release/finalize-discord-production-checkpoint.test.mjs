import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  canonicalJson,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";
import {
  checkpointCandidateArtifactName,
} from "./discord-production-checkpoint-receipt.mjs";
import {
  candidateFixture,
  completedCheckpointJobList,
  identity as checkpointCandidateIdentity,
  checkpointJobList,
} from "./discord-production-checkpoint-receipt.test.mjs";
import {
  createDiscordSuccessfulDeploymentTopologyContract,
  validateDiscordCheckpointCandidatePrerequisites,
} from "./discord-deployment-recovery.mjs";
import {
  DISCORD_PRODUCTION_CHECKPOINT_RECEIPT_SCHEMA_ID,
  materializeCompletedJobTopology,
  parseCanonicalReceiptBytes,
  parseRawTagObject,
  validateAcceptedReleaseArtifactBinding,
  validateCandidateArtifactUploadWindow,
  validateCheckpointChronology,
  validateCompletedDiscordRun,
  validateExactCandidateArtifactCatalog,
  validateImmutableCheckpointReleaseReadback,
  validateDiscordProductionCheckpointReceipt,
  validateProductionObservationJobWindow,
  validateRemoteLsRemote,
} from "./finalize-discord-production-checkpoint.mjs";

const REPOSITORY = "daejunnom/Clearra";
const REPOSITORY_ID = 1309293231;
const SOURCE_COMMIT = "a".repeat(40);
const TAG_OBJECT = "b".repeat(40);
const DIGEST = `sha256:${"c".repeat(64)}`;
const RUN_ID = "101";
const RUN_ATTEMPT = "2";
const ARTIFACT_ID = "401";

const IDENTITY = Object.freeze({
  repository: REPOSITORY,
  sourceCommit: SOURCE_COMMIT,
  discordWorkflowRunId: RUN_ID,
  discordWorkflowRunAttempt: RUN_ATTEMPT,
  artifactId: ARTIFACT_ID,
  artifactDigest: DIGEST,
});

function completedRun(overrides = {}) {
  return {
    id: Number(RUN_ID),
    run_attempt: Number(RUN_ATTEMPT),
    name: "Deploy Discord Production",
    path: ".github/workflows/discord-deploy.yml",
    event: "workflow_run",
    head_branch: "main",
    head_sha: SOURCE_COMMIT,
    status: "completed",
    conclusion: "success",
    run_started_at: "2026-08-31T00:00:00Z",
    updated_at: "2026-08-31T00:00:30Z",
    repository: { id: REPOSITORY_ID, full_name: REPOSITORY },
    head_repository: { id: REPOSITORY_ID, full_name: REPOSITORY },
    ...overrides,
  };
}

function candidateArtifact(overrides = {}) {
  return {
    id: Number(ARTIFACT_ID),
    name: checkpointCandidateArtifactName(SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
    expired: false,
    digest: DIGEST,
    size_in_bytes: 512,
    created_at: "2026-08-31T00:00:14Z",
    archive_download_url:
      `https://api.github.com/repos/${REPOSITORY}/actions/artifacts/${ARTIFACT_ID}/zip`,
    workflow_run: {
      id: Number(RUN_ID),
      repository_id: REPOSITORY_ID,
      head_repository_id: REPOSITORY_ID,
      head_branch: "main",
      head_sha: SOURCE_COMMIT,
    },
    ...overrides,
  };
}

function successfulCompletedTopology() {
  return [
    { job_id: "1", job_name: "authority", job_conclusion: "success", steps: [] },
    {
      job_id: "2",
      job_name: "Prepare immutable Discord candidate inputs",
      job_conclusion: "success",
      steps: [],
    },
    { job_id: "3", job_name: "promote", job_conclusion: "success", steps: [] },
    {
      job_id: "4",
      job_name: "sync-observe",
      job_conclusion: "success",
      steps: [
        {
          name: "Authority-bound global sync and sole canonical four-surface observation",
          number: 30,
          conclusion: "success",
          started_at: "2026-08-30T23:40:00Z",
          completed_at: "2026-08-31T00:00:09Z",
        },
        {
          name: "Upload durable sync and sole canonical observation evidence",
          number: 40,
          conclusion: "success",
          started_at: "2026-08-31T00:00:10Z",
          completed_at: "2026-08-31T00:00:12Z",
        },
        {
          name: "Upload canonical Discord production checkpoint candidate",
          number: 50,
          conclusion: "success",
          started_at: "2026-08-31T00:00:13Z",
          completed_at: "2026-08-31T00:00:16Z",
        },
      ],
    },
  ];
}

test("rejects an in-progress or unsuccessful Discord checkpoint run", () => {
  assert.doesNotThrow(() => validateCompletedDiscordRun(completedRun(), IDENTITY));
  assert.throws(
    () => validateCompletedDiscordRun(
      completedRun({ status: "in_progress", conclusion: null }),
      IDENTITY,
    ),
    /not exact completed success/u,
  );
  assert.throws(
    () => validateCompletedDiscordRun(completedRun({ conclusion: "failure" }), IDENTITY),
    /not exact completed success/u,
  );
  assert.throws(
    () => validateCompletedDiscordRun(
      completedRun({ updated_at: "2026-08-30T23:59:59Z" }),
      IDENTITY,
    ),
    /predates its start/u,
  );
});

test("rejects missing, duplicate, foreign, expired, or wrong-attempt candidate artifacts", () => {
  const selected = candidateArtifact();
  assert.doesNotThrow(() =>
    validateExactCandidateArtifactCatalog([selected], selected, IDENTITY));
  assert.throws(
    () => validateExactCandidateArtifactCatalog([], selected, IDENTITY),
    /missing, duplicate, or foreign/u,
  );
  assert.throws(
    () => validateExactCandidateArtifactCatalog([
      selected,
      candidateArtifact({ id: 402 }),
    ], selected, IDENTITY),
    /missing, duplicate, or foreign/u,
  );
  assert.throws(
    () => validateExactCandidateArtifactCatalog([
      selected,
      candidateArtifact({
        id: 402,
        name: checkpointCandidateArtifactName(SOURCE_COMMIT, RUN_ID, "1"),
      }),
    ], selected, IDENTITY),
    /missing, duplicate, or foreign/u,
  );
  const expired = candidateArtifact({ expired: true });
  assert.throws(
    () => validateExactCandidateArtifactCatalog([expired], expired, IDENTITY),
    /differs from the exact-one run catalog/u,
  );
  const wrongAttempt = candidateArtifact({
    name: checkpointCandidateArtifactName(SOURCE_COMMIT, RUN_ID, "1"),
  });
  assert.throws(
    () => validateExactCandidateArtifactCatalog([wrongAttempt], wrongAttempt, IDENTITY),
    /missing, duplicate, or foreign/u,
  );
});

test("rejects a failed candidate upload and artifact timestamps outside its step window", () => {
  const topology = successfulCompletedTopology();
  const runAuthority = {
    startedAt: Date.parse("2026-08-31T00:00:00Z"),
    completedAt: Date.parse("2026-08-31T00:00:30Z"),
  };
  assert.doesNotThrow(() => validateCandidateArtifactUploadWindow(
    topology,
    Date.parse("2026-08-31T00:00:14Z"),
    runAuthority,
  ));
  const failed = structuredClone(topology);
  failed[3].steps[2].conclusion = "failure";
  assert.throws(
    () => validateCandidateArtifactUploadWindow(
      failed,
      Date.parse("2026-08-31T00:00:14Z"),
      runAuthority,
    ),
    /not exact success/u,
  );
  for (const timestamp of ["2026-08-31T00:00:12Z", "2026-08-31T00:00:17Z"]) {
    assert.throws(
      () => validateCandidateArtifactUploadWindow(
        topology,
        Date.parse(timestamp),
        runAuthority,
      ),
      /outside its exact upload window/u,
    );
  }
  const overlapsObservationUpload = structuredClone(topology);
  overlapsObservationUpload[3].steps[2].started_at =
    "2026-08-31T00:00:11Z";
  assert.throws(
    () => validateCandidateArtifactUploadWindow(
      overlapsObservationUpload,
      Date.parse("2026-08-31T00:00:14Z"),
      runAuthority,
    ),
    /outside its exact upload window/u,
  );
  const completesAfterRun = structuredClone(topology);
  completesAfterRun[3].steps[2].completed_at =
    "2026-08-31T00:00:31Z";
  assert.throws(
    () => validateCandidateArtifactUploadWindow(
      completesAfterRun,
      Date.parse("2026-08-31T00:00:14Z"),
      runAuthority,
    ),
    /outside its exact upload window/u,
  );
});

test("seals all four completed Discord jobs and every contract step into the receipt topology", () => {
  const contract = createDiscordSuccessfulDeploymentTopologyContract();
  const response = {
    total_count: contract.jobs.length,
    jobs: contract.jobs.map((job, jobIndex) => ({
      id: 1000 + jobIndex,
      name: job.job_name,
      status: "completed",
      conclusion: "success",
      started_at: "2026-08-31T00:00:00Z",
      completed_at: "2026-08-31T00:21:00Z",
      steps: job.steps.map((step, stepIndex) => ({
        name: step.name,
        number: stepIndex + 1,
        status: "completed",
        conclusion: step.expected_conclusion,
        started_at: step.expected_conclusion === "skipped"
          ? null
          : "2026-08-31T00:00:00Z",
        completed_at: step.expected_conclusion === "skipped"
          ? null
          : "2026-08-31T00:21:00Z",
      })),
    })),
  };
  const sealed = materializeCompletedJobTopology(response, {
    completedAt: Date.parse("2026-08-31T00:21:01Z"),
  });
  assert.deepEqual(
    sealed.map((job) => [job.job_name, job.steps.length]),
    contract.jobs.map((job) => [job.job_name, job.steps.length]),
  );
  const missingStep = structuredClone(response);
  missingStep.jobs[3].steps.pop();
  assert.throws(
    () => materializeCompletedJobTopology(missingStep, {
      completedAt: Date.parse("2026-08-31T00:21:01Z"),
    }),
    /step topology is incomplete/u,
  );
  const forgedConclusion = structuredClone(response);
  forgedConclusion.jobs[0].steps[0].conclusion = "skipped";
  assert.throws(
    () => materializeCompletedJobTopology(forgedConclusion, {
      completedAt: Date.parse("2026-08-31T00:21:01Z"),
    }),
    /step topology is foreign/u,
  );
});

test("self-validates every completed job and step retained in the durable receipt", async () => {
  const candidateIdentity = checkpointCandidateIdentity();
  const candidate = await candidateFixture(
    validateDiscordCheckpointCandidatePrerequisites(
      checkpointJobList(),
      candidateIdentity,
    ),
  );
  const completedAt = Date.parse("2026-08-31T04:00:00.000Z");
  const completedJobs = completedCheckpointJobList();
  for (const job of completedJobs.jobs) {
    for (const step of job.steps) {
      if (step.conclusion === "skipped") {
        step.started_at = null;
        step.completed_at = null;
      }
    }
    const timestamps = job.steps.flatMap((step) =>
      step.started_at === null ? [] : [Date.parse(step.started_at), Date.parse(step.completed_at)]);
    job.started_at = new Date(Math.min(...timestamps)).toISOString();
    job.completed_at = new Date(Math.max(...timestamps)).toISOString();
  }
  const topology = materializeCompletedJobTopology(completedJobs, {
    completedAt,
  });
  const candidateUpload = topology.find(({ job_name: name }) => name === "sync-observe")
    .steps.find(({ name }) => name === "Upload canonical Discord production checkpoint candidate");
  const artifactCreatedAt = candidateUpload.started_at;
  const receipt = sealCanonicalReport({
    schema_id: DISCORD_PRODUCTION_CHECKPOINT_RECEIPT_SCHEMA_ID,
    repository: candidate.repository,
    repository_id: String(REPOSITORY_ID),
    release: "v0.8.0",
    version: "0.8.0",
    source_commit: candidate.source_commit,
    accepted_workflow_path: ".github/workflows/release-cli.yml",
    accepted_workflow_run_id: candidate.accepted_workflow_run_id,
    accepted_workflow_run_attempt: candidate.accepted_workflow_run_attempt,
    discord_workflow_path: ".github/workflows/discord-deploy.yml",
    discord_workflow_run_id: candidate.discord_workflow_run_id,
    discord_workflow_run_attempt: candidate.discord_workflow_run_attempt,
    discord_workflow_started_at: "2026-08-31T00:00:00.000Z",
    discord_workflow_completed_at: "2026-08-31T04:00:00.000Z",
    checkpoint_candidate_artifact: {
      artifact_id: "501",
      artifact_name: checkpointCandidateArtifactName(
        candidate.source_commit,
        candidate.discord_workflow_run_id,
        candidate.discord_workflow_run_attempt,
      ),
      artifact_digest: `sha256:${"a".repeat(64)}`,
      artifact_created_at: artifactCreatedAt,
      archive_sha256: "a".repeat(64),
      file_name: "discord-production-checkpoint-candidate.json",
      file_sha256: "b".repeat(64),
      candidate_report_sha256: candidate.report_sha256,
    },
    checkpoint_candidate: candidate,
    completed_job_topology: topology,
    completed_job_topology_sha256: createHash("sha256")
      .update(canonicalJson(topology), "utf8").digest("hex"),
    tag: {
      name: "v0.8.0",
      target_commit: candidate.source_commit,
      annotated: true,
      message_contract: "exact-canonical-receipt-bytes",
      tagger: {
        name: "Clearra Release",
        email: "release@example.invalid",
        date: "2026-08-31T04:00:01Z",
      },
    },
    github_release_contract: {
      tag: "v0.8.0",
      title: "Clearra v0.8.0",
      source_commit: candidate.source_commit,
      draft: false,
      prerelease: false,
      immutable: true,
      asset_count: 3,
      canonical_acceptance_evidence_sha256:
        candidate.canonical_acceptance_evidence_sha256,
    },
    status: "ready-for-annotated-tag-and-immutable-release",
  });
  assert.equal(validateDiscordProductionCheckpointReceipt(receipt), receipt);

  const missingStep = structuredClone(receipt);
  missingStep.completed_job_topology[3].steps.pop();
  missingStep.completed_job_topology_sha256 = topologySha256(
    missingStep.completed_job_topology,
  );
  delete missingStep.report_sha256;
  assert.throws(
    () => validateDiscordProductionCheckpointReceipt(sealCanonicalReport(missingStep)),
    /topology contract/u,
  );
  const changedJobId = structuredClone(receipt);
  changedJobId.completed_job_topology[0].job_id = "0";
  changedJobId.completed_job_topology_sha256 = topologySha256(
    changedJobId.completed_job_topology,
  );
  delete changedJobId.report_sha256;
  assert.throws(
    () => validateDiscordProductionCheckpointReceipt(sealCanonicalReport(changedJobId)),
    /completed Discord job receipt ID/u,
  );
  const changedStepTimestamp = structuredClone(receipt);
  changedStepTimestamp.completed_job_topology[0].steps[0].completed_at =
    "2026-08-31T04:00:00.001Z";
  changedStepTimestamp.completed_job_topology_sha256 = topologySha256(
    changedStepTimestamp.completed_job_topology,
  );
  delete changedStepTimestamp.report_sha256;
  assert.throws(
    () => validateDiscordProductionCheckpointReceipt(sealCanonicalReport(changedStepTimestamp)),
    /timestamps leave their job/u,
  );
});

test("rejects equal or inverted observation, artifact, completion, and tagger chronology", () => {
  const valid = {
    observationEndedAt: Date.parse("2026-08-31T00:00:10Z"),
    artifactCreatedAt: Date.parse("2026-08-31T00:00:20Z"),
    discordCompletedAt: Date.parse("2026-08-31T00:00:30Z"),
    taggerAt: Date.parse("2026-08-31T00:00:40Z"),
  };
  assert.doesNotThrow(() => validateCheckpointChronology(valid));
  for (const invalid of [
    { ...valid, taggerAt: valid.discordCompletedAt },
    { ...valid, taggerAt: valid.discordCompletedAt - 1_000 },
    { ...valid, artifactCreatedAt: valid.discordCompletedAt + 1_000 },
    { ...valid, observationEndedAt: valid.artifactCreatedAt + 1_000 },
  ]) {
    assert.throws(
      () => validateCheckpointChronology(invalid),
      /chronology is invalid/u,
    );
  }
});

test("binds the canonical 1200-second observation report to its completed job window", () => {
  const topology = successfulCompletedTopology();
  const observation = {
    started_at: "2026-08-30T23:40:01.250Z",
    ended_at: "2026-08-31T00:00:01.250Z",
  };
  assert.doesNotThrow(() =>
    validateProductionObservationJobWindow(topology, observation));
  assert.throws(
    () => validateProductionObservationJobWindow(topology, {
      ...observation,
      started_at: "2026-08-30T23:39:59.999Z",
    }),
    /outside its exact completed job window/u,
  );
  assert.throws(
    () => validateProductionObservationJobWindow(topology, {
      ...observation,
      ended_at: "2026-08-31T00:00:09.001Z",
    }),
    /outside its exact completed job window/u,
  );
  const failed = structuredClone(topology);
  failed[3].steps[0].conclusion = "failure";
  assert.throws(
    () => validateProductionObservationJobWindow(failed, observation),
    /not exact success/u,
  );
});

test("preserves exact canonical receipt bytes through local and remote annotated tag readback", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-checkpoint-tag-"));
  const checkout = join(root, "checkout");
  const remote = join(root, "remote.git");
  const message = Buffer.from(`${canonicalJson(sealCanonicalReport({
    schema_id: "clearra.discord-production-checkpoint-receipt.test.v1",
    state: "exact",
  }))}\n`, "utf8");
  try {
    git(["init", "--bare", remote]);
    git(["init", checkout]);
    git(["-C", checkout, "config", "user.name", "Clearra Release"]);
    git(["-C", checkout, "config", "user.email", "release@example.invalid"]);
    git(["-C", checkout, "commit", "--allow-empty", "-m", "seed"], {
      GIT_AUTHOR_DATE: "2026-08-31T00:00:00+00:00",
      GIT_COMMITTER_DATE: "2026-08-31T00:00:00+00:00",
    });
    const sourceCommit = git(["-C", checkout, "rev-parse", "HEAD"])
      .toString("utf8").trim();
    git([
      "-C", checkout, "tag", "-a", "--cleanup=verbatim", "-F", "-",
      "v0.8.0", sourceCommit,
    ], {
      GIT_COMMITTER_DATE: "2026-08-31T00:00:40+00:00",
    }, message);
    const tagObject = git(["-C", checkout, "rev-parse", "refs/tags/v0.8.0"])
      .toString("utf8").trim();
    const local = parseRawTagObject(git(["-C", checkout, "cat-file", "tag", tagObject]));
    assert.equal(local.targetCommit, sourceCommit);
    assert.deepEqual(local.message, message);
    assert.deepEqual(parseCanonicalReceiptBytes(local.message).bytes, message);
    git(["-C", checkout, "remote", "add", "origin", remote]);
    git(["-C", checkout, "push", "origin", "refs/tags/v0.8.0:refs/tags/v0.8.0"]);
    const listing = git([
      "-C", checkout, "ls-remote", "origin",
      "refs/tags/v0.8.0", "refs/tags/v0.8.0^{}",
    ]).toString("utf8");
    const remoteIdentity = validateRemoteLsRemote(listing, "v0.8.0", sourceCommit);
    assert.equal(remoteIdentity.tagObjectSha, tagObject);
    const remoteObject = parseRawTagObject(git([
      "--git-dir", remote, "cat-file", "tag", remoteIdentity.tagObjectSha,
    ]));
    assert.deepEqual(remoteObject.message, message);
    assert.throws(
      () => parseCanonicalReceiptBytes(message.subarray(0, -1)),
      /not exact canonical JSON bytes/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("rejects a non-bot or noncanonical immutable three-asset Release", () => {
  const acceptedArtifacts = [
    ["linux-cli", "clearra-cli-linux-x64-v0.8.0", "d"],
    ["windows-cli", "clearra-cli-windows-x64-v0.8.0.exe", "e"],
    ["windows-gui", "clearra-gui-windows-x64-v0.8.0.exe", "f"],
  ].map(([role, name, nibble], index) => ({
    role,
    name,
    sha256: nibble.repeat(64),
    size_bytes: 100 + index,
    source_commit: SOURCE_COMMIT,
  }));
  const release = immutableRelease(acceptedArtifacts);
  assert.doesNotThrow(() => validateImmutableCheckpointReleaseReadback(release, {
    repository: REPOSITORY,
    sourceCommit: SOURCE_COMMIT,
    tag: "v0.8.0",
    taggerAt: "2026-08-31T00:00:40Z",
    acceptedArtifacts,
  }));
  const nonBot = structuredClone(release);
  nonBot.author.login = "release-admin";
  assert.throws(
    () => validateImmutableCheckpointReleaseReadback(nonBot, {
      repository: REPOSITORY,
      sourceCommit: SOURCE_COMMIT,
      tag: "v0.8.0",
      taggerAt: "2026-08-31T00:00:40Z",
      acceptedArtifacts,
    }),
    /stable GitHub Actions bot authority/u,
  );
  const mutable = structuredClone(release);
  mutable.immutable = false;
  assert.throws(
    () => validateImmutableCheckpointReleaseReadback(mutable, {
      repository: REPOSITORY,
      sourceCommit: SOURCE_COMMIT,
      tag: "v0.8.0",
      taggerAt: "2026-08-31T00:00:40Z",
      acceptedArtifacts,
    }),
    /identity is invalid/u,
  );
  const wrongAsset = structuredClone(release);
  wrongAsset.assets[1].digest = `sha256:${"0".repeat(64)}`;
  assert.throws(
    () => validateImmutableCheckpointReleaseReadback(wrongAsset, {
      repository: REPOSITORY,
      sourceCommit: SOURCE_COMMIT,
      tag: "v0.8.0",
      taggerAt: "2026-08-31T00:00:40Z",
      acceptedArtifacts,
    }),
    /asset differs from accepted bytes/u,
  );
  const sameSecond = structuredClone(release);
  sameSecond.published_at = "2026-08-31T00:00:40Z";
  assert.throws(
    () => validateImmutableCheckpointReleaseReadback(sameSecond, {
      repository: REPOSITORY,
      sourceCommit: SOURCE_COMMIT,
      tag: "v0.8.0",
      taggerAt: "2026-08-31T00:00:40Z",
      acceptedArtifacts,
    }),
    /identity is invalid/u,
  );
});

test("binds candidate product fragments fieldwise to current canonical acceptance", () => {
  const fragments = [{
    role: "linux-cli",
    name: "clearra-cli-linux-x64-v0.8.0",
    sha256: "d".repeat(64),
    size_bytes: 100,
    source_commit: SOURCE_COMMIT,
  }];
  assert.equal(validateAcceptedReleaseArtifactBinding(fragments, structuredClone(fragments)), fragments);
  const changed = structuredClone(fragments);
  changed[0].size_bytes += 1;
  assert.throws(
    () => validateAcceptedReleaseArtifactBinding(fragments, changed),
    /differ from canonical acceptance/u,
  );
});

function immutableRelease(acceptedArtifacts) {
  const bot = {
    login: "github-actions[bot]",
    id: 41898282,
    type: "Bot",
    site_admin: false,
    url: "https://api.github.com/users/github-actions%5Bbot%5D",
    html_url: "https://github.com/apps/github-actions",
  };
  return {
    id: 900,
    tag_name: "v0.8.0",
    target_commitish: SOURCE_COMMIT,
    name: "Clearra v0.8.0",
    draft: false,
    prerelease: false,
    immutable: true,
    published_at: "2026-08-31T00:00:41Z",
    url: `https://api.github.com/repos/${REPOSITORY}/releases/900`,
    html_url: `https://github.com/${REPOSITORY}/releases/tag/v0.8.0`,
    assets_url: `https://api.github.com/repos/${REPOSITORY}/releases/900/assets`,
    upload_url:
      `https://uploads.github.com/repos/${REPOSITORY}/releases/900/assets{?name,label}`,
    author: structuredClone(bot),
    assets: acceptedArtifacts.map((artifact, index) => ({
      id: 1000 + index,
      name: artifact.name,
      state: "uploaded",
      size: artifact.size_bytes,
      digest: `sha256:${artifact.sha256}`,
      url:
        `https://api.github.com/repos/${REPOSITORY}/releases/assets/${1000 + index}`,
      browser_download_url:
        `https://github.com/${REPOSITORY}/releases/download/v0.8.0/${artifact.name}`,
      uploader: structuredClone(bot),
    })),
  };
}

function topologySha256(topology) {
  return createHash("sha256").update(canonicalJson(topology), "utf8").digest("hex");
}

function git(args, extraEnvironment = {}, input = undefined) {
  const result = spawnSync("git", args, {
    encoding: null,
    input,
    env: { ...process.env, ...extraEnvironment },
    windowsHide: true,
    shell: false,
  });
  if (result.error || result.signal || result.status !== 0) {
    throw new Error(`git ${args.join(" ")} failed: ${Buffer.from(result.stderr ?? []).toString("utf8")}`);
  }
  return Buffer.from(result.stdout ?? []);
}

import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID,
  DISCORD_RECOVERY_RESULT_SCHEMA_ID,
  resolveDiscordRecoveryAuthority,
  sealDiscordRecoveryResult,
  verifyDiscordRecoveryResult,
} from "./discord-deployment-recovery.mjs";
import { sealDiscordCatalogRecoveryDisposition } from
  "./discord-catalog-recovery-authority.mjs";

const SOURCE = "0123456789abcdef0123456789abcdef01234567";
const REPOSITORY = "daejunnom/Clearra";
const RUN_ID = "701";
const RUN_ATTEMPT = "3";
const PRESTAGE_ARTIFACT_NAME =
  `discord-prestage-recovery-authority-${SOURCE}-run-${RUN_ID}-attempt-${RUN_ATTEMPT}`;
const LIVE_ARTIFACT_NAME =
  `discord-live-recovery-authority-${SOURCE}-run-${RUN_ID}-attempt-${RUN_ATTEMPT}`;

async function writeNotRequiredCatalogDisposition(root, recoveryRunId, recoveryAttempt) {
  const path = join(root, `catalog-disposition-${recoveryRunId}-${recoveryAttempt}.json`);
  const report = await sealDiscordCatalogRecoveryDisposition({
    repository: REPOSITORY,
    sourceCommit: SOURCE,
    originalWorkflowRunId: RUN_ID,
    originalWorkflowRunAttempt: RUN_ATTEMPT,
    recoveryWorkflowRunId: recoveryRunId,
    recoveryWorkflowRunAttempt: recoveryAttempt,
    required: false,
  });
  await writeFile(path, `${JSON.stringify(report)}\n`, { mode: 0o600 });
  return path;
}

function run(overrides = {}) {
  return {
    id: Number(RUN_ID),
    run_attempt: Number(RUN_ATTEMPT),
    run_number: 41,
    created_at: "2026-08-31T00:00:00Z",
    run_started_at: "2026-08-31T00:00:10Z",
    updated_at: "2026-08-31T00:00:20Z",
    name: "Deploy Discord Production",
    path: ".github/workflows/discord-deploy.yml",
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "cancelled",
    head_branch: "main",
    head_sha: SOURCE,
    repository: { id: 1309293231, full_name: REPOSITORY },
    head_repository: { id: 1309293231, full_name: REPOSITORY },
    ...overrides,
  };
}

function artifact(overrides = {}) {
  return {
    id: 801,
    name: LIVE_ARTIFACT_NAME,
    digest: `sha256:${"a".repeat(64)}`,
    created_at: "2026-08-31T00:00:15Z",
    expired: false,
    size_in_bytes: 4096,
    workflow_run: {
      id: Number(RUN_ID),
      repository_id: 1309293231,
      head_repository_id: 1309293231,
      head_sha: SOURCE,
      head_branch: "main",
    },
    ...overrides,
  };
}

function jobList(overrides = {}) {
  const workflowRunId = String(overrides.workflowRunId ?? RUN_ID);
  const workflowRunAttempt = String(
    overrides.workflowRunAttempt ?? (workflowRunId === RUN_ID ? RUN_ATTEMPT : "1"),
  );
  const sourceCommit = overrides.sourceCommit ?? SOURCE;
  const steps = overrides.steps ?? [
    {
      name: "Upload prestage authority before Oracle freeze or Cloud zero traffic",
      number: 6,
      status: "completed",
      conclusion: "failure",
    },
    {
      name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
      number: 7,
      status: "completed",
      conclusion: "skipped",
    },
    {
      name: "Upload live-transition authority before Oracle activation or Cloud traffic",
      number: 8,
      status: "completed",
      conclusion: "skipped",
    },
    {
      name: "Activate Oracle, verify the real path, then cut Cloud to 100 percent",
      number: 9,
      status: "completed",
      conclusion: "skipped",
    },
  ];
  const exactJob = (id, name, conclusion, jobSteps = []) => ({
    id,
    run_id: Number(workflowRunId),
    run_attempt: Number(workflowRunAttempt),
    head_sha: sourceCommit,
    head_branch: "main",
    name,
    status: "completed",
    conclusion,
    html_url: `https://github.com/${REPOSITORY}/actions/runs/${workflowRunId}/job/${id}`,
    steps: jobSteps,
  });
  return {
    total_count: 4,
    jobs: [
      exactJob(898, "authority", "success"),
      exactJob(899, "Prepare immutable Discord candidate inputs", "success"),
      { ...exactJob(901, "promote", "failure", steps), ...overrides.job },
      exactJob(902, "sync-observe", "skipped"),
    ],
  };
}

function liveJobList(overrides = {}) {
  return jobList({
    ...overrides,
    steps: [
      {
        name: "Upload prestage authority before Oracle freeze or Cloud zero traffic",
        number: 6, status: "completed", conclusion: "success",
      },
      {
        name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
        number: 7, status: "completed", conclusion: "success",
      },
      {
        name: "Upload live-transition authority before Oracle activation or Cloud traffic",
        number: 8, status: "completed", conclusion: "success",
      },
      {
        name: "Activate Oracle, verify the real path, then cut Cloud to 100 percent",
        number: 9, status: "completed", conclusion: "skipped",
      },
    ],
  });
}

function runJobCatalog(entries = []) {
  return {
    schema_id: "clearra.discord-deployment-run-job-catalog.v1",
    runs: entries.map(({ workflowRunId, workflowRunAttempt = "1", pages }) => ({
      workflow_run_id: String(workflowRunId),
      workflow_run_attempt: String(workflowRunAttempt),
      pages,
    })),
  };
}

function runAttemptCatalog(latestRuns = [run()]) {
  const attempts = [];
  for (const latest of latestRuns) {
    for (let attempt = 1; attempt <= latest.run_attempt; attempt += 1) {
      if (attempt === latest.run_attempt) {
        attempts.push(latest);
      } else if (latest.id === Number(RUN_ID) && attempt === Number(RUN_ATTEMPT)) {
        attempts.push(run());
      } else {
        const started = new Date(Date.parse(latest.created_at) + attempt * 1_000)
          .toISOString().replace(".000Z", "Z");
        const updated = new Date(Date.parse(latest.created_at) + (attempt * 1_000) + 500)
          .toISOString();
        attempts.push(run({
          ...latest,
          run_attempt: attempt,
          run_started_at: started,
          updated_at: updated,
          status: "completed",
          conclusion: "success",
        }));
      }
    }
  }
  return {
    schema_id: "clearra.discord-deployment-primary-attempt-catalog.v1",
    attempts,
  };
}

const options = {
  repository: REPOSITORY,
  workflowRunId: RUN_ID,
  workflowRunAttempt: RUN_ATTEMPT,
  recoveryWorkflowRunId: "990",
  recoveryWorkflowRunAttempt: "1",
  runList: {
    total_count: 1,
    workflow_runs: [run()],
  },
  runAttemptCatalog: runAttemptCatalog(),
  jobList: jobList(),
  runJobCatalog: runJobCatalog(),
};

test("recovery authority binds the exact original run attempt and one immutable artifact", () => {
  const authority = resolveDiscordRecoveryAuthority(
    run(),
    {
      total_count: 2,
      artifacts: [
        artifact({ id: 800, name: PRESTAGE_ARTIFACT_NAME }),
        artifact(),
      ],
    },
    { ...options, jobList: liveJobList() },
  );
  assert.equal(authority.schema_id, DISCORD_RECOVERY_AUTHORITY_SCHEMA_ID);
  assert.equal(authority.workflow_run_attempt, RUN_ATTEMPT);
  assert.equal(authority.recovery_required, true);
  assert.equal(authority.recovery_stage, "live");
  assert.equal(authority.artifact_name, LIVE_ARTIFACT_NAME);
  assert.equal(authority.artifact_id, "801");
  assert.equal(authority.artifact_created_at, "2026-08-31T00:00:15Z");
  assert.equal(authority.no_mutation_job_step_proof, null);
  assert.equal(authority.prestage_only_job_step_proof, null);
  assert.deepEqual(authority.freshness_proof.potential_superseders, []);
  assert.match(authority.report_sha256, /^[0-9a-f]{64}$/u);
});

test("absence of prestage proves no Oracle or Cloud runtime mutation, not no external work", () => {
  const authority = resolveDiscordRecoveryAuthority(
    run({ conclusion: "failure" }),
    { total_count: 0, artifacts: [] },
    options,
  );
  assert.equal(authority.recovery_required, false);
  assert.equal(authority.recovery_stage, "none");
  assert.equal(authority.recovery_reason, "job-steps-prove-no-prestage-upload-or-runtime-mutation");
  assert.equal(authority.artifact_id, null);
  assert.equal(authority.artifact_name, PRESTAGE_ARTIFACT_NAME);
  assert.equal(authority.no_mutation_job_step_proof.job_name, "promote");
  assert.equal(authority.no_mutation_job_step_proof.prestage_upload_step.conclusion, "failure");
  assert.equal(
    authority.no_mutation_job_step_proof.runtime_mutation_steps[0].conclusion,
    "skipped",
  );
  assert.equal(authority.prestage_only_job_step_proof, null);
});

test("one exact skipped promote job with zero steps is a durable no-mutation proof", () => {
  const authority = resolveDiscordRecoveryAuthority(
    run({ conclusion: "cancelled" }),
    { total_count: 0, artifacts: [] },
    {
      ...options,
      jobList: jobList({
        job: { conclusion: "skipped" },
        steps: [],
      }),
    },
  );
  assert.equal(authority.recovery_required, false);
  assert.equal(authority.no_mutation_job_step_proof.job_name, "promote");
  assert.equal(authority.no_mutation_job_step_proof.job_conclusion, "skipped");
  assert.equal(authority.no_mutation_job_step_proof.prestage_upload_step, null);
});

test("missing prestage artifact fails closed after upload success or runtime mutation", () => {
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "failure" }),
      { total_count: 0, artifacts: [] },
      {
        ...options,
        jobList: jobList({
          steps: [
            {
              name: "Upload prestage authority before Oracle freeze or Cloud zero traffic",
              number: 6,
              status: "completed",
              conclusion: "success",
            },
            {
              name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
              number: 7,
              status: "completed",
              conclusion: "cancelled",
            },
          ],
        }),
      },
    ),
    /artifact is absent after its upload step succeeded/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "cancelled" }),
      { total_count: 0, artifacts: [] },
      {
        ...options,
        jobList: jobList({
          steps: [
            {
              name: "Upload prestage authority before Oracle freeze or Cloud zero traffic",
              number: 6,
              status: "completed",
              conclusion: "failure",
            },
            {
              name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
              number: 7,
              status: "completed",
              conclusion: "cancelled",
            },
          ],
        }),
      },
    ),
    /artifact is absent after runtime mutation began/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "cancelled" }),
      { total_count: 0, artifacts: [] },
      {
        ...options,
        jobList: jobList({ steps: [] }),
      },
    ),
    /lacks the expected prestage upload step/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "failure" }),
      { total_count: 0, artifacts: [] },
      { ...options, jobList: { total_count: 0, jobs: [] } },
    ),
    /closed primary topology/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "failure" }),
      { total_count: 0, artifacts: [] },
      {
        ...options,
        jobList: jobList({
          job: { name: "renamed-mutator" },
          steps: [{
            name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
            number: 1,
            status: "completed",
            conclusion: "success",
          }],
        }),
      },
    ),
    /closed primary topology/u,
  );
  const foreignMutator = jobList({ job: { conclusion: "skipped" }, steps: [] });
  foreignMutator.total_count += 1;
  foreignMutator.jobs.push({
    ...foreignMutator.jobs[0],
    id: 903,
    name: "foreign-mutator",
    conclusion: "success",
    html_url: `https://github.com/${REPOSITORY}/actions/runs/${RUN_ID}/job/903`,
  });
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "cancelled" }),
      { total_count: 0, artifacts: [] },
      { ...options, jobList: foreignMutator },
    ),
    /closed primary topology/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "failure" }),
      { total_count: 0, artifacts: [] },
      {
        ...options,
        jobList: jobList({
          steps: [{
            name: "Foreign successful mutation",
            number: 1,
            status: "completed",
            conclusion: "success",
          }],
        }),
      },
    ),
    /foreign or unordered topology|promote step authority is invalid/u,
  );
});

test("prestage-only authority selects bounded inactive and zero-traffic cleanup", () => {
  const prestageOnlyJobList = jobList({
    steps: [
      {
        name: "Upload prestage authority before Oracle freeze or Cloud zero traffic",
        number: 6,
        status: "completed",
        conclusion: "success",
      },
      {
        name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
        number: 7,
        status: "completed",
        conclusion: "cancelled",
      },
      {
        name: "Upload live-transition authority before Oracle activation or Cloud traffic",
        number: 8,
        status: "completed",
        conclusion: "skipped",
      },
      {
        name: "Activate Oracle, verify the real path, then cut Cloud to 100 percent",
        number: 9,
        status: "completed",
        conclusion: "skipped",
      },
    ],
  });
  const authority = resolveDiscordRecoveryAuthority(
    run({ conclusion: "timed_out" }),
    { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
    { ...options, jobList: prestageOnlyJobList },
  );
  assert.equal(authority.recovery_required, true);
  assert.equal(authority.recovery_stage, "prestage");
  assert.equal(authority.artifact_name, PRESTAGE_ARTIFACT_NAME);
  assert.equal(authority.prestage_only_job_step_proof.prestage_upload_step.conclusion, "success");
  assert.equal(authority.prestage_only_job_step_proof.live_upload_step.conclusion, "skipped");
});

test("prestage-only resolution rejects a deleted live artifact or activation that began", () => {
  const artifactList = {
    total_count: 1,
    artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })],
  };
  const baseSteps = [
    {
      name: "Upload prestage authority before Oracle freeze or Cloud zero traffic",
      number: 6,
      status: "completed",
      conclusion: "success",
    },
    {
      name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
      number: 7,
      status: "completed",
      conclusion: "success",
    },
  ];
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "failure" }),
      artifactList,
      {
        ...options,
        jobList: jobList({
          steps: [
            ...baseSteps,
            {
              name: "Upload live-transition authority before Oracle activation or Cloud traffic",
              number: 8,
              status: "completed",
              conclusion: "success",
            },
          ],
        }),
      },
    ),
    /live artifact is absent after its upload step succeeded/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ conclusion: "cancelled" }),
      artifactList,
      {
        ...options,
        jobList: jobList({
          steps: [
            ...baseSteps,
            {
              name: "Upload live-transition authority before Oracle activation or Cloud traffic",
              number: 8,
              status: "completed",
              conclusion: "failure",
            },
            {
              name: "Activate Oracle, verify the real path, then cut Cloud to 100 percent",
              number: 9,
              status: "completed",
              conclusion: "cancelled",
            },
          ],
        }),
      },
    ),
    /live artifact is absent after activation began/u,
  );
});

test("recovery resolution fails closed for foreign, truncated, ambiguous, or expired authority", () => {
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ head_repository: { full_name: "someone/fork" } }),
      { total_count: 2, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME }), artifact()] },
      options,
    ),
    /exact completed primary authority/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 3, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME }), artifact()] },
      options,
    ),
    /complete and non-truncated/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      {
        total_count: 3,
        artifacts: [
          artifact({ name: PRESTAGE_ARTIFACT_NAME }),
          artifact(),
          artifact({ id: 802 }),
        ],
      },
      options,
    ),
    /ambiguous/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME, expired: true })] },
      options,
    ),
    /differs from the exact original run/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      {
        total_count: 1,
        artifacts: [artifact({
          name: PRESTAGE_ARTIFACT_NAME,
          created_at: "2026-08-31T00:00:30Z",
        })],
      },
      options,
    ),
    /exact run attempt window/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      {
        total_count: 2,
        artifacts: [
          artifact({ name: PRESTAGE_ARTIFACT_NAME, expired: true }),
          artifact(),
        ],
      },
      options,
    ),
    /prestage recovery artifact differs/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ run_attempt: 2 }),
      { total_count: 2, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME }), artifact()] },
      options,
    ),
    /exact completed primary authority/u,
  );
});

test("recovery freshness ignores exact queued or completed no-op runs and blocks mutators", () => {
  const newer = run({
    id: 702,
    run_attempt: 1,
    run_number: 42,
    created_at: "2026-08-31T00:01:00Z",
    run_started_at: "2026-08-31T00:01:10Z",
    updated_at: "2026-08-31T00:01:20Z",
    conclusion: "success",
  });
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: { total_count: 2, workflow_runs: [newer, run()] },
        runAttemptCatalog: runAttemptCatalog([newer, run()]),
        runJobCatalog: runJobCatalog([{
          workflowRunId: "702",
          pages: [jobList({
            workflowRunId: "702",
            steps: [
              {
                name: "Upload prestage authority before Oracle freeze or Cloud zero traffic",
                number: 6,
                status: "completed",
                conclusion: "success",
              },
              {
                name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
                number: 7,
                status: "completed",
                conclusion: "success",
              },
            ],
          })],
        }]),
      },
    ),
    /runtime-mutation authority/u,
  );
  const newerDifferentSource = run({
    id: 703,
    run_attempt: 1,
    run_number: 43,
    created_at: "2026-08-31T00:02:00Z",
    run_started_at: "2026-08-31T00:02:10Z",
    updated_at: "2026-08-31T00:02:20Z",
    head_sha: "f".repeat(40),
    conclusion: "success",
  });
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: { total_count: 2, workflow_runs: [newerDifferentSource, run()] },
        runAttemptCatalog: runAttemptCatalog([newerDifferentSource, run()]),
        runJobCatalog: runJobCatalog([{
          workflowRunId: "703",
          pages: [jobList({
            workflowRunId: "703",
            sourceCommit: "f".repeat(40),
            steps: [{
              name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
              number: 7,
              status: "completed",
              conclusion: "success",
            }],
          })],
        }]),
      },
    ),
    /runtime-mutation authority/u,
  );
  const lateRerunOfOlderRun = run({
    id: 699,
    run_attempt: 4,
    run_number: 40,
    created_at: "2026-08-30T23:50:00Z",
    run_started_at: "2026-08-31T00:03:00Z",
    updated_at: "2026-08-31T00:03:20Z",
    head_sha: "e".repeat(40),
    conclusion: "success",
  });
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: { total_count: 2, workflow_runs: [lateRerunOfOlderRun, run()] },
        runAttemptCatalog: runAttemptCatalog([lateRerunOfOlderRun, run()]),
        runJobCatalog: runJobCatalog([{
          workflowRunId: "699",
          workflowRunAttempt: "4",
          pages: [jobList({
            workflowRunId: "699",
            workflowRunAttempt: "4",
            sourceCommit: "e".repeat(40),
            steps: [{
              name: "Freeze and stage Oracle, deploy and smoke zero traffic, then seal live authority",
              number: 7,
              status: "completed",
              conclusion: "success",
            }],
          })],
        }]),
      },
    ),
    /runtime-mutation authority/u,
  );
  const completedNoop = run({
    id: 704,
    run_attempt: 1,
    run_number: 44,
    created_at: "2026-08-31T00:04:00Z",
    run_started_at: "2026-08-31T00:04:10Z",
    updated_at: "2026-08-31T00:04:20Z",
    head_sha: "c".repeat(40),
    conclusion: "success",
  });
  const completedNoopAuthority = resolveDiscordRecoveryAuthority(
    run(),
    {
      total_count: 2,
      artifacts: [artifact({ id: 800, name: PRESTAGE_ARTIFACT_NAME }), artifact()],
    },
    {
      ...options,
      jobList: liveJobList(),
      runList: { total_count: 2, workflow_runs: [completedNoop, run()] },
      runAttemptCatalog: runAttemptCatalog([completedNoop, run()]),
      runJobCatalog: runJobCatalog([{
        workflowRunId: "704",
        pages: [jobList({
          workflowRunId: "704",
          sourceCommit: "c".repeat(40),
          job: { conclusion: "skipped" },
          steps: [],
        })],
      }]),
    },
  );
  assert.equal(completedNoopAuthority.recovery_stage, "live");
  assert.equal(
    completedNoopAuthority.freshness_proof.potential_superseders[0].decision,
    "completed-exact-no-runtime-mutation",
  );
  assert.equal(
    completedNoopAuthority.freshness_proof.potential_superseders[0]
      .no_mutation_job_step_proof.job_conclusion,
    "skipped",
  );

  const queuedNewer = run({
    id: 705,
    run_attempt: 1,
    run_number: 45,
    created_at: "2026-08-31T00:05:00Z",
    run_started_at: null,
    updated_at: "2026-08-31T00:05:00Z",
    status: "queued",
    conclusion: null,
  });
  const queuedAuthority = resolveDiscordRecoveryAuthority(
    run(),
    {
      total_count: 2,
      artifacts: [artifact({ id: 800, name: PRESTAGE_ARTIFACT_NAME }), artifact()],
    },
    {
      ...options,
      jobList: liveJobList(),
      runList: { total_count: 2, workflow_runs: [queuedNewer, run()] },
      runAttemptCatalog: runAttemptCatalog([queuedNewer, run()]),
    },
  );
  assert.equal(queuedAuthority.recovery_stage, "live");
  assert.equal(
    queuedAuthority.freshness_proof.potential_superseders[0].decision,
    "queued-behind-shared-group",
  );
  assert.equal(
    queuedAuthority.freshness_proof.potential_superseders[0].run_started_at,
    null,
  );

  const queuedAdvancedAttempt = run({
    run_attempt: 4,
    run_started_at: "2026-08-31T00:06:00Z",
    updated_at: "2026-08-31T00:06:00Z",
    status: "queued",
    conclusion: null,
  });
  const queuedAdvancedAuthority = resolveDiscordRecoveryAuthority(
    run(),
    {
      total_count: 2,
      artifacts: [artifact({ id: 800, name: PRESTAGE_ARTIFACT_NAME }), artifact()],
    },
    {
      ...options,
      jobList: liveJobList(),
      runList: { total_count: 1, workflow_runs: [queuedAdvancedAttempt] },
      runAttemptCatalog: runAttemptCatalog([queuedAdvancedAttempt]),
    },
  );
  assert.equal(queuedAdvancedAuthority.recovery_stage, "live");
  assert.equal(
    queuedAdvancedAuthority.freshness_proof.potential_superseders[0]
      .workflow_run_attempt,
    "4",
  );
  const pendingOlderRun = run({
    id: 698,
    run_attempt: 2,
    run_number: 39,
    created_at: "2026-08-30T23:40:00Z",
    run_started_at: "2026-08-30T23:40:10Z",
    updated_at: "2026-08-30T23:40:15Z",
    status: "in_progress",
    conclusion: null,
  });
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: { total_count: 2, workflow_runs: [pendingOlderRun, run()] },
        runAttemptCatalog: runAttemptCatalog([pendingOlderRun, run()]),
      },
    ),
    /concurrent in-progress deployment ambiguity/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: {
          total_count: 1,
          workflow_runs: [run({ head_sha: "d".repeat(40) })],
        },
        runAttemptCatalog: runAttemptCatalog([run({ head_sha: "d".repeat(40) })]),
      },
    ),
    /exact original authority/u,
  );
  const completedAdvancedNoop = run({
    run_attempt: 4,
    run_started_at: "2026-08-31T00:07:00Z",
    updated_at: "2026-08-31T00:07:20Z",
    conclusion: "success",
  });
  assert.equal(resolveDiscordRecoveryAuthority(
    run(),
    {
      total_count: 2,
      artifacts: [artifact({ id: 800, name: PRESTAGE_ARTIFACT_NAME }), artifact()],
    },
    {
      ...options,
      jobList: liveJobList(),
      runList: { total_count: 1, workflow_runs: [completedAdvancedNoop] },
      runAttemptCatalog: runAttemptCatalog([completedAdvancedNoop]),
      runJobCatalog: runJobCatalog([{
        workflowRunId: RUN_ID,
        workflowRunAttempt: "4",
        pages: [jobList({
          workflowRunAttempt: "4",
          job: { conclusion: "skipped" },
          steps: [],
        })],
      }]),
    },
  ).recovery_stage, "live");
  const laterRerunNoop = run({
    id: 706,
    run_attempt: 2,
    run_number: 46,
    created_at: "2026-08-31T00:08:00Z",
    run_started_at: "2026-08-31T00:09:00Z",
    updated_at: "2026-08-31T00:09:20Z",
    head_sha: "b".repeat(40),
    conclusion: "success",
  });
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: { total_count: 2, workflow_runs: [laterRerunNoop, run()] },
        runAttemptCatalog: runAttemptCatalog([laterRerunNoop, run()]),
        runJobCatalog: runJobCatalog([
          {
            workflowRunId: "706",
            workflowRunAttempt: "1",
            pages: [liveJobList({
              workflowRunId: "706",
              workflowRunAttempt: "1",
              sourceCommit: "b".repeat(40),
            })],
          },
          {
            workflowRunId: "706",
            workflowRunAttempt: "2",
            pages: [jobList({
              workflowRunId: "706",
              workflowRunAttempt: "2",
              sourceCommit: "b".repeat(40),
              job: { conclusion: "skipped" },
              steps: [],
            })],
          },
        ]),
      },
    ),
    /runtime-mutation authority/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: {
          total_count: 1,
          workflow_runs: [run({
            run_started_at: "2026-08-31T00:00:30Z",
            updated_at: "2026-08-31T00:00:20Z",
          })],
        },
        runAttemptCatalog: runAttemptCatalog([run({
          run_started_at: "2026-08-31T00:00:30Z",
          updated_at: "2026-08-31T00:00:20Z",
        })]),
      },
    ),
    /timestamps are internally inconsistent/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: {
          total_count: 2,
          workflow_runs: [
            run({
              id: 696,
              run_attempt: 1,
              run_number: 37,
              status: "queued",
              conclusion: "success",
            }),
            run(),
          ],
        },
        runAttemptCatalog: runAttemptCatalog([
          run({
            id: 696,
            run_attempt: 1,
            run_number: 37,
            status: "queued",
            conclusion: "success",
          }),
          run(),
        ]),
      },
    ),
    /status\/conclusion is inconsistent/u,
  );
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run({ updated_at: "2026-08-31T00:00:21Z" }),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      options,
    ),
    /exact original authority/u,
  );
  const invertedOther = run({
    id: 697,
    run_attempt: 1,
    run_number: 38,
    created_at: "2026-08-30T23:30:00Z",
    run_started_at: "2026-08-30T23:30:20Z",
    updated_at: "2026-08-30T23:30:10Z",
  });
  assert.throws(
    () => resolveDiscordRecoveryAuthority(
      run(),
      { total_count: 1, artifacts: [artifact({ name: PRESTAGE_ARTIFACT_NAME })] },
      {
        ...options,
        runList: { total_count: 2, workflow_runs: [invertedOther, run()] },
        runAttemptCatalog: runAttemptCatalog([invertedOther, run()]),
      },
    ),
    /timestamps are internally inconsistent/u,
  );
});

test("recovery result seals the closed exact evidence set without secret material", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-discord-recovery-"));
  try {
    const bindings = [];
    for (const name of [
      "candidate_state",
      "cloud_pre_mutation_classification",
      "cloud_prior_authority",
      "cloud_restore_readback",
      "cloud_candidate_residue_readback",
      "oracle_pre_mutation_classification",
      "oracle_restore_attestation",
      "oracle_rollback_capture",
      "oracle_stage_manifest",
      "prestage_state",
      "recovery_authority",
    ]) {
      const path = join(root, `${name}.txt`);
      await writeFile(path, `${name}\n`, { mode: 0o600 });
      bindings.push(`${name}=${path}`);
    }
    const catalogDisposition = await writeNotRequiredCatalogDisposition(root, "901", "2");
    const result = await sealDiscordRecoveryResult({
      stage: "live",
      repository: REPOSITORY,
      sourceCommit: SOURCE,
      originalWorkflowRunId: RUN_ID,
      originalWorkflowRunAttempt: RUN_ATTEMPT,
      recoveryWorkflowRunId: "901",
      recoveryWorkflowRunAttempt: "2",
      artifactId: "801",
      artifactDigest: `sha256:${"a".repeat(64)}`,
      recoveredAt: "2026-08-31T00:00:00.000Z",
      catalogDisposition,
      catalogRecoveryRequired: false,
      bindings,
    });
    assert.equal(result.schema_id, DISCORD_RECOVERY_RESULT_SCHEMA_ID);
    assert.equal(result.recovery_stage, "live");
    assert.deepEqual(result.bindings.map((entry) => entry.name), bindings.map((entry) => entry.split("=")[0]).sort());
    await assert.rejects(
      sealDiscordRecoveryResult({
        stage: "live",
        repository: REPOSITORY,
        sourceCommit: SOURCE,
        originalWorkflowRunId: RUN_ID,
        originalWorkflowRunAttempt: RUN_ATTEMPT,
        recoveryWorkflowRunId: "901",
        recoveryWorkflowRunAttempt: "2",
        artifactId: "801",
        artifactDigest: `sha256:${"a".repeat(64)}`,
        recoveredAt: "2026-08-31T00:00:00.000Z",
        catalogDisposition,
        catalogRecoveryRequired: false,
        bindings: bindings.slice(1),
      }),
      /closed set/u,
    );
    const resultPath = join(root, "recovery-result.json");
    await writeFile(resultPath, `${JSON.stringify(result)}\n`, { mode: 0o600 });
    await verifyDiscordRecoveryResult(resultPath, {
      stage: "live",
      repository: REPOSITORY,
      sourceCommit: SOURCE,
      originalWorkflowRunId: RUN_ID,
      originalWorkflowRunAttempt: RUN_ATTEMPT,
      recoveryWorkflowRunId: "901",
      recoveryWorkflowRunAttempt: "2",
      artifactId: "801",
      artifactDigest: `sha256:${"a".repeat(64)}`,
      catalogRecoveryRequired: false,
    });
    const tampered = { ...result, artifact_id: "802" };
    await writeFile(resultPath, `${JSON.stringify(tampered)}\n`);
    await assert.rejects(
      verifyDiscordRecoveryResult(resultPath, {
        stage: "live",
        repository: REPOSITORY,
        sourceCommit: SOURCE,
        originalWorkflowRunId: RUN_ID,
        originalWorkflowRunAttempt: RUN_ATTEMPT,
        recoveryWorkflowRunId: "901",
        recoveryWorkflowRunAttempt: "2",
        artifactId: "801",
        artifactDigest: `sha256:${"a".repeat(64)}`,
        catalogRecoveryRequired: false,
      }),
      /SHA-256 differs/u,
    );
    await writeFile(resultPath, "{\n");
    await assert.rejects(
      verifyDiscordRecoveryResult(resultPath, {
        stage: "live",
        repository: REPOSITORY,
        sourceCommit: SOURCE,
        originalWorkflowRunId: RUN_ID,
        originalWorkflowRunAttempt: RUN_ATTEMPT,
        recoveryWorkflowRunId: "901",
        recoveryWorkflowRunAttempt: "2",
        artifactId: "801",
        artifactDigest: `sha256:${"a".repeat(64)}`,
        catalogRecoveryRequired: false,
      }),
      /not valid JSON/u,
    );
    const stale = await sealDiscordRecoveryResult({
      stage: "live",
      repository: REPOSITORY,
      sourceCommit: SOURCE,
      originalWorkflowRunId: RUN_ID,
      originalWorkflowRunAttempt: RUN_ATTEMPT,
      recoveryWorkflowRunId: "901",
      recoveryWorkflowRunAttempt: "2",
      artifactId: "802",
      artifactDigest: `sha256:${"a".repeat(64)}`,
      recoveredAt: "2026-08-31T00:00:00.000Z",
      catalogDisposition,
      catalogRecoveryRequired: false,
      bindings,
    });
    await writeFile(resultPath, `${JSON.stringify(stale)}\n`);
    await assert.rejects(
      verifyDiscordRecoveryResult(resultPath, {
        stage: "live",
        repository: REPOSITORY,
        sourceCommit: SOURCE,
        originalWorkflowRunId: RUN_ID,
        originalWorkflowRunAttempt: RUN_ATTEMPT,
        recoveryWorkflowRunId: "901",
        recoveryWorkflowRunAttempt: "2",
        artifactId: "801",
        artifactDigest: `sha256:${"a".repeat(64)}`,
        catalogRecoveryRequired: false,
      }),
      /artifact_id differs/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("prestage cleanup result binds exact run, artifact, resolution, and cleanup readbacks", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-discord-prestage-recovery-"));
  try {
    const bindings = [];
    for (const name of [
      "cloud_cleanup_readback",
      "cloud_pre_mutation_readback",
      "cloud_candidate_residue_readback",
      "intended_candidate_authority",
      "oracle_backup_cleanup",
      "oracle_inactive_cleanup",
      "oracle_rollback_capture",
      "prestage_state",
      "recovery_authority",
    ]) {
      const path = join(root, `${name}.txt`);
      await writeFile(path, `${name}\n`, { mode: 0o600 });
      bindings.push(`${name}=${path}`);
    }
    const catalogDisposition = await writeNotRequiredCatalogDisposition(root, "902", "1");
    const result = await sealDiscordRecoveryResult({
      stage: "prestage",
      repository: REPOSITORY,
      sourceCommit: SOURCE,
      originalWorkflowRunId: RUN_ID,
      originalWorkflowRunAttempt: RUN_ATTEMPT,
      recoveryWorkflowRunId: "902",
      recoveryWorkflowRunAttempt: "1",
      artifactId: "800",
      artifactDigest: `sha256:${"b".repeat(64)}`,
      recoveredAt: "2026-08-31T00:02:00.000Z",
      catalogDisposition,
      catalogRecoveryRequired: false,
      bindings,
    });
    assert.equal(result.recovery_stage, "prestage");
    assert.deepEqual(
      result.bindings.map(({ name }) => name),
      bindings.map((entry) => entry.split("=")[0]).sort(),
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

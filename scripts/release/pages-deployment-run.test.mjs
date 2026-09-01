import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { sealCanonicalReport } from "./canonical-release-evidence.mjs";
import {
  resolvePagesDeploymentRun,
  validatePagesDeploymentRunList,
  verifyPagesDeploymentReport,
} from "./pages-deployment-run.mjs";

const SOURCE = "0123456789abcdef0123456789abcdef01234567";
const REPOSITORY = "daejunnom/Clearra";

function run(overrides = {}) {
  return {
    id: 701,
    run_attempt: 1,
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
    head_branch: "main",
    head_sha: SOURCE,
    path: ".github/workflows/pages.yml",
    head_repository: { full_name: REPOSITORY },
    ...overrides,
  };
}

test("resolves exactly one first-attempt same-SHA Pages deployment", async () => {
  const calls = [];
  const result = await resolvePagesDeploymentRun(
    { repository: REPOSITORY, sourceCommit: SOURCE },
    {
      run: async (command, arguments_) => {
        calls.push([command, arguments_]);
        return JSON.stringify({ total_count: 2, workflow_runs: [
          run(),
          run({ id: 702, status: "completed", conclusion: "failure" }),
        ] });
      },
    },
  );
  assert.equal(result.id, "701");
  assert.equal(result.attempt, "1");
  assert.equal(
    result.artifactName,
    `clearra-pages-deployment-authority-${SOURCE}-run-701-attempt-1`,
  );
  assert.equal(calls[0][0], "gh");
  assert.ok(calls[0][1].includes(`head_sha=${SOURCE}`));
});

test("fails closed for ambiguous, truncated, rerun, or foreign Pages histories", () => {
  const options = { repository: REPOSITORY, sourceCommit: SOURCE };
  assert.throws(
    () => validatePagesDeploymentRunList({ total_count: 2, workflow_runs: [run()] }, options),
    /complete and non-truncated/u,
  );
  assert.throws(
    () => validatePagesDeploymentRunList({ total_count: 2, workflow_runs: [run(), run({ id: 2 })] }, options),
    /exactly one successful/u,
  );
  assert.throws(
    () => validatePagesDeploymentRunList({ total_count: 1, workflow_runs: [run({ run_attempt: 2 })] }, options),
    /rerun attempts are forbidden/u,
  );
  assert.throws(
    () => validatePagesDeploymentRunList({
      total_count: 1,
      workflow_runs: [run({ head_repository: { full_name: "someone/fork" } })],
    }, options),
    /same-repository authority/u,
  );
});

test("verifies canonical Pages report against run and acceptance bindings", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-pages-run-"));
  try {
    const report = sealCanonicalReport({
      schema_id: "clearra.pages.deployment-authority.v2",
      mode: "forward",
      repository: REPOSITORY,
      source_commit: SOURCE,
      workflow_source_commit: SOURCE,
      workflow_run_id: "701",
      workflow_run_attempt: "1",
      workflow_path: ".github/workflows/pages.yml",
      accepted_run_id: "601",
      accepted_run_attempt: "1",
      artifact_id: "501",
      artifact_name: "github-pages",
      artifact_digest: `sha256:${"a".repeat(64)}`,
      artifact_sha256: "a".repeat(64),
      artifact_api_readback_sha256: "b".repeat(64),
      workflow_run_api_readback_sha256: "c".repeat(64),
      deployment_id: SOURCE,
      deployment_status: "succeed",
      deployment_api_readback_sha256: "d".repeat(64),
      page_url: "https://daejunnom.github.io/Clearra/",
      base_path: "/Clearra",
      pages_configuration_api_readback_sha256: "e".repeat(64),
      live_identity_sha256: "f".repeat(64),
      live_payload_set_sha256: null,
      rollback_capture_report_sha256: null,
      rollback_artifact_sha256: null,
      rollback_tar_sha256: null,
      rollback_capture_run_id: null,
      rollback_report_artifact_id: null,
      rollback_report_artifact_name: null,
      rollback_report_artifact_digest: null,
      rollback_report_artifact_api_readback_sha256: null,
      rollback_report_file_sha256: null,
      status: "active",
    });
    const path = join(root, "pages-deployment-authority.json");
    const canonical = JSON.stringify(report, Object.keys(report).sort());
    // canonicalJson is shallow here because the report has no nested objects.
    await writeFile(path, `${canonical}\n`, { mode: 0o600 });
    const verified = await verifyPagesDeploymentReport(path, {
      repository: REPOSITORY,
      sourceCommit: SOURCE,
      workflowRunId: "701",
      workflowRunAttempt: "1",
      acceptedRunId: "601",
      acceptedRunAttempt: "1",
    });
    assert.equal(verified.status, "active");
    await assert.rejects(
      verifyPagesDeploymentReport(path, {
        repository: REPOSITORY,
        sourceCommit: SOURCE,
        workflowRunId: "999",
        workflowRunAttempt: "1",
        acceptedRunId: "601",
        acceptedRunAttempt: "1",
      }),
      /exact accepted run binding/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

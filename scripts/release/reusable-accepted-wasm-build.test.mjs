import assert from "node:assert/strict";
import test from "node:test";

import { resolveReusableAcceptedWasmBuild } from "./reusable-accepted-wasm-build.mjs";

const REPOSITORY = "daejunnom/Clearra";
const SOURCE = "a".repeat(40);
const CURRENT_RUN = "300";

test("selects the latest complete failed exact-source WASM producer", async () => {
  const calls = [];
  const result = await resolveReusableAcceptedWasmBuild(options(), {
    listRuns: async () => collection("workflow_runs", [
      run(CURRENT_RUN, { status: "in_progress", conclusion: null, created_at: time(30) }),
      run("200", { conclusion: "failure", created_at: time(20) }),
      run("100", { conclusion: "cancelled", created_at: time(10) }),
    ]),
    listJobs: async (runId, attempt) => {
      calls.push(["jobs", runId, attempt]);
      return collection("jobs", [job(runId)]);
    },
    listArtifacts: async (runId) => {
      calls.push(["artifacts", runId]);
      return collection("artifacts", [artifact(runId)]);
    },
  });
  assert.deepEqual(result, {
    runId: "200",
    runAttempt: "1",
    artifactId: "900",
    artifactName: artifactName("200"),
    artifactDigest: `sha256:${"b".repeat(64)}`,
    artifactBytes: 10_000_000,
  });
  assert.deepEqual(calls, [["jobs", "200", "1"], ["artifacts", "200"]]);
});

test("skips an invalid newer candidate and uses an independently valid older attempt", async () => {
  const result = await resolveReusableAcceptedWasmBuild(options(), {
    listRuns: async () => collection("workflow_runs", [
      run(CURRENT_RUN, { status: "queued", conclusion: null, created_at: time(30) }),
      run("200", { conclusion: "failure", created_at: time(20) }),
      run("100", { conclusion: "timed_out", created_at: time(10) }),
    ]),
    listJobs: async (runId) => collection("jobs", [
      job(runId, runId === "200" ? { conclusion: "failure" } : {}),
    ]),
    listArtifacts: async (runId) => collection("artifacts", [artifact(runId)]),
  });
  assert.equal(result.runId, "100");
});

test("never reuses a successful prior canonical run or an active prior run", async () => {
  await assert.rejects(
    resolveReusableAcceptedWasmBuild(options(), dependencies([
      run(CURRENT_RUN, { status: "in_progress", conclusion: null, created_at: time(30) }),
      run("200", { conclusion: "success", created_at: time(20) }),
    ])),
    /existing successful canonical run/u,
  );
  const activeOnly = await resolveReusableAcceptedWasmBuild(options(), dependencies([
    run(CURRENT_RUN, { status: "in_progress", conclusion: null, created_at: time(30) }),
    run("200", { status: "in_progress", conclusion: null, created_at: time(20) }),
  ]));
  assert.equal(activeOnly, null);
});

test("expired, ambiguous, wrong-run, or failed-upload artifacts have no reuse authority", async () => {
  for (const mutate of [
    (value) => { value.expired = true; },
    (value) => { value.workflow_run.id = 201; },
    (value) => { value.digest = "sha256:short"; },
    (value) => { value.created_at = time(25); },
  ]) {
    const result = await resolveReusableAcceptedWasmBuild(options(), {
      listRuns: async () => collection("workflow_runs", [
        run(CURRENT_RUN, { status: "in_progress", conclusion: null, created_at: time(30) }),
        run("200", { conclusion: "failure", created_at: time(20) }),
      ]),
      listJobs: async () => collection("jobs", [job("200")]),
      listArtifacts: async () => {
        const value = artifact("200");
        mutate(value);
        return collection("artifacts", [value]);
      },
    });
    assert.equal(result, null);
  }

  const duplicate = await resolveReusableAcceptedWasmBuild(options(), {
    listRuns: async () => collection("workflow_runs", [
      run(CURRENT_RUN, { status: "in_progress", conclusion: null, created_at: time(30) }),
      run("200", { conclusion: "failure", created_at: time(20) }),
    ]),
    listJobs: async () => collection("jobs", [job("200")]),
    listArtifacts: async () => collection("artifacts", [artifact("200"), artifact("200")]),
  });
  assert.equal(duplicate, null);

  const failedUpload = await resolveReusableAcceptedWasmBuild(options(), {
    listRuns: async () => collection("workflow_runs", [
      run(CURRENT_RUN, { status: "in_progress", conclusion: null, created_at: time(30) }),
      run("200", { conclusion: "failure", created_at: time(20) }),
    ]),
    listJobs: async () => collection("jobs", [job("200", { uploadConclusion: "failure" })]),
    listArtifacts: async () => collection("artifacts", [artifact("200")]),
  });
  assert.equal(failedUpload, null);
});

test("rejects truncated history, foreign identity, reruns, and a missing current run", async () => {
  await assert.rejects(
    resolveReusableAcceptedWasmBuild(options(), {
      ...dependencies([]),
      listRuns: async () => ({ total_count: 2, workflow_runs: [run(CURRENT_RUN)] }),
    }),
    /complete and bounded/u,
  );
  await assert.rejects(
    resolveReusableAcceptedWasmBuild(options(), dependencies([
      run(CURRENT_RUN, { head_sha: "c".repeat(40) }),
    ])),
    /exact canonical workflow identity/u,
  );
  await assert.rejects(
    resolveReusableAcceptedWasmBuild(
      { ...options(), currentRunAttempt: "2" },
      dependencies([run(CURRENT_RUN, { run_attempt: 2 })]),
    ),
    /first canonical run attempt/u,
  );
  await assert.rejects(
    resolveReusableAcceptedWasmBuild(options(), dependencies([run("200")])),
    /one active current run/u,
  );
});

function options() {
  return {
    repository: REPOSITORY,
    sourceCommit: SOURCE,
    currentRunId: CURRENT_RUN,
    currentRunAttempt: "1",
  };
}

function dependencies(runs) {
  return {
    listRuns: async () => collection("workflow_runs", runs),
    listJobs: async (runId) => collection("jobs", [job(runId)]),
    listArtifacts: async (runId) => collection("artifacts", [artifact(runId)]),
  };
}

function collection(key, values) {
  return { total_count: values.length, [key]: values };
}

function run(id, overrides = {}) {
  return {
    id: Number(id),
    run_attempt: 1,
    event: "workflow_dispatch",
    head_branch: "main",
    head_sha: SOURCE,
    path: ".github/workflows/release-cli.yml",
    status: "completed",
    conclusion: "failure",
    created_at: time(10),
    ...overrides,
  };
}

function job(runId, overrides = {}) {
  const uploadConclusion = overrides.uploadConclusion ?? "success";
  return {
    id: 800,
    run_id: Number(runId),
    run_attempt: 1,
    head_sha: SOURCE,
    name: "release-acceptance-wasm-build",
    status: "completed",
    conclusion: "success",
    started_at: time(20),
    completed_at: time(22),
    steps: [{
      name: "Upload accepted WASM build",
      status: "completed",
      conclusion: uploadConclusion,
    }],
    ...overrides,
  };
}

function artifact(runId) {
  return {
    id: 900,
    name: artifactName(runId),
    size_in_bytes: 10_000_000,
    expired: false,
    created_at: time(21),
    digest: `sha256:${"b".repeat(64)}`,
    archive_download_url: `https://api.github.com/repos/${REPOSITORY}/actions/artifacts/900/zip`,
    workflow_run: {
      id: Number(runId),
      head_branch: "main",
      head_sha: SOURCE,
    },
  };
}

function artifactName(runId) {
  return `accepted-wasm-build-${SOURCE}-run-${runId}-attempt-1`;
}

function time(minute) {
  return `2026-09-09T14:${String(minute).padStart(2, "0")}:00Z`;
}

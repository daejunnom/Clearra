import assert from "node:assert/strict";
import test from "node:test";
import { resolveApprovedReleaseSource } from "./approved-release-source.mjs";
import { checkpointCandidateArtifactName } from "./discord-production-checkpoint.mjs";

const repository = "daejunnom/Clearra";
const source = "a".repeat(40), controller = "b".repeat(40), tagObject = "c".repeat(40);
function fixture({ historical = false, release = null, tagCommit = null, mutateRun } = {}) {
  const calls = [];
  const context = { repository, ref: "refs/heads/main", controllerSource: historical ? controller : source,
    eventName: "workflow_dispatch", event: { repository: { full_name: repository },
      inputs: { discord_run_id: "10", discord_run_attempt: "1" } } };
  const run = { id: 10, run_attempt: 1, path: ".github/workflows/discord-deploy.yml",
    event: "workflow_run", head_branch: "main", head_sha: source,
    status: "completed", conclusion: "success", repository: { full_name: repository },
    head_repository: { full_name: repository } };
  const dependencies = {
    verifyCanonical: async input => { calls.push({ verify: input }); return { id: "20", attempt: "2" }; },
    apiGet: async path => {
      calls.push({ path });
      if (path === "/actions/runs/10/attempts/1") return mutateRun ? mutateRun({ ...run }) : run;
      if (path === "/git/ref/heads/main") return { object: { type: "commit", sha: context.controllerSource } };
      if (path === "/releases/tags/v0.8.1") return release;
      if (path === "/git/ref/tags/v0.8.1") return tagCommit === null ? null : {
        ref: "refs/tags/v0.8.1", object: { type: "tag", sha: tagObject } };
      if (path === `/git/tags/${tagObject}`) return { sha: tagObject, tag: "v0.8.1",
        object: { type: "commit", sha: tagCommit } };
      if (path === "/actions/runs/10/artifacts?per_page=100") return { total_count: 1, artifacts: [{
        id: 30, expired: false, digest: `sha256:${"d".repeat(64)}`,
        name: checkpointCandidateArtifactName(source, "10", "1") }] };
      throw new Error(`unexpected read: ${path}`);
    },
  };
  return { context, dependencies, calls, run };
}

test("publication resolves exact completed production and approved artifacts without dispatches", async () => {
  const f = fixture();
  const result = await resolveApprovedReleaseSource(f.context, f.dependencies);
  assert.equal(result.publish, "true");
  assert.equal(result.source_commit, source);
  assert.equal(result.accepted_run_id, "20");
  assert.equal(result.accepted_run_attempt, "2");
  assert.equal(result.discord_run_id, "10");
  assert.equal(result.artifact_id, "30");
  assert.equal(result.tag_exists, "false");
  assert.deepEqual(f.calls.filter(call => call.verify).map(call => call.verify), [
    { repository, sourceCommit: source, expectedCount: 1 },
  ]);
});

test("automatic routing is bound to the exact successful Discord completion", async () => {
  const f = fixture();
  f.context.eventName = "workflow_run";
  f.context.event.action = "completed";
  f.context.event.workflow_run = f.run;
  assert.equal((await resolveApprovedReleaseSource(f.context, f.dependencies)).source_commit, source);
  for (const mutation of [{ conclusion: "failure" }, { head_sha: controller },
    { path: ".github/workflows/release-cli.yml" }, { head_repository: { full_name: "fork/Clearra" } }]) {
    await assert.rejects(resolveApprovedReleaseSource({ ...f.context, event: { ...f.context.event,
      workflow_run: { ...f.run, ...mutation } } }, f.dependencies), /requires successful|run identity/u);
  }
});

test("failed, pending, foreign and non-main Discord runs cannot qualify publication", async () => {
  for (const mutation of [{ status: "in_progress", conclusion: null }, { conclusion: "failure" },
    { run_attempt: 2 }, { head_branch: "other" }, { event: "pull_request" },
    { repository: { full_name: "fork/Clearra" } }, { head_sha: "HEAD" }]) {
    const f = fixture({ mutateRun: run => ({ ...run, ...mutation }) });
    await assert.rejects(resolveApprovedReleaseSource(f.context, f.dependencies), /run identity/u);
    assert.equal(f.calls.filter(call => call.verify).length, 0);
  }
});

test("historical source is permitted only for read-only matching immutable evidence repair", async () => {
  const f = fixture({ historical: true, tagCommit: source, release: { immutable: true } });
  const result = await resolveApprovedReleaseSource(f.context, f.dependencies);
  assert.equal(result.status, "immutable-evidence-repair");
  assert.equal(result.source_commit, source);
  assert.equal(result.controller_source_commit, controller);
  for (const setup of [{ historical: true }, { historical: true, tagCommit: source,
    release: { immutable: false, draft: true } }]) {
    const g = fixture(setup);
    await assert.rejects(resolveApprovedReleaseSource(g.context, g.dependencies), /read-only immutable/u);
  }
});

test("another source's published immutable version is preserved with no false publication receipt", async () => {
  const f = fixture({ tagCommit: controller, release: { immutable: true, tag_name: "v0.8.1",
    target_commitish: controller, draft: false, prerelease: false } });
  const result = await resolveApprovedReleaseSource(f.context, f.dependencies);
  assert.equal(result.publish, "false");
  assert.equal(result.published_source_commit, controller);
  assert.equal(result.source_commit, source);
  assert.equal(result.status, "immutable-version-already-published");
  assert.equal(f.calls.filter(call => call.verify).length, 0);
});

test("missing or ambiguous checkpoint artifacts cannot masquerade as production evidence", async () => {
  for (const catalog of [{ total_count: 0, artifacts: [] }, { total_count: 101, artifacts: [] },
    { total_count: 1, artifacts: [{ id: 30, expired: true }] }]) {
    const f = fixture(), api = f.dependencies.apiGet;
    f.dependencies.apiGet = path => path.endsWith("/artifacts?per_page=100") ? catalog : api(path);
    await assert.rejects(resolveApprovedReleaseSource(f.context, f.dependencies), /artifact catalog|checkpoint artifact/u);
  }
});

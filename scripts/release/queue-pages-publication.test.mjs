import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { queuePagesPublication } from "./queue-pages-publication.mjs";
import { classifyDeploymentImpact } from "./deployment-impact.mjs";

const repository = "daejunnom/Clearra";
const sourceCommit = "a".repeat(40);
const snapshotSha = "b".repeat(40);
const options = { repository, sourceCommit, snapshotSha, acceptanceRunId: "10",
  ref: "refs/heads/main", attempt: "1" };
const base = `repos/${repository}`;
const workflowNames = { "10": "release-cli.yml", "20": "pages-rollback.yml", "30": "pages.yml" };

function fixture({ mutateRun, mutateReceipt, mutateMain, verify, pause, post } = {}) {
  const calls = [];
  const messages = [];
  let time = 0;
  let mainReads = 0;
  const reads = new Map();
  const dependencies = {
    now: () => time,
    pause: async (ms) => { time += pause ? pause(ms) : ms; },
    record: (message) => messages.push(message),
    verifyCanonical: async (input) => {
      calls.push({ kind: "verify", input });
      if (verify) await verify(input);
    },
    api: async (method, endpoint, body) => {
      calls.push({ method, endpoint, body });
      if (endpoint === `${base}/git/ref/heads/main`) {
        mainReads += 1;
        return { object: { type: "commit", sha: mutateMain?.(mainReads) ?? sourceCommit } };
      }
      if (method === "POST") {
        post?.(endpoint, body);
        assert.ok(endpoint === `${base}/actions/workflows/pages-rollback.yml/dispatches` ||
          endpoint === `${base}/actions/workflows/pages.yml/dispatches`);
        const id = endpoint.includes("pages-rollback.yml") ? 20 : 30;
        const receipt = { workflow_run_id: id,
          run_url: `https://api.github.com/${base}/actions/runs/${id}`,
          html_url: `https://github.com/${repository}/actions/runs/${id}` };
        return mutateReceipt?.(receipt) ?? receipt;
      }
      const id = endpoint.split("/").at(-1);
      assert.ok(Object.hasOwn(workflowNames, id), endpoint);
      const count = (reads.get(id) ?? 0) + 1;
      reads.set(id, count);
      const run = { id: Number(id), run_attempt: 1, head_sha: sourceCommit,
        head_branch: "main", event: "workflow_dispatch",
        path: `.github/workflows/${workflowNames[id]}`, repository: { full_name: repository },
        status: "completed", conclusion: "success" };
      return mutateRun?.(run, count) ?? run;
    },
  };
  return { dependencies, calls, messages, posts: () => calls.filter((call) => call.method === "POST") };
}

test("queues capture and Pages only after exact success and canonical history verification", async () => {
  const f = fixture({ mutateRun: (run, count) => count === 1 ?
    { ...run, status: "in_progress", conclusion: null } : run });
  const result = await queuePagesPublication(options, f.dependencies);
  assert.deepEqual(result, { acceptanceRunId: "10", captureRunId: "20", pagesRunId: "30", sourceCommit });
  assert.deepEqual(f.posts().map((call) => call.body), [
    { ref: "main", inputs: { mode: "capture", snapshot_sha: snapshotSha,
      expected_current_main: sourceCommit, legacy_release_tag: "", current_pages_sha: "",
      snapshot_run_id: "", restore_authorization: "" } },
    { ref: "main", inputs: { accepted_sha: sourceCommit, rollback_snapshot_sha: snapshotSha,
      rollback_capture_run_id: "20" } },
  ]);
  const operations = f.calls.filter((call) => call.kind === "verify" || call.method === "POST");
  assert.deepEqual(operations.map((call) => call.kind ?? call.method), ["verify", "POST", "verify", "POST"]);
  for (const call of operations.filter((call) => call.kind === "verify")) {
    assert.deepEqual(call.input, { repository, sourceCommit, expectedCount: 1,
      expectedRunId: "10", expectedRunAttempt: "1" });
  }
  assert.equal(f.messages.filter((message) => message.startsWith("Dispatched")).length, 2);
});

test("rejects foreign, malformed, branch and rerun queue inputs before API access", async () => {
  for (const change of [{ repository: "other/Clearra" }, { sourceCommit: "main" },
    { snapshotSha: "HEAD" }, { acceptanceRunId: "1e2" }, { attempt: "2" },
    { ref: "refs/tags/v0.8.0" }]) {
    const f = fixture();
    await assert.rejects(queuePagesPublication({ ...options, ...change }, f.dependencies), /exact main/u);
    assert.equal(f.calls.length, 0);
  }
});

test("failed, cancelled and timed-out acceptance never starts rollback or Pages", async () => {
  for (const conclusion of ["failure", "cancelled", "timed_out", "skipped", "neutral"]) {
    const f = fixture({ mutateRun: (run) => ({ ...run, conclusion }) });
    await assert.rejects(queuePagesPublication(options, f.dependencies), /stopped after release-cli/u);
    assert.equal(f.posts().length, 0);
  }
});

test("verifies every run identity including first attempt rather than trusting its success label", async () => {
  for (const change of [{ id: 11 }, { run_attempt: 2 }, { head_sha: snapshotSha },
    { head_branch: "other" }, { path: ".github/workflows/candidate-preflight.yml" },
    { event: "push" }, { repository: { full_name: "other/Clearra" } }]) {
    const f = fixture({ mutateRun: (run) => ({ ...run, ...change }) });
    await assert.rejects(queuePagesPublication(options, f.dependencies), /run identity/u);
    assert.equal(f.posts().length, 0);
  }
});

test("does not fabricate acceptance when canonical history validation fails", async () => {
  const f = fixture({ verify: async () => { throw new Error("duplicate canonical success"); } });
  await assert.rejects(queuePagesPublication(options, f.dependencies), /duplicate canonical/u);
  assert.equal(f.posts().length, 0);
});

test("a failed capture prevents Pages publication", async () => {
  const f = fixture({ mutateRun: (run) => run.id === 20 ? { ...run, conclusion: "failure" } : run });
  await assert.rejects(queuePagesPublication(options, f.dependencies), /stopped after pages-rollback/u);
  assert.equal(f.posts().length, 1);
});

test("main movement before capture stops the queued publication", async () => {
  const f = fixture({ mutateMain: (count) => count > 1 ? snapshotSha : sourceCommit });
  await assert.rejects(queuePagesPublication(options, f.dependencies), /no longer exact current main/u);
  assert.equal(f.posts().length, 0);
});

test("a finite lease does not turn pending acceptance into success", async () => {
  const f = fixture({ mutateRun: (run) => ({ ...run, status: "queued", conclusion: null }),
    pause: () => 121 * 60_000 });
  await assert.rejects(queuePagesPublication(options, f.dependencies), /lease expired/u);
  assert.equal(f.posts().length, 0);
});

test("uncertain or foreign dispatch receipts are never guessed or retried", async () => {
  for (const receipt of [{}, { workflow_run_id: 20, html_url: "https://other.invalid/run" }]) {
    const f = fixture({ mutateReceipt: () => receipt });
    await assert.rejects(queuePagesPublication(options, f.dependencies), /receipt uncertain/u);
    assert.equal(f.posts().length, 1);
  }
  const f = fixture({ post: () => { throw new Error("uncertain transport failure"); } });
  await assert.rejects(queuePagesPublication(options, f.dependencies), /transport failure/u);
  assert.equal(f.posts().length, 1);
});

test("a failed Pages child is reported without redispatching publication", async () => {
  const f = fixture({ mutateRun: (run) => run.id === 30 ? { ...run, conclusion: "failure" } : run });
  await assert.rejects(queuePagesPublication(options, f.dependencies), /stopped after pages.yml/u);
  assert.equal(f.posts().length, 2);
});

test("the queue has manual-only scope, bounded lifetime and no direct publication permission", () => {
  const workflow = readFileSync(new URL("../../.github/workflows/queue-pages-publication.yml", import.meta.url), "utf8");
  assert.match(workflow, /on:\s+workflow_dispatch:/u);
  assert.doesNotMatch(workflow, /^ {2}(?:push|pull_request|workflow_run|schedule):/mu);
  assert.match(workflow, /permissions:\s+contents: read\s+actions: write/u);
  assert.doesNotMatch(workflow, /pages: write|contents: write|id-token: write|uses: actions\/deploy-pages/u);
  assert.match(workflow, /group: queued-pages-publication/u);
  assert.doesNotMatch(workflow, /group: pages\s/u);
  assert.match(workflow, /timeout-minutes: 125/u);
  assert.match(workflow, /persist-credentials: false/u);
  assert.match(workflow, /run: node scripts\/release\/queue-pages-publication.mjs/u);
  assert.doesNotMatch(workflow, /npm ci|cargo |wasm-bindgen|continue-on-error/u);
});

test("queue-only changes do not request unrelated CLI or Discord deployment", () => {
  for (const path of [".github/workflows/queue-pages-publication.yml", "scripts/release/queue-pages-publication.mjs"]) {
    const impact = classifyDeploymentImpact([path]);
    assert.equal(impact.deployPages, true);
    assert.equal(impact.deployCli, false);
    assert.equal(impact.deployDiscord, false);
    assert.equal(impact.deployGui, false);
    assert.equal(impact.requiresFullGate, true);
  }
});

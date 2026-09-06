import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { resolveCanonicalAcceptanceRun } from "./canonical-acceptance-run.mjs";

const REPOSITORY = "daejunnom/Clearra";
const SHA = /^[0-9a-f]{40}$/u;
const ID = /^[1-9][0-9]*$/u;
const WAIT_MS = 30_000;
const LEASE_MS = 120 * 60_000;
const PENDING = new Set(["queued", "in_progress", "waiting", "pending", "requested"]);

// This module only sequences existing authority owners. It never creates an
// acceptance receipt, restores Pages, modifies a release, or rebuilds artifacts.
export async function queuePagesPublication(options, dependencies) {
  const { sourceCommit, snapshotSha, acceptanceRunId, repository, ref, attempt } = options;
  if (repository !== REPOSITORY || ref !== "refs/heads/main" || attempt !== "1" ||
      !SHA.test(sourceCommit ?? "") || !SHA.test(snapshotSha ?? "") ||
      !ID.test(acceptanceRunId ?? "")) {
    throw new Error("Pages queue requires exact main, full commits and first-attempt run IDs");
  }
  const { api, verifyCanonical, now, pause, record } = dependencies;
  const deadline = now() + LEASE_MS;
  const base = `repos/${repository}`;
  const runUrl = (id) => `https://github.com/${repository}/actions/runs/${id}`;
  const currentMain = async () => {
    const head = await api("GET", `${base}/git/ref/heads/main`);
    if (head?.object?.sha !== sourceCommit || head?.object?.type !== "commit") {
      throw new Error("Pages queue source is no longer exact current main");
    }
  };
  const withinLease = () => {
    if (now() >= deadline) throw new Error("Pages queue lease expired; no next stage dispatched");
  };
  const waitFor = async (workflow, id) => {
    record(`Waiting for ${workflow}: ${runUrl(id)}`);
    while (true) {
      withinLease();
      await currentMain();
      const run = await api("GET", `${base}/actions/runs/${id}`);
      if (String(run?.id) !== id || String(run?.run_attempt) !== "1" ||
          run?.head_sha !== sourceCommit || run?.head_branch !== "main" ||
          run?.event !== "workflow_dispatch" ||
          run?.path !== `.github/workflows/${workflow}` ||
          run?.repository?.full_name !== repository) {
        throw new Error(`Pages queue rejected the bound ${workflow} run identity`);
      }
      if (run.status === "completed") {
        if (run.conclusion !== "success") {
          throw new Error(`Pages queue stopped after ${workflow}: ${run.conclusion}`);
        }
        return;
      }
      if (!PENDING.has(run.status) || run.conclusion !== null) {
        throw new Error(`Pages queue rejected the ${workflow} execution state`);
      }
      await pause(WAIT_MS);
    }
  };
  const dispatch = async (workflow, inputs) => {
    withinLease();
    await currentMain();
    // GitHub API 2026-03-10 returns the created run ID. Never retry a POST or
    // infer ownership from a recent-run list after an uncertain dispatch.
    const receipt = await api("POST", `${base}/actions/workflows/${workflow}/dispatches`, {
      ref: "main", inputs,
    });
    const id = String(receipt?.workflow_run_id ?? "");
    if (!ID.test(id) || receipt?.html_url !== runUrl(id) ||
        receipt?.run_url !== `https://api.github.com/${base}/actions/runs/${id}`) {
      throw new Error(`Pages queue dispatch receipt uncertain for ${workflow}; do not retry automatically`);
    }
    record(`Dispatched ${workflow}: ${runUrl(id)}`);
    return id;
  };
  const verify = () => verifyCanonical({
    repository, sourceCommit, expectedCount: 1,
    expectedRunId: acceptanceRunId, expectedRunAttempt: "1",
  });

  await waitFor("release-cli.yml", acceptanceRunId);
  await verify();
  const captureRunId = await dispatch("pages-rollback.yml", {
    mode: "capture", snapshot_sha: snapshotSha, expected_current_main: sourceCommit,
    legacy_release_tag: "", current_pages_sha: "", snapshot_run_id: "", restore_authorization: "",
  });
  await waitFor("pages-rollback.yml", captureRunId);
  await verify();
  const pagesRunId = await dispatch("pages.yml", {
    accepted_sha: sourceCommit, rollback_snapshot_sha: snapshotSha,
    rollback_capture_run_id: captureRunId,
  });
  await waitFor("pages.yml", pagesRunId);
  record(`Pages workflow completed: ${runUrl(pagesRunId)}`);
  return { acceptanceRunId, captureRunId, pagesRunId, sourceCommit };
}

export function githubApi(method, endpoint, body) {
  const args = ["api", "--method", method, "-H", "X-GitHub-Api-Version: 2026-03-10", endpoint];
  if (body !== undefined) args.push("--input", "-");
  let output;
  try {
    output = execFileSync("gh", args, {
      encoding: "utf8", input: body === undefined ? undefined : JSON.stringify(body),
      timeout: 30_000, maxBuffer: 4 * 1024 * 1024,
      windowsHide: true, stdio: ["pipe", "pipe", "pipe"],
    });
  } catch {
    throw new Error(`GitHub ${method} failed at ${endpoint}; dispatches are not retried`);
  }
  try { return JSON.parse(output); }
  catch { throw new Error(`GitHub ${method} returned an invalid receipt at ${endpoint}`); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const record = (message) => {
    process.stdout.write(`${message}\n`);
    if (process.env.GITHUB_STEP_SUMMARY) {
      appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${message}\n\n`);
    }
  };
  try {
    if (process.argv.length !== 2) throw new Error("Pages queue accepts only workflow environment inputs");
    await queuePagesPublication({
      repository: process.env.GITHUB_REPOSITORY, ref: process.env.GITHUB_REF,
      attempt: process.env.GITHUB_RUN_ATTEMPT, sourceCommit: process.env.GITHUB_SHA,
      snapshotSha: process.env.SNAPSHOT_SHA, acceptanceRunId: process.env.ACCEPTANCE_RUN_ID,
    }, { api: githubApi, verifyCanonical: resolveCanonicalAcceptanceRun,
      now: Date.now, pause: sleep, record });
  } catch (error) {
    record(`Pages queue failed: ${error.message}`);
    process.exitCode = 1;
  }
}

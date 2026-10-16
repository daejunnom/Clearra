// Resolve publication inputs only. This owner never builds, deploys, changes a
// tag/release, dispatches acceptance or turns a failed run into a successful one.
import { execFileSync } from "node:child_process";
import { appendFile, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { resolveCanonicalAcceptanceRun } from "./canonical-acceptance-run.mjs";
import { checkpointCandidateArtifactName } from "./discord-production-checkpoint.mjs";
import { CURRENT_PRODUCT_TAG, CURRENT_PRODUCT_VERSION } from "./current-product-release.mjs";

const REPOSITORY = "daejunnom/Clearra";
const SHA = /^[0-9a-f]{40}$/u;
const ID = /^[1-9][0-9]*$/u;
const DIGEST = /^sha256:[0-9a-f]{64}$/u;

export async function resolveApprovedReleaseSource(context, dependencies) {
  const { repository, ref, controllerSource, eventName, event } = context;
  if (repository !== REPOSITORY || ref !== "refs/heads/main" ||
      event?.repository?.full_name !== repository || !SHA.test(controllerSource ?? "")) {
    throw new Error("Release publication requires the trusted repository main controller");
  }
  const upstream = eventName === "workflow_run" ? event.workflow_run : null;
  if (eventName !== "workflow_dispatch" && !(eventName === "workflow_run" &&
      event.action === "completed" && upstream?.conclusion === "success" &&
      upstream?.status === "completed" && upstream?.path === ".github/workflows/discord-deploy.yml" &&
      upstream?.head_repository?.full_name === repository)) {
    throw new Error("Release publication requires successful Discord completion or explicit publication retry");
  }
  const runId = String(upstream?.id ?? event.inputs?.discord_run_id ?? "");
  const runAttempt = String(upstream?.run_attempt ?? event.inputs?.discord_run_attempt ?? "");
  if (!ID.test(runId) || !ID.test(runAttempt)) throw new Error("Exact Discord run and attempt are required");
  const { apiGet, verifyCanonical } = dependencies;
  const [run, main, release, tagRef] = await Promise.all([
    apiGet(`/actions/runs/${runId}/attempts/${runAttempt}`),
    apiGet("/git/ref/heads/main"),
    apiGet(`/releases/tags/${CURRENT_PRODUCT_TAG}`, { allowNotFound: true }),
    apiGet(`/git/ref/tags/${CURRENT_PRODUCT_TAG}`, { allowNotFound: true }),
  ]);
  if (main?.object?.type !== "commit" || main.object.sha !== controllerSource) {
    throw new Error("Release publication controller is no longer current main");
  }
  const source = run?.head_sha;
  if (String(run?.id) !== runId || String(run?.run_attempt) !== runAttempt ||
      run?.path !== ".github/workflows/discord-deploy.yml" || run.head_branch !== "main" ||
      !["workflow_dispatch", "workflow_run"].includes(run.event) ||
      run.status !== "completed" || run.conclusion !== "success" || !SHA.test(source ?? "") ||
      run.repository?.full_name !== repository || run.head_repository?.full_name !== repository ||
      (upstream && upstream.head_sha !== source)) {
    throw new Error("Release publication rejected the exact completed Discord run identity");
  }
  let tagCommit = null;
  if (tagRef !== null) {
    if (tagRef?.ref !== `refs/tags/${CURRENT_PRODUCT_TAG}` ||
        tagRef.object?.type !== "tag" || !SHA.test(tagRef.object.sha ?? "")) {
      throw new Error("Release tag must remain an exact annotated tag");
    }
    const tag = await apiGet(`/git/tags/${tagRef.object.sha}`);
    if (tag?.sha !== tagRef.object.sha || tag.tag !== CURRENT_PRODUCT_TAG ||
        tag.object?.type !== "commit" || !SHA.test(tag.object.sha ?? "")) {
      throw new Error("Release tag target is invalid");
    }
    tagCommit = tag.object.sha;
  }
  if (release !== null && release?.immutable === true && tagCommit !== source) {
    if (!tagCommit || release.tag_name !== CURRENT_PRODUCT_TAG || release.draft !== false ||
        release.prerelease !== false || release.target_commitish !== tagCommit) {
      throw new Error("Existing immutable Release and tag do not agree");
    }
    // Version reuse is not publication of this candidate. Keep the published
    // bytes untouched and emit no publication receipt for a different source.
    return { publish: "false", status: "immutable-version-already-published",
      published_source_commit: tagCommit, source_commit: source };
  }
  if (tagCommit !== null && tagCommit !== source) throw new Error("Release tag belongs to another source");
  if (source !== controllerSource && !(release?.immutable === true && tagCommit === source)) {
    throw new Error("Historical source is permitted only for read-only immutable Release evidence repair");
  }
  const accepted = await verifyCanonical({ repository, sourceCommit: source, expectedCount: 1 });
  const catalog = await apiGet(`/actions/runs/${runId}/artifacts?per_page=100`);
  if (!Number.isSafeInteger(catalog?.total_count) || !Array.isArray(catalog.artifacts) ||
      catalog.total_count !== catalog.artifacts.length || catalog.total_count > 100) {
    throw new Error("Discord checkpoint artifact catalog is incomplete");
  }
  const name = checkpointCandidateArtifactName(source, runId, runAttempt);
  const matches = catalog.artifacts.filter(artifact => artifact?.name === name);
  if (matches.length !== 1 || matches[0].expired !== false ||
      !ID.test(String(matches[0].id)) || !DIGEST.test(matches[0].digest ?? "")) {
    throw new Error("One durable exact Discord checkpoint artifact is required");
  }
  return {
    publish: "true", status: release?.immutable === true ? "immutable-evidence-repair" : "publish-approved-artifacts",
    source_commit: source, controller_source_commit: controllerSource,
    version: CURRENT_PRODUCT_VERSION, tag: CURRENT_PRODUCT_TAG,
    accepted_run_id: String(accepted.id), accepted_run_attempt: String(accepted.attempt),
    discord_run_id: runId, discord_run_attempt: runAttempt,
    artifact_id: String(matches[0].id), artifact_digest: matches[0].digest,
    tag_exists: String(tagCommit !== null), release_exists: String(release !== null),
    release_immutable: String(release?.immutable === true),
  };
}

export function publicationApiGet(repository) {
  return async (path, { allowNotFound = false } = {}) => {
    if (repository !== REPOSITORY || !path.startsWith("/") || path.includes("..")) {
      throw new Error("Publication API path is outside the trusted repository");
    }
    try {
      const raw = execFileSync("gh", ["api", "--method", "GET", "-H",
        "X-GitHub-Api-Version: 2026-03-10", `/repos/${repository}${path}`], {
        encoding: "utf8", timeout: 30_000, maxBuffer: 8 * 1024 * 1024,
        windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
      });
      return JSON.parse(raw);
    } catch (error) {
      if (allowNotFound && /\(HTTP 404\)/u.test(String(error.stderr ?? ""))) return null;
      throw new Error(`Publication API read failed at ${path}`);
    }
  };
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    if (process.argv.length !== 2) throw new Error("Publication source accepts workflow environment inputs only");
    const env = process.env;
    const result = await resolveApprovedReleaseSource({
      repository: env.GITHUB_REPOSITORY, ref: env.GITHUB_REF, controllerSource: env.GITHUB_SHA,
      eventName: env.GITHUB_EVENT_NAME, event: JSON.parse(await readFile(env.GITHUB_EVENT_PATH, "utf8")),
    }, { apiGet: publicationApiGet(env.GITHUB_REPOSITORY), verifyCanonical: resolveCanonicalAcceptanceRun });
    await appendFile(env.GITHUB_OUTPUT, Object.entries(result).map(([key, value]) => `${key}=${value}\n`).join(""));
    console.log(`approved_release=${result.status} source=${result.source_commit}`);
    if (env.GITHUB_STEP_SUMMARY) await appendFile(env.GITHUB_STEP_SUMMARY,
      `Release: ${result.status}\n\nApplication: ${result.source_commit}\n\n` +
      (result.published_source_commit ? `Existing immutable application: ${result.published_source_commit}\n` : ""));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 2;
  }
}

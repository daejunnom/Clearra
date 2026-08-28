import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const DIGEST_PATTERN = /^sha256:[0-9a-f]{64}$/u;
const SHA_PATTERN = /^[0-9a-f]{40}$/u;
const DECIMAL_ID_PATTERN = /^[1-9][0-9]*$/u;
const RELEASE_TAG = "v0.8.0";
const MINIMUM_RETENTION_MS = 89 * 24 * 60 * 60 * 1000;
const EXPECTED_CONTRACT = Object.freeze({
  schema: "clearra.pages.identity.v2",
  contractSchemaVersion: "clearra.search.contract.v2",
  supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
  artifactSchemaVersion: "clearra.solution-data.v1",
});

function fail(message) {
  throw new Error(message);
}

function requireString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    fail(`${label} must be a non-empty string`);
  }
  return value;
}

function requirePattern(value, pattern, label) {
  const text = requireString(value, label);
  if (!pattern.test(text)) {
    fail(`${label} has an invalid format`);
  }
  return text;
}

function requireSha(value, label) {
  return requirePattern(value, SHA_PATTERN, label);
}

function requireDecimalId(value, label) {
  return requirePattern(value, DECIMAL_ID_PATTERN, label);
}

function requireDate(value, label) {
  const text = requireString(value, label);
  const parsed = Date.parse(text);
  if (!Number.isFinite(parsed)) {
    fail(`${label} must be an ISO timestamp`);
  }
  return parsed;
}

function requireObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  return value;
}

export function validatePagesIdentity(identityValue, manifestValue, expectedSha) {
  const sha = requireSha(expectedSha, "expected Pages SHA");
  const identity = requireObject(identityValue, "Pages identity");
  if (
    identity.schema !== EXPECTED_CONTRACT.schema ||
    identity.sourceCommit !== sha ||
    identity.engineBuildId !== sha ||
    identity.contractSchemaVersion !== EXPECTED_CONTRACT.contractSchemaVersion ||
    identity.supplySemanticsId !== EXPECTED_CONTRACT.supplySemanticsId ||
    identity.artifactSchemaVersion !== EXPECTED_CONTRACT.artifactSchemaVersion ||
    typeof identity.version !== "string" ||
    identity.version.length === 0
  ) {
    fail("Pages identity does not match the exact release contract");
  }

  const manifest = requireObject(manifestValue, "Pages WASM manifest");
  const runtimeIdentity = requireObject(
    requireObject(manifest.build, "Pages WASM build").runtime_identity,
    "Pages WASM runtime identity",
  );
  if (
    runtimeIdentity.source_commit !== sha ||
    runtimeIdentity.engine_build_id !== sha ||
    runtimeIdentity.contract_schema_version !== EXPECTED_CONTRACT.contractSchemaVersion ||
    runtimeIdentity.supply_semantics_id !== EXPECTED_CONTRACT.supplySemanticsId ||
    runtimeIdentity.artifact_schema_version !== EXPECTED_CONTRACT.artifactSchemaVersion
  ) {
    fail("Pages WASM manifest does not match the exact release contract");
  }
}

export function validateCanonicalRuns(value, expectedSha, label = "canonical runs") {
  const sha = requireSha(expectedSha, `${label} SHA`);
  const payload = requireObject(value, label);
  if (!Array.isArray(payload.workflow_runs)) {
    fail(`${label}.workflow_runs must be an array`);
  }
  const accepted = payload.workflow_runs.filter(
    (run) =>
      run !== null &&
      typeof run === "object" &&
      run.event === "workflow_dispatch" &&
      run.status === "completed" &&
      run.conclusion === "success" &&
      run.head_sha === sha &&
      run.path === ".github/workflows/release-cli.yml",
  );
  if (accepted.length < 1) {
    fail(`${label} has no successful exact-SHA workflow_dispatch acceptance`);
  }
}

export function expectedCaptureArtifactName({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureRunAttempt,
}) {
  const snapshot = requireSha(snapshotSha, "snapshot SHA");
  const authority = requireSha(authoritySha, "authority SHA");
  const runId = requireDecimalId(String(captureRunId), "capture run ID");
  const attempt = requireDecimalId(String(captureRunAttempt), "capture run attempt");
  return `clearra-pages-rollback-${snapshot}-authority-${authority}-run-${runId}-attempt-${attempt}`;
}

export function validateRunAttemptPolicy(mode, runAttempt) {
  const attempt = requireDecimalId(String(runAttempt), "current run attempt");
  if (mode !== "capture" && attempt !== "1") {
    fail("forward and restore mutations require a fresh workflow dispatch, not a rerun");
  }
  return attempt;
}

export function validateCaptureAuthority({
  snapshotSha,
  authoritySha,
  captureRunId,
  captureArtifactId,
  captureArtifactName,
  captureArtifactDigest,
  captureTarSha256,
  captureRun,
  captureJobs,
  artifact,
  consumerRun,
}) {
  const snapshot = requireSha(snapshotSha, "snapshot SHA");
  const authority = requireSha(authoritySha, "authority SHA");
  const runId = requireDecimalId(String(captureRunId), "capture run ID");
  const artifactId = requireDecimalId(String(captureArtifactId), "capture artifact ID");
  const digest = requirePattern(captureArtifactDigest, DIGEST_PATTERN, "capture artifact digest");
  requirePattern(captureTarSha256, SHA256_PATTERN, "capture Pages tar SHA-256");

  const run = requireObject(captureRun, "capture run");
  if (
    String(run.id) !== runId ||
    run.event !== "workflow_dispatch" ||
    run.status !== "completed" ||
    run.conclusion !== "success" ||
    run.head_branch !== "main" ||
    run.head_sha !== authority ||
    run.path !== ".github/workflows/pages-rollback.yml"
  ) {
    fail("capture run is not the successful exact-main rollback workflow authority");
  }
  const expectedName = expectedCaptureArtifactName({
    snapshotSha: snapshot,
    authoritySha: authority,
    captureRunId: runId,
    captureRunAttempt: run.run_attempt,
  });
  if (captureArtifactName !== expectedName) {
    fail("capture artifact name is not bound to its exact run attempt");
  }

  const jobsPayload = requireObject(captureJobs, "capture jobs");
  if (!Array.isArray(jobsPayload.jobs)) {
    fail("capture jobs must contain a jobs array");
  }
  const captureBuildJobs = jobsPayload.jobs.filter((job) => job?.name === "capture-build");
  if (captureBuildJobs.length !== 1) {
    fail("capture run must contain exactly one capture-build job");
  }
  const captureBuild = requireObject(captureBuildJobs[0], "capture-build job");
  if (captureBuild.status !== "completed" || captureBuild.conclusion !== "success") {
    fail("capture-build job did not complete successfully");
  }

  const artifactValue = requireObject(artifact, "capture artifact");
  const artifactRun = requireObject(artifactValue.workflow_run, "capture artifact workflow run");
  if (
    String(artifactValue.id) !== artifactId ||
    artifactValue.name !== expectedName ||
    artifactValue.expired !== false ||
    artifactValue.digest !== digest ||
    String(artifactRun.id) !== runId ||
    artifactRun.head_branch !== "main" ||
    artifactRun.head_sha !== authority
  ) {
    fail("capture artifact metadata does not match its exact run authority");
  }

  const artifactCreated = requireDate(artifactValue.created_at, "artifact created_at");
  const artifactExpires = requireDate(artifactValue.expires_at, "artifact expires_at");
  const jobStarted = requireDate(captureBuild.started_at, "capture-build started_at");
  const jobCompleted = requireDate(captureBuild.completed_at, "capture-build completed_at");
  const captureCompleted = requireDate(run.updated_at, "capture run updated_at");
  if (
    artifactExpires - artifactCreated < MINIMUM_RETENTION_MS ||
    artifactCreated < jobStarted ||
    artifactCreated > jobCompleted ||
    jobCompleted > captureCompleted
  ) {
    fail("capture artifact retention or job ordering is invalid");
  }

  if (consumerRun !== undefined) {
    const consumer = requireObject(consumerRun, "consumer run");
    if (
      String(consumer.id) === runId ||
      consumer.event !== "workflow_dispatch" ||
      consumer.head_branch !== "main" ||
      consumer.head_sha !== authority
    ) {
      fail("consumer run is not an independent exact-main workflow_dispatch run");
    }
    const consumerCreated = requireDate(consumer.created_at, "consumer run created_at");
    if (captureCompleted >= consumerCreated) {
      fail("capture run must complete before the consuming Pages mutation starts");
    }
  }
}

function env(name, { optional = false } = {}) {
  const value = process.env[name];
  if ((value === undefined || value === "") && !optional) {
    fail(`${name} is required`);
  }
  return value ?? "";
}

function assertEmpty(value, label) {
  if (value !== "") {
    fail(`${label} must be empty in this mode`);
  }
}

function assertExact(value, expected, label) {
  if (value !== expected) {
    fail(`${label} differs from the exact authority`);
  }
}

async function parseJsonResponse(response, label) {
  const text = await response.text();
  if (!response.ok) {
    fail(`${label} request failed with HTTP ${response.status}`);
  }
  try {
    return JSON.parse(text);
  } catch {
    fail(`${label} response is not JSON`);
  }
}

function apiClient({ repository, token, apiUrl }) {
  const base = `${apiUrl.replace(/\/$/u, "")}/repos/${repository}`;
  const headers = {
    Accept: "application/vnd.github+json",
    Authorization: `Bearer ${token}`,
    "X-GitHub-Api-Version": "2022-11-28",
  };
  return {
    async get(path, label) {
      return parseJsonResponse(await fetch(`${base}${path}`, { headers }), label);
    },
    async requireAbsent(path, label) {
      const response = await fetch(`${base}${path}`, { headers });
      if (response.status !== 404) {
        fail(`${label} must be absent before a pre-release Pages rollback`);
      }
    },
  };
}

async function fetchPublicJson(url, label) {
  const response = await fetch(url, {
    headers: { Accept: "application/json" },
    cache: "no-store",
  });
  return parseJsonResponse(response, label);
}

async function canonicalRuns(api, sha, label) {
  const query = new URLSearchParams({
    event: "workflow_dispatch",
    status: "success",
    head_sha: sha,
    per_page: "100",
  });
  const runs = await api.get(
    `/actions/workflows/release-cli.yml/runs?${query.toString()}`,
    label,
  );
  validateCanonicalRuns(runs, sha, label);
}

async function main() {
  const mode = env("PAGES_AUTHORITY_MODE");
  if (!new Set(["capture", "forward", "restore"]).has(mode)) {
    fail("PAGES_AUTHORITY_MODE must be capture, forward, or restore");
  }
  const phase = env("PAGES_AUTHORITY_PHASE");
  const authoritySha = requireSha(env("AUTHORITY_SHA"), "authority SHA");
  const snapshotSha = requireSha(env("SNAPSHOT_SHA"), "snapshot SHA");
  const currentPagesSha = requireSha(env("CURRENT_PAGES_SHA"), "current Pages SHA");
  const repository = requireString(env("GITHUB_REPOSITORY"), "GitHub repository");
  const runId = requireDecimalId(env("GITHUB_RUN_ID"), "current run ID");
  const runAttempt = validateRunAttemptPolicy(mode, env("GITHUB_RUN_ATTEMPT"));
  const githubRef = env("GITHUB_REF");
  const githubSha = requireSha(env("GITHUB_SHA"), "workflow SHA");
  const token = requireString(env("GH_TOKEN"), "GitHub token");
  const apiUrl = requireString(env("GITHUB_API_URL"), "GitHub API URL");
  const expectedPath = mode === "forward"
    ? ".github/workflows/pages.yml"
    : ".github/workflows/pages-rollback.yml";

  assertExact(githubRef, "refs/heads/main", "workflow ref");
  assertExact(githubSha, authoritySha, "workflow SHA");
  if (mode === "restore") {
    assertExact(currentPagesSha, authoritySha, "restore current Pages SHA");
  } else {
    assertExact(currentPagesSha, snapshotSha, `${mode} current Pages SHA`);
  }

  const api = apiClient({ repository, token, apiUrl });
  const remoteMain = await api.get("/git/ref/heads/main", "remote main");
  assertExact(remoteMain?.object?.sha, authoritySha, "remote main SHA");

  const currentRun = await api.get(`/actions/runs/${runId}`, "current workflow run");
  if (
    String(currentRun.id) !== runId ||
    String(currentRun.run_attempt) !== runAttempt ||
    currentRun.event !== "workflow_dispatch" ||
    currentRun.head_branch !== "main" ||
    currentRun.head_sha !== authoritySha ||
    currentRun.path !== expectedPath
  ) {
    fail("current workflow run is not bound to the exact main authority");
  }

  const comparison = await api.get(
    `/compare/${snapshotSha}...${authoritySha}`,
    "snapshot ancestry",
  );
  if (!new Set(["ahead", "identical"]).has(comparison.status)) {
    fail("snapshot SHA must be the authority main SHA or its ancestor");
  }
  await canonicalRuns(api, snapshotSha, "snapshot canonical runs");
  if (authoritySha !== snapshotSha) {
    await canonicalRuns(api, authoritySha, "authority canonical runs");
  }

  const pages = await api.get("/pages", "Pages configuration");
  const pageUrl = requireString(pages.html_url, "Pages URL").replace(/\/$/u, "");
  const cacheBuster = encodeURIComponent(`${mode}-${phase}-${runId}`);
  const [identity, manifest] = await Promise.all([
    fetchPublicJson(
      `${pageUrl}/clearra-build-identity.json?authority=${cacheBuster}`,
      "live Pages identity",
    ),
    fetchPublicJson(
      `${pageUrl}/wasm/clearra_wasm.manifest.json?authority=${cacheBuster}`,
      "live Pages WASM manifest",
    ),
  ]);
  validatePagesIdentity(identity, manifest, currentPagesSha);

  await api.requireAbsent(`/git/ref/tags/${RELEASE_TAG}`, `${RELEASE_TAG} tag`);
  await api.requireAbsent(`/releases/tags/${RELEASE_TAG}`, `${RELEASE_TAG} release`);

  const captureFields = {
    captureRunId: env("CAPTURE_RUN_ID", { optional: true }),
    captureArtifactId: env("CAPTURE_ARTIFACT_ID", { optional: true }),
    captureArtifactName: env("CAPTURE_ARTIFACT_NAME", { optional: true }),
    captureArtifactDigest: env("CAPTURE_ARTIFACT_DIGEST", { optional: true }),
    captureTarSha256: env("CAPTURE_TAR_SHA256", { optional: true }),
  };
  const restoreAuthorization = env("RESTORE_AUTHORIZATION", { optional: true });

  if (mode === "capture") {
    for (const [label, value] of Object.entries(captureFields)) {
      assertEmpty(value, label);
    }
    assertEmpty(restoreAuthorization, "restore authorization");
  } else {
    const captureRun = await api.get(
      `/actions/runs/${requireDecimalId(captureFields.captureRunId, "capture run ID")}`,
      "capture run",
    );
    const [captureJobs, artifact] = await Promise.all([
      api.get(`/actions/runs/${captureFields.captureRunId}/jobs?per_page=100`, "capture jobs"),
      api.get(
        `/actions/artifacts/${requireDecimalId(captureFields.captureArtifactId, "capture artifact ID")}`,
        "capture artifact",
      ),
    ]);
    validateCaptureAuthority({
      snapshotSha,
      authoritySha,
      ...captureFields,
      captureRun,
      captureJobs,
      artifact,
      consumerRun: currentRun,
    });
    if (mode === "forward") {
      assertEmpty(restoreAuthorization, "restore authorization");
    } else {
      assertExact(
        restoreAuthorization,
        `ROLLBACK:${currentPagesSha}:TO:${snapshotSha}`,
        "restore authorization",
      );
    }
  }

  console.log(`pages_rollback_authority=passed mode=${mode} phase=${phase}`);
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    console.error(`pages_rollback_authority=failed reason=${error.message}`);
    process.exitCode = 2;
  });
}

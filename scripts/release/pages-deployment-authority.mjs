import { createHash } from "node:crypto";
import { appendFile, lstat, open } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  canonicalSha256,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requirePlainObject,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";

export const PAGES_DEPLOYMENT_AUTHORITY_SCHEMA_ID =
  "clearra.pages.deployment-authority.v1";

const DECIMAL_ID = /^[1-9][0-9]*$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const BASE_PATH = /^\/[A-Za-z0-9._-]+$/u;
const ARTIFACT_DIGEST = /^sha256:([0-9a-f]{64})$/u;
const FORWARD_VERSION = "0.8.0";
const RESTORE_VERSION = "0.7.5";
const MODES = new Set(["forward", "restore"]);
const REPORT_FIELDS = Object.freeze([
  "schema_id",
  "mode",
  "repository",
  "source_commit",
  "workflow_source_commit",
  "workflow_run_id",
  "workflow_run_attempt",
  "workflow_path",
  "accepted_run_id",
  "accepted_run_attempt",
  "artifact_id",
  "artifact_name",
  "artifact_digest",
  "artifact_sha256",
  "artifact_api_readback_sha256",
  "workflow_run_api_readback_sha256",
  "deployment_id",
  "deployment_status",
  "deployment_api_readback_sha256",
  "page_url",
  "base_path",
  "pages_configuration_api_readback_sha256",
  "live_identity_sha256",
  "status",
  "report_sha256",
]);
const FORWARD_IDENTITY_FIELDS = Object.freeze([
  "acceptedRunAttempt",
  "acceptedRunId",
  "artifactSchemaVersion",
  "basePath",
  "contractSchemaVersion",
  "engineBuildId",
  "files",
  "schema",
  "sourceCommit",
  "supplySemanticsId",
  "version",
]);
const RESTORE_IDENTITY_FIELDS = Object.freeze([
  "artifactSchemaVersion",
  "contractSchemaVersion",
  "engineBuildId",
  "schema",
  "sourceCommit",
  "supplySemanticsId",
  "version",
]);

export function validatePagesDeploymentAuthorityReport(report, {
  expectedSourceCommit,
} = {}) {
  requireExactKeys(report, REPORT_FIELDS, "Pages deployment authority report");
  verifyCanonicalReportHash(report, "Pages deployment authority report");
  if (report.schema_id !== PAGES_DEPLOYMENT_AUTHORITY_SCHEMA_ID) {
    throw new Error("Pages deployment authority report schema is invalid");
  }
  if (!MODES.has(report.mode)) {
    throw new Error("Pages deployment authority mode is invalid");
  }
  requirePattern(report.repository, REPOSITORY, "Pages repository");
  const sourceCommit = requireSourceCommit(
    report.source_commit,
    "Pages deployment source commit",
  );
  const workflowSourceCommit = requireSourceCommit(
    report.workflow_source_commit,
    "Pages workflow source commit",
  );
  if (expectedSourceCommit !== undefined && sourceCommit !== expectedSourceCommit) {
    throw new Error("Pages deployment report source differs from the expected source");
  }
  requirePattern(report.workflow_run_id, DECIMAL_ID, "Pages workflow run ID");
  requirePattern(report.workflow_run_attempt, DECIMAL_ID, "Pages workflow run attempt");
  if (report.mode === "forward") {
    requirePattern(report.accepted_run_id, DECIMAL_ID, "Pages accepted run ID");
    requirePattern(report.accepted_run_attempt, DECIMAL_ID, "Pages accepted run attempt");
  } else if (report.accepted_run_id !== null || report.accepted_run_attempt !== null) {
    throw new Error("restored legacy Pages authority must not invent accepted run identity");
  }
  requirePattern(report.artifact_id, DECIMAL_ID, "Pages artifact ID");
  if (report.artifact_name !== "github-pages") {
    throw new Error("Pages deployment artifact name must be github-pages");
  }
  const digest = requirePattern(
    report.artifact_digest,
    ARTIFACT_DIGEST,
    "Pages artifact digest",
  );
  requireSha256(report.artifact_sha256, "Pages artifact SHA-256");
  if (digest.slice("sha256:".length) !== report.artifact_sha256) {
    throw new Error("Pages artifact digest and SHA-256 differ");
  }
  for (const [field, label] of [
    ["artifact_api_readback_sha256", "Pages artifact API readback SHA-256"],
    ["workflow_run_api_readback_sha256", "Pages workflow run API readback SHA-256"],
    ["deployment_api_readback_sha256", "Pages deployment API readback SHA-256"],
    ["pages_configuration_api_readback_sha256", "Pages configuration API readback SHA-256"],
    ["live_identity_sha256", "Pages live identity SHA-256"],
  ]) {
    requireSha256(report[field], label);
  }
  requireNonEmptyString(report.workflow_path, "Pages workflow path");
  const expectedPath = report.mode === "forward"
    ? ".github/workflows/pages.yml"
    : ".github/workflows/pages-rollback.yml";
  if (report.workflow_path !== expectedPath) {
    throw new Error("Pages deployment workflow path differs from its mode");
  }
  const expectedDeploymentId = report.mode === "forward"
    ? sourceCommit
    : workflowSourceCommit;
  if (report.deployment_id !== expectedDeploymentId) {
    throw new Error("Pages deployment ID differs from the deploy action build version");
  }
  if (report.deployment_status !== "succeed" || report.status !== "active") {
    throw new Error("Pages deployment authority is not active and successful");
  }
  const pageUrl = requirePagesUrl(report.page_url, report.base_path);
  if (pageUrl !== report.page_url) {
    throw new Error("Pages deployment URL is not canonical");
  }
  rejectSecretMaterial(report, "Pages deployment authority report");
  return report;
}

export async function producePagesDeploymentAuthority(input, {
  getGithubJson,
  fetchPublicJson,
  sleep = (milliseconds) => new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds)),
  attempts = 12,
} = {}) {
  if (typeof getGithubJson !== "function" || typeof fetchPublicJson !== "function") {
    throw new Error("Pages deployment producer requires GitHub and public JSON readers");
  }
  const mode = requireMode(input.mode);
  const repository = requirePattern(input.repository, REPOSITORY, "Pages repository");
  const sourceCommit = requireSourceCommit(input.sourceCommit, "Pages deployment source commit");
  const workflowSourceCommit = requireSourceCommit(
    input.workflowSourceCommit,
    "Pages workflow source commit",
  );
  const workflowRunId = requirePattern(
    String(input.workflowRunId ?? ""),
    DECIMAL_ID,
    "Pages workflow run ID",
  );
  const workflowRunAttempt = requirePattern(
    String(input.workflowRunAttempt ?? ""),
    DECIMAL_ID,
    "Pages workflow run attempt",
  );
  const artifactId = requirePattern(
    String(input.artifactId ?? ""),
    DECIMAL_ID,
    "Pages artifact ID",
  );
  const artifactName = requireNonEmptyString(input.artifactName, "Pages artifact name");
  if (artifactName !== "github-pages") {
    throw new Error("Pages deployment artifact name must be github-pages");
  }
  const basePath = requirePattern(input.basePath, BASE_PATH, "Pages base path");
  const pageUrl = requirePagesUrl(input.pageUrl, basePath);
  const workflowPath = mode === "forward"
    ? ".github/workflows/pages.yml"
    : ".github/workflows/pages-rollback.yml";
  const deploymentId = mode === "forward" ? sourceCommit : workflowSourceCommit;
  const maximumAttempts = requireAttemptCount(attempts);

  const [workflowRun, artifact, pagesConfiguration] = await Promise.all([
    getGithubJson(`/actions/runs/${workflowRunId}`, "Pages workflow run"),
    getGithubJson(`/actions/artifacts/${artifactId}`, "Pages artifact"),
    getGithubJson("/pages", "Pages configuration"),
  ]);
  validateWorkflowRun(workflowRun, {
    workflowRunId,
    workflowRunAttempt,
    workflowSourceCommit,
    workflowPath,
  });
  const artifactDigest = validateArtifact(artifact, {
    artifactId,
    artifactName,
    workflowRunId,
    workflowSourceCommit,
  });
  requirePlainObject(pagesConfiguration, "Pages configuration");
  if (normalizePagesUrl(pagesConfiguration.html_url) !== pageUrl) {
    throw new Error("Pages configuration URL differs from the deployed page URL");
  }

  let deploymentReadback;
  let liveIdentity;
  let finalError;
  for (let attempt = 1; attempt <= maximumAttempts; attempt += 1) {
    try {
      deploymentReadback = await getGithubJson(
        `/pages/deployments/${encodeURIComponent(deploymentId)}`,
        "Pages deployment status",
      );
      requirePlainObject(deploymentReadback, "Pages deployment status");
      if (deploymentReadback.status !== "succeed") {
        throw new Error("Pages deployment status has not converged to succeed");
      }
      const identityUrl = new URL("clearra-build-identity.json", pageUrl);
      identityUrl.searchParams.set("authority", `${workflowRunId}-${workflowRunAttempt}-${attempt}`);
      liveIdentity = await fetchPublicJson(
        identityUrl.toString(),
        "live Pages identity",
      );
      validateLiveIdentity(liveIdentity, {
        mode,
        sourceCommit,
        basePath,
        expectedAcceptedRunId: input.acceptedRunId,
        expectedAcceptedRunAttempt: input.acceptedRunAttempt,
      });
      finalError = undefined;
      break;
    } catch (error) {
      finalError = error;
      if (attempt < maximumAttempts) await sleep(10_000);
    }
  }
  if (finalError) throw finalError;

  const report = sealCanonicalReport({
    schema_id: PAGES_DEPLOYMENT_AUTHORITY_SCHEMA_ID,
    mode,
    repository,
    source_commit: sourceCommit,
    workflow_source_commit: workflowSourceCommit,
    workflow_run_id: workflowRunId,
    workflow_run_attempt: workflowRunAttempt,
    workflow_path: workflowPath,
    accepted_run_id: mode === "forward" ? String(liveIdentity.acceptedRunId) : null,
    accepted_run_attempt: mode === "forward"
      ? String(liveIdentity.acceptedRunAttempt)
      : null,
    artifact_id: artifactId,
    artifact_name: artifactName,
    artifact_digest: artifactDigest,
    artifact_sha256: artifactDigest.slice("sha256:".length),
    artifact_api_readback_sha256: canonicalSha256(artifact),
    workflow_run_api_readback_sha256: canonicalSha256(workflowRun),
    deployment_id: deploymentId,
    deployment_status: "succeed",
    deployment_api_readback_sha256: canonicalSha256(deploymentReadback),
    page_url: pageUrl,
    base_path: basePath,
    pages_configuration_api_readback_sha256: canonicalSha256(pagesConfiguration),
    live_identity_sha256: canonicalSha256(liveIdentity),
    status: "active",
  });
  validatePagesDeploymentAuthorityReport(report, { expectedSourceCommit: sourceCommit });
  return report;
}

export async function writePagesDeploymentAuthorityReport(path, report) {
  validatePagesDeploymentAuthorityReport(report);
  const target = resolve(requireNonEmptyString(path, "Pages authority report path"));
  await assertSafeDirectoryChain(dirname(target));
  const bytes = `${canonicalJson(report)}\n`;
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(bytes, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  return createHash("sha256").update(bytes, "utf8").digest("hex");
}

function validateWorkflowRun(run, expected) {
  requirePlainObject(run, "Pages workflow run");
  if (
    String(run.id) !== expected.workflowRunId ||
    String(run.run_attempt) !== expected.workflowRunAttempt ||
    run.event !== "workflow_dispatch" ||
    run.head_branch !== "main" ||
    run.head_sha !== expected.workflowSourceCommit ||
    run.path !== expected.workflowPath ||
    run.status !== "in_progress" ||
    run.conclusion !== null
  ) {
    throw new Error("Pages workflow run readback differs from the active exact-main attempt");
  }
}

function validateArtifact(artifact, expected) {
  requirePlainObject(artifact, "Pages artifact");
  requirePlainObject(artifact.workflow_run, "Pages artifact workflow run");
  if (
    String(artifact.id) !== expected.artifactId ||
    artifact.name !== expected.artifactName ||
    artifact.expired !== false ||
    String(artifact.workflow_run.id) !== expected.workflowRunId ||
    artifact.workflow_run.head_branch !== "main" ||
    artifact.workflow_run.head_sha !== expected.workflowSourceCommit
  ) {
    throw new Error("Pages artifact API readback differs from the exact workflow artifact");
  }
  return requirePattern(artifact.digest, ARTIFACT_DIGEST, "Pages artifact digest");
}

function validateLiveIdentity(identity, {
  mode,
  sourceCommit,
  basePath,
  expectedAcceptedRunId,
  expectedAcceptedRunAttempt,
}) {
  requireExactKeys(
    identity,
    mode === "forward" ? FORWARD_IDENTITY_FIELDS : RESTORE_IDENTITY_FIELDS,
    "live Pages identity",
  );
  if (
    identity.schema !== "clearra.pages.identity.v2" ||
    identity.sourceCommit !== sourceCommit ||
    identity.engineBuildId !== sourceCommit ||
    identity.contractSchemaVersion !== "clearra.search.contract.v2" ||
    identity.supplySemanticsId !== "clearra.supply.projected-terminal-lookahead.v1" ||
    identity.artifactSchemaVersion !== "clearra.solution-data.v1" ||
    (mode === "forward" && identity.basePath !== basePath) ||
    identity.version !== (mode === "forward" ? FORWARD_VERSION : RESTORE_VERSION)
  ) {
    throw new Error("live Pages identity differs from the deployed source contract");
  }
  if (mode === "restore") return;
  const acceptedRunId = requirePattern(
    String(identity.acceptedRunId ?? ""),
    DECIMAL_ID,
    "live Pages accepted run ID",
  );
  const acceptedRunAttempt = requirePattern(
    String(identity.acceptedRunAttempt ?? ""),
    DECIMAL_ID,
    "live Pages accepted run attempt",
  );
  if (
    (expectedAcceptedRunId !== undefined &&
      acceptedRunId !== String(expectedAcceptedRunId)) ||
    (expectedAcceptedRunAttempt !== undefined &&
      acceptedRunAttempt !== String(expectedAcceptedRunAttempt))
  ) {
    throw new Error("live Pages identity differs from the accepted run authority");
  }
  if (!Array.isArray(identity.files) || identity.files.length === 0) {
    throw new Error("live Pages identity files must be a non-empty array");
  }
  let previous = "";
  for (const [index, file] of identity.files.entries()) {
    requireExactKeys(file, ["path", "sha256", "size"], `live Pages file ${index}`);
    if (
      typeof file.path !== "string" ||
      file.path.length === 0 ||
      file.path.startsWith("/") ||
      file.path.includes("\\") ||
      file.path.split("/").includes("..") ||
      (previous && previous.localeCompare(file.path, "en") >= 0) ||
      !Number.isSafeInteger(file.size) ||
      file.size < 0
    ) {
      throw new Error("live Pages identity file set is invalid or unsorted");
    }
    requireSha256(file.sha256, `live Pages file ${file.path} SHA-256`);
    previous = file.path;
  }
}

function requirePagesUrl(value, basePath) {
  const url = parseCredentialFreeHttpsUrl(value, "Pages URL");
  requirePattern(basePath, BASE_PATH, "Pages base path");
  if (url.pathname !== `${basePath}/`) {
    throw new Error("Pages URL path differs from the exact base path");
  }
  return url.toString();
}

function normalizePagesUrl(value) {
  return parseCredentialFreeHttpsUrl(value, "Pages configuration URL").toString();
}

function parseCredentialFreeHttpsUrl(value, label) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error(`${label} must be credential-free HTTPS without query or fragment`);
  }
  if (!url.pathname.endsWith("/")) url.pathname = `${url.pathname}/`;
  return url;
}

function requireMode(value) {
  if (!MODES.has(value)) throw new Error("Pages deployment mode must be forward or restore");
  return value;
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} has an invalid format`);
  }
  return value;
}

function requireAttemptCount(value) {
  if (!Number.isSafeInteger(value) || value < 1 || value > 20) {
    throw new Error("Pages deployment readback attempts must be 1 through 20");
  }
  return value;
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Pages authority report path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function env(name, { optional = false } = {}) {
  const value = process.env[name];
  if ((value === undefined || value === "") && !optional) {
    throw new Error(`${name} is required`);
  }
  return value ?? undefined;
}

function githubReader({ repository, token, apiUrl }) {
  const base = `${apiUrl.replace(/\/$/u, "")}/repos/${repository}`;
  return async (path, label) => {
    const response = await fetch(`${base}${path}`, {
      method: "GET",
      redirect: "error",
      cache: "no-store",
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: `Bearer ${token}`,
        "X-GitHub-Api-Version": "2022-11-28",
      },
    });
    return parseJsonResponse(response, label);
  };
}

async function publicJsonReader(url, label) {
  const response = await fetch(url, {
    method: "GET",
    redirect: "error",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  return parseJsonResponse(response, label);
}

async function parseJsonResponse(response, label) {
  const text = await response.text();
  if (!response.ok) throw new Error(`${label} returned HTTP ${response.status}`);
  try {
    const value = JSON.parse(text);
    requirePlainObject(value, label);
    return value;
  } catch (error) {
    if (error instanceof SyntaxError) throw new Error(`${label} is not valid JSON`);
    throw error;
  }
}

async function main() {
  const repository = env("GITHUB_REPOSITORY");
  const token = env("GH_TOKEN");
  const report = await producePagesDeploymentAuthority({
    mode: env("PAGES_DEPLOYMENT_MODE"),
    repository,
    sourceCommit: env("SOURCE_COMMIT"),
    workflowSourceCommit: env("GITHUB_SHA"),
    workflowRunId: env("GITHUB_RUN_ID"),
    workflowRunAttempt: env("GITHUB_RUN_ATTEMPT"),
    artifactId: env("PAGES_ARTIFACT_ID"),
    artifactName: env("PAGES_ARTIFACT_NAME"),
    pageUrl: env("PAGE_URL"),
    basePath: env("EXPECTED_BASE_PATH"),
    acceptedRunId: env("EXPECTED_ACCEPTED_RUN_ID", { optional: true }),
    acceptedRunAttempt: env("EXPECTED_ACCEPTED_RUN_ATTEMPT", { optional: true }),
  }, {
    getGithubJson: githubReader({
      repository,
      token,
      apiUrl: env("GITHUB_API_URL"),
    }),
    fetchPublicJson: publicJsonReader,
  });
  const reportPath = env("PAGES_AUTHORITY_REPORT_PATH");
  const reportFileSha256 = await writePagesDeploymentAuthorityReport(reportPath, report);
  const githubOutput = env("GITHUB_OUTPUT");
  await appendFile(
    githubOutput,
    [
      `deployment_id=${report.deployment_id}`,
      `artifact_sha256=${report.artifact_sha256}`,
      `report_sha256=${report.report_sha256}`,
      `report_file_sha256=${reportFileSha256}`,
      "",
    ].join("\n"),
    "utf8",
  );
  console.log(
    `pages_deployment_authority=sealed mode=${report.mode} source=${report.source_commit} run=${report.workflow_run_id}/${report.workflow_run_attempt}`,
  );
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(
      `pages_deployment_authority=failed reason=${error instanceof Error ? error.message : String(error)}`,
    );
    process.exitCode = 2;
  });
}

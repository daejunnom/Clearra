import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  normalizeDiscordCatalog,
  validateCanonicalDiscordCatalog,
  validateDiscordCatalogSyncReport,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";
import {
  loadCommandRegistrationCredentials,
  verifyGlobalCommandRegistration,
} from "../../apps/clearra-discord-bot/src/discord/command-registration.mjs";
import { DiscordRestClient } from "../../apps/clearra-discord-bot/src/discord/rest.mjs";
import {
  ARTIFACT_SCHEMA_VERSION,
  CONTRACT_SCHEMA_VERSION,
  normalizeRuntimeIdentity,
  RUNTIME_IDENTITY_SCHEMA,
  SUPPLY_SEMANTICS_ID,
} from "../../apps/clearra-discord-bot/src/job-service/runtime-identity.mjs";
import {
  canonicalJson,
  canonicalSha256,
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requirePlainObject,
  requireSha256,
  requireSourceCommit,
} from "./canonical-release-evidence.mjs";
import {
  validateCloudCandidateSmokeReport,
} from "./cloud-candidate-smoke-report.mjs";
import {
  validatePagesDeploymentAuthorityReport,
} from "./pages-deployment-authority.mjs";
import {
  readDiscordCommandSyncAuthority,
  validateDiscordCommandSyncAuthority,
} from "./discord-command-sync-authority.mjs";
import {
  PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
  validateSurfaceProbeResult,
} from "./observe-production-surfaces.mjs";

const DISCORD_SNOWFLAKE = /^\d{17,20}$/u;
const IMAGE_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const PROJECT_ID = /^[a-z][a-z0-9-]{4,61}[a-z0-9]$/u;
const REGION = /^[a-z]+(?:-[a-z0-9]+)+[0-9]$/u;
const VERSION = "0.8.0";
const PAGES_IDENTITY_SCHEMA = "clearra.pages.identity.v2";
const MAX_HTTP_BYTES = 2 * 1024 * 1024;
const MAX_CONTROL_PLANE_BYTES = 4 * 1024 * 1024;
const DEFAULT_HTTP_TIMEOUT_MS = 30_000;
const DEFAULT_CONTROL_PLANE_TIMEOUT_MS = 30_000;

export async function probeDiscordProductionSurface({
  sourceCommit,
  applicationId,
  catalog,
  catalogFileSha256,
  syncAuthority,
  syncAuthorityFileSha256,
  syncReport,
  sequence,
  rest,
  now = () => new Date().toISOString(),
}) {
  const commit = requireSourceCommit(sourceCommit);
  const application = requireDiscordApplicationId(applicationId);
  const observationSequence = requireSequence(sequence);
  const catalogFileHash = requireSha256(
    catalogFileSha256,
    "Discord canonical catalog file SHA-256",
  );
  const syncAuthorityFileHash = requireSha256(
    syncAuthorityFileSha256,
    "Discord command sync authority file SHA-256",
  );
  validateCanonicalDiscordCatalog(catalog, commit);
  validateDiscordCommandSyncAuthority(syncAuthority, {
    sourceCommit: commit,
    catalog,
    catalogFileSha256: catalogFileHash,
  });
  validateDiscordCatalogSyncReport(syncReport, {
    expectedSourceCommit: commit,
    expectedApplicationId: application,
    expectedCatalog: catalog,
    expectedCatalogFileSha256: catalogFileHash,
    expectedSyncAuthority: syncAuthority,
    expectedSyncAuthorityFileSha256: syncAuthorityFileHash,
  });
  if (!rest || typeof rest.getGlobalCommands !== "function") {
    throw new Error("Discord production probe requires a read-only REST client");
  }

  const observedAt = canonicalTimestamp(now(), "Discord probe observation time");
  const readback = await rest.getGlobalCommands(application);
  verifyGlobalCommandRegistration(catalog.commands, readback);
  validateDiscordReadbackIdentity(readback, application);
  const normalized = normalizeDiscordCatalog(readback, {
    allowResponseMetadata: true,
  });
  const readbackSha256 = canonicalSha256(normalized);
  if (readbackSha256 !== syncReport.current_after_sha256) {
    throw new Error("Discord live command catalog differs from the sealed sync readback");
  }
  const commandNames = normalized.map((command) =>
    `${command.type ?? 1}:${command.name}`).sort((left, right) =>
    left.localeCompare(right, "en"));
  const probeId = createProbeId({
    surface: "discord",
    sourceCommit: commit,
    sequence: observationSequence,
    observedAt,
    evidence: { readback_sha256: readbackSha256 },
  });

  return finishProbe({
    schema_id: PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
    surface: "discord",
    source_commit: commit,
    identity: {
      source_commit: commit,
      application_id: application,
      command_catalog_sha256: catalog.catalog_sha256,
      command_catalog_prior_snapshot_sha256: syncReport.prior_snapshot_sha256,
      command_catalog_readback_sha256: readbackSha256,
      command_catalog_sync_report_sha256: syncReport.report_sha256,
      accepted_run_id: syncReport.accepted_run_id,
      accepted_run_attempt: syncReport.accepted_run_attempt,
      accepted_ctk3_manifest_sha256: syncReport.accepted_ctk3_manifest_sha256,
      canonical_acceptance_evidence_sha256:
        syncReport.canonical_acceptance_evidence_sha256,
      canonical_acceptance_evidence_file_sha256:
        syncReport.canonical_acceptance_evidence_file_sha256,
      command_catalog_file_sha256: syncReport.command_catalog_file_sha256,
      command_sync_authority_sha256: syncReport.command_sync_authority_sha256,
      command_sync_authority_file_sha256:
        syncReport.command_sync_authority_file_sha256,
      command_count: normalized.length,
      command_names: commandNames,
      status: "active",
    },
    freshness: {
      probe_id: probeId,
      readback_sha256: readbackSha256,
    },
  }, commit);
}

export async function probeCloudProductionSurface({
  sourceCommit,
  projectId,
  region,
  serviceName,
  revision,
  tag,
  imageDigest,
  smokeReport,
  sequence,
  runControlPlane = runGcloudJson,
  fetchJson = fetchJsonBounded,
  now = () => new Date().toISOString(),
}) {
  const commit = requireSourceCommit(sourceCommit);
  const project = requirePattern(projectId, PROJECT_ID, "Cloud project ID");
  const location = requirePattern(region, REGION, "Cloud region");
  const service = requirePattern(serviceName, IDENTIFIER, "Cloud service name");
  const expectedRevision = requirePattern(revision, IDENTIFIER, "Cloud revision");
  const expectedTag = requirePattern(tag, IDENTIFIER, "Cloud traffic tag");
  const expectedImageDigest = requirePattern(
    imageDigest,
    IMAGE_DIGEST,
    "Cloud image digest",
  );
  const observationSequence = requireSequence(sequence);
  validateCloudCandidateSmokeReport(smokeReport, {
    expectedSourceCommit: commit,
  });
  if (
    smokeReport.project_id !== project ||
    smokeReport.region !== location ||
    smokeReport.service_name !== service ||
    smokeReport.candidate_revision !== expectedRevision ||
    smokeReport.candidate_tag !== expectedTag ||
    smokeReport.image_digest !== expectedImageDigest
  ) {
    throw new Error("Cloud observation authority differs from the candidate smoke report");
  }
  if (typeof runControlPlane !== "function" || typeof fetchJson !== "function") {
    throw new Error("Cloud production probe dependencies are invalid");
  }

  const observedAt = canonicalTimestamp(now(), "Cloud probe observation time");
  const serviceReadback = await runControlPlane({
    kind: "service",
    projectId: project,
    region: location,
    name: service,
  });
  const revisionReadback = await runControlPlane({
    kind: "revision",
    projectId: project,
    region: location,
    name: expectedRevision,
  });
  const authority = validateCloudControlPlane({
    serviceReadback,
    revisionReadback,
    serviceName: service,
    revision: expectedRevision,
    tag: expectedTag,
    imageDigest: expectedImageDigest,
  });
  if (new URL(authority.taggedUrl).origin !== smokeReport.candidate_url) {
    throw new Error("Cloud tagged URL differs from the candidate smoke authority");
  }
  const stableHealth = await fetchJson(
    cacheBustedHealthUrl(authority.stableUrl, commit, observationSequence),
    "Cloud stable health",
  );
  const taggedHealth = await fetchJson(
    cacheBustedHealthUrl(authority.taggedUrl, commit, observationSequence),
    "Cloud tagged health",
  );
  validateCloudHealth(stableHealth, commit, "Cloud stable health");
  validateCloudHealth(taggedHealth, commit, "Cloud tagged health");
  if (
    canonicalJson(stableHealth.runtime) !== canonicalJson(taggedHealth.runtime)
  ) {
    throw new Error("Cloud stable and tagged health identities differ");
  }

  const evidence = {
    service_readback_sha256: canonicalSha256(serviceReadback),
    revision_readback_sha256: canonicalSha256(revisionReadback),
    stable_health_sha256: canonicalSha256(stableHealth),
    tagged_health_sha256: canonicalSha256(taggedHealth),
  };
  const probeId = createProbeId({
    surface: "cloud",
    sourceCommit: commit,
    sequence: observationSequence,
    observedAt,
    evidence,
  });
  return finishProbe({
    schema_id: PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
    surface: "cloud",
    source_commit: commit,
    identity: {
      source_commit: commit,
      engine_build_id: commit,
      revision: expectedRevision,
      image_digest: expectedImageDigest,
      traffic_percent: 100,
      cpu: "8",
      memory: "16Gi",
      concurrency: 1,
      min_instances: 0,
      max_instances: 4,
      startup_cpu_boost: true,
      contract_schema_version: CONTRACT_SCHEMA_VERSION,
      supply_semantics_id: SUPPLY_SEMANTICS_ID,
      artifact_schema_version: ARTIFACT_SCHEMA_VERSION,
      job_smoke_report_sha256: smokeReport.report_sha256,
      stable_url: authority.stableUrl,
      tagged_url: authority.taggedUrl,
      status: "active",
    },
    freshness: { probe_id: probeId, ...evidence },
  }, commit);
}

export async function probePagesProductionSurface({
  sourceCommit,
  deploymentReport,
  sequence,
  fetchJson = fetchJsonBounded,
  fetchDeploymentStatus = readPagesDeploymentStatusControlPlane,
  now = () => new Date().toISOString(),
}) {
  const commit = requireSourceCommit(sourceCommit);
  validatePagesDeploymentAuthorityReport(deploymentReport, {
    expectedSourceCommit: commit,
  });
  if (deploymentReport.mode !== "forward") {
    throw new Error("Pages production probe requires a forward deployment report");
  }
  const url = requireCredentialFreeHttpsUrl(deploymentReport.page_url, "Pages URL");
  const deployment = deploymentReport.deployment_id;
  const artifact = deploymentReport.artifact_sha256;
  const expectedBasePath = deploymentReport.base_path;
  const runId = deploymentReport.accepted_run_id;
  const runAttempt = deploymentReport.accepted_run_attempt;
  const observationSequence = requireSequence(sequence);
  if (typeof fetchJson !== "function" || typeof fetchDeploymentStatus !== "function") {
    throw new Error("Pages production probe requires public and GitHub HTTP readers");
  }
  const normalizedPageUrl = requirePagesUrlBasePath(url, expectedBasePath);
  const observedAt = canonicalTimestamp(now(), "Pages probe observation time");
  const identityUrl = new URL("clearra-build-identity.json", normalizedPageUrl);
  identityUrl.searchParams.set("source", commit);
  identityUrl.searchParams.set("observation", String(observationSequence));
  const [readback, deploymentReadback] = await Promise.all([
    fetchJson(identityUrl.toString(), "Pages build identity"),
    fetchDeploymentStatus(deploymentReport),
  ]);
  validatePagesIdentity(readback, {
    sourceCommit: commit,
    basePath: expectedBasePath,
    acceptedRunId: runId,
    acceptedRunAttempt: runAttempt,
  });
  const readbackSha256 = canonicalSha256(readback);
  if (readbackSha256 !== deploymentReport.live_identity_sha256) {
    throw new Error("Pages live identity differs from the sealed deployment readback");
  }
  requirePlainObject(deploymentReadback, "Pages deployment status readback");
  if (deploymentReadback.status !== "succeed") {
    throw new Error("Pages deployment status is not succeed");
  }
  const deploymentReadbackSha256 = canonicalSha256(deploymentReadback);
  if (deploymentReadbackSha256 !== deploymentReport.deployment_api_readback_sha256) {
    throw new Error("Pages deployment status differs from the sealed API readback");
  }
  const probeId = createProbeId({
    surface: "pages",
    sourceCommit: commit,
    sequence: observationSequence,
    observedAt,
    evidence: {
      deployment_readback_sha256: deploymentReadbackSha256,
      identity_readback_sha256: readbackSha256,
    },
  });
  return finishProbe({
    schema_id: PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
    surface: "pages",
    source_commit: commit,
    identity: {
      source_commit: commit,
      engine_build_id: commit,
      version: VERSION,
      deployment_id: deployment,
      artifact_sha256: artifact,
      base_path: expectedBasePath,
      url: normalizedPageUrl,
      status: "active",
    },
    freshness: {
      probe_id: probeId,
      deployment_readback_sha256: deploymentReadbackSha256,
      identity_readback_sha256: readbackSha256,
    },
  }, commit);
}

export async function readCloudRunControlPlane({
  projectId,
  region,
  serviceName,
  revision,
}) {
  const serviceReadback = await runGcloudJson({
    kind: "service",
    projectId,
    region,
    name: serviceName,
  });
  const revisionReadback = await runGcloudJson({
    kind: "revision",
    projectId,
    region,
    name: revision,
  });
  return { serviceReadback, revisionReadback };
}

export async function readPagesDeploymentStatusControlPlane(report) {
  validatePagesDeploymentAuthorityReport(report);
  return runJsonCommand(
    process.platform === "win32" ? "gh.exe" : "gh",
    [
      "api",
      "--method",
      "GET",
      `repos/${report.repository}/pages/deployments/${report.deployment_id}`,
      "--header",
      "Accept: application/vnd.github+json",
      "--header",
      "X-GitHub-Api-Version: 2022-11-28",
    ],
    DEFAULT_CONTROL_PLANE_TIMEOUT_MS,
    MAX_CONTROL_PLANE_BYTES,
    "Pages deployment status readback",
  );
}

export function validateCloudCandidateControlPlane({
  serviceReadback,
  revisionReadback,
  serviceName,
  revision,
  tag,
  imageDigest,
}) {
  validateCloudRevisionAuthority({
    serviceReadback,
    revisionReadback,
    serviceName,
    revision,
    imageDigest,
  });
  const traffic = serviceReadback.status?.traffic;
  const tagged = traffic.filter((entry) =>
    entry?.tag === tag && entry?.revisionName === revision);
  const activeCandidate = traffic.filter((entry) =>
    entry?.revisionName === revision && Number(entry?.percent) > 0);
  if (
    serviceReadback.status?.latestCreatedRevisionName !== revision ||
    tagged.length !== 1 ||
    Number(tagged[0]?.percent ?? 0) !== 0 ||
    activeCandidate.length !== 0
  ) {
    throw new Error("Cloud candidate is not the exact tagged zero-traffic revision");
  }
  return {
    candidateUrl: requireCredentialFreeHttpsUrl(
      tagged[0]?.url,
      "Cloud candidate tagged URL",
    ),
    serviceReadbackSha256: canonicalSha256(serviceReadback),
    revisionReadbackSha256: canonicalSha256(revisionReadback),
  };
}

function validateDiscordReadbackIdentity(commands, applicationId) {
  if (!Array.isArray(commands) || commands.length < 1) {
    throw new Error("Discord readback is not a non-empty command catalog");
  }
  const commandIds = new Set();
  const commandKeys = new Set();
  for (const command of commands) {
    requirePlainObject(command, "Discord readback command");
    const key = `${command.type ?? 1}:${command.name ?? ""}`;
    if (
      command.application_id !== applicationId ||
      !DISCORD_SNOWFLAKE.test(command.id ?? "") ||
      !DISCORD_SNOWFLAKE.test(command.version ?? "") ||
      commandIds.has(command.id) ||
      commandKeys.has(key)
    ) {
      throw new Error("Discord readback command identity metadata is invalid");
    }
    commandIds.add(command.id);
    commandKeys.add(key);
  }
}

function validateCloudControlPlane({
  serviceReadback,
  revisionReadback,
  serviceName,
  revision,
  tag,
  imageDigest,
}) {
  validateCloudRevisionAuthority({
    serviceReadback,
    revisionReadback,
    serviceName,
    revision,
    imageDigest,
  });
  const traffic = serviceReadback.status?.traffic;
  const active = traffic.filter((entry) => Number(entry?.percent) === 100);
  const tagged = traffic.filter((entry) =>
    entry?.tag === tag && entry?.revisionName === revision);
  if (
    active.length !== 1 ||
    active[0]?.revisionName !== revision ||
    tagged.length !== 1
  ) {
    throw new Error("Cloud traffic is not pinned to the exact tagged revision");
  }
  const stableUrl = requireCredentialFreeHttpsUrl(
    serviceReadback.status?.url,
    "Cloud stable URL",
  );
  const taggedUrl = requireCredentialFreeHttpsUrl(
    tagged[0]?.url,
    "Cloud tagged URL",
  );
  return { stableUrl, taggedUrl };
}

function validateCloudRevisionAuthority({
  serviceReadback,
  revisionReadback,
  serviceName,
  revision,
  imageDigest,
}) {
  requirePlainObject(serviceReadback, "Cloud service readback");
  requirePlainObject(revisionReadback, "Cloud revision readback");
  if (serviceReadback.metadata?.name !== serviceName) {
    throw new Error("Cloud service readback returned the wrong service");
  }
  if (revisionReadback.metadata?.name !== revision) {
    throw new Error("Cloud revision readback returned the wrong revision");
  }
  if (!Array.isArray(serviceReadback.status?.traffic)) {
    throw new Error("Cloud service traffic readback is missing");
  }
  const serviceTemplate = serviceReadback.spec?.template;
  const serviceSpec = serviceTemplate?.spec;
  const revisionSpec = revisionReadback.spec;
  requireCloudContainerShape(serviceSpec, "Cloud service template");
  requireCloudContainerShape(revisionSpec, "Cloud active revision");
  requireScaleAuthority(serviceReadback, "service", 0, 4);
  requireScaleAuthority(revisionReadback, "revision", 0, 4);
  requireStartupBoost(serviceTemplate, "Cloud service template");
  requireStartupBoost(revisionReadback, "Cloud active revision");
  requireReadyRevision(revisionReadback);
  for (const [value, label] of [
    [serviceSpec?.containers?.[0]?.image, "Cloud service image"],
    [revisionSpec?.containers?.[0]?.image, "Cloud revision image"],
    [revisionReadback.status?.imageDigest, "Cloud revision status image"],
  ]) {
    if (extractImageDigest(value) !== imageDigest) {
      throw new Error(`${label} differs from the immutable deployment digest`);
    }
  }
  return true;
}

function requireCloudContainerShape(spec, label) {
  requirePlainObject(spec, `${label} spec`);
  const containers = spec.containers;
  if (
    Number(spec.containerConcurrency) !== 1 ||
    !Array.isArray(containers) ||
    containers.length !== 1 ||
    String(containers[0]?.resources?.limits?.cpu ?? "") !== "8" ||
    String(containers[0]?.resources?.limits?.memory ?? "") !== "16Gi"
  ) {
    throw new Error(`${label} resource contract differs from 8 CPU/16Gi/concurrency-one`);
  }
}

function requireScaleAuthority(resource, level, expectedMin, expectedMax) {
  const annotations = resource.metadata?.annotations ?? {};
  const scaling = resource.spec?.scaling ?? {};
  const min = firstDefined(
    annotations["autoscaling.knative.dev/minScale"],
    annotations["run.googleapis.com/minScale"],
    scaling.minInstanceCount,
  );
  const max = firstDefined(
    annotations["autoscaling.knative.dev/maxScale"],
    annotations["run.googleapis.com/maxScale"],
    scaling.maxInstanceCount,
  );
  if (Number(min) !== expectedMin || Number(max) !== expectedMax) {
    throw new Error(`Cloud ${level} scale authority is not min-zero/max-four`);
  }
}

function requireStartupBoost(resource, label) {
  const annotations = resource?.metadata?.annotations ?? {};
  if (String(annotations["run.googleapis.com/startup-cpu-boost"] ?? "") !== "true") {
    throw new Error(`${label} startup CPU boost is not enabled`);
  }
}

function requireReadyRevision(revision) {
  const conditions = revision.status?.conditions;
  if (
    !Array.isArray(conditions) ||
    !conditions.some((condition) => condition?.type === "Ready" && condition?.status === "True")
  ) {
    throw new Error("Cloud revision is not Ready");
  }
}

function validateCloudHealth(value, sourceCommit, label) {
  requireExactKeys(value, [
    "status",
    "activeJobs",
    "workerLimit",
    "runtime",
  ], label);
  if (
    value.status !== "ok" ||
    !Number.isSafeInteger(value.activeJobs) ||
    value.activeJobs < 0 ||
    value.workerLimit !== 8
  ) {
    throw new Error(`${label} service status is invalid`);
  }
  requireExactKeys(value.runtime, [
    "schema",
    "sourceCommit",
    "engineBuildId",
    "contractSchemaVersion",
    "supplySemanticsId",
    "artifactSchemaVersion",
  ], `${label} runtime identity`);
  const runtime = normalizeRuntimeIdentity(value.runtime);
  if (
    runtime.schema !== RUNTIME_IDENTITY_SCHEMA ||
    runtime.sourceCommit !== sourceCommit ||
    runtime.engineBuildId !== sourceCommit ||
    runtime.contractSchemaVersion !== CONTRACT_SCHEMA_VERSION ||
    runtime.supplySemanticsId !== SUPPLY_SEMANTICS_ID ||
    runtime.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION
  ) {
    throw new Error(`${label} runtime identity differs from the release source`);
  }
}

function validatePagesIdentity(value, {
  sourceCommit,
  basePath,
  acceptedRunId,
  acceptedRunAttempt,
}) {
  requireExactKeys(value, [
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
  ], "Pages live build identity");
  if (
    value.schema !== PAGES_IDENTITY_SCHEMA ||
    value.sourceCommit !== sourceCommit ||
    value.engineBuildId !== sourceCommit ||
    value.contractSchemaVersion !== CONTRACT_SCHEMA_VERSION ||
    value.supplySemanticsId !== SUPPLY_SEMANTICS_ID ||
    value.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION ||
    value.version !== VERSION ||
    value.basePath !== basePath ||
    String(value.acceptedRunId) !== acceptedRunId ||
    String(value.acceptedRunAttempt) !== acceptedRunAttempt
  ) {
    throw new Error("Pages live build identity differs from accepted deployment authority");
  }
  if (!Array.isArray(value.files) || value.files.length < 1) {
    throw new Error("Pages live build identity has no closed payload file set");
  }
  let priorPath = "";
  const paths = new Set();
  for (const file of value.files) {
    requireExactKeys(file, ["path", "size", "sha256"], "Pages identity file");
    if (
      typeof file.path !== "string" ||
      !/^[A-Za-z0-9._-]+(?:\/[A-Za-z0-9._-]+)*$/u.test(file.path) ||
      file.path.localeCompare(priorPath, "en") <= 0 ||
      paths.has(file.path) ||
      !Number.isSafeInteger(file.size) ||
      file.size < 0
    ) {
      throw new Error("Pages identity closed file set is invalid or unsorted");
    }
    requireSha256(file.sha256, "Pages identity file SHA-256");
    priorPath = file.path;
    paths.add(file.path);
  }
  rejectSecretMaterial(value, "Pages live build identity");
}

async function runGcloudJson({ kind, projectId, region, name }) {
  const resource = kind === "service" ? "services" : "revisions";
  return runJsonCommand(
    process.platform === "win32" ? "gcloud.cmd" : "gcloud",
    [
      "run",
      resource,
      "describe",
      name,
      `--project=${projectId}`,
      `--region=${region}`,
      "--format=json",
    ],
    DEFAULT_CONTROL_PLANE_TIMEOUT_MS,
    MAX_CONTROL_PLANE_BYTES,
    "Cloud control-plane readback",
  );
}

async function runJsonCommand(executable, arguments_, timeoutMs, maxBytes, label) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(executable, arguments_, {
      shell: false,
      windowsHide: true,
      stdio: ["ignore", "pipe", "ignore"],
    });
    const chunks = [];
    let size = 0;
    let settled = false;
    const timer = setTimeout(() => {
      child.kill();
      finish(new Error(`${label} timed out`));
    }, timeoutMs);
    child.stdout.on("data", (chunk) => {
      size += chunk.length;
      if (size > maxBytes) {
        child.kill();
        finish(new Error(`${label} exceeded its output bound`));
        return;
      }
      chunks.push(chunk);
    });
    child.on("error", () => finish(new Error(`${label} failed to start`)));
    child.on("exit", (code, signal) => {
      if (code !== 0 || signal) {
        finish(new Error(`${label} did not exit successfully`));
        return;
      }
      try {
        const value = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        requirePlainObject(value, label);
        finish(null, value);
      } catch {
        finish(new Error(`${label} did not return one JSON object`));
      }
    });
    function finish(error, value) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) rejectPromise(error);
      else resolvePromise(value);
    }
  });
}

async function fetchJsonBounded(url, label) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), DEFAULT_HTTP_TIMEOUT_MS);
  let response;
  try {
    response = await fetch(url, {
      method: "GET",
      redirect: "error",
      cache: "no-store",
      headers: { accept: "application/json" },
      signal: controller.signal,
    });
  } catch {
    throw new Error(`${label} request failed`);
  } finally {
    clearTimeout(timer);
  }
  if (!response.ok) throw new Error(`${label} returned HTTP ${response.status}`);
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > MAX_HTTP_BYTES) {
    throw new Error(`${label} exceeded its response bound`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength > MAX_HTTP_BYTES) {
    throw new Error(`${label} exceeded its response bound`);
  }
  try {
    const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    requirePlainObject(value, label);
    return value;
  } catch {
    throw new Error(`${label} did not return one UTF-8 JSON object`);
  }
}

async function readCanonicalJsonFile(path, label, expectedFileSha256) {
  const target = resolve(requireNonEmptyString(path, `${label} path`));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const raw = await readFile(target, "utf8");
  const expectedDigest = requireSha256(
    expectedFileSha256,
    `${label} file SHA-256`,
  );
  const actualDigest = createHash("sha256").update(raw, "utf8").digest("hex");
  if (actualDigest !== expectedDigest) {
    throw new Error(`${label} file changed after probe-spec materialization`);
  }
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  return value;
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("production probe input path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function finishProbe(value, sourceCommit) {
  rejectSecretMaterial(value, "production surface probe");
  validateSurfaceProbeResult(value, {
    expectedSurface: value.surface,
    expectedSourceCommit: sourceCommit,
  });
  return Object.freeze(value);
}

function createProbeId({ surface, sourceCommit, sequence, observedAt, evidence }) {
  return canonicalSha256({
    schema_id: "clearra.production-probe-id.v1",
    surface,
    source_commit: sourceCommit,
    sequence,
    observed_at: observedAt,
    evidence,
  });
}

function cacheBustedHealthUrl(baseUrl, sourceCommit, sequence) {
  const url = new URL("health", baseUrl);
  url.searchParams.set("source", sourceCommit);
  url.searchParams.set("observation", String(sequence));
  return url.toString();
}

function requirePagesUrlBasePath(value, basePath) {
  const url = new URL(value);
  const expectedPath = `${basePath}/`;
  if (url.pathname !== expectedPath) {
    throw new Error("Pages URL path differs from the exact deployment base path");
  }
  return url.toString();
}

function requireCredentialFreeHttpsUrl(value, label) {
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
  return url.toString();
}

function extractImageDigest(value) {
  const text = typeof value === "string" ? value : "";
  const match = text.match(/(?:^|@)(sha256:[0-9a-f]{64})$/u);
  return match?.[1] ?? "";
}

function firstDefined(...values) {
  return values.find((value) => value !== undefined && value !== null);
}

function requireDiscordApplicationId(value) {
  return requirePattern(value, DISCORD_SNOWFLAKE, "Discord application ID");
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function requireSequence(value) {
  const sequence = typeof value === "string" && /^\d+$/u.test(value)
    ? Number(value)
    : value;
  if (!Number.isSafeInteger(sequence) || sequence < 0) {
    throw new Error("production probe observation sequence is invalid");
  }
  return sequence;
}

function parseCliArguments(args) {
  if (!Array.isArray(args) || args.length === 0) {
    throw new Error("production surface probe command is required");
  }
  const command = args[0];
  const specifications = new Map([
    ["discord", [
      "--source-commit",
      "--application-id",
      "--catalog",
      "--catalog-file-sha256",
      "--sync-authority",
      "--sync-authority-file-sha256",
      "--sync-report",
      "--sync-report-file-sha256",
      "--observation-sequence",
    ]],
    ["cloud", [
      "--source-commit",
      "--project-id",
      "--region",
      "--service-name",
      "--revision",
      "--tag",
      "--image-digest",
      "--smoke-report",
      "--smoke-report-file-sha256",
      "--observation-sequence",
    ]],
    ["pages", [
      "--source-commit",
      "--deployment-report",
      "--deployment-report-file-sha256",
      "--observation-sequence",
    ]],
  ]);
  const required = specifications.get(command);
  if (!required) throw new Error(`unsupported production surface probe: ${String(command)}`);
  const allowed = new Set(required);
  const values = {};
  for (let index = 1; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option)) {
      throw new Error(`unsupported ${command} production probe argument: ${String(option)}`);
    }
    if (Object.hasOwn(values, option)) {
      throw new Error(`duplicate ${command} production probe argument: ${option}`);
    }
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values[option] = value;
  }
  for (const option of required) {
    if (!Object.hasOwn(values, option)) throw new Error(`${option} is required`);
  }
  return { command, values };
}

async function main() {
  const { command, values } = parseCliArguments(process.argv.slice(2));
  let result;
  if (command === "discord") {
    const credentials = loadCommandRegistrationCredentials({
      ...process.env,
      DISCORD_APPLICATION_ID: values["--application-id"],
    });
    const catalog = await readCanonicalJsonFile(
      values["--catalog"],
      "Discord canonical catalog",
      values["--catalog-file-sha256"],
    );
    const syncAuthorityInput = await readDiscordCommandSyncAuthority(
      values["--sync-authority"],
      values["--sync-authority-file-sha256"],
      {
        sourceCommit: values["--source-commit"],
        catalog,
        catalogFileSha256: values["--catalog-file-sha256"],
      },
    );
    const syncReport = await readCanonicalJsonFile(
      values["--sync-report"],
      "Discord command sync report",
      values["--sync-report-file-sha256"],
    );
    result = await probeDiscordProductionSurface({
      sourceCommit: values["--source-commit"],
      applicationId: values["--application-id"],
      catalog,
      catalogFileSha256: values["--catalog-file-sha256"],
      syncAuthority: syncAuthorityInput.authority,
      syncAuthorityFileSha256: syncAuthorityInput.fileSha256,
      syncReport,
      sequence: values["--observation-sequence"],
      rest: new DiscordRestClient(credentials.token),
    });
  } else if (command === "cloud") {
    result = await probeCloudProductionSurface({
      sourceCommit: values["--source-commit"],
      projectId: values["--project-id"],
      region: values["--region"],
      serviceName: values["--service-name"],
      revision: values["--revision"],
      tag: values["--tag"],
      imageDigest: values["--image-digest"],
      smokeReport: await readCanonicalJsonFile(
        values["--smoke-report"],
        "Cloud candidate smoke report",
        values["--smoke-report-file-sha256"],
      ),
      sequence: values["--observation-sequence"],
    });
  } else {
    result = await probePagesProductionSurface({
      sourceCommit: values["--source-commit"],
      deploymentReport: await readCanonicalJsonFile(
        values["--deployment-report"],
        "Pages deployment authority report",
        values["--deployment-report-file-sha256"],
      ),
      sequence: values["--observation-sequence"],
    });
  }
  process.stdout.write(`${canonicalJson(result)}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `production_surface_probe=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}

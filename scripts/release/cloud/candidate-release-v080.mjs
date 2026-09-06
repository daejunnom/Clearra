#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { lstat, open } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  canonicalSha256,
  canonicalTimestamp,
  sealCanonicalReport,
} from "../canonical-release-evidence.mjs";
import {
  CLOUD_CANDIDATE_SMOKE_SCHEMA_ID,
  validateCloudCandidateSmokeReport,
} from "../cloud-candidate-smoke-report.mjs";

export const CLOUD_CANDIDATE_CONTRACT = "clearra.cloud.zero-traffic-candidate.v1";
export const CLOUD_SMOKE_CONTRACT = CLOUD_CANDIDATE_SMOKE_SCHEMA_ID;

const REGION = "asia-northeast1";
const REPOSITORY = "clearra";
const SERVICE = "clearra-current-job";
const JOB_BEARER_SECRET = "clearra-job-token";
const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const PROJECT_ID = /^[a-z][a-z0-9-]{4,28}[a-z0-9]$/u;
const RESOURCE_NAME = /^[a-z][a-z0-9-]{0,62}$/u;
const SECRET_VERSION = /^[1-9][0-9]{0,18}$/u;
const IMAGE_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const EXECUTION_NAME = /^[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?$/u;
const CLOSED_GCLOUD_ATOM = /^[A-Za-z0-9_./:=,@+\-]+$/u;
// One read-only diagnostic log selector, not an arbitrary shell/filter surface.
const CLOSED_GCLOUD_PARITY_LOG_FILTER = /^resource\.type=cloud_run_job AND resource\.labels\.project_id=[a-z][a-z0-9-]{4,61}[a-z0-9] AND resource\.labels\.location=asia-northeast1 AND resource\.labels\.job_name=clearra-parity-[0-9a-f]{7}-[1-9][0-9]{0,19}$/u;
// Cloud Run system labels use the full namespaced key. Logging requires a
// quoted field component when it contains '/', not labels.execution_name.
// https://docs.cloud.google.com/logging/docs/api/platform-logs
// https://docs.cloud.google.com/logging/docs/view/logging-query-language
const CLOSED_GCLOUD_LOG_FILTER = /^--log-filter=labels\."run\.googleapis\.com\/execution_name"="[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?" AND textPayload:"candidate_smoke_job=passed"$/u;
const DEFAULT_LOG_ATTEMPTS = 30;
const DEFAULT_LOG_RETRY_DELAY_MS = 2_000;

const SERVICE_ENVIRONMENT = Object.freeze({
  CLEARRA_EXECUTABLE: "/usr/local/bin/clearra",
  CLEARRA_SEARCH_TIMEOUT_MS: "840000",
  CLEARRA_REVERSE_SEARCH_TIMEOUT_MS: "840000",
  CLEARRA_FORWARD_SEARCH_TIMEOUT_MS: "900000",
  CLEARRA_EXPECTED_VCPUS: "8",
  CLEARRA_SEARCH_WORKERS_PER_SESSION: "8",
  CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1",
  CLEARRA_MAX_CONCURRENT_JOBS: "1",
  CLEARRA_MAX_OUTPUT_BYTES: "4194304",
});

export async function deployZeroTrafficCandidate(options, dependencies = {}) {
  const authority = candidateAuthority(options, {
    requireImage: options?.imageDigest !== undefined,
  });
  const runJson = dependencies.runJson ?? runGcloudJson;
  const imageDigest = authority.imageDigest ?? resolveImmutableImage(
    await runJson([
      "artifacts", "docker", "images", "describe", authority.imageTag,
      `--project=${authority.projectId}`,
      "--format=json",
    ]),
    authority.imageBase,
  );
  const deployedAuthority = Object.freeze({ ...authority, imageDigest });

  await runJson(buildServiceDeployArguments(deployedAuthority));
  const [service, revision] = await Promise.all([
    runJson(serviceDescribeArguments(deployedAuthority)),
    runJson(revisionDescribeArguments(deployedAuthority)),
  ]);
  const candidateUrl = validateZeroTrafficReadback(
    { service, revision },
    deployedAuthority,
  );
  return Object.freeze({
    contract: CLOUD_CANDIDATE_CONTRACT,
    sourceCommit: authority.sourceCommit,
    projectId: authority.projectId,
    region: REGION,
    service: SERVICE,
    priorRevision: authority.priorRevision,
    candidateRevision: authority.candidateRevision,
    candidateTag: authority.candidateTag,
    candidateUrl,
    imageDigest,
    jobBearerSecret: JOB_BEARER_SECRET,
    jobBearerSecretVersion: authority.jobBearerSecretVersion,
  });
}

export async function smokeZeroTrafficCandidate(options, dependencies = {}) {
  const authority = candidateAuthority(options, { requireImage: true, requireCandidateUrl: true });
  const runJson = dependencies.runJson ?? runGcloudJson;
  const run = dependencies.run ?? runGcloud;
  const now = dependencies.now ?? (() => new Date());
  const wait = dependencies.wait ?? waitFor;
  const logAttempts = canonicalLogAttempts(
    dependencies.logAttempts ?? DEFAULT_LOG_ATTEMPTS,
  );

  const [service, revision] = await Promise.all([
    runJson(serviceDescribeArguments(authority)),
    runJson(revisionDescribeArguments(authority)),
  ]);
  const candidateUrl = validateZeroTrafficReadback({ service, revision }, authority);
  if (candidateUrl !== authority.candidateUrl) {
    throw new Error("candidate smoke URL differs from the zero-traffic readback");
  }
  const startedAt = canonicalClock(now, "managed candidate smoke start");

  let jobCreated = false;
  try {
    await runJson(buildSmokeJobDeployArguments(authority));
    jobCreated = true;
    const job = await runJson(smokeJobDescribeArguments(authority));
    validateSmokeJobReadback(job, authority);
    const execution = await runJson(smokeJobExecuteArguments(authority));
    const executionName = validateSmokeExecution(execution);
    const attestation = await readSmokeLogAttestation(
      runJson,
      authority,
      executionName,
      { wait, attempts: logAttempts },
    );
    const endedAt = canonicalClock(now, "managed candidate smoke end");
    if (Date.parse(endedAt) < Date.parse(startedAt)) {
      throw new Error("managed candidate smoke clock moved backwards");
    }
    const report = sealCanonicalReport({
      schema_id: CLOUD_SMOKE_CONTRACT,
      source_commit: authority.sourceCommit,
      project_id: authority.projectId,
      region: REGION,
      service_name: SERVICE,
      candidate_revision: authority.candidateRevision,
      candidate_tag: authority.candidateTag,
      candidate_url: candidateUrl,
      image_digest: authority.imageDigest.slice(authority.imageDigest.lastIndexOf("@") + 1),
      started_at: startedAt,
      ended_at: endedAt,
      smoke_job: authority.smokeJob,
      execution_name: executionName,
      job_id: attestation.jobId,
      zero_traffic_verified: true,
      service_readback_sha256: canonicalSha256(service),
      revision_readback_sha256: canonicalSha256(revision),
      execution_readback_sha256: canonicalSha256(execution),
      solution_set_hash: attestation.solutionSetHash,
      status: "passed",
    });
    validateCloudCandidateSmokeReport(report, {
      expectedSourceCommit: authority.sourceCommit,
    });
    await run(smokeJobDeleteArguments(authority));
    jobCreated = false;
    return Object.freeze(report);
  } catch (error) {
    if (jobCreated) {
      try {
        await run(smokeJobDeleteArguments(authority));
      } catch {
        // Preserve the primary smoke/readback failure. The deterministic job name
        // makes any failed cleanup visible to the next invocation.
      }
    }
    throw error;
  }
}

export function candidateAuthority(options, requirements = {}) {
  const projectId = requiredPattern(options?.projectId, PROJECT_ID, "Cloud project ID");
  const sourceCommit = requiredPattern(options?.sourceCommit, SOURCE_COMMIT, "source commit");
  const priorRevision = requiredPattern(options?.priorRevision, RESOURCE_NAME, "prior revision");
  const jobBearerSecretVersion = requiredPattern(
    options?.jobBearerSecretVersion,
    SECRET_VERSION,
    "job bearer Secret version",
  );
  const commitPrefix = sourceCommit.slice(0, 7);
  const candidateRevision = `${SERVICE}-v080-${commitPrefix}`;
  if (priorRevision === candidateRevision) {
    throw new Error("prior revision must differ from the candidate revision");
  }
  const imageBase = `${REGION}-docker.pkg.dev/${projectId}/${REPOSITORY}/${SERVICE}`;
  const authority = {
    projectId,
    sourceCommit,
    priorRevision,
    jobBearerSecretVersion,
    runtimeServiceAccount: `${SERVICE}@${projectId}.iam.gserviceaccount.com`,
    imageBase,
    imageTag: `${imageBase}:source-${sourceCommit}`,
    candidateRevision,
    candidateTag: `candidate-${commitPrefix}`,
    revisionSuffix: `v080-${commitPrefix}`,
    smokeJob: `clearra-v080-candidate-smoke-${commitPrefix}`,
  };
  if (requirements.requireImage) {
    authority.imageDigest = canonicalImageDigest(options?.imageDigest, imageBase);
  }
  if (requirements.requireCandidateUrl) {
    authority.candidateUrl = canonicalCandidateOrigin(options?.candidateUrl);
  }
  return Object.freeze(authority);
}

export function buildServiceDeployArguments(authority) {
  const environment = Object.entries(SERVICE_ENVIRONMENT)
    .map(([name, value]) => `${name}=${value}`)
    .join(",");
  return Object.freeze([
    "run", "deploy", SERVICE,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    `--image=${authority.imageDigest}`,
    `--revision-suffix=${authority.revisionSuffix}`,
    `--tag=${authority.candidateTag}`,
    "--no-traffic",
    `--service-account=${authority.runtimeServiceAccount}`,
    "--ingress=all",
    "--no-invoker-iam-check",
    "--port=8080",
    "--concurrency=1",
    "--min=0",
    "--min-instances=0",
    "--max=4",
    "--max-instances=4",
    "--cpu=8",
    "--memory=16Gi",
    "--no-cpu-throttling",
    "--cpu-boost",
    "--timeout=900s",
    `--set-secrets=CLEARRA_JOB_TOKEN=${JOB_BEARER_SECRET}:${authority.jobBearerSecretVersion}`,
    `--set-env-vars=${environment}`,
    "--quiet",
    "--format=json",
  ]);
}

export function buildSmokeJobDeployArguments(authority) {
  const smokeArguments = [
    "./scripts/run-cloud-candidate-smoke-job.mjs",
    "--candidate-url",
    authority.candidateUrl,
    "--source-commit",
    authority.sourceCommit,
  ].join(",");
  return Object.freeze([
    "run", "jobs", "deploy", authority.smokeJob,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    `--image=${authority.imageDigest}`,
    `--service-account=${authority.runtimeServiceAccount}`,
    "--tasks=1",
    "--parallelism=1",
    "--max-retries=0",
    "--task-timeout=120s",
    "--cpu=1",
    "--memory=512Mi",
    "--command=node",
    `--args=${smokeArguments}`,
    `--set-secrets=CLEARRA_CANDIDATE_JOB_TOKEN=${JOB_BEARER_SECRET}:${authority.jobBearerSecretVersion}`,
    "--quiet",
    "--format=json",
  ]);
}

export function validateZeroTrafficReadback({ service, revision }, authority) {
  if (
    service?.metadata?.name !== SERVICE ||
    revision?.metadata?.name !== authority.candidateRevision
  ) {
    throw new Error("Cloud candidate control-plane readback returned the wrong resource");
  }
  if (service?.status?.latestCreatedRevisionName !== authority.candidateRevision) {
    throw new Error("Cloud candidate revision readback differs from the exact source identity");
  }
  const traffic = Array.isArray(service?.status?.traffic) ? service.status.traffic : [];
  const candidate = traffic.filter((entry) =>
    entry?.tag === authority.candidateTag &&
    entry?.revisionName === authority.candidateRevision);
  const active = traffic.filter((entry) => Number(entry?.percent ?? 0) > 0);
  if (
    candidate.length !== 1 ||
    Number(candidate[0]?.percent ?? 0) !== 0 ||
    active.length !== 1 ||
    Number(active[0]?.percent) !== 100 ||
    active[0]?.revisionName !== authority.priorRevision
  ) {
    throw new Error("Cloud candidate readback does not prove zero-traffic isolation");
  }
  const candidateUrl = canonicalCandidateOrigin(candidate[0]?.url);
  validateSingleContainer(service, authority, "Cloud service readback", {
    secretEnvironment: "CLEARRA_JOB_TOKEN",
  });
  validateSingleContainer(revision, authority, "Cloud revision readback", {
    secretEnvironment: "CLEARRA_JOB_TOKEN",
  });
  const revisionImage = revision?.status?.imageDigest;
  if (canonicalImageDigest(revisionImage, authority.imageBase) !== authority.imageDigest) {
    throw new Error("Cloud revision status image differs from the immutable deployment digest");
  }
  validateCandidateResources(service, revision);
  const serviceAccount = firstDefined(
    service?.spec?.template?.spec?.serviceAccountName,
    revision?.spec?.serviceAccountName,
    revision?.spec?.template?.spec?.serviceAccountName,
  );
  if (serviceAccount !== authority.runtimeServiceAccount) {
    throw new Error("Cloud candidate runtime service account readback drifted");
  }
  return candidateUrl;
}

function validateCandidateResources(service, revision) {
  const serviceTemplate = service?.spec?.template;
  const serviceSpec = serviceTemplate?.spec;
  const revisionSpec = revision?.spec;
  for (const [spec, label] of [
    [serviceSpec, "Cloud service template"],
    [revisionSpec, "Cloud revision"],
  ]) {
    const containers = Array.isArray(spec?.containers) ? spec.containers : [];
    if (
      Number(spec?.containerConcurrency) !== 1 ||
      containers.length !== 1 ||
      String(containers[0]?.resources?.limits?.cpu ?? "") !== "8" ||
      String(containers[0]?.resources?.limits?.memory ?? "") !== "16Gi"
    ) {
      throw new Error(`${label} resource readback drifted`);
    }
  }
  validateScaleReadback(service, "service");
  validateScaleReadback(revision, "revision");
  if (
    String(serviceTemplate?.metadata?.annotations?.["run.googleapis.com/startup-cpu-boost"] ?? "") !== "true" ||
    String(revision?.metadata?.annotations?.["run.googleapis.com/startup-cpu-boost"] ?? "") !== "true"
  ) {
    throw new Error("Cloud candidate startup CPU boost readback drifted");
  }
  // The service returns 202 while the CLI continues in the background. Only
  // explicit instance-based CPU allocation preserves that execution contract.
  // Omission means request-based billing, not a default false value:
  // https://docs.cloud.google.com/run/docs/configuring/billing-settings
  if (
    serviceTemplate?.metadata?.annotations?.["run.googleapis.com/cpu-throttling"] !== "false" ||
    revision?.metadata?.annotations?.["run.googleapis.com/cpu-throttling"] !== "false"
  ) {
    throw new Error("Cloud candidate always-allocated CPU readback drifted");
  }
  const ready = Array.isArray(revision?.status?.conditions)
    ? revision.status.conditions
    : [];
  if (!ready.some((condition) => condition?.type === "Ready" && condition?.status === "True")) {
    throw new Error("Cloud candidate revision is not Ready");
  }
}

function validateScaleReadback(resource, label) {
  const annotations = resource?.metadata?.annotations ?? {};
  const scaling = resource?.spec?.scaling ?? {};
  const minimums = [
    annotations["autoscaling.knative.dev/minScale"],
    annotations["run.googleapis.com/minScale"],
    scaling.minInstanceCount,
  ].filter((value) => value !== undefined);
  const maximums = [
    annotations["autoscaling.knative.dev/maxScale"],
    annotations["run.googleapis.com/maxScale"],
    scaling.maxInstanceCount,
  ].filter((value) => value !== undefined);
  // Cloud Run canonicalizes an explicit zero minimum to the omitted/default
  // representation. Every value that is present must agree exactly; the
  // non-default maximum must remain explicitly observable as four.
  if (
    minimums.some((value) => !isExactScaleValue(value, 0)) ||
    maximums.length === 0 ||
    maximums.some((value) => !isExactScaleValue(value, 4))
  ) {
    throw new Error(`Cloud candidate ${label} scale readback drifted`);
  }
}

function isExactScaleValue(value, expected) {
  return (
    (typeof value === "number" && Number.isSafeInteger(value) && value === expected) ||
    (typeof value === "string" && value === String(expected))
  );
}

export function validateSmokeJobReadback(job, authority) {
  const container = validateSingleContainer(job, authority, "Cloud smoke Job readback", {
    secretEnvironment: "CLEARRA_CANDIDATE_JOB_TOKEN",
    command: ["node"],
    args: [
      "./scripts/run-cloud-candidate-smoke-job.mjs",
      "--candidate-url",
      authority.candidateUrl,
      "--source-commit",
      authority.sourceCommit,
    ],
  });
  if (!container) throw new Error("Cloud smoke Job container readback is unavailable");
  const serviceAccounts = deepValuesForKey(job, new Set(["serviceAccount", "serviceAccountName"]));
  if (!serviceAccounts.includes(authority.runtimeServiceAccount)) {
    throw new Error("Cloud smoke Job service account readback drifted");
  }
  requireDeepNumericValue(job, "taskCount", 1, "Cloud smoke Job task count");
  requireDeepNumericValue(job, "parallelism", 1, "Cloud smoke Job parallelism");
  requireDeepNumericValue(job, "maxRetries", 0, "Cloud smoke Job retry policy");
  const timeouts = deepValuesForKey(job, new Set(["timeout", "timeoutSeconds"]));
  if (!timeouts.some((value) => value === "120s" || value === 120 || value === "120")) {
    throw new Error("Cloud smoke Job timeout readback drifted");
  }
}

export function validateSmokeExecution(execution) {
  const status = execution?.status ?? execution;
  const conditions = Array.isArray(status?.conditions)
    ? status.conditions
    : [];
  const completed = conditions.filter((condition) =>
    condition?.type === "Completed" && String(condition?.status).toLowerCase() === "true");
  if (
    completed.length !== 1 ||
    Number(status?.succeededCount) !== 1 ||
    Number(status?.failedCount ?? 0) !== 0
  ) {
    throw new Error("managed-secret candidate smoke execution did not complete exactly once");
  }
  const name = execution?.metadata?.name ?? execution?.name;
  const executionName = typeof name === "string" ? name.split("/").at(-1) : "";
  if (!EXECUTION_NAME.test(executionName)) {
    throw new Error("managed-secret candidate smoke execution identity is unavailable");
  }
  return executionName;
}

export function validateSmokeLogAttestation(logs, authority, executionName) {
  if (!Array.isArray(logs)) {
    throw new Error("managed candidate smoke logs are not a closed JSON array");
  }
  const marker = /^candidate_smoke_job=passed source_commit=([0-9a-f]{40}) job_id=(candidate-smoke-[0-9a-f]{12}-[0-9a-z]+) solution_set_hash=(cts1:[0-9a-f]{16})$/u;
  const matching = logs.flatMap((entry) => {
    if (entry?.labels?.["run.googleapis.com/execution_name"] !== executionName) return [];
    const match = typeof entry?.textPayload === "string"
      ? entry.textPayload.trim().match(marker)
      : null;
    return match ? [match] : [];
  });
  if (matching.length !== 1 || matching[0][1] !== authority.sourceCommit) {
    throw new Error("managed candidate smoke execution lacks one exact result attestation");
  }
  return Object.freeze({
    jobId: matching[0][2],
    solutionSetHash: matching[0][3],
  });
}

export async function readSmokeLogAttestation(
  runJson,
  authority,
  executionName,
  { wait = waitFor, attempts = DEFAULT_LOG_ATTEMPTS } = {},
) {
  const attemptCount = canonicalLogAttempts(attempts);
  let lastError;
  for (let attempt = 1; attempt <= attemptCount; attempt += 1) {
    try {
      const logs = await runJson(smokeJobLogsArguments(authority, executionName));
      return validateSmokeLogAttestation(logs, authority, executionName);
    } catch (error) {
      lastError = error;
      if (attempt < attemptCount) {
        await wait(DEFAULT_LOG_RETRY_DELAY_MS);
      }
    }
  }
  throw lastError;
}

export function resolveImmutableImage(metadata, imageBase) {
  const digest = metadata?.image_summary?.digest;
  const fullyQualified = metadata?.image_summary?.fully_qualified_digest;
  if (!IMAGE_DIGEST.test(digest ?? "") || fullyQualified !== `${imageBase}@${digest}`) {
    throw new Error("Artifact Registry tag did not resolve to one canonical immutable image digest");
  }
  return fullyQualified;
}

function validateSingleContainer(resource, authority, label, expectedProcess = null) {
  const arrays = deepValuesForKey(resource, new Set(["containers"]))
    .filter((value) => Array.isArray(value));
  const candidates = arrays.filter((value) =>
    value.length === 1 && value[0]?.image === authority.imageDigest);
  if (candidates.length !== 1) {
    throw new Error(`${label} does not contain exactly one immutable candidate container`);
  }
  const container = candidates[0][0];
  validateSecretReference(
    container,
    authority,
    expectedProcess?.secretEnvironment ?? "CLEARRA_JOB_TOKEN",
  );
  if (expectedProcess) {
    if (
      JSON.stringify(container.command) !== JSON.stringify(expectedProcess.command) ||
      JSON.stringify(container.args) !== JSON.stringify(expectedProcess.args)
    ) {
      throw new Error(`${label} process arguments drifted`);
    }
  }
  return container;
}

function validateSecretReference(container, authority, environmentName) {
  const bindings = (Array.isArray(container?.env) ? container.env : [])
    .filter((entry) => entry?.name === environmentName);
  const reference = bindings[0]?.valueFrom?.secretKeyRef;
  const v1Reference =
    reference?.name === JOB_BEARER_SECRET &&
    String(reference?.key) === authority.jobBearerSecretVersion;
  const v2Reference =
    reference?.secret === JOB_BEARER_SECRET &&
    String(reference?.version) === authority.jobBearerSecretVersion;
  if (
    bindings.length !== 1 ||
    Object.hasOwn(bindings[0] ?? {}, "value") ||
    Number(v1Reference) + Number(v2Reference) !== 1
  ) {
    throw new Error("Cloud candidate must use only the pinned managed job-bearer Secret reference");
  }
}

function requireDeepNumericValue(resource, key, expected, label) {
  const values = deepValuesForKey(resource, new Set([key]));
  if (!values.some((value) => Number(value) === expected)) {
    throw new Error(`${label} drifted`);
  }
}

function deepValuesForKey(value, keys, output = []) {
  if (Array.isArray(value)) {
    for (const entry of value) deepValuesForKey(entry, keys, output);
    return output;
  }
  if (value === null || typeof value !== "object") return output;
  for (const [key, child] of Object.entries(value)) {
    if (keys.has(key)) output.push(child);
    deepValuesForKey(child, keys, output);
  }
  return output;
}

function serviceDescribeArguments(authority) {
  return [
    "run", "services", "describe", SERVICE,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    "--format=json",
  ];
}

function revisionDescribeArguments(authority) {
  return [
    "run", "revisions", "describe", authority.candidateRevision,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    "--format=json",
  ];
}

function smokeJobDescribeArguments(authority) {
  return [
    "run", "jobs", "describe", authority.smokeJob,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    "--format=json",
  ];
}

function smokeJobExecuteArguments(authority) {
  return [
    "run", "jobs", "execute", authority.smokeJob,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    "--wait",
    "--format=json",
  ];
}

function smokeJobLogsArguments(authority, executionName) {
  return [
    "run", "jobs", "logs", "read", authority.smokeJob,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    `--log-filter=labels."run.googleapis.com/execution_name"="${executionName}" AND textPayload:"candidate_smoke_job=passed"`,
    "--freshness=1h",
    "--order=desc",
    "--limit=10",
    "--format=json",
  ];
}

function smokeJobDeleteArguments(authority) {
  return [
    "run", "jobs", "delete", authority.smokeJob,
    `--project=${authority.projectId}`,
    `--region=${REGION}`,
    "--quiet",
  ];
}

function canonicalImageDigest(value, imageBase) {
  const prefix = `${imageBase}@`;
  if (typeof value !== "string" || !value.startsWith(prefix)) {
    throw new Error("Cloud image must be the exact Artifact Registry image@sha256 authority");
  }
  const digest = value.slice(prefix.length);
  if (!IMAGE_DIGEST.test(digest)) {
    throw new Error("Cloud image must be the exact Artifact Registry image@sha256 authority");
  }
  return value;
}

function canonicalCandidateOrigin(value) {
  let url;
  try {
    url = new URL(String(value));
  } catch {
    throw new Error("Cloud candidate URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/" ||
    !url.hostname.endsWith(".run.app") ||
    String(value) !== url.origin
  ) {
    throw new Error("Cloud candidate URL must be a credential-free HTTPS run.app origin");
  }
  return url.origin;
}

function requiredPattern(value, pattern, label) {
  const text = typeof value === "string" ? value : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function firstDefined(...values) {
  return values.find((value) => value !== undefined && value !== null);
}

function canonicalClock(now, label) {
  const value = now();
  const text = value instanceof Date ? value.toISOString() : String(value);
  return canonicalTimestamp(text, label);
}

function canonicalLogAttempts(value) {
  if (!Number.isSafeInteger(value) || value < 1 || value > 30) {
    throw new Error("managed candidate smoke log attempt count is invalid");
  }
  return value;
}

function waitFor(milliseconds) {
  return new Promise((resolveWait) => setTimeout(resolveWait, milliseconds));
}

async function writeCanonicalReportNew(path, report) {
  const target = resolve(path);
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(report)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("candidate smoke report path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function runGcloudJson(arguments_) {
  const output = runGcloud(arguments_);
  try {
    return JSON.parse(output);
  } catch {
    throw new Error("gcloud returned non-JSON candidate authority");
  }
}

export function gcloudProcessInvocation(
  arguments_,
  platform = process.platform,
  environment = process.env,
) {
  if (
    !Array.isArray(arguments_) ||
    arguments_.length === 0 ||
    arguments_.some((value) =>
      typeof value !== "string" ||
      (!CLOSED_GCLOUD_ATOM.test(value) && !CLOSED_GCLOUD_LOG_FILTER.test(value) &&
        !CLOSED_GCLOUD_PARITY_LOG_FILTER.test(value)))
  ) {
    throw new Error("gcloud candidate arguments are not a closed command surface");
  }
  if (platform === "win32") {
    const command = environment?.ComSpec || environment?.COMSPEC || "cmd.exe";
    return Object.freeze({
      command,
      arguments: Object.freeze(["/d", "/s", "/c", "gcloud.cmd", ...arguments_]),
    });
  }
  return Object.freeze({
    command: "gcloud",
    arguments: Object.freeze([...arguments_]),
  });
}

export function runGcloud(arguments_, dependencies = {}) {
  const platform = dependencies.platform ?? process.platform;
  const environment = dependencies.environment ?? process.env;
  const spawn = dependencies.spawn ?? spawnSync;
  const invocation = gcloudProcessInvocation(arguments_, platform, environment);
  const result = spawn(invocation.command, invocation.arguments, {
    encoding: "utf8",
    env: environment,
    maxBuffer: 4 * 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  if (result.error) {
    const code = /^[A-Z0-9_]+$/u.test(result.error.code ?? "")
      ? result.error.code
      : "UNKNOWN";
    throw new Error(`gcloud candidate process failed to start (${platform}:${code})`);
  }
  if (result.status !== 0) {
    const status = Number.isSafeInteger(result.status) ? result.status : "unknown";
    throw new Error(`gcloud candidate process exited unsuccessfully (${status})`);
  }
  return result.stdout;
}

async function main() {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      project: { type: "string" },
      "source-commit": { type: "string" },
      "prior-revision": { type: "string" },
      "job-bearer-secret-version": { type: "string" },
      "image-digest": { type: "string" },
      "candidate-url": { type: "string" },
      output: { type: "string" },
    },
    strict: true,
  });
  if (positionals.length !== 1 || !["deploy", "smoke"].includes(positionals[0])) {
    throw new Error("usage: candidate-release-v080.mjs (deploy|smoke) [closed options]");
  }
  const options = {
    projectId: values.project,
    sourceCommit: values["source-commit"],
    priorRevision: values["prior-revision"],
    jobBearerSecretVersion: values["job-bearer-secret-version"],
    imageDigest: values["image-digest"],
    candidateUrl: values["candidate-url"],
  };
  if (positionals[0] === "deploy" && (options.candidateUrl || values.output)) {
    throw new Error("deploy does not accept smoke-only URL or output authority");
  }
  if (positionals[0] === "deploy") {
    process.stdout.write(`${JSON.stringify(await deployZeroTrafficCandidate(options))}\n`);
    return;
  }
  if (typeof values.output !== "string" || values.output.length === 0) {
    throw new Error("smoke requires a new canonical report output path");
  }
  const report = await smokeZeroTrafficCandidate(options);
  await writeCanonicalReportNew(values.output, report);
  process.stdout.write(`${CLOUD_SMOKE_CONTRACT} ${report.report_sha256}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch {
    process.stderr.write("cloud_candidate_release=failed\n");
    process.exitCode = 2;
  }
}

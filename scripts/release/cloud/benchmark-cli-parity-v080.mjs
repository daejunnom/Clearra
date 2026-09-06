#!/usr/bin/env node
/**
 * Same-image diagnostic Job lifecycle, never a release/deployment authority.
 * Existing services are read-only; only a successfully created, UID-fenced Job
 * and its one owned execution may be removed. Raw logs/errors are never emitted.
 * CLI references: https://docs.cloud.google.com/sdk/gcloud/reference/run/jobs/create
 * https://docs.cloud.google.com/sdk/gcloud/reference/logging/read
 * https://docs.cloud.google.com/sdk/gcloud/reference/run/jobs/delete
 */
import { createHash } from "node:crypto";
import { lstat, open } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { candidateAuthority, runGcloud, validateSmokeExecution,
  validateZeroTrafficReadback } from "./candidate-release-v080.mjs";
import { currentRuntimeIdentityForCommit } from "../../../apps/clearra-discord-bot/src/job-service/runtime-identity.mjs";

export const PARITY_JOB_SCHEMA = "clearra.diagnostic.cloud-cli-parity-job.v1";
const INNER_SCHEMA = "clearra.diagnostic.cloud-cli-parity.v1";
const REGION = "asia-northeast1";
const SERVICE = "clearra-current-job";
const SHA256 = /^[0-9a-f]{64}$/u;
const RESOURCE = /^[a-z][a-z0-9-]{0,62}$/u;
const UID = /^[A-Za-z0-9-]{1,128}$/u;
const TIMING_KEYS = ["direct_process_ms", "service_process_ms", "service_job_ms", "service_http_wall_ms"];
const FIXTURES = ["ctk3-left-4l-p7-jstris180-pc-all", "ctk3-left-4l-p7-jstris180-pc-minimals",
  "ctk3-left-4l-p7-jstris180-build-all"];
const TIMING_SCOPE = "process-spawn-to-close; job-accept-to-result; loopback-http-wall";
class ParityFailure extends Error {
  constructor(code, report = undefined) { super(code); this.code = code; this.report = report; }
}
function requireThat(condition, code) { if (!condition) throw new ParityFailure(code); }
function hash(value) { return createHash("sha256").update(JSON.stringify(value)).digest("hex"); }
function equal(a, b) { return JSON.stringify(a) === JSON.stringify(b); }
function exactNumber(value, expected) { return value === expected || value === String(expected); }
function flags(a) { return [`--project=${a.projectId}`, `--region=${REGION}`]; }
function describeJob(a) { return ["run", "jobs", "describe", a.jobName, ...flags(a), "--format=json"]; }
function describeExecution(a, name) {
  return ["run", "jobs", "executions", "describe", name, ...flags(a), "--format=json"];
}
function metadataIdentity(value, name) {
  requireThat(value?.metadata?.name === name && UID.test(value?.metadata?.uid ?? ""), "resource_identity_mismatch");
  return value.metadata.uid;
}
function jobLabels(a) { return { "clearra-diagnostic": "cli-parity-v1", "clearra-source": a.sourceCommit,
  "clearra-run-id": a.runId }; }
function jobEnvironment(a) { return [
  { name: "CLEARRA_SOURCE_COMMIT", value: a.sourceCommit },
  { name: "CLEARRA_ENGINE_BUILD_ID", value: a.sourceCommit },
]; }

export function parityJobAuthority(options) {
  const mode = options.mode ?? "zero-traffic-candidate";
  requireThat(["zero-traffic-candidate", "isolated-image"].includes(mode), "invalid_parity_mode");
  const authority = mode === "isolated-image" ? isolatedImageAuthority(options)
    : candidateAuthority(options, { requireImage: true, requireCandidateUrl: true });
  requireThat(typeof options.runId === "string" && /^[1-9][0-9]{0,19}$/u.test(options.runId), "invalid_run_id");
  const jobName = `clearra-parity-${authority.sourceCommit.slice(0, 7)}-${options.runId}`;
  requireThat(RESOURCE.test(jobName), "invalid_job_name");
  const selectedWorkers = options.workers ?? (mode === "isolated-image" ? 4 : 8);
  requireThat([4, 8, "4", "8"].includes(selectedWorkers), "invalid_diagnostic_worker_profile");
  const workers = Number(selectedWorkers);
  requireThat(mode === "isolated-image" || workers === 8, "candidate_worker_profile_mismatch");
  return Object.freeze({ ...authority, mode, runId: options.runId, jobName,
    workers, cpus: workers, memory: workers === 4 ? "8Gi" : "16Gi" });
}

// Explicit diagnostics-only scope; never invent a candidate/prior revision or
// a secret version to pass production readback validation. The report from this
// mode cannot attest that any production service configuration is correct.
function isolatedImageAuthority(options) {
  const { projectId, sourceCommit, imageDigest } = options;
  requireThat(typeof projectId === "string" && /^[a-z][a-z0-9-]{4,61}[a-z0-9]$/u.test(projectId), "invalid_project");
  requireThat(typeof sourceCommit === "string" && /^[0-9a-f]{40}$/u.test(sourceCommit), "invalid_source_commit");
  const imageBase = `${REGION}-docker.pkg.dev/${projectId}/clearra/${SERVICE}`;
  requireThat(typeof imageDigest === "string" && imageDigest.startsWith(`${imageBase}@sha256:`) &&
    SHA256.test(imageDigest.slice(`${imageBase}@sha256:`.length)), "invalid_immutable_image");
  requireThat(["candidateUrl", "priorRevision", "jobBearerSecretVersion"].every((key) => options[key] === undefined),
    "isolated_image_cannot_claim_candidate_binding");
  return { projectId, sourceCommit, imageDigest,
    runtimeServiceAccount: `${SERVICE}@${projectId}.iam.gserviceaccount.com`,
    candidateRevision: null, priorRevision: null };
}

export function parityJobArguments(a) {
  // gcloud's ArgList rejects duplicate list items before making an API call.
  // Bind each value to its option; standalone repeated "8" values are invalid.
  return ["./scripts/benchmark-cloud-cli-parity.mjs", "--executable=/usr/local/bin/clearra",
    `--source-commit=${a.sourceCommit}`, `--cpus=${a.cpus}`, `--workers=${a.workers}`];
}

export function buildParityJobCreateArguments(a) {
  return ["run", "jobs", "create", a.jobName, ...flags(a), `--image=${a.imageDigest}`,
    `--service-account=${a.runtimeServiceAccount}`, "--tasks=1", "--parallelism=1", "--max-retries=0",
    "--task-timeout=900s", `--cpu=${a.cpus}`, `--memory=${a.memory}`, "--command=node",
    `--args=${parityJobArguments(a).join(",")}`,
    `--set-env-vars=${jobEnvironment(a).map(({ name, value }) => `${name}=${value}`).join(",")}`,
    `--labels=${Object.entries(jobLabels(a)).map(([key, value]) => `${key}=${value}`).join(",")}`,
    "--quiet", "--format=json"];
}

export function buildParityLogReadArguments(a) {
  // `run jobs logs read` prints human-formatted payloads even with --format=json;
  // structured jsonPayload entries become blank lines, losing their evidence.
  // Logging read preserves the envelope. Exact execution/schema checks still
  // occur in extractParityLogReport; an unrelated entry never becomes authority.
  return ["logging", "read", `resource.type=cloud_run_job AND resource.labels.project_id=${a.projectId}` +
    ` AND resource.labels.location=${REGION} AND resource.labels.job_name=${a.jobName}`,
    `--project=${a.projectId}`, "--freshness=1d", "--limit=100", "--format=json"];
}

// Validate the documented v1 gcloud readback shape, not a deep search that can
// accidentally accept the right value from an unrelated nested resource.
function validateExecutionSpec(spec, a) {
  const task = spec?.template?.spec;
  requireThat(exactNumber(spec?.taskCount, 1) && exactNumber(spec?.parallelism, 1) &&
    exactNumber(task?.maxRetries, 0) &&
    [900, "900", "900s"].includes(task?.timeoutSeconds) &&
    task?.serviceAccountName === a.runtimeServiceAccount, "job_scheduling_mismatch");
  requireThat(Array.isArray(task.containers) && task.containers.length === 1 &&
    (!task.volumes || task.volumes.length === 0), "job_container_mismatch");
  const c = task.containers[0];
  requireThat(c.image === a.imageDigest && equal(c.command, ["node"]) && equal(c.args, parityJobArguments(a)) &&
    String(c.resources?.limits?.cpu) === String(a.cpus) && c.resources?.limits?.memory === a.memory &&
    (!c.volumeMounts || c.volumeMounts.length === 0), "job_image_or_command_mismatch");
  const env = Array.isArray(c.env) ? c.env : [];
  requireThat(env.length === 2 && env.every((entry) => Object.keys(entry).length === 2 &&
    typeof entry.value === "string" && jobEnvironment(a).some((expected) =>
      expected.name === entry.name && expected.value === entry.value)) &&
    new Set(env.map((entry) => entry.name)).size === 2, "job_environment_or_secret_mismatch");
}

export function validateParityJobReadback(job, a, expectedUid = null) {
  const uid = metadataIdentity(job, a.jobName);
  requireThat(expectedUid === null || uid === expectedUid, "job_uid_changed");
  requireThat(Object.entries(jobLabels(a)).every(([key, value]) => job.metadata.labels?.[key] === value),
    "job_label_mismatch");
  // Secret volumes/annotations are not needed, even though no token is printed.
  for (const metadata of [job.metadata, job.spec?.template?.metadata, job.spec?.template?.spec?.template?.metadata]) {
    requireThat(!Object.keys(metadata?.annotations ?? {}).some((key) => /secret/iu.test(key)), "job_secret_annotation");
  }
  validateExecutionSpec(job?.spec?.template?.spec, a);
  return uid;
}

export function validateParityExecutionReadback(execution, a, job, jobUid, expectedName = null, expectedUid = null) {
  validateParityJobReadback(job, a, jobUid);
  const name = execution?.metadata?.name;
  requireThat(typeof name === "string" && RESOURCE.test(name) && name.startsWith(`${a.jobName}-`) &&
    (expectedName === null || name === expectedName), "execution_name_mismatch");
  const uid = metadataIdentity(execution, name);
  requireThat(expectedUid === null || expectedUid === uid, "execution_uid_changed");
  // Cloud Run v1 does not support Kubernetes ownerReferences. The freshly
  // read UID-fenced Job's single latestCreatedExecution is the parent binding.
  requireThat(exactNumber(job.status?.executionCount, 1) &&
    job.status?.latestCreatedExecution?.name === name, "execution_owner_mismatch");
  validateExecutionSpec(execution.spec, a);
  return { name, uid };
}

function median(values) { return [...values].sort((a, b) => a - b)[1]; }
function checkedTimings(raw) {
  requireThat(raw && Object.keys(raw).length === TIMING_KEYS.length, "invalid_timing_sample");
  return Object.fromEntries(TIMING_KEYS.map((key) => {
    const value = raw[key];
    requireThat(typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 120_000,
      "invalid_timing_sample");
    return [key, value];
  }));
}

export function validateParityReport(raw, a) {
  const runtime = currentRuntimeIdentityForCommit(a.sourceCommit);
  requireThat(raw?.schema_id === INNER_SCHEMA && raw.status === "passed" && raw.release_authority === false &&
    raw.cpus === a.cpus && raw.workers === a.workers && Number.isSafeInteger(raw.process_visible_cpus) &&
    raw.process_visible_cpus >= a.cpus && raw.process_visible_cpus <= 4096 &&
    raw.fixture_timeout_ms === 120_000 && raw.total_timeout_ms === 900_000 &&
    raw.timing_scope === TIMING_SCOPE && raw.cold_start_and_capability_excluded === true &&
    raw.pure_solver_timing === false && raw.performance_threshold_applied === false &&
    SHA256.test(raw.cli_binary_sha256 ?? ""), "invalid_parity_report");
  requireThat(Object.keys(runtime).every((key) => raw.runtime_identity?.[key] === runtime[key]) &&
    Object.keys(raw.runtime_identity).length === Object.keys(runtime).length, "reported_source_mismatch");
  requireThat(Array.isArray(raw.fixtures) && raw.fixtures.length === FIXTURES.length, "missing_fixture");
  const warnings = [];
  const fixtures = FIXTURES.map((id, index) => {
    const row = raw.fixtures[index];
    requireThat(row?.fixture_id === id && row.warm_pairs === 1 && row.measured_pairs === 3 &&
      Array.isArray(row.timings) && row.timings.length === 3 && SHA256.test(row.argv_sha256 ?? "") &&
      SHA256.test(row.normalized_keys_sha256 ?? "") && typeof row.solution_count === "string" &&
      /^[1-9][0-9]*$/u.test(row.solution_count), "incomplete_fixture");
    const minimum = index === 1;
    requireThat(minimum ? row.result_kind === "pc-minimum-cover.v2" && row.solution_count === "25" &&
      row.normalized_solution_set_hash === null && [row.canonical_members_sha256, row.set_identity_sha256,
        row.candidate_map_sha256].every((value) => SHA256.test(value ?? "")) :
      (index === 0 ? ["pc", "pc-scenario"].includes(row.result_kind) : row.result_kind === "build-probability") &&
      row.pattern_count === "5040" && /^cts1:[0-9a-f]{16}$/u.test(row.normalized_solution_set_hash ?? ""),
    "incomplete_result_identity");
    const timings = row.timings.map(checkedTimings);
    const medians = Object.fromEntries(TIMING_KEYS.map((key) => [key, median(timings.map((sample) => sample[key]))]));
    requireThat(Object.keys(row.medians_ms ?? {}).length === TIMING_KEYS.length &&
      TIMING_KEYS.every((key) => row.medians_ms[key] === medians[key]), "median_mismatch");
    const delta = medians.service_process_ms - medians.direct_process_ms;
    const jobOverhead = medians.service_job_ms - medians.service_process_ms;
    // Direction only: no invented performance threshold becomes a release gate.
    if (delta > 0 || jobOverhead > 0) warnings.push({ fixture_id: id,
      priority: "P2", code: "positive_warm_route_delta_measurement_only" });
    return { fixture_id: id, argv_sha256: row.argv_sha256, result_kind: row.result_kind,
      solution_count: row.solution_count, normalized_solution_set_hash: row.normalized_solution_set_hash,
      normalized_keys_sha256: row.normalized_keys_sha256,
      ...(minimum ? { canonical_members_sha256: row.canonical_members_sha256,
        set_identity_sha256: row.set_identity_sha256, candidate_map_sha256: row.candidate_map_sha256 } :
        { pattern_count: row.pattern_count }),
      warm_pairs: 1, measured_pairs: 3, timings, medians_ms: medians,
      warm_service_process_delta_ms: delta, warm_job_overhead_ms: jobOverhead,
      service_to_direct_process_ratio: medians.direct_process_ms === 0 ? null :
        medians.service_process_ms / medians.direct_process_ms };
  });
  requireThat(fixtures.flatMap((row) => row.timings).reduce((sum, sample) =>
    sum + sample.direct_process_ms + sample.service_http_wall_ms, 0) <= 900_000, "diagnostic_deadline_exceeded");
  // Return a whitelist, never the arbitrary incoming JSON/log object.
  return { schema_id: INNER_SCHEMA, status: "passed", release_authority: false, runtime_identity: runtime,
    cli_binary_sha256: raw.cli_binary_sha256, cpus: a.cpus, workers: a.workers, process_visible_cpus: raw.process_visible_cpus,
    fixture_timeout_ms: 120_000, total_timeout_ms: 900_000, timing_scope: TIMING_SCOPE,
    cold_start_and_capability_excluded: true, pure_solver_timing: false, performance_threshold_applied: false,
    fixtures, warnings };
}

export function extractParityLogReport(logs, a, executionName) {
  requireThat(Array.isArray(logs) && logs.length <= 100, "invalid_log_envelope");
  const reports = [];
  for (const entry of logs) {
    const labels = entry?.resource?.labels;
    if (entry?.resource?.type !== "cloud_run_job" || labels?.project_id !== a.projectId ||
      labels?.location !== REGION || labels?.job_name !== a.jobName ||
      entry.labels?.["run.googleapis.com/execution_name"] !== executionName) continue;
    let payload = entry.jsonPayload;
    if (!payload && typeof entry.textPayload === "string" && entry.textPayload.length <= 65_536) {
      try { payload = JSON.parse(entry.textPayload); } catch { continue; }
    }
    if (payload?.schema_id === INNER_SCHEMA) reports.push(payload);
  }
  requireThat(reports.length !== 0, "missing_parity_report");
  requireThat(reports.length === 1, "duplicate_parity_report");
  return validateParityReport(reports[0], a);
}

async function reserveNewReport(path) {
  requireThat(typeof path === "string" && path.length > 0, "missing_report_output");
  const target = resolve(path);
  let parent = dirname(target);
  while (true) {
    const info = await lstat(parent);
    requireThat(info.isDirectory() && !info.isSymbolicLink(), "unsafe_report_parent");
    const next = dirname(parent);
    if (next === parent) break;
    parent = next;
  }
  return open(target, "wx", 0o600);
}

export async function benchmarkCliParity(options, dependencies = {}) {
  const a = parityJobAuthority(options);
  // Reserve before any remote operation. A failed attempt retains its own report.
  const file = await reserveNewReport(options.output);
  const run = dependencies.run ?? runGcloud;
  const runJson = dependencies.runJson ?? (async (args) => JSON.parse(await run(args)));
  const sleep = dependencies.sleep ?? ((ms) => new Promise((done) => setTimeout(done, ms)));
  let stage = "candidate_readback";
  let createdUid = null;
  let executionIdentity = null;
  let createAttempted = false;
  let executeAttempted = false;
  let diagnostic = null;
  let failure = null;
  const report = { schema_id: PARITY_JOB_SCHEMA, release_authority: false, status: "failed",
    measurement_binding: a.mode, production_service_verified: false,
    source_commit: a.sourceCommit, run_id: a.runId, project_id: a.projectId, region: REGION,
    compute_profile: { cpus: a.cpus, workers: a.workers, memory: a.memory },
    candidate_revision: a.candidateRevision, prior_revision: a.priorRevision,
    image_digest: a.imageDigest, job_name: a.jobName, started_at: new Date().toISOString(),
    cleanup: { job: "not_created", execution: "not_created" } };
  async function candidateReadback() {
    const [service, revision] = await Promise.all([
      runJson(["run", "services", "describe", SERVICE, ...flags(a), "--format=json"]),
      runJson(["run", "revisions", "describe", a.candidateRevision, ...flags(a), "--format=json"]),
    ]);
    requireThat(validateZeroTrafficReadback({ service, revision }, a) === a.candidateUrl, "candidate_url_mismatch");
    return { service_sha256: hash(service), revision_sha256: hash(revision) };
  }
  try {
    if (a.mode === "zero-traffic-candidate") report.candidate_before = await candidateReadback();
    stage = "job_create";
    createAttempted = true;
    const created = await runJson(buildParityJobCreateArguments(a));
    createdUid = metadataIdentity(created, a.jobName); // Successful CREATE only, never an existing-job adoption.
    report.cleanup.job = "pending";
    stage = "job_readback";
    const job = await runJson(describeJob(a));
    validateParityJobReadback(job, a, createdUid);
    requireThat(exactNumber(job.status?.executionCount ?? 0, 0) && !job.status?.latestCreatedExecution,
      "job_already_executed");
    report.job_readback_sha256 = hash(job);
    stage = "job_execute";
    executeAttempted = true;
    const executed = await runJson(["run", "jobs", "execute", a.jobName, ...flags(a), "--wait", "--format=json"]);
    const executedJob = await runJson(describeJob(a));
    executionIdentity = validateParityExecutionReadback(executed, a, executedJob, createdUid);
    report.cleanup.execution = "pending";
    stage = "execution_readback";
    const execution = await runJson(describeExecution(a, executionIdentity.name));
    validateParityExecutionReadback(execution, a, executedJob, createdUid, executionIdentity.name, executionIdentity.uid);
    requireThat(validateSmokeExecution(execution) === executionIdentity.name, "execution_not_successful");
    for (const key of ["runningCount", "cancelledCount", "retriedCount"]) {
      requireThat(exactNumber(execution.status?.[key] ?? 0, 0), "execution_partial_or_retried");
    }
    const elapsed = Date.parse(execution.status?.completionTime) - Date.parse(execution.status?.startTime);
    requireThat(Number.isFinite(elapsed) && elapsed >= 0 && elapsed <= 900_000, "diagnostic_deadline_exceeded");
    report.execution_name = executionIdentity.name;
    report.execution_elapsed_ms = elapsed;
    report.execution_readback_sha256 = hash(execution);
    stage = "log_attestation";
    for (let attempt = 0; attempt < 3; attempt += 1) {
      const logs = await runJson(buildParityLogReadArguments(a));
      try { diagnostic = extractParityLogReport(logs, a, executionIdentity.name); break; }
      catch (error) {
        if (!(error instanceof ParityFailure) || error.code !== "missing_parity_report" || attempt === 2) throw error;
        await sleep(5_000);
      }
    }
    if (a.mode === "zero-traffic-candidate") {
      stage = "candidate_post_readback";
      report.candidate_after = await candidateReadback();
      report.zero_traffic_verified = true;
      report.production_service_verified = true;
    }
  } catch (error) {
    failure = error instanceof ParityFailure ? error.code : `${stage}_failed`;
  } finally {
    if (createdUid !== null) {
      try {
        const current = await runJson(describeJob(a));
        validateParityJobReadback(current, a, createdUid);
        const count = current.status?.executionCount ?? 0;
        requireThat(exactNumber(count, 0) || (executeAttempted && exactNumber(count, 1)), "cleanup_execution_count_mismatch");
        requireThat(exactNumber(count, 1) || (!current.status?.latestCreatedExecution && executionIdentity === null),
          "cleanup_execution_reference_mismatch");
        if (exactNumber(count, 1)) {
          const name = current.status?.latestCreatedExecution?.name;
          requireThat(typeof name === "string" && RESOURCE.test(name) && name.startsWith(`${a.jobName}-`),
            "cleanup_execution_name_mismatch");
          const execution = await runJson(describeExecution(a, name));
          const owned = validateParityExecutionReadback(execution, a, current, createdUid,
            executionIdentity?.name ?? name, executionIdentity?.uid ?? null);
          report.execution_name = owned.name;
          report.cleanup.execution = "pending";
          // The deployer has run.jobs.delete, not run.executions.delete.
          // Deleting this exclusively owned parent terminates its executions.
          // Keep the UID/count/parent checks, but do not widen IAM for cleanup.
        }
        await run(["run", "jobs", "delete", a.jobName, ...flags(a), "--quiet"]);
        report.cleanup.job = "deleted";
        if (report.cleanup.execution === "pending") report.cleanup.execution = "owned-parent-deleted";
      } catch {
        report.cleanup.job = "failed_or_identity_unverified";
        if (executeAttempted) report.cleanup.execution = "failed_or_identity_unverified";
        failure ??= "cleanup_failed";
      }
    } else if (createAttempted) {
      // CREATE may have failed because the name already exists or its response
      // was lost. Neither case authorizes adoption/deletion of that resource.
      report.cleanup.job = "create_outcome_unowned_no_delete";
    }
    report.ended_at = new Date().toISOString();
    report.status = failure === null && diagnostic !== null ? "passed" : "failed";
    if (failure !== null) report.failure_code = failure;
    if (diagnostic !== null) report.diagnostic = diagnostic;
    report.report_sha256 = hash(report);
    try { await file.writeFile(`${JSON.stringify(report, null, 2)}\n`); await file.sync(); }
    finally { await file.close(); }
  }
  if (report.status !== "passed") throw new ParityFailure(failure ?? "diagnostic_incomplete", report);
  return report;
}

async function main() {
  try {
    const { values } = parseArgs({ strict: true, options: {
      project: { type: "string" }, "source-commit": { type: "string" }, "prior-revision": { type: "string" },
      "image-digest": { type: "string" }, "candidate-url": { type: "string" },
      "job-bearer-secret-version": { type: "string" }, "run-id": { type: "string" }, output: { type: "string" },
      mode: { type: "string" }, workers: { type: "string" },
    } });
    const result = await benchmarkCliParity({ mode: values.mode, workers: values.workers, projectId: values.project, sourceCommit: values["source-commit"],
      priorRevision: values["prior-revision"], imageDigest: values["image-digest"], candidateUrl: values["candidate-url"],
      jobBearerSecretVersion: values["job-bearer-secret-version"], runId: values["run-id"], output: values.output });
    process.stdout.write(`${PARITY_JOB_SCHEMA} ${result.status} ${result.report_sha256}\n`);
  } catch (error) {
    process.stderr.write(`${PARITY_JOB_SCHEMA} failed ${error instanceof ParityFailure ? error.code : "diagnostic_wrapper_failed"}\n`);
    process.exitCode = 1;
  }
}
if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();

#!/usr/bin/env node

import { execFileSync, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { canonicalJson, canonicalSha256 } from "../canonical-release-evidence.mjs";
import { runGcloud } from "./candidate-release-v080.mjs";

const REGION = "asia-northeast1";
const SERVICE = "clearra-current-job";
const SECRET = "clearra-job-token";
const SCHEMA = "clearra.p7p4-solutions-http-diagnostic.v1";
const PROBE_SCHEMA = "clearra.p7p4-solutions-http-probe.v1";
const ID = /^[1-9][0-9]{0,19}$/u;
const HASH = /^[0-9a-f]{64}$/u;
const STAGES = ["queue", "prepare", "execute", "artifact", "projection", "render", "delivery"];
const FAILURES = ["engine-oom", "engine-signal", "engine-exit", "output-limit", "deadline",
  "transport-timeout", "cancelled", "result-validation"];
const delay = (ms) => new Promise((done) => setTimeout(done, ms));

export function p7p4DiagnosticAuthority(environment) {
  const sourceCommit = environment.GITHUB_SHA;
  const projectId = environment.GCP_PROJECT_ID;
  const imageBase = `${REGION}-docker.pkg.dev/${projectId}/clearra/${SERVICE}`;
  if (environment.GITHUB_EVENT_NAME !== "workflow_dispatch" || environment.GITHUB_REF !== "refs/heads/main" ||
      environment.GITHUB_REPOSITORY !== "daejunnom/Clearra" ||
      environment.GITHUB_WORKFLOW_REF !== "daejunnom/Clearra/.github/workflows/cloud-cli-diagnostic.yml@refs/heads/main" ||
      environment.CLEARRA_DIAGNOSTIC_MODE !== "p7p4-solutions" ||
      !/^[0-9a-f]{40}$/u.test(sourceCommit ?? "") || sourceCommit !== environment.CLEARRA_DIAGNOSTIC_EXPECTED_MAIN ||
      !/^[a-z][a-z0-9-]{4,28}[a-z0-9]$/u.test(projectId ?? "") ||
      !ID.test(environment.GITHUB_RUN_ID ?? "") || environment.GITHUB_RUN_ATTEMPT !== "1" ||
      environment.CLEARRA_DIAGNOSTIC_REVISION !== `${SERVICE}-v080-${sourceCommit.slice(0, 7)}` ||
      !environment.CLEARRA_DIAGNOSTIC_IMAGE_DIGEST?.startsWith(`${imageBase}@sha256:`) ||
      !HASH.test(environment.CLEARRA_DIAGNOSTIC_IMAGE_DIGEST.slice(imageBase.length + 8)) ||
      !HASH.test(environment.CLEARRA_DIAGNOSTIC_CLI_SHA256 ?? "")) {
    throw new Error("P7P4 diagnostic requires exact trusted main, immutable image and CLI authority");
  }
  return Object.freeze({ sourceCommit, projectId, imageDigest: environment.CLEARRA_DIAGNOSTIC_IMAGE_DIGEST,
    cliSha256: environment.CLEARRA_DIAGNOSTIC_CLI_SHA256, revision: environment.CLEARRA_DIAGNOSTIC_REVISION,
    runId: environment.GITHUB_RUN_ID, runAttempt: environment.GITHUB_RUN_ATTEMPT,
    jobName: `clearra-parity-${sourceCommit.slice(0, 7)}-${environment.GITHUB_RUN_ID}`,
    runtimeServiceAccount: `${SERVICE}@${projectId}.iam.gserviceaccount.com` });
}

function scope(a) { return [`--project=${a.projectId}`, `--region=${REGION}`, "--quiet", "--format=json"]; }
function serviceRead(a) { return ["run", "services", "describe", SERVICE, ...scope(a)]; }
function revisionRead(a) { return ["run", "revisions", "describe", a.revision, ...scope(a)]; }
function jobRead(a) { return ["run", "jobs", "describe", a.jobName, ...scope(a)]; }

function canonicalOrigin(value) {
  let url;
  try { url = new URL(value); } catch { /* Rejected below. */ }
  if (!url || url.protocol !== "https:" || url.username || url.password || url.search || url.hash ||
      url.pathname !== "/" || url.origin !== value || !url.hostname.endsWith(".run.app")) {
    throw new Error("P7P4 service origin is invalid");
  }
  return url.origin;
}

function tokenVersion(container, name) {
  const entries = (container?.env ?? []).filter((entry) => entry.name === name);
  const ref = entries[0]?.valueFrom?.secretKeyRef;
  if (entries.length !== 1 || Object.hasOwn(entries[0], "value") ||
      ref?.name !== SECRET || !/^[1-9][0-9]{0,18}$/u.test(String(ref?.key ?? ""))) {
    throw new Error("P7P4 managed token reference differs");
  }
  return String(ref.key);
}

function envMap(container) {
  const entries = container?.env ?? [];
  const map = new Map();
  for (const entry of entries) {
    if (typeof entry.name !== "string" || map.has(entry.name)) throw new Error("P7P4 environment is ambiguous");
    map.set(entry.name, entry.value);
  }
  return map;
}

export function validateP7p4ProductionReadback(service, revision, a) {
  const active = (service?.status?.traffic ?? []).filter((entry) => Number(entry.percent ?? 0) > 0);
  if (service?.metadata?.name !== SERVICE || revision?.metadata?.name !== a.revision ||
      active.length !== 1 || Number(active[0].percent) !== 100 || active[0].revisionName !== a.revision ||
      revision?.status?.imageDigest !== a.imageDigest ||
      !(revision?.status?.conditions ?? []).some((condition) => condition.type === "Ready" && condition.status === "True")) {
    throw new Error("P7P4 active revision or immutable image differs");
  }
  const versions = [];
  for (const spec of [service?.spec?.template?.spec, revision?.spec]) {
    const c = spec?.containers?.[0];
    if (spec?.containers?.length !== 1 || spec.serviceAccountName !== a.runtimeServiceAccount ||
        Number(spec.containerConcurrency) !== 1 || Number(spec.timeoutSeconds) !== 900 ||
        c.image !== a.imageDigest || !["8", "8000m"].includes(String(c.resources?.limits?.cpu)) ||
        c.resources?.limits?.memory !== "16Gi" || (c.command?.length ?? 0) !== 0 ||
        (c.args?.length ?? 0) !== 0 || (c.volumeMounts?.length ?? 0) !== 0 ||
        (spec.volumes?.length ?? 0) !== 0) throw new Error("P7P4 production resource contract differs");
    const env = envMap(c);
    for (const [key, value] of Object.entries({ CLEARRA_EXECUTABLE: "/usr/local/bin/clearra",
      CLEARRA_EXPECTED_VCPUS: "8", CLEARRA_SEARCH_WORKERS_PER_SESSION: "8", CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1",
      CLEARRA_MAX_CONCURRENT_JOBS: "1", CLEARRA_SEARCH_TIMEOUT_MS: "840000",
      CLEARRA_REVERSE_SEARCH_TIMEOUT_MS: "840000", CLEARRA_MAX_OUTPUT_BYTES: "4194304" })) {
      if (env.get(key) !== value) throw new Error("P7P4 production worker or deadline policy differs");
    }
    for (const key of ["CLEARRA_MAX_CTK3_FILE_BYTES", "CLEARRA_MAX_GIF_BYTES"]) {
      if (env.has(key) && env.get(key) !== "25165824") throw new Error("P7P4 production artifact limit differs");
    }
    if (env.has("CLEARRA_PC_SEARCH_TIMEOUT_MS") && env.get("CLEARRA_PC_SEARCH_TIMEOUT_MS") !== "840000") {
      throw new Error("P7P4 canonical PC deadline differs");
    }
    versions.push(tokenVersion(c, "CLEARRA_JOB_TOKEN"));
  }
  if (versions[0] !== versions[1] ||
      service.spec.template.metadata?.annotations?.["run.googleapis.com/cpu-throttling"] !== "false" ||
      revision.metadata.annotations?.["run.googleapis.com/cpu-throttling"] !== "false") {
    throw new Error("P7P4 production runtime policy differs");
  }
  return { serviceUrl: canonicalOrigin(service.status.url), secretVersion: versions[0],
    serviceReadbackSha256: canonicalSha256(service), revisionReadbackSha256: canonicalSha256(revision) };
}

function labels(p) { return { "clearra-diagnostic": "p7p4-solutions", "clearra-source": p.sourceCommit,
  "clearra-workflow-run": p.runId, "clearra-workflow-attempt": p.runAttempt, "clearra-probe-owner": p.ownerNonce }; }
function probeArgs(p) { return ["./scripts/run-p7p4-solutions-probe-job.mjs", "--source-commit", p.sourceCommit,
  "--cli-sha256", p.cliSha256, "--revision", p.revision, "--job-name", p.jobName, "--service-url", p.serviceUrl]; }

export function p7p4JobCreateArguments(p) {
  return ["run", "jobs", "create", p.jobName, ...scope(p), `--image=${p.imageDigest}`,
    `--service-account=${p.runtimeServiceAccount}`, "--tasks=1", "--parallelism=1", "--max-retries=0",
    "--task-timeout=900s", "--cpu=1", "--memory=512Mi", "--command=node", `--args=${probeArgs(p).join(",")}`,
    `--set-secrets=CLEARRA_CANDIDATE_JOB_TOKEN=${SECRET}:${p.secretVersion}`,
    `--set-env-vars=CLEARRA_P7P4_OWNER_DEADLINE_UNIX_MS=${p.ownerDeadlineUnixMs},CLEARRA_P7P4_OWNER_NONCE=${p.ownerNonce}`,
    `--labels=${Object.entries(labels(p)).map(([key, value]) => `${key}=${value}`).join(",")}`];
}

function ownedJobUid(job, p, expected) {
  const uid = job?.metadata?.uid;
  if (job?.metadata?.name !== p.jobName || !/^[A-Za-z0-9_-]{1,128}$/u.test(uid ?? "") ||
      (expected !== undefined && uid !== expected) ||
      Object.entries(labels(p)).some(([key, value]) => job.metadata.labels?.[key] !== value)) {
    throw new Error("P7P4 Job ownership differs");
  }
  return uid;
}

function validateClientTask(task, p) {
  const c = task?.containers?.[0];
  if (task?.containers?.length !== 1 || task.serviceAccountName !== p.runtimeServiceAccount ||
      Number(task.maxRetries) !== 0 || !["900", "900s"].includes(String(task.timeoutSeconds ?? task.timeout)) ||
      c.image !== p.imageDigest || !["1", "1000m"].includes(String(c.resources?.limits?.cpu)) || c.resources?.limits?.memory !== "512Mi" ||
      canonicalJson(c.command) !== canonicalJson(["node"]) || canonicalJson(c.args) !== canonicalJson(probeArgs(p)) ||
      (task.volumes?.length ?? 0) !== 0 || (c.volumeMounts?.length ?? 0) !== 0 ||
      tokenVersion(c, "CLEARRA_CANDIDATE_JOB_TOKEN") !== p.secretVersion) {
    throw new Error("P7P4 client task differs");
  }
  const env = envMap(c);
  if (env.size !== 3 || env.get("CLEARRA_P7P4_OWNER_DEADLINE_UNIX_MS") !== String(p.ownerDeadlineUnixMs) ||
      env.get("CLEARRA_P7P4_OWNER_NONCE") !== p.ownerNonce) {
    throw new Error("P7P4 client deadline or secret environment differs");
  }
}

export function validateP7p4Job(job, p, uid) {
  const owner = ownedJobUid(job, p, uid);
  const execution = job?.spec?.template?.spec;
  if (Number(execution?.taskCount) !== 1 || Number(execution.parallelism) !== 1) {
    throw new Error("P7P4 Job task cardinality differs");
  }
  validateClientTask(execution.template?.spec, p);
  return owner;
}

// Cloud Run v1 does not support ObjectMeta.ownerReferences. Its documented
// JobStatus pointer binds the one created Execution to this exact owned Job.
// https://docs.cloud.google.com/run/docs/reference/rest/v1/namespaces.jobs
export function validateP7p4JobExecutionOwner(job, p, jobUid, executionName = undefined) {
  validateP7p4Job(job, p, jobUid);
  const count = Number(job.status?.executionCount ?? 0);
  const latest = job.status?.latestCreatedExecution?.name;
  if (executionName === undefined ? count !== 0 || latest !== undefined : count !== 1 || latest !== executionName) {
    throw new Error("P7P4 Job execution association differs");
  }
}

export function validateP7p4Execution(execution, p, executionName = undefined, executionUid = undefined) {
  const meta = execution?.metadata;
  if (!meta?.name?.startsWith(`${p.jobName}-`) || !/^[a-z0-9-]{1,127}$/u.test(meta.name) ||
      !/^[A-Za-z0-9_-]{1,128}$/u.test(meta.uid ?? "") ||
      (executionName !== undefined && meta.name !== executionName) || (executionUid !== undefined && meta.uid !== executionUid) ||
      Number(execution.spec?.taskCount) !== 1 || Number(execution.spec?.parallelism) !== 1) {
    throw new Error("P7P4 execution ownership differs");
  }
  validateClientTask(execution.spec.template?.spec, p);
  return { name: meta.name, uid: meta.uid };
}

function targetArguments(lines) {
  return ["pc", "--count", "unique", "--lines", String(lines), "--board-mask", "0x0",
    "--height", String(lines), "--pieces", String(lines === 2 ? 5 : 10), "--patterns", "P7P4",
    "--hold", "empty", "--objective", "unique", "--rule", "srs-plus"];
}

// Rebuild the public receipt from an allow-list; raw provider output never
// reaches an artifact, including when validation or remote execution fails.
export function sanitizeP7p4ProbeReceipt(r, p, executionName) {
  if (Buffer.byteLength(JSON.stringify(r)) > 32 * 1024 || r?.schema !== PROBE_SCHEMA || r.diagnosticOnly !== true ||
      r.sourceCommit !== p.sourceCommit || r.cliSha256 !== p.cliSha256 || r.revision !== p.revision ||
      r.jobName !== p.jobName || r.executionName !== executionName || r.ownerNonce !== p.ownerNonce ||
      r.capabilityId !== "pc.solutions" ||
      r.countPolicy !== "CountUnique" || r.rule !== "srs-plus" || r.hold !== "empty" || r.queueKnowledge !== "oracle" ||
      r.sourcePattern !== "P7P4" || r.originalPatternCount !== 4_233_600 || r.originalSequenceLength !== 11 ||
      r.automaticPcTargets !== true || r.expectedVcpus !== 8 || r.sharedDeadlineMs !== 840000 ||
      !Number.isSafeInteger(r.deadlineUnixMs) || r.deadlineUnixMs < 1 || r.deadlineUnixMs > p.ownerDeadlineUnixMs ||
      !["passed", "failed"].includes(r.status) || !Array.isArray(r.targets) || r.targets.length < 1 || r.targets.length > 2 ||
      !Number.isSafeInteger(r.durationMs) || r.durationMs < 0 || r.durationMs > 900000 ||
      typeof r.startedAt !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/u.test(r.startedAt) ||
      !Number.isFinite(Date.parse(r.startedAt)) || r.deadlineUnixMs > Date.parse(r.startedAt) + 840000) {
    throw new Error("P7P4 probe receipt authority differs");
  }
  const targets = r.targets.map((target, index) => {
    const lines = [2, 4][index];
    const args = targetArguments(lines);
    if (target.lines !== lines || target.jobId !== `${p.jobName}-${lines}l` ||
        canonicalJson(target.arguments) !== canonicalJson(args) || !["passed", "failed"].includes(target.status) ||
        !Number.isSafeInteger(target.durationMs) || target.durationMs < 0 || target.durationMs > 900000) {
      throw new Error("P7P4 target receipt differs");
    }
    const safe = { lines, jobId: target.jobId, status: target.status, arguments: args, durationMs: target.durationMs,
      stageDurationsMs: Object.fromEntries(STAGES.flatMap((stage) => {
        const duration = target.stageDurationsMs?.[stage];
        return Number.isSafeInteger(duration) && duration >= 0 && duration <= 900000 ? [[stage, duration]] : [];
      })) };
    if (target.status === "failed") {
      if (index !== r.targets.length - 1 || !FAILURES.includes(target.failureCode) || !STAGES.includes(target.failureStage)) {
        throw new Error("P7P4 failed target receipt differs");
      }
      return { ...safe, failureCode: target.failureCode, failureStage: target.failureStage };
    }
    if (!["pc", "pc-scenario"].includes(target.kind) || target.complete !== true ||
        target.materializedPatternCount !== (lines === 2 ? 5040 : 4233600) ||
        !Number.isSafeInteger(target.solutionCount) || target.solutionCount < (lines === 4 ? 1 : 0) ||
        !/^cts1:[0-9a-f]{16}$/u.test(target.solutionSetHash ?? "") || !HASH.test(target.resultSha256 ?? "") ||
        !Number.isSafeInteger(target.workersUsed) || target.workersUsed < (target.solutionCount === 0 ? 0 : 1) ||
        target.workersUsed > 8 || (lines === 4 && target.workersUsed !== 8) ||
        !Number.isSafeInteger(target.stdoutBytes) || target.stdoutBytes < 1 ||
        !Number.isSafeInteger(target.stderrBytes) || target.stderrBytes < 0 ||
        target.stdoutBytes + target.stderrBytes > 4 * 1024 ** 2) throw new Error("P7P4 complete result receipt differs");
    const artifact = target.artifact;
    if (target.solutionCount === 0 ? artifact !== null :
      (!artifact || artifact.complete !== true || artifact.solutionCount !== target.solutionCount ||
       !HASH.test(artifact.sha256 ?? "") || !Number.isSafeInteger(artifact.byteLength) ||
       artifact.byteLength < 1 || artifact.byteLength > 24 * 1024 ** 2)) throw new Error("P7P4 artifact receipt differs");
    return { ...safe, kind: target.kind, complete: true, materializedPatternCount: target.materializedPatternCount,
      solutionCount: target.solutionCount, solutionSetHash: target.solutionSetHash, resultSha256: target.resultSha256,
      workersUsed: target.workersUsed, stdoutBytes: target.stdoutBytes, stderrBytes: target.stderrBytes,
      artifact: artifact ? { byteLength: artifact.byteLength, sha256: artifact.sha256,
        solutionCount: artifact.solutionCount, complete: true } : null };
  });
  if ((r.status === "passed") !== (targets.length === 2 && targets.every((target) => target.status === "passed")) ||
      (r.status === "failed" && targets.at(-1).status !== "failed")) throw new Error("P7P4 series status differs");
  return { schema: PROBE_SCHEMA, diagnosticOnly: true, status: r.status, sourceCommit: p.sourceCommit,
    cliSha256: p.cliSha256, revision: p.revision, jobName: p.jobName, executionName, ownerNonce: p.ownerNonce,
    capabilityId: "pc.solutions", countPolicy: "CountUnique", rule: "srs-plus", hold: "empty", queueKnowledge: "oracle",
    sourcePattern: "P7P4", originalPatternCount: 4233600, originalSequenceLength: 11, automaticPcTargets: true,
    expectedVcpus: 8, sharedDeadlineMs: 840000, deadlineUnixMs: r.deadlineUnixMs,
    startedAt: r.startedAt, durationMs: r.durationMs, targets };
}

export function runDiagnosticCloudCommand(args, { deadlineUnixMs, spawn = spawnSync, now = Date.now } = {}) {
  const remaining = (deadlineUnixMs ?? now() + 45000) - now();
  if (remaining <= 0) throw new Error("P7P4 control-plane deadline expired");
  const timeout = Math.min(45000, remaining);
  const raw = runGcloud(args, { spawn: (command, argv, options) => spawn(command, argv, { ...options, timeout }) });
  // Delete is a successful command without a JSON resource response. The
  // shared helper has already checked its exit status before returning.
  return args.slice(0, 3).join(" ") === "run jobs delete" ? null : JSON.parse(raw);
}

export async function runP7p4HttpDiagnostic(a, dependencies = {}) {
  const call = dependencies.call ?? runDiagnosticCloudCommand;
  const now = dependencies.now ?? Date.now;
  const wait = dependencies.wait ?? delay;
  const started = now();
  const controllerDeadline = started + 18 * 60_000;
  const workDeadline = controllerDeadline - 60_000;
  const request = (args, cleanup = false) => call(args, { deadlineUnixMs: cleanup ? controllerDeadline : workDeadline });
  const report = { schema: SCHEMA, diagnosticOnly: true, status: "failed", phase: "production-readback",
    ...a, cleanup: "not-created" };
  let p;
  let uid;
  let createAttempted = false;
  try {
    const live = validateP7p4ProductionReadback(await request(serviceRead(a)), await request(revisionRead(a)), a);
    p = { ...a, ...live, ownerNonce: (dependencies.nonce ?? (() => randomBytes(8).toString("hex")))(),
      ownerDeadlineUnixMs: Math.min(now() + 900000, workDeadline - 30000) };
    if (!/^[0-9a-f]{16}$/u.test(p.ownerNonce) || now() >= p.ownerDeadlineUnixMs) throw new Error("Invalid P7P4 owner");
    Object.assign(report, { serviceUrl: live.serviceUrl, serviceReadbackSha256: live.serviceReadbackSha256,
      revisionReadbackSha256: live.revisionReadbackSha256, ownerDeadlineUnixMs: p.ownerDeadlineUnixMs });
    report.phase = "job-create";
    createAttempted = true;
    const created = await request(p7p4JobCreateArguments(p));
    uid = ownedJobUid(created, p);
    report.cleanup = "pending";
    validateP7p4JobExecutionOwner(created, p, uid);
    report.phase = "job-readback";
    validateP7p4JobExecutionOwner(await request(jobRead(p)), p, uid);
    report.phase = "job-execute";
    const execution = await request(["run", "jobs", "execute", p.jobName, ...scope(p), "--async"]);
    const owner = validateP7p4Execution(execution, p);
    Object.assign(report, { jobUid: uid, executionName: owner.name, executionUid: owner.uid });
    report.phase = "execution-owner-readback";
    let parentBound = false;
    for (let read = 0; read < 10 && now() < workDeadline; read += 1) {
      const parent = await request(jobRead(p));
      validateP7p4Job(parent, p, uid);
      if (Number(parent.status?.executionCount ?? 0) !== 0 || parent.status?.latestCreatedExecution?.name !== undefined) {
        validateP7p4JobExecutionOwner(parent, p, uid, owner.name);
        parentBound = true;
        break;
      }
      if (read < 9) await wait(2000);
    }
    if (!parentBound) throw new Error("P7P4 Job execution association is unavailable");
    report.phase = "execution-wait";
    let completed;
    while (now() < Math.min(workDeadline - 15000, p.ownerDeadlineUnixMs + 30000)) {
      const current = await request(["run", "jobs", "executions", "describe", owner.name, ...scope(p)]);
      validateP7p4Execution(current, p, owner.name, owner.uid);
      const terminal = current.status?.conditions?.find((condition) => condition.type === "Completed");
      if (terminal && ["True", "False"].includes(String(terminal.status))) { completed = current; break; }
      await wait(2000);
    }
    if (!completed) throw new Error("P7P4 execution observation expired");
    validateP7p4JobExecutionOwner(await request(jobRead(p)), p, uid, owner.name);
    report.executionReadbackSha256 = canonicalSha256(completed);
    report.phase = "log-read";
    const filter = `resource.type=cloud_run_job AND resource.labels.project_id=${p.projectId} AND resource.labels.location=${REGION} AND resource.labels.job_name=${p.jobName}`;
    let probe;
    for (let read = 0; read < 10 && !probe && now() < workDeadline - 5000; read += 1) {
      const entries = await request(["logging", "read", filter, `--project=${p.projectId}`,
        "--limit=100", "--order=asc", "--quiet", "--format=json"]);
      if (!Array.isArray(entries) || entries.length > 100) throw new Error("P7P4 log bound exceeded");
      const matches = entries.flatMap((entry) => {
        if (entry?.resource?.labels?.job_name !== p.jobName ||
            entry.labels?.["run.googleapis.com/execution_name"] !== owner.name) return [];
        let payload = entry.jsonPayload;
        if (!payload && typeof entry.textPayload === "string" && Buffer.byteLength(entry.textPayload) <= 32768) {
          try { payload = JSON.parse(entry.textPayload); } catch { return []; }
        }
        return payload?.schema === PROBE_SCHEMA ? [payload] : [];
      });
      if (matches.length > 1) throw new Error("P7P4 terminal receipt is ambiguous");
      if (matches.length === 1) probe = sanitizeP7p4ProbeReceipt(matches[0], p, owner.name);
      else if (read < 9) await wait(2000);
    }
    if (!probe) throw new Error("P7P4 terminal receipt is missing");
    report.probe = probe;
    report.phase = "final-production-readback";
    validateP7p4ProductionReadback(await request(serviceRead(a)), await request(revisionRead(a)), a);
    const succeeded = completed.status?.conditions?.some((condition) => condition.type === "Completed" && condition.status === "True");
    report.status = probe.status === "passed" && succeeded && Number(completed.status.succeededCount) === 1 &&
      Number(completed.status.failedCount ?? 0) === 0 ? "passed" : "failed";
    report.phase = "complete";
  } catch { /* Only the closed phase and validated receipts are retained. */ }
  finally {
    if (p && createAttempted) {
      try {
        const current = await request(jobRead(p), true);
        uid = ownedJobUid(current, p, uid);
        report.jobUid = uid;
        await request(["run", "jobs", "delete", p.jobName, ...scope(p)], true);
        report.cleanup = "deleted-owned-job";
      } catch { report.cleanup = "ownership-or-delete-unverified"; report.status = "failed"; }
    }
  }
  report.durationMs = Math.max(0, now() - started);
  return report;
}

async function main() {
  const a = p7p4DiagnosticAuthority(process.env);
  if (execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", timeout: 10000 }).trim() !== a.sourceCommit) {
    throw new Error("P7P4 checkout differs from trusted main");
  }
  if (process.argv.length === 3 && process.argv[2] === "--validate-only") return;
  if (process.argv.length !== 2) throw new Error("P7P4 diagnostic arguments are invalid");
  const report = await runP7p4HttpDiagnostic(a);
  const output = resolve(`_local/artifacts/diagnostics/p7p4-solutions/${a.runId}-${a.runAttempt}/result.json`);
  const bytes = `${JSON.stringify(report, null, 2)}\n`;
  if (Buffer.byteLength(bytes) > 64 * 1024) throw new Error("P7P4 diagnostic report exceeds its bound");
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, bytes, { flag: "wx" });
  process.stdout.write(`${JSON.stringify(report)}\n`);
  process.exitCode = report.status === "passed" ? 0 : 2;
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  main().catch(() => { process.stderr.write("p7p4_solutions_diagnostic=failed-before-report\n"); process.exitCode = 2; });
}

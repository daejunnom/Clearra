import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { gcloudProcessInvocation } from "./candidate-release-v080.mjs";
import { p7p4DiagnosticAuthority, p7p4JobCreateArguments, validateP7p4ProductionReadback,
  validateP7p4Job, validateP7p4JobExecutionOwner, validateP7p4Execution, sanitizeP7p4ProbeReceipt,
  runDiagnosticCloudCommand, runP7p4HttpDiagnostic } from "./p7p4-solutions-http-diagnostic.mjs";

const source = "a".repeat(40);
const image = `asia-northeast1-docker.pkg.dev/clearra-cloud/clearra/clearra-current-job@sha256:${"b".repeat(64)}`;
const environment = { GITHUB_EVENT_NAME: "workflow_dispatch", GITHUB_REF: "refs/heads/main",
  GITHUB_REPOSITORY: "daejunnom/Clearra", GITHUB_SHA: source,
  GITHUB_WORKFLOW_REF: "daejunnom/Clearra/.github/workflows/cloud-cli-diagnostic.yml@refs/heads/main",
  GITHUB_RUN_ID: "12345", GITHUB_RUN_ATTEMPT: "1", GCP_PROJECT_ID: "clearra-cloud",
  CLEARRA_DIAGNOSTIC_MODE: "p7p4-solutions", CLEARRA_DIAGNOSTIC_EXPECTED_MAIN: source,
  CLEARRA_DIAGNOSTIC_REVISION: "clearra-current-job-v080-aaaaaaa",
  CLEARRA_DIAGNOSTIC_IMAGE_DIGEST: image, CLEARRA_DIAGNOSTIC_CLI_SHA256: "c".repeat(64) };
const authority = p7p4DiagnosticAuthority(environment);
const plan = { ...authority, serviceUrl: "https://clearra-current-job-example-an.a.run.app", secretVersion: "7",
  ownerNonce: "1234567890abcdef", ownerDeadlineUnixMs: 901000 };
const secret = (name) => ({ name, valueFrom: { secretKeyRef: { name: "clearra-job-token", key: "7" } } });

function production() {
  const spec = { serviceAccountName: plan.runtimeServiceAccount, containerConcurrency: 1, timeoutSeconds: 900,
    containers: [{ image, resources: { limits: { cpu: "8", memory: "16Gi" } }, env: [
      ...Object.entries({ CLEARRA_EXECUTABLE: "/usr/local/bin/clearra", CLEARRA_EXPECTED_VCPUS: "8",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "8", CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1",
        CLEARRA_MAX_CONCURRENT_JOBS: "1", CLEARRA_SEARCH_TIMEOUT_MS: "840000",
        CLEARRA_REVERSE_SEARCH_TIMEOUT_MS: "840000", CLEARRA_MAX_OUTPUT_BYTES: "4194304" })
        .map(([name, value]) => ({ name, value })), secret("CLEARRA_JOB_TOKEN")] }] };
  const annotations = { "run.googleapis.com/cpu-throttling": "false" };
  return { service: { metadata: { name: "clearra-current-job" },
    spec: { template: { metadata: { annotations }, spec } },
    status: { url: plan.serviceUrl, traffic: [{ percent: 100, revisionName: plan.revision }] } },
  revision: { metadata: { name: plan.revision, annotations }, spec: structuredClone(spec),
    status: { imageDigest: image, conditions: [{ type: "Ready", status: "True" }] } } };
}

function ownedJob() {
  return { metadata: { name: plan.jobName, uid: "owned-job-uid", labels: {
    "clearra-diagnostic": "p7p4-solutions", "clearra-source": source,
    "clearra-workflow-run": plan.runId, "clearra-workflow-attempt": "1", "clearra-probe-owner": plan.ownerNonce } },
  spec: { template: { spec: { taskCount: 1, parallelism: 1, template: { spec: {
    serviceAccountName: plan.runtimeServiceAccount, maxRetries: 0, timeoutSeconds: "900",
    containers: [{ image, command: ["node"], args: ["./scripts/run-p7p4-solutions-probe-job.mjs",
      "--source-commit", source, "--cli-sha256", plan.cliSha256, "--revision", plan.revision,
      "--job-name", plan.jobName, "--service-url", plan.serviceUrl],
    resources: { limits: { cpu: "1", memory: "512Mi" } }, env: [secret("CLEARRA_CANDIDATE_JOB_TOKEN"),
      { name: "CLEARRA_P7P4_OWNER_DEADLINE_UNIX_MS", value: String(plan.ownerDeadlineUnixMs) },
      { name: "CLEARRA_P7P4_OWNER_NONCE", value: plan.ownerNonce }] }] } } } } } };
}

function execution(job = ownedJob()) {
  return { metadata: { name: `${plan.jobName}-execution`, uid: "owned-execution-uid" },
  spec: structuredClone(job.spec.template.spec), status: { conditions: [{ type: "Completed", status: "True" }],
    succeededCount: 1, failedCount: 0 } };
}

function target(lines) {
  return { lines, jobId: `${plan.jobName}-${lines}l`, status: "passed", durationMs: 10,
    arguments: ["pc", "--count", "unique", "--lines", String(lines), "--board-mask", "0x0", "--height", String(lines),
      "--pieces", String(lines === 2 ? 5 : 10), "--patterns", "P7P4", "--hold", "empty", "--objective", "unique", "--rule", "srs-plus"],
    stageDurationsMs: { prepare: 1, execute: 8, artifact: 1 }, kind: "pc-scenario", complete: true,
    materializedPatternCount: lines === 2 ? 5040 : 4233600, solutionCount: 1,
    solutionSetHash: "cts1:1234567890abcdef", resultSha256: "d".repeat(64), workersUsed: lines === 2 ? 1 : 8,
    stdoutBytes: 500, stderrBytes: 0, artifact: { byteLength: 100, sha256: "e".repeat(64), solutionCount: 1, complete: true } };
}

function receipt() {
  return { schema: "clearra.p7p4-solutions-http-probe.v1", diagnosticOnly: true, status: "passed",
    sourceCommit: source, cliSha256: plan.cliSha256, revision: plan.revision, jobName: plan.jobName,
    executionName: `${plan.jobName}-execution`, ownerNonce: plan.ownerNonce,
    capabilityId: "pc.solutions", countPolicy: "CountUnique",
    rule: "srs-plus", hold: "empty", queueKnowledge: "oracle", sourcePattern: "P7P4",
    originalPatternCount: 4233600, originalSequenceLength: 11, automaticPcTargets: true, expectedVcpus: 8,
    sharedDeadlineMs: 840000, deadlineUnixMs: 841000, startedAt: "1970-01-01T00:00:01.000Z", durationMs: 20,
    targets: [target(2), target(4)] };
}

function harness(overrides = {}) {
  const calls = [];
  let clock = 1000;
  let liveReads = 0;
  let jobReads = 0;
  let executed = false;
  const job = ownedJob();
  const run = execution();
  const proof = receipt();
  const call = async (args, options) => {
    calls.push(args);
    gcloudProcessInvocation(args, "linux", {});
    assert.ok(options.deadlineUnixMs > clock);
    if (args.slice(0, 3).join(" ") === "run services describe") {
      const value = production().service;
      return overrides.service?.(value, ++liveReads) ?? value;
    }
    if (args.slice(0, 3).join(" ") === "run revisions describe") return production().revision;
    if (args.slice(0, 3).join(" ") === "run jobs create") {
      if (overrides.createError) throw new Error("PRIVATE provider error");
      return overrides.created?.(structuredClone(job)) ?? structuredClone(job);
    }
    if (args.slice(0, 3).join(" ") === "run jobs describe") {
      const value = structuredClone(job);
      if (executed) value.status = { executionCount: 1, latestCreatedExecution: { name: run.metadata.name } };
      return overrides.jobRead?.(value, ++jobReads) ?? value;
    }
    if (args.slice(0, 3).join(" ") === "run jobs execute") { executed = true; return structuredClone(run); }
    if (args.slice(0, 4).join(" ") === "run jobs executions describe") return overrides.execution?.(structuredClone(run)) ?? structuredClone(run);
    if (args.slice(0, 2).join(" ") === "logging read") return [{
      resource: { labels: { job_name: plan.jobName } }, labels: { "run.googleapis.com/execution_name": run.metadata.name },
      jsonPayload: overrides.receipt?.(structuredClone(proof)) ?? structuredClone(proof) }];
    if (args.slice(0, 3).join(" ") === "run jobs delete") return {};
    throw new Error("Unexpected diagnostic call");
  };
  return { calls, deps: { call, now: () => clock, nonce: () => plan.ownerNonce,
    wait: async () => { clock += overrides.expireWait ? 1000000 : 2000; } } };
}

test("manual diagnostic authority is exact-main, post-deploy, immutable and non-retrying", () => {
  assert.equal(authority.jobName, plan.jobName);
  for (const change of [{ GITHUB_REF: "refs/heads/other" }, { GITHUB_EVENT_NAME: "push" },
    { CLEARRA_DIAGNOSTIC_MODE: "read-logs" }, { GITHUB_RUN_ATTEMPT: "2" },
    { CLEARRA_DIAGNOSTIC_EXPECTED_MAIN: "f".repeat(40) }, { CLEARRA_DIAGNOSTIC_REVISION: "clearra-current-job-v080-fffffff" },
    { CLEARRA_DIAGNOSTIC_IMAGE_DIGEST: `sha256:${"b".repeat(64)}` },
    { CLEARRA_DIAGNOSTIC_IMAGE_DIGEST: `${image}extra` }, { CLEARRA_DIAGNOSTIC_CLI_SHA256: "main" },
    { GITHUB_WORKFLOW_REF: environment.GITHUB_WORKFLOW_REF.replace("cloud-cli-diagnostic", "other") }]) {
    assert.throws(() => p7p4DiagnosticAuthority({ ...environment, ...change }));
  }
  const argv = p7p4JobCreateArguments(plan);
  assert.deepEqual(argv.slice(0, 3), ["run", "jobs", "create"]);
  for (const item of ["--max-retries=0", "--tasks=1", "--parallelism=1", "--task-timeout=900s",
    "--cpu=1", "--memory=512Mi", `--image=${image}`]) assert.ok(argv.includes(item));
  gcloudProcessInvocation(argv, "linux", {});
});

test("production readback proves the active image, actual8CPU engine and original limits", () => {
  const live = production();
  assert.equal(validateP7p4ProductionReadback(live.service, live.revision, authority).secretVersion, "7");
  for (const mutate of [
    (x) => { x.service.status.traffic[0].percent = 99; },
    (x) => { x.revision.status.imageDigest = image.replace(/b/gu, "f"); },
    (x) => { x.revision.spec.containers[0].resources.limits.cpu = "4"; },
    (x) => { x.service.spec.template.spec.timeoutSeconds = 1800; },
    (x) => { x.revision.spec.containers[0].command = ["unexpected-process"]; },
    (x) => { x.revision.spec.containers[0].env.push({ name: "CLEARRA_PC_SEARCH_TIMEOUT_MS", value: "300000" }); },
    (x) => { x.service.spec.template.spec.containers[0].env.find((v) => v.name === "CLEARRA_EXPECTED_VCPUS").value = "16"; },
    (x) => { x.revision.spec.containers[0].env.at(-1).valueFrom.secretKeyRef.name = "other"; },
  ]) { const changed = production(); mutate(changed); assert.throws(() => validateP7p4ProductionReadback(changed.service, changed.revision, authority)); }
});

test("job and execution readbacks bind UID ownership, template, secret reference and task limits", () => {
  const job = ownedJob();
  assert.equal(validateP7p4Job(job, plan), job.metadata.uid);
  const run = execution(job);
  assert.equal(validateP7p4Execution(run, plan).name, `${plan.jobName}-execution`);
  validateP7p4JobExecutionOwner(job, plan, job.metadata.uid);
  const parent = structuredClone(job);
  parent.status = { executionCount: 1, latestCreatedExecution: { name: run.metadata.name } };
  validateP7p4JobExecutionOwner(parent, plan, job.metadata.uid, run.metadata.name);
  for (const mutate of [(x) => { x.status.executionCount = 2; },
    (x) => { x.status.latestCreatedExecution.name = `${plan.jobName}-foreign`; },
    (x) => { x.metadata.uid = "replaced-job"; }]) {
    const changed = structuredClone(parent); mutate(changed);
    assert.throws(() => validateP7p4JobExecutionOwner(changed, plan, job.metadata.uid, run.metadata.name));
  }
  const normalized = ownedJob();
  normalized.spec.template.spec.template.spec.containers[0].resources.limits.cpu = "1000m";
  assert.equal(validateP7p4Job(normalized, plan), job.metadata.uid);
  for (const mutate of [(x) => { x.metadata.uid = "other"; },
    (x) => { x.spec.template.spec.maxRetries = 1; },
    (x) => { x.spec.template.spec.containers[0].image = image.replace(/b/gu, "f"); },
    (x) => { x.spec.template.spec.containers[0].env.push({ name: "UNEXPECTED", value: "PRIVATE" }); }]) {
    const changed = execution(job); mutate(changed);
    assert.throws(() => validateP7p4Execution(changed, plan, run.metadata.name, run.metadata.uid));
  }
});

test("shared gcloud invocation is time-bounded and successful delete requires no JSON body", () => {
  const observed = [];
  const spawn = (command, argv, options) => {
    observed.push({ command, argv, options });
    return { status: 0, stdout: argv.slice(0, 3).join(" ") === "run jobs delete" ? "" : "{}" };
  };
  assert.deepEqual(runDiagnosticCloudCommand(["run", "jobs", "describe", plan.jobName, "--format=json"],
    { spawn, now: () => 1000, deadlineUnixMs: 6000 }), {});
  assert.equal(observed[0].options.timeout, 5000);
  assert.equal(observed[0].options.maxBuffer, 4 * 1024 ** 2);
  assert.equal(runDiagnosticCloudCommand(["run", "jobs", "delete", plan.jobName, "--quiet", "--format=json"],
    { spawn, now: () => 1000 }), null);
  assert.throws(() => runDiagnosticCloudCommand(["run", "jobs", "describe", plan.jobName],
    { spawn, now: () => 1000, deadlineUnixMs: 1000 }), /deadline expired/u);
  assert.equal(observed.length, 2);
});

test("receipt validation preserves full request, exact projected counts and complete artifact proof without raw fields", () => {
  const value = receipt(); value.private = "PRIVATE"; value.targets[1].artifact.private = "PRIVATE";
  const sanitized = sanitizeP7p4ProbeReceipt(value, plan, value.executionName);
  assert.deepEqual(sanitized.targets.map((x) => x.materializedPatternCount), [5040, 4233600]);
  assert.doesNotMatch(JSON.stringify(sanitized), /PRIVATE/u);
  for (const mutate of [(x) => { x.targets[0].status = "failed"; },
    (x) => { x.targets[0].materializedPatternCount = 4233600; },
    (x) => { x.targets[1].workersUsed = 4; }, (x) => { x.targets[1].artifact.complete = false; },
    (x) => { x.targets[1].arguments.push("--preserve-b2b"); }, (x) => { x.targets[1].artifact.byteLength = 25 * 1024 ** 2; },
    (x) => { x.deadlineUnixMs = plan.ownerDeadlineUnixMs + 1; }]) {
    const changed = receipt(); mutate(changed); assert.throws(() => sanitizeP7p4ProbeReceipt(changed, plan, changed.executionName));
  }
  const failed = receipt(); failed.status = "failed";
  failed.targets = [{ lines: 2, jobId: target(2).jobId, arguments: target(2).arguments, status: "failed",
    durationMs: 0, failureCode: "deadline", failureStage: "execute" }];
  assert.equal(sanitizeP7p4ProbeReceipt(failed, plan, failed.executionName).status, "failed");
});

test("controller runs one fresh managed-secret HTTP task and cleans only its owned Job", async () => {
  const h = harness(); const report = await runP7p4HttpDiagnostic(authority, h.deps);
  assert.equal(report.status, "passed"); assert.equal(report.cleanup, "deleted-owned-job");
  assert.equal(report.probe.targets[1].workersUsed, 8);
  for (const verb of ["create", "execute", "delete"]) assert.equal(h.calls.filter((a) => a.slice(0, 3).join(" ") === `run jobs ${verb}`).length, 1);
  assert.equal(h.calls.filter((a) => a.slice(0, 3).join(" ") === "run services describe").length, 2);
  assert.doesNotMatch(JSON.stringify(h.calls), /update-traffic|run deploy|secret.*access/u);
});

test("created policy-invalid or lost-response jobs still clean up only after matching owner nonce and UID", async () => {
  for (const overrides of [{ created: (job) => { job.spec.template.spec.template.spec.maxRetries = 1; return job; } }, { createError: true }]) {
    const h = harness(overrides); const report = await runP7p4HttpDiagnostic(authority, h.deps);
    assert.equal(report.status, "failed"); assert.equal(report.cleanup, "deleted-owned-job");
    assert.equal(h.calls.filter((a) => a.slice(0, 3).join(" ") === "run jobs execute").length, 0);
  }
  const h = harness({ createError: true, jobRead: (job) => { job.metadata.labels["clearra-probe-owner"] = "different"; return job; } });
  const report = await runP7p4HttpDiagnostic(authority, h.deps);
  assert.equal(report.status, "failed"); assert.equal(report.cleanup, "ownership-or-delete-unverified");
  assert.equal(h.calls.filter((a) => a.slice(0, 3).join(" ") === "run jobs delete").length, 0);
});

test("execution drift, deadline expiry and post-probe production drift never retry or publish success", async () => {
  for (const overrides of [
    { execution: (run) => { run.metadata.uid = "foreign"; return run; } },
    { jobRead: (job) => { if (job.status) job.status.executionCount = 2; return job; } },
    { execution: (run) => { run.status.conditions = []; return run; }, expireWait: true },
    { service: (service, read) => { if (read > 1) service.status.traffic[0].percent = 90; return service; } },
    { receipt: (proof) => { proof.targets[1].workersUsed = 4; return proof; } },
  ]) {
    const h = harness(overrides); const report = await runP7p4HttpDiagnostic(authority, h.deps);
    assert.equal(report.status, "failed"); assert.equal(report.cleanup, "deleted-owned-job");
    assert.equal(h.calls.filter((a) => a.slice(0, 3).join(" ") === "run jobs execute").length, 1);
    assert.doesNotMatch(JSON.stringify(report), /PRIVATE/u);
  }
});

test("controller imports with all package and built-codec dependencies forbidden", () => {
  const url = new URL("./p7p4-solutions-http-diagnostic.mjs", import.meta.url).href;
  execFileSync(process.execPath, ["--input-type=module", "--eval", `
    import { registerHooks } from "node:module";
    registerHooks({ resolve(specifier, context, next) {
      if (!["node:", ".", "/", "file:"].some(prefix => specifier.startsWith(prefix))) throw new Error("Package forbidden");
      const value = next(specifier, context);
      if (value.url.includes("node_modules") || value.url.includes("packages/ctk3/dist")) throw new Error("Built package forbidden");
      return value;
    } });
    await import(${JSON.stringify(url)});
  `], { timeout: 10000, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
});

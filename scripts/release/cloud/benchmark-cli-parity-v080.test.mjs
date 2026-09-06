import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { currentRuntimeIdentityForCommit } from "../../../apps/clearra-discord-bot/src/job-service/runtime-identity.mjs";
import { gcloudProcessInvocation } from "./candidate-release-v080.mjs";
import { PARITY_JOB_SCHEMA, benchmarkCliParity, buildParityJobCreateArguments, extractParityLogReport,
  parityJobArguments, parityJobAuthority, validateParityExecutionReadback,
  validateParityJobReadback, validateParityReport } from "./benchmark-cli-parity-v080.mjs";

const options = Object.freeze({ projectId: "clearra-cloud", sourceCommit: "1".repeat(40),
  priorRevision: "clearra-current-job-v075-0000000", jobBearerSecretVersion: "7", runId: "34012345678",
  imageDigest: `asia-northeast1-docker.pkg.dev/clearra-cloud/clearra/clearra-current-job@sha256:${"a".repeat(64)}`,
  candidateUrl: "https://candidate-1111111---clearra-current-job-test-an.a.run.app" });
const a = parityJobAuthority(options);
const jobUid = "f1111111-2222-3333-4444-555555555555";
const executionUid = "e1111111-2222-3333-4444-555555555555";
const executionName = `${a.jobName}-abcde`;
const copy = (value) => structuredClone(value);

function serviceContainer() {
  return { image: a.imageDigest, resources: { limits: { cpu: "8", memory: "16Gi" } }, env: [{
    name: "CLEARRA_JOB_TOKEN", valueFrom: { secretKeyRef: { name: "clearra-job-token", key: "7" } },
  }] };
}
function service() {
  return { metadata: { name: "clearra-current-job", annotations: { "run.googleapis.com/maxScale": "4" } },
    status: { latestCreatedRevisionName: a.candidateRevision, traffic: [
      { revisionName: a.priorRevision, percent: 100 },
      { revisionName: a.candidateRevision, tag: a.candidateTag, url: a.candidateUrl, percent: 0 },
    ] }, spec: { template: { metadata: { annotations: { "run.googleapis.com/startup-cpu-boost": "true",
      "run.googleapis.com/cpu-throttling": "false" } },
      spec: { serviceAccountName: a.runtimeServiceAccount, containerConcurrency: 1, containers: [serviceContainer()] } } } };
}
function revision() {
  return { metadata: { name: a.candidateRevision, annotations: { "autoscaling.knative.dev/maxScale": "4",
    "run.googleapis.com/startup-cpu-boost": "true", "run.googleapis.com/cpu-throttling": "false" } },
  status: { imageDigest: a.imageDigest, conditions: [{ type: "Ready", status: "True" }] },
  spec: { serviceAccountName: a.runtimeServiceAccount, containerConcurrency: 1, containers: [serviceContainer()] } };
}
function executionSpec() {
  return { taskCount: 1, parallelism: 1, template: { spec: {
    maxRetries: 0, timeoutSeconds: "900", serviceAccountName: a.runtimeServiceAccount,
    containers: [{ image: a.imageDigest, command: ["node"], args: parityJobArguments(a),
      resources: { limits: { cpu: "8", memory: "16Gi" } }, env: [
        { name: "CLEARRA_SOURCE_COMMIT", value: a.sourceCommit },
        { name: "CLEARRA_ENGINE_BUILD_ID", value: a.sourceCommit },
      ] }],
  } } };
}
function job(executed = false) {
  return { metadata: { name: a.jobName, uid: jobUid, labels: { "clearra-diagnostic": "cli-parity-v1",
    "clearra-source": a.sourceCommit, "clearra-run-id": a.runId } },
  spec: { template: { spec: executionSpec() } },
  status: executed ? { executionCount: 1, latestCreatedExecution: { name: executionName } } : { executionCount: 0 } };
}
function execution() {
  return { metadata: { name: executionName, uid: executionUid }, spec: executionSpec(),
    status: { succeededCount: 1, failedCount: 0, runningCount: 0, retriedCount: 0, cancelledCount: 0,
      conditions: [{ type: "Completed", status: "True" }],
      startTime: "2026-09-07T01:00:00Z", completionTime: "2026-09-07T01:05:00Z" } };
}
function innerReport() {
  return { schema_id: "clearra.diagnostic.cloud-cli-parity.v1", status: "passed", release_authority: false,
    runtime_identity: copy(currentRuntimeIdentityForCommit(a.sourceCommit)), cli_binary_sha256: "c".repeat(64),
    cpus: 8, workers: 8, process_visible_cpus: 8, fixture_timeout_ms: 120_000, total_timeout_ms: 900_000,
    timing_scope: "process-spawn-to-close; job-accept-to-result; loopback-http-wall",
    cold_start_and_capability_excluded: true, pure_solver_timing: false, performance_threshold_applied: false,
    fixtures: ["pc-all", "pc-minimals", "build-all"].map((suffix, index) => {
      const sample = { direct_process_ms: index === 1 ? 20_000 : 100,
        service_process_ms: index === 1 ? 20_200 : 110,
        service_job_ms: index === 1 ? 20_202 : 112, service_http_wall_ms: index === 1 ? 20_204 : 114 };
      return { fixture_id: `ctk3-left-4l-p7-jstris180-${suffix}`, argv_sha256: "d".repeat(64),
        normalized_keys_sha256: "e".repeat(64), solution_count: index === 1 ? "25" : "246",
        result_kind: ["pc-scenario", "pc-minimum-cover.v2", "build-probability"][index],
        normalized_solution_set_hash: index === 1 ? null : `cts1:${"b".repeat(16)}`,
        ...(index === 1 ? { canonical_members_sha256: "2".repeat(64), set_identity_sha256: "3".repeat(64),
          candidate_map_sha256: "4".repeat(64) } : { pattern_count: "5040" }),
        warm_pairs: 1, measured_pairs: 3, timings: [copy(sample), copy(sample), copy(sample)], medians_ms: sample };
    }) };
}
function logEntry(payload = innerReport()) {
  return { resource: { type: "cloud_run_job", labels: { project_id: a.projectId, location: "asia-northeast1", job_name: a.jobName } },
    labels: { "run.googleapis.com/execution_name": executionName }, jsonPayload: payload };
}
function mockCloud(changes = {}) {
  const calls = [];
  let executed = false;
  const dependencies = {
    sleep: async () => {},
    run: async (args) => {
      gcloudProcessInvocation(args, "linux", {});
      calls.push(args);
      assert.ok(args.includes("delete"), "mock run may only delete exact diagnostic resources");
      if (changes.deleteFailure) throw new Error("sensitive arbitrary process error must not be printed");
      return "";
    },
    runJson: async (args) => {
      gcloudProcessInvocation(args, "win32", { ComSpec: "cmd.exe" });
      calls.push(args);
      if (args[1] === "services") return changes.service?.(service()) ?? service();
      if (args[1] === "revisions") return changes.revision?.(revision()) ?? revision();
      if (args[2] === "create") {
        if (changes.createFailure) throw new Error("AlreadyExists with arbitrary sensitive text");
        return changes.created?.(job()) ?? job();
      }
      if (args[2] === "describe") return changes.job?.(job(executed), calls) ?? job(executed);
      if (args[2] === "execute") {
        executed = !changes.noExecutionCreated;
        if (changes.executeFailure) throw new Error("sensitive task stderr must not be printed");
        return changes.execution?.(execution()) ?? execution();
      }
      if (args[2] === "executions" && args[3] === "describe") return changes.execution?.(execution()) ?? execution();
      if (args[2] === "logs") return changes.logs?.([logEntry()], calls) ?? [logEntry()];
      assert.fail(`unexpected mock call kind ${args.slice(0, 3).join(" ")}`);
    },
  };
  return { calls, dependencies };
}
async function withOutput(run) {
  const dir = await mkdtemp(join(tmpdir(), "clearra-parity-wrapper-"));
  try { return await run(join(dir, "report.json")); }
  finally { await rm(dir, { recursive: true, force: true }); }
}
async function failedRun(changes, code, verify = () => {}) {
  return withOutput(async (output) => {
    const cloud = mockCloud(changes);
    await assert.rejects(benchmarkCliParity({ ...options, output }, cloud.dependencies), (error) => {
      assert.equal(error.code, code); return true;
    });
    const text = await readFile(output, "utf8");
    assert.ok(!text.includes("sensitive"));
    const report = JSON.parse(text);
    assert.equal(report.status, "failed");
    assert.equal(report.release_authority, false);
    await verify(report, cloud.calls);
  });
}

test("create uses same immutable image, closed nonsecret args, CPU8 and no service mutation", () => {
  const args = buildParityJobCreateArguments(a);
  assert.equal(args[2], "create");
  for (const value of ["--tasks=1", "--parallelism=1", "--max-retries=0", "--task-timeout=900s", "--cpu=8", "--memory=16Gi"])
    assert.ok(args.includes(value));
  assert.ok(args.includes(`--image=${a.imageDigest}`));
  assert.ok(!args.some((value) => /secret|traffic|deploy|execute-now|accepted/iu.test(value)));
  assert.equal(gcloudProcessInvocation(args, "win32", {}).arguments[4], "run");
  assert.equal(parityJobAuthority({ ...options, runId: "9".repeat(20) }).jobName.length <= 49, true);
  for (const value of ["0", "1;whoami", "1 2", "01", "9".repeat(21)])
    assert.throws(() => parityJobAuthority({ ...options, runId: value }), /invalid_run_id/);
  assert.throws(() => parityJobAuthority({ ...options, imageDigest: "mutable:latest" }));
});

test("success validates parent/execution ownership and deletes the parent without additional IAM", async () => {
  await withOutput(async (output) => {
    const { calls, dependencies } = mockCloud();
    const result = await benchmarkCliParity({ ...options, output }, dependencies);
    assert.equal(result.schema_id, PARITY_JOB_SCHEMA);
    assert.equal(result.status, "passed");
    assert.equal(result.release_authority, false);
    assert.deepEqual(result.cleanup, { job: "deleted", execution: "owned-parent-deleted" });
    assert.equal(result.execution_elapsed_ms, 300_000);
    assert.equal(result.diagnostic.fixtures[0].service_to_direct_process_ratio, 1.1);
    assert.equal(result.diagnostic.fixtures[0].warm_job_overhead_ms, 2);
    assert.equal(result.diagnostic.performance_threshold_applied, false);
    assert.equal(result.diagnostic.warnings.length, 3);
    assert.deepEqual(JSON.parse(await readFile(output, "utf8")), result);
    const deletes = calls.filter((args) => args.includes("delete"));
    assert.equal(deletes.length, 1);
    assert.equal(deletes[0][3], a.jobName);
    assert.ok(!deletes.some((args) => args.includes("executions")));
    assert.equal(calls.filter((args) => args[2] === "execute").length, 1);
    assert.ok(calls.filter((args) => args[1] === "services" || args[1] === "revisions").every((args) => args[2] === "describe"));
    assert.ok(calls.filter((args) => args[2] === "logs").every((args) =>
      args.includes("--limit=100") && !args.some((value) => value.includes("log-filter"))));
  });
});

test("isolated image diagnostic never reads or mutates a production service and cannot claim candidate authority", async () => {
  const isolated = { mode: "isolated-image", projectId: options.projectId, sourceCommit: options.sourceCommit,
    imageDigest: options.imageDigest, runId: options.runId };
  const ia = parityJobAuthority(isolated);
  assert.equal(ia.candidateRevision, null);
  assert.equal(ia.priorRevision, null);
  for (const key of ["candidateUrl", "priorRevision", "jobBearerSecretVersion"])
    assert.throws(() => parityJobAuthority({ ...isolated, [key]: options[key] }), /cannot_claim_candidate_binding/);
  for (const patch of [{ mode: "automatic" }, { projectId: "p;whoami" }, { sourceCommit: "main" },
    { imageDigest: options.imageDigest.replace("clearra-cloud/", "another-project/") }, { imageDigest: "image:latest" }])
    assert.throws(() => parityJobAuthority({ ...isolated, ...patch }));
  await withOutput(async (output) => {
    const cloud = mockCloud();
    const result = await benchmarkCliParity({ ...isolated, output }, cloud.dependencies);
    assert.equal(result.status, "passed");
    assert.equal(result.measurement_binding, "isolated-image");
    assert.equal(result.production_service_verified, false);
    assert.equal(result.release_authority, false);
    assert.equal(result.zero_traffic_verified, undefined);
    assert.equal(result.candidate_before, undefined);
    assert.deepEqual(result.cleanup, { job: "deleted", execution: "owned-parent-deleted" });
    assert.ok(cloud.calls.every((args) => args[0] === "run" && args[1] === "jobs"));
  });
});

test("standalone Cloud workflow is manual, protected, exact-source bound and has no publication step", async () => {
  const source = await readFile(new URL("../../../.github/workflows/cloud-cli-diagnostic.yml", import.meta.url), "utf8");
  assert.match(source, /workflow_dispatch:/u);
  assert.match(source, /environment: discord-path-confirmation/u);
  assert.match(source, /contents: read/u);
  assert.match(source, /persist-credentials: false/u);
  assert.match(source, /cancel-in-progress: false/u);
  assert.match(source, /git ls-remote origin refs\/heads\/main/u);
  assert.match(source, /--mode isolated-image/u);
  assert.equal((source.match(/gcloud builds submit/gu) ?? []).length, 1);
  const buildJob = source.split('  build-image:')[1].split('  evaluate:')[0];
  const evaluateJob = source.split('  evaluate:')[1];
  assert.match(buildJob, /service_account: \$\{\{ vars.GCP_BUILD_SERVICE_ACCOUNT \}\}/u);
  assert.doesNotMatch(buildJob, /environment:|GCP_DEPLOY_SERVICE_ACCOUNT/u);
  assert.match(buildJob, /--region=asia-northeast1/u);
  assert.match(buildJob, /--gcs-source-staging-dir="gs:\/\/clearra-cloud_cloudbuild\/source"/u);
  assert.match(buildJob, /--service-account="projects\//u);
  assert.match(evaluateJob, /needs: build-image/u);
  assert.match(evaluateJob, /GCP_DEPLOY_SERVICE_ACCOUNT/u);
  assert.doesNotMatch(evaluateJob, /gcloud builds submit|GCP_BUILD_SERVICE_ACCOUNT/u);
  assert.equal((source.match(/node scripts\/release\/cloud\/benchmark-cli-parity-v080.mjs/gu) ?? []).length, 1);
  assert.doesNotMatch(source, /workflow_run:|\n  push:|secrets\.|contents: write|gcloud run|restore|invoke-.*\.ps1|uses:.*deploy-pages/u);
});

test("output create_new refuses existing content before any Cloud access", async () => {
  await withOutput(async (output) => {
    await writeFile(output, "sentinel");
    const cloud = mockCloud();
    await assert.rejects(benchmarkCliParity({ ...options, output }, cloud.dependencies), { code: "EEXIST" });
    assert.equal(await readFile(output, "utf8"), "sentinel");
    assert.equal(cloud.calls.length, 0);
  });
});

test("zero-traffic drift blocks create and never touches production", async () => {
  await failedRun({ service: (value) => { value.status.traffic[0].percent = 99; return value; } },
    "candidate_readback_failed", (report, calls) => {
      assert.equal(report.cleanup.job, "not_created"); assert.ok(!calls.some((args) => args.includes("create") || args.includes("delete")));
    });
});

test("create collision or ambiguous failure never adopts/deletes a preexisting Job", async () => {
  await failedRun({ createFailure: true }, "job_create_failed", (report, calls) => {
    assert.equal(report.cleanup.job, "create_outcome_unowned_no_delete");
    assert.ok(!calls.some((args) => args.includes("delete") || args.includes("execute")));
  });
});

test("failed execution discovers its one exact parent-bound execution and cleans both", async () => {
  await failedRun({ executeFailure: true }, "job_execute_failed", (report, calls) => {
    assert.deepEqual(report.cleanup, { job: "deleted", execution: "owned-parent-deleted" });
    assert.equal(calls.filter((args) => args.includes("delete")).length, 1);
    assert.equal(report.diagnostic, undefined);
  });
});

test("failed launch with no created execution deletes only the owned unexecuted Job", async () => {
  await failedRun({ executeFailure: true, noExecutionCreated: true }, "job_execute_failed", (report, calls) => {
    assert.deepEqual(report.cleanup, { job: "deleted", execution: "not_created" });
    assert.equal(calls.filter((args) => args.includes("delete")).length, 1);
  });
});

test("cleanup failure changes outer status even when complete parity samples were collected", async () => {
  await failedRun({ deleteFailure: true }, "cleanup_failed", (report, calls) => {
    assert.equal(report.diagnostic.status, "passed");
    assert.equal(report.cleanup.job, "failed_or_identity_unverified");
    assert.equal(calls.filter((args) => args.includes("delete")).length, 1);
  });
});

test("replaced Job UID forbids cleanup and prevents issued workload", async () => {
  await failedRun({ job: (value) => { value.metadata.uid = "different-uid"; return value; } },
    "job_uid_changed", (report, calls) => {
      assert.equal(report.cleanup.job, "failed_or_identity_unverified");
      assert.ok(!calls.some((args) => args.includes("delete") || args.includes("execute")));
    });
});

test("second execution or unrelated parent reference cannot acquire cleanup authority", async () => {
  await failedRun({ job: (value) => {
    if (value.status.executionCount === 1) value.status.executionCount = 2;
    return value;
  } }, "execution_owner_mismatch", (report, calls) => {
    assert.equal(report.cleanup.job, "failed_or_identity_unverified");
    assert.ok(!calls.some((args) => args.includes("delete")));
  });
});

test("elapsed beyond 900 seconds is not a successful benchmark", async () => {
  await failedRun({ execution: (value) => { value.status.completionTime = "2026-09-07T01:15:01Z"; return value; } },
    "diagnostic_deadline_exceeded", (report) => assert.equal(report.cleanup.job, "deleted"));
});

test("partial/retried executions never use passed log text as success authority", async () => {
  await failedRun({ execution: (value) => { value.status.retriedCount = 1; return value; } }, "execution_partial_or_retried");
  await failedRun({ execution: (value) => { value.status.succeededCount = 0; return value; } }, "execution_readback_failed");
});

test("every job resource and secret-free argv/source binding is exact", () => {
  const mutations = [
    (value) => { value.spec.template.spec.taskCount = 2; },
    (value) => { value.spec.template.spec.parallelism = 2; },
    (value) => { value.spec.template.spec.template.spec.maxRetries = 1; },
    (value) => { value.spec.template.spec.template.spec.timeoutSeconds = "901"; },
    (value) => { value.spec.template.spec.template.spec.containers[0].image = "mutable:latest"; },
    (value) => { value.spec.template.spec.template.spec.containers[0].resources.limits.cpu = "4"; },
    (value) => { value.spec.template.spec.template.spec.containers[0].resources.limits.memory = "8Gi"; },
    (value) => { value.spec.template.spec.template.spec.containers[0].command = ["sh"]; },
    (value) => { value.spec.template.spec.template.spec.containers[0].args.push("--workers", "1"); },
    (value) => { value.spec.template.spec.template.spec.containers[0].env[0].value = "2".repeat(40); },
    (value) => { value.spec.template.spec.template.spec.containers[0].env.push({ name: "TOKEN", valueFrom: {} }); },
    (value) => { value.spec.template.spec.template.spec.volumes = [{ secret: {} }]; },
    (value) => { value.metadata.annotations = { "run.googleapis.com/secrets": "not-read" }; },
    (value) => { value.spec.template.spec.template.spec.serviceAccountName = "other"; },
  ];
  assert.equal(validateParityJobReadback(job(), a), jobUid);
  for (const mutate of mutations) { const value = job(); mutate(value); assert.throws(() => validateParityJobReadback(value, a)); }
  const reversed = job(); reversed.spec.template.spec.template.spec.containers[0].env.reverse();
  assert.equal(validateParityJobReadback(reversed, a), jobUid);
});

test("execution UID, exact job pointer and immutable spec are independently checked", () => {
  assert.deepEqual(validateParityExecutionReadback(execution(), a, job(true), jobUid), { name: executionName, uid: executionUid });
  const wrongParent = job(true); wrongParent.status.latestCreatedExecution.name += "-old";
  assert.throws(() => validateParityExecutionReadback(execution(), a, wrongParent, jobUid), /owner/);
  assert.throws(() => validateParityExecutionReadback(execution(), a, job(true), jobUid, executionName, "old-uid"), /uid/);
  const wrongArgs = execution(); wrongArgs.spec.template.spec.containers[0].args[0] = "different.mjs";
  assert.throws(() => validateParityExecutionReadback(wrongArgs, a, job(true), jobUid), /command/);
});

test("logs bind exact project/region/job/execution/schema and strip arbitrary fields", () => {
  const raw = innerReport(); raw.credentials = "must-not-escape"; raw.fixtures[0].raw_stdout = "must-not-escape";
  const clean = extractParityLogReport([logEntry(raw)], a, executionName);
  assert.ok(!JSON.stringify(clean).includes("must-not-escape"));
  const text = logEntry(); delete text.jsonPayload; text.textPayload = JSON.stringify(raw);
  assert.equal(extractParityLogReport([text], a, executionName).status, "passed");
  for (const mutate of [
    (value) => { value.labels["run.googleapis.com/execution_name"] += "-other"; },
    (value) => { value.resource.labels.project_id = "other-project"; },
    (value) => { value.resource.labels.location = "us-east1"; },
    (value) => { value.resource.labels.job_name = "other-job"; },
  ]) { const entry = logEntry(); mutate(entry); assert.throws(() => extractParityLogReport([entry], a, executionName), /missing/); }
  assert.throws(() => extractParityLogReport([logEntry(), logEntry()], a, executionName), /duplicate/);
});

test("report rejects identity drift, missing samples, bad counts, nonfinite/overdeadline samples and false medians", () => {
  for (const mutate of [
    (raw) => { raw.status = "failed"; },
    (raw) => { raw.release_authority = true; },
    (raw) => { raw.runtime_identity.sourceCommit = "2".repeat(40); },
    (raw) => { raw.workers = 7; },
    (raw) => { raw.process_visible_cpus = 4; },
    (raw) => { raw.fixtures.pop(); },
    (raw) => { raw.fixtures[1].solution_count = "24"; },
    (raw) => { raw.fixtures[0].pattern_count = "5039"; },
    (raw) => { raw.fixtures[0].timings.pop(); },
    (raw) => { raw.fixtures[0].timings[0].direct_process_ms = NaN; },
    (raw) => { raw.fixtures[0].timings[0].direct_process_ms = 120_001; },
    (raw) => { raw.fixtures[0].medians_ms.direct_process_ms = 101; },
    (raw) => { raw.fixtures[0].normalized_keys_sha256 = "missing"; },
  ]) { const raw = innerReport(); mutate(raw); assert.throws(() => validateParityReport(raw, a)); }
});

test("missing log ingestion gets only three bounded reads and no arbitrary retry of duplicate evidence", async () => {
  let reads = 0;
  await withOutput(async (output) => {
    const cloud = mockCloud({ logs: (logs) => (++reads < 3 ? [] : logs) });
    assert.equal((await benchmarkCliParity({ ...options, output }, cloud.dependencies)).status, "passed");
    assert.equal(reads, 3);
  });
  await failedRun({ logs: () => [logEntry(), logEntry()] }, "duplicate_parity_report", (_, calls) =>
    assert.equal(calls.filter((args) => args[2] === "logs").length, 1));
});

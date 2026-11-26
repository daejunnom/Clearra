#!/usr/bin/env node

import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { open } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import { validateSolutionTransportArtifact } from "../src/clearra/solution-artifact.mjs";
import { validDiscordPcSolutionsResult } from "../src/discord/pc-solutions-result.mjs";
import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArgumentPlan } from "../src/discord/slash-command-input.mjs";
import { currentRuntimeIdentityForCommit, productBuildIdentityMatchesRuntime,
  runtimeIdentityFromEnvironment, runtimeIdentityMatches } from "../src/job-service/runtime-identity.mjs";

export const P7P4_SOLUTIONS_PROBE_SCHEMA = "clearra.p7p4-solutions-http-probe.v1";
export const P7P4_PROBE_TIMEOUT_MS = 840_000;
const MAX_OUTPUT_BYTES = 4 * 1024 ** 2;
const MAX_ARTIFACT_BYTES = 24 * 1024 ** 2;
const SHA = /^[0-9a-f]{40}$/u;
const HASH = /^[0-9a-f]{64}$/u;
const STAGES = ["queue", "prepare", "execute", "artifact", "projection", "render", "delivery"];
const FAILURES = ["engine-oom", "engine-signal", "engine-exit", "output-limit", "deadline",
  "transport-timeout", "cancelled", "result-validation"];

// Use the same public option decoder as slash and text ingress. This is one
// automatic user request, retaining all eleven draws for both target heights.
export function publicP7p4SolutionsPlan() {
  const plan = buildSlashCommandArgumentPlan(findSlashCommand("pc").subcommands.solutions, [
    { name: "field", value: "grid:__________" }, { name: "next", value: "P7P4" },
    { name: "hold", value: "empty" }, { name: "kicktable", value: "srs-plus" },
  ]);
  if (plan.automaticPcTargets !== true || plan.argumentSets.length !== 2 ||
      plan.argumentSets.some((argv, index) => argv[argv.indexOf("--lines") + 1] !== String([2, 4][index]))) {
    throw new Error("P7P4 public target plan differs");
  }
  return plan;
}

function probeAuthority(options, environment) {
  const sourceCommit = options?.sourceCommit;
  let origin;
  try { origin = new URL(options?.serviceUrl); } catch { /* Rejected below. */ }
  if (!SHA.test(sourceCommit ?? "") || !HASH.test(options?.cliSha256 ?? "") ||
      options.revision !== `clearra-current-job-v080-${sourceCommit.slice(0, 7)}` ||
      !new RegExp(`^clearra-parity-${sourceCommit.slice(0, 7)}-[1-9][0-9]{0,19}$`, "u").test(options.jobName ?? "") ||
      !origin || origin.protocol !== "https:" || origin.username || origin.password ||
      origin.search || origin.hash || origin.pathname !== "/" || origin.origin !== options.serviceUrl ||
      !origin.hostname.endsWith(".run.app") ||
      typeof options.authorizationToken !== "string" || options.authorizationToken.length === 0) {
    throw new Error("P7P4 probe authority is invalid");
  }
  const expected = currentRuntimeIdentityForCommit(sourceCommit);
  if (!runtimeIdentityMatches(runtimeIdentityFromEnvironment(environment, { required: true }), expected) ||
      environment.CLOUD_RUN_JOB !== options.jobName || environment.CLOUD_RUN_TASK_INDEX !== "0" ||
      environment.CLOUD_RUN_TASK_ATTEMPT !== "0" || environment.CLOUD_RUN_TASK_COUNT !== "1" ||
      !/^[0-9a-f]{16}$/u.test(environment.CLEARRA_P7P4_OWNER_NONCE ?? "") ||
      typeof environment.CLOUD_RUN_EXECUTION !== "string" ||
      !environment.CLOUD_RUN_EXECUTION.startsWith(`${options.jobName}-`) ||
      !/^[a-z0-9-]{1,127}$/u.test(environment.CLOUD_RUN_EXECUTION)) {
    throw new Error("P7P4 probe runtime ownership differs");
  }
  return expected;
}

async function hashPackagedExecutable() {
  const handle = await open("/usr/local/bin/clearra", constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await handle.stat();
    if (!before.isFile() || before.size < 1 || before.size > 256 * 1024 ** 2) {
      throw new Error("P7P4 packaged executable is invalid");
    }
    const hash = createHash("sha256");
    let size = 0;
    for await (const bytes of handle.createReadStream({ autoClose: false })) {
      size += bytes.length;
      if (size > before.size) throw new Error("P7P4 packaged executable changed");
      hash.update(bytes);
    }
    const after = await handle.stat();
    if (size !== before.size || before.size !== after.size || before.mtimeMs !== after.mtimeMs ||
        before.ctimeMs !== after.ctimeMs) throw new Error("P7P4 packaged executable changed");
    return hash.digest("hex");
  } finally { await handle.close(); }
}

function safeDiagnostics(value, fallback = "result-validation", fallbackStage = "projection") {
  return {
    failureCode: FAILURES.includes(value?.failureCode) ? value.failureCode : fallback,
    failureStage: STAGES.includes(value?.failureStage) ? value.failureStage : fallbackStage,
    stageDurationsMs: Object.fromEntries(STAGES.flatMap((stage) => {
      const duration = value?.stageDurationsMs?.[stage];
      return Number.isSafeInteger(duration) && duration >= 0 && duration <= 900_000 ? [[stage, duration]] : [];
    })),
  };
}

function completeTarget(result, expectedRuntime, lines) {
  if (result?.exitCode !== 0 || result.signal !== null || result.failureCode ||
      typeof result.stdout !== "string" || typeof result.stderr !== "string" ||
      Buffer.byteLength(result.stdout) + Buffer.byteLength(result.stderr) > MAX_OUTPUT_BYTES) {
    throw Object.assign(new Error("P7P4 execution is incomplete"), safeDiagnostics(result, "engine-exit"));
  }
  const payload = JSON.parse(result.stdout);
  const summary = payload?.summary;
  if (!validDiscordPcSolutionsResult(payload) ||
      !productBuildIdentityMatchesRuntime(payload.runtime_identity, expectedRuntime) ||
      summary.count_complete !== true || summary.solution_count_calculated !== true ||
      summary.solution_set_materialized !== true || summary.solution_keys_complete !== true ||
      summary.rule_profile !== "srs-plus" || summary.queue_knowledge !== "oracle" ||
      // Generic Unique marginalizes the unused suffix exactly: five locks
      // plus empty-Hold lookahead need 7P6 prefixes; ten locks need all P7P4.
      String(summary.materialized_pattern_count) !== String(lines === 2 ? 5_040 : 4_233_600) ||
      !Number.isSafeInteger(summary.normalized_unique_solution_count) || summary.normalized_unique_solution_count < 0 ||
      (lines === 4 && summary.normalized_unique_solution_count === 0) ||
      summary.solution_found !== (summary.normalized_unique_solution_count > 0) ||
      !/^cts1:[0-9a-f]{16}$/u.test(summary.normalized_solution_set_hash ?? "") ||
      summary.workers_requested !== "auto" || !Number.isSafeInteger(summary.workers_used) ||
      summary.workers_used < (summary.normalized_unique_solution_count === 0 ? 0 : 1) ||
      summary.workers_used > 8 || (lines === 4 && summary.workers_used !== 8)) {
    throw new Error("P7P4 result identity or completeness differs");
  }
  const count = summary.normalized_unique_solution_count;
  const artifact = count > 0 ? validateSolutionTransportArtifact(result.artifact, MAX_ARTIFACT_BYTES) : null;
  if ((count === 0 && result.artifact != null) || (artifact && artifact.solutionCount !== count)) {
    throw new Error("P7P4 native artifact count differs");
  }
  return { kind: payload.kind, materializedPatternCount: lines === 2 ? 5_040 : 4_233_600,
    solutionCount: count, solutionSetHash: summary.normalized_solution_set_hash,
    workersUsed: summary.workers_used, complete: true,
    stdoutBytes: Buffer.byteLength(result.stdout), stderrBytes: Buffer.byteLength(result.stderr),
    resultSha256: createHash("sha256").update(result.stdout).digest("hex"),
    artifact: artifact ? { byteLength: artifact.byteLength, sha256: artifact.sha256,
      solutionCount: artifact.solutionCount, complete: true } : null };
}

export async function runP7p4SolutionsProbe(options, dependencies = {}) {
  const runtime = dependencies.runtime ?? { platform: process.platform, nodeVersion: process.versions.node };
  if (runtime.platform !== "linux" || runtime.nodeVersion !== "22.23.2") {
    throw new Error("P7P4 probe requires pinned Linux Node");
  }
  const environment = dependencies.environment ?? process.env;
  const expectedRuntime = probeAuthority(options, environment);
  const now = dependencies.now ?? Date.now;
  const startedAt = now();
  if (!Number.isSafeInteger(startedAt) || startedAt < 0) throw new Error("P7P4 clock is invalid");
  if (options.ownerDeadlineUnixMs !== undefined &&
      (!Number.isSafeInteger(options.ownerDeadlineUnixMs) || options.ownerDeadlineUnixMs < 1)) {
    throw new Error("P7P4 owner deadline is invalid");
  }
  const deadlineUnixMs = Math.min(startedAt + P7P4_PROBE_TIMEOUT_MS,
    options.ownerDeadlineUnixMs ?? Number.MAX_SAFE_INTEGER);
  const digest = await (dependencies.hashExecutable ?? hashPackagedExecutable)();
  if (digest !== options.cliSha256) throw new Error("P7P4 packaged CLI hash differs");
  const plan = publicP7p4SolutionsPlan();
  const Executor = dependencies.Executor ?? ClearraJobExecutor;
  const executor = new Executor({ endpoint: new URL("/jobs", options.serviceUrl),
    authorizationToken: options.authorizationToken, expectedRuntimeIdentity: expectedRuntime,
    searchTimeoutMs: P7P4_PROBE_TIMEOUT_MS, pcSearchTimeoutMs: P7P4_PROBE_TIMEOUT_MS,
    reverseSearchTimeoutMs: P7P4_PROBE_TIMEOUT_MS, forwardSearchTimeoutMs: P7P4_PROBE_TIMEOUT_MS,
    maxOutputBytes: MAX_OUTPUT_BYTES, maxArtifactBytes: MAX_ARTIFACT_BYTES,
    pollIntervalMs: 250, cancelTimeoutMs: 2_000, now });
  const report = { schema: P7P4_SOLUTIONS_PROBE_SCHEMA, diagnosticOnly: true, status: "passed",
    sourceCommit: options.sourceCommit, cliSha256: digest, revision: options.revision,
    jobName: options.jobName, executionName: environment.CLOUD_RUN_EXECUTION,
    ownerNonce: environment.CLEARRA_P7P4_OWNER_NONCE,
    capabilityId: "pc.solutions", countPolicy: "CountUnique", rule: "srs-plus", hold: "empty",
    queueKnowledge: "oracle", sourcePattern: "P7P4", originalPatternCount: 4_233_600,
    originalSequenceLength: 11, automaticPcTargets: true, expectedVcpus: 8,
    sharedDeadlineMs: P7P4_PROBE_TIMEOUT_MS, deadlineUnixMs,
    startedAt: new Date(startedAt).toISOString(), targets: [] };
  for (const [index, arguments_] of plan.argumentSets.entries()) {
    const lines = [2, 4][index];
    const targetStartedAt = now();
    const jobId = `${options.jobName}-${lines}l`;
    const target = { lines, jobId, status: "failed", arguments: [...arguments_] };
    let currentStage = "execute";
    try {
      if (now() >= deadlineUnixMs) throw { failureCode: "deadline", failureStage: "execute" };
      const result = await executor.execute(arguments_, { jobId, deadlineUnixMs, timeoutClass: "pc_reverse" });
      currentStage = "projection";
      if (now() >= deadlineUnixMs) throw { failureCode: "deadline", failureStage: "projection",
        stageDurationsMs: result?.stageDurationsMs };
      const receipt = completeTarget(result, expectedRuntime, lines);
      if (now() >= deadlineUnixMs) throw { failureCode: "deadline", failureStage: "projection",
        stageDurationsMs: result?.stageDurationsMs };
      Object.assign(target, receipt, {
        status: "passed", stageDurationsMs: safeDiagnostics(result).stageDurationsMs });
    } catch (error) {
      Object.assign(target, safeDiagnostics(error, "result-validation", currentStage));
      report.status = "failed";
    }
    target.durationMs = Math.max(0, now() - targetStartedAt);
    report.targets.push(target);
    if (report.status !== "passed") break;
  }
  report.durationMs = Math.max(0, now() - startedAt);
  if (Buffer.byteLength(JSON.stringify(report)) > 32 * 1024) throw new Error("P7P4 report exceeds its bound");
  return report;
}

async function main() {
  const { values } = parseArgs({ strict: true, options: Object.fromEntries(
    ["source-commit", "cli-sha256", "revision", "job-name", "service-url"].map((key) => [key, { type: "string" }])) });
  const report = await runP7p4SolutionsProbe({ sourceCommit: values["source-commit"],
    cliSha256: values["cli-sha256"], revision: values.revision, jobName: values["job-name"],
    serviceUrl: values["service-url"], authorizationToken: process.env.CLEARRA_CANDIDATE_JOB_TOKEN,
    ownerDeadlineUnixMs: Number(process.env.CLEARRA_P7P4_OWNER_DEADLINE_UNIX_MS) });
  process.stdout.write(`${JSON.stringify(report)}\n`);
  process.exitCode = report.status === "passed" ? 0 : 2;
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  main().catch(() => { process.stderr.write("p7p4_solutions_probe=failed-before-report\n"); process.exitCode = 2; });
}

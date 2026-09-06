#!/usr/bin/env node
/**
 * Diagnostic only: same binary/argv/CPU authority, direct CLI versus the real
 * loopback Job Service. No Discord ingress, remote request, deployment, solver
 * hints, or release authority. The CLI's raw canonical set is compared before
 * the existing Discord projection; no input/output/environment is reported.
 */
import { spawn as nodeSpawn, execFile as nodeExecFile } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, realpath } from "node:fs/promises";
import { availableParallelism } from "node:os";
import { isAbsolute, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  assertDiscordCanonicalOnlyResult,
  canonicalClearraOperationalCommand,
  ClearraJobExecutor,
  prepareClearraArguments,
} from "../src/clearra/command.mjs";
import { loadClearraJobServiceConfig } from "../src/job-service/config.mjs";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";
import { ClearraJobService } from "../src/job-service/server.mjs";
import {
  currentRuntimeIdentityForCommit,
  productBuildIdentityMatchesRuntime,
} from "../src/job-service/runtime-identity.mjs";

export const PARITY_SCHEMA = "clearra.diagnostic.cloud-cli-parity.v1";
const FIXTURE_TIMEOUT_MS = 120_000;
const TOTAL_TIMEOUT_MS = 15 * 60_000;
const OUTPUT_LIMIT = 4 * 1024 * 1024;
const WARM_PAIRS = 1;
const MEASURED_PAIRS = 3;
const HEX_256 = /^[0-9a-f]{64}$/u;
const HASH = /^cts1:[0-9a-f]{16}$/u;

// ctk3_w0kCQBjwwAMPPAD37g is the LEFT 16-cell field, not its mirror.
// Six placements fill its 24 holes; P7 includes terminal hold lookahead.
// Build's target is ONLY the complement, not the final full board.
const PC_INPUT = Object.freeze([
  "--lines", "4", "--board-mask", "0x3c0f03c0f", "--height", "4",
  "--pieces", "6", "--patterns", "P7", "--hold", "empty",
  "--rule", "jstris-180", "--backend", "cpu",
]);
export const PARITY_FIXTURES = Object.freeze([
  Object.freeze({
    id: "ctk3-left-4l-p7-jstris180-pc-all",
    kind: "all-pc",
    arguments: Object.freeze(["pc", ...PC_INPUT, "--count", "unique"]),
  }),
  Object.freeze({
    id: "ctk3-left-4l-p7-jstris180-pc-minimals",
    kind: "minimum-pc",
    arguments: Object.freeze(["pc", "minimals", ...PC_INPUT]),
  }),
  Object.freeze({
    id: "ctk3-left-4l-p7-jstris180-build-all",
    kind: "all-build",
    arguments: Object.freeze([
      "build-probability", "--base-mask", "0x3c0f03c0f",
      "--target-mask", "0xfc3f0fc3f0", "--height", "4", "--patterns", "P7",
      "--hold", "empty", "--rule", "jstris-180", "--backend", "cpu",
      "--no-mirror", "--aggregate", "buildability", "--result-mode", "all-solutions",
    ]),
  }),
]);

class DiagnosticFailure extends Error {
  constructor(code) {
    super(code);
    this.code = code;
  }
}
function requireDiagnostic(condition, code) {
  if (!condition) throw new DiagnosticFailure(code);
}
function digest(value) {
  return createHash("sha256").update(value).digest("hex");
}
function jsonDigest(value) {
  return digest(JSON.stringify(value));
}
function decimal(value) {
  const text = String(value);
  requireDiagnostic(/^(?:0|[1-9][0-9]*)$/u.test(text), "invalid_result_count");
  return text;
}
function sortedKeys(keys) {
  requireDiagnostic(Array.isArray(keys) && keys.length > 0 && keys.every((key) =>
    typeof key === "string" && key.length > 0 && key.length <= 65_536), "missing_complete_solution_keys");
  const sorted = [...keys].sort();
  requireDiagnostic(sorted.every((key, i) => i === 0 || key !== sorted[i - 1]), "duplicate_solution_key");
  return sorted;
}

/** Validate the CLI payload, never the narrower Discord display projection. */
export function validateParityResult(result, fixture, runtime) {
  requireDiagnostic(result?.exitCode === 0 && result?.signal === null, "cli_not_successful");
  let payload;
  try { payload = JSON.parse(result.stdout); } catch { throw new DiagnosticFailure("invalid_cli_json"); }
  requireDiagnostic(payload?.schema_version === 2 &&
    productBuildIdentityMatchesRuntime(payload.runtime_identity, runtime), "runtime_identity_mismatch");
  const summary = payload.summary;
  requireDiagnostic(summary && typeof summary === "object", "missing_result_summary");
  const resource = payload.resource_report;
  requireDiagnostic(!resource || (resource.truncated !== true &&
    !["partial", "cancelled", "failed"].includes(resource.result_completeness)), "incomplete_resource_report");

  if (fixture.kind === "minimum-pc") {
    requireDiagnostic(payload.kind === "pc-minimum-cover.v2" &&
      payload.contract?.command?.kind === payload.kind &&
      summary.capability_id === "pc.minimals" && summary.result_contract === payload.kind &&
      summary.payload_kind === "coverage-portfolio" &&
      summary.set_contract === "portfolio-alternative-set.v1" &&
      summary.page_contract === "portfolio-alternative-page.v1" &&
      summary.member_page_contract === "portfolio-member-page.v1" &&
      summary.alternative_index === "1" && summary.member_page_number === "1" &&
      summary.total_member_pages === "1" && HEX_256.test(summary.set_identity_sha256) &&
      HEX_256.test(summary.candidate_map_sha256) &&
      BigInt(decimal(summary.known_alternative_count)) >= 1n &&
      Array.isArray(summary.members), "incomplete_canonical_minimum");
    // Total tie enumeration is deliberately lazy; it is NOT a completeness
    // requirement for the exact first canonical set. 25 is post-validation
    // only: it never appears in argv, solver bounds, hints, or a stop policy.
    requireDiagnostic(decimal(summary.optimal_cardinality) === "25" && summary.members.length === 25,
      "unexpected_minimum_cardinality");
    let previous = 0n;
    for (const member of summary.members) {
      const id = BigInt(decimal(member.candidate_id));
      requireDiagnostic(id > previous, "noncanonical_minimum_order");
      previous = id;
    }
    requireDiagnostic(summary.canonical_selection === "smallest-canonical-candidate-id" &&
      JSON.stringify(summary.canonical_witness) === JSON.stringify(summary.members[0]), "canonical_witness_mismatch");
    const keys = sortedKeys(summary.members.map((member) => member.normalized_solution_key));
    return {
      payload,
      evidence: {
        result_kind: payload.kind,
        solution_count: "25",
        normalized_solution_set_hash: null, // This typed contract exports SHA identities, not cts1.
        normalized_keys_sha256: jsonDigest(keys),
        canonical_members_sha256: jsonDigest(summary.members),
        set_identity_sha256: summary.set_identity_sha256,
        candidate_map_sha256: summary.candidate_map_sha256,
      },
    };
  }
  const expectedKinds = fixture.kind === "all-pc" ? ["pc", "pc-scenario"] : ["build-probability"];
  requireDiagnostic(expectedKinds.includes(payload.kind) &&
    summary.count_complete === true && summary.probability_complete === true &&
    // Generic CLI summary keeps the exact "none" sentinel; resource_report
    // separately normalizes that same absence of truncation to JSON null.
    (summary.count_truncated_reason === "none" || summary.count_truncated_reason === null ||
      summary.count_truncated_reason === undefined) &&
    payload.contract?.solution_data?.status === "complete" &&
    payload.contract.solution_data.requested === true &&
    payload.contract?.artifacts?.schema_version === "clearra.solution-data.v1", "incomplete_all_solutions");
  const keys = sortedKeys(payload.contract.artifacts.solution_keys);
  const count = decimal(summary.normalized_unique_solution_count ?? summary.unique_solution_count);
  requireDiagnostic(BigInt(count) === BigInt(keys.length) && HASH.test(summary.normalized_solution_set_hash),
    "solution_count_or_hash_mismatch");
  const patternCount = decimal(summary.materialized_pattern_count);
  requireDiagnostic(patternCount === "5040", "fixture_pattern_universe_mismatch");
  return {
    payload,
    evidence: {
      result_kind: payload.kind,
      solution_count: count,
      pattern_count: patternCount,
      normalized_solution_set_hash: summary.normalized_solution_set_hash,
      normalized_keys_sha256: jsonDigest(keys),
    },
  };
}

async function hashExecutable(executable) {
  requireDiagnostic(isAbsolute(executable), "executable_must_be_absolute");
  const info = await lstat(executable);
  requireDiagnostic(info.isFile() && !info.isSymbolicLink(), "executable_must_be_regular_file");
  requireDiagnostic(resolve(await realpath(executable)) === resolve(executable), "executable_path_not_canonical");
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(executable)) hash.update(chunk);
  return hash.digest("hex");
}

// The product runner intentionally has a private env helper. Mirror its exact
// process.env + expected-vCPU override here, and audit each actual spawn below.
function directEnvironment(cpus) {
  return { ...process.env, CLEARRA_EXPECTED_VCPUS: String(cpus) };
}
function monotonicDuration(start, now) {
  const duration = now() - start;
  requireDiagnostic(Number.isFinite(duration) && duration >= 0, "invalid_monotonic_clock");
  return duration;
}
function median(values) {
  requireDiagnostic(values.length > 0 && values.every(Number.isFinite), "missing_timing_samples");
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

/** Real loopback service, injected child processes only in Node mock tests. */
export async function benchmarkCloudCliParity(options, dependencies = {}) {
  requireDiagnostic(process.platform !== "win32" ||
    (typeof dependencies.spawn === "function" && typeof dependencies.execFile === "function" &&
      typeof dependencies.hashExecutable === "function"), "native_windows_execution_forbidden");
  const runtime = currentRuntimeIdentityForCommit(options?.sourceCommit);
  const cpus = Number(options?.cpus);
  const workers = Number(options?.workers ?? cpus);
  const visibleCpus = Number((dependencies.availableParallelism ?? availableParallelism)());
  requireDiagnostic(Number.isSafeInteger(cpus) && cpus >= 1 && cpus <= 64 &&
    Number.isSafeInteger(workers) && workers >= 1 && workers <= cpus && visibleCpus >= cpus,
  "invalid_cpu_authority");
  const executable = options?.executable;
  requireDiagnostic(typeof executable === "string" && executable.length > 0, "missing_executable");
  const cliHash = await (dependencies.hashExecutable ?? hashExecutable)(executable);
  requireDiagnostic(HEX_256.test(cliHash), "invalid_executable_hash");
  const clock = dependencies.clock ?? (() => performance.now());
  const now = dependencies.now ?? Date.now;
  const spawn = dependencies.spawn ?? nodeSpawn;
  const overall = new AbortController();
  const totalTimer = setTimeout(() => overall.abort(), TOTAL_TIMEOUT_MS);
  const token = randomBytes(32).toString("hex");
  // Diagnostic-local config only. No product env defaults or process.env writes.
  const config = loadClearraJobServiceConfig({
    NODE_ENV: "production", CLEARRA_EXECUTABLE: executable,
    CLEARRA_SOURCE_COMMIT: runtime.sourceCommit, CLEARRA_ENGINE_BUILD_ID: runtime.engineBuildId,
    CLEARRA_SEARCH_CONTRACT_REVISION: runtime.contractSchemaVersion,
    CLEARRA_SUPPLY_SEMANTICS_ID: runtime.supplySemanticsId,
    CLEARRA_ARTIFACT_SCHEMA_VERSION: runtime.artifactSchemaVersion,
    CLEARRA_EXPECTED_VCPUS: String(cpus), CLEARRA_SEARCH_WORKERS_PER_SESSION: String(workers),
    CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1", CLEARRA_MAX_CONCURRENT_JOBS: "1",
    CLEARRA_LISTEN_HOST: "127.0.0.1", CLEARRA_JOB_TOKEN: token,
    CLEARRA_SEARCH_TIMEOUT_MS: String(FIXTURE_TIMEOUT_MS),
    CLEARRA_REVERSE_SEARCH_TIMEOUT_MS: String(FIXTURE_TIMEOUT_MS),
    CLEARRA_FORWARD_SEARCH_TIMEOUT_MS: String(FIXTURE_TIMEOUT_MS),
    CLEARRA_MAX_OUTPUT_BYTES: String(OUTPUT_LIMIT), CLEARRA_JOB_TERMINATION_GRACE_MS: "1000",
    CLEARRA_JOB_MAX_RETAINED_RESULTS: "1",
  }, { availableParallelism: () => visibleCpus });
  config.port = 0; // Ephemeral loopback port; no product config parser change.
  let active = null;
  let expectedArguments = null;
  const operations = [];
  function captureSpawn(file, args, settings) {
    requireDiagnostic(active === null && file === executable &&
      JSON.stringify(args) === JSON.stringify(expectedArguments) && settings.shell === false &&
      settings.env?.CLEARRA_EXPECTED_VCPUS === String(cpus), "actual_spawn_policy_mismatch");
    const started = clock();
    const child = spawn(file, args, settings);
    const record = { child, overflow: false, deadlineExceeded: false, result: null, processMs: null, promise: null };
    active = record;
    const out = [];
    const err = [];
    let bytes = 0;
    let forced = null;
    const terminate = () => {
      child.kill("SIGTERM");
      forced ??= setTimeout(() => child.kill("SIGKILL"), config.terminationGraceMs);
    };
    const collect = (target, chunk) => {
      bytes += Buffer.byteLength(chunk);
      if (bytes > OUTPUT_LIMIT) { record.overflow = true; terminate(); return; }
      target.push(Buffer.from(chunk));
    };
    const deadline = setTimeout(() => { record.deadlineExceeded = true; terminate(); }, FIXTURE_TIMEOUT_MS);
    overall.signal.addEventListener("abort", terminate, { once: true });
    record.promise = new Promise((resolveRecord) => {
      child.stdout.on("data", (chunk) => collect(out, chunk));
      child.stderr.on("data", (chunk) => collect(err, chunk));
      child.once("error", () => { record.spawnError = true; });
      child.once("close", (code, signal) => {
        clearTimeout(deadline);
        if (forced) clearTimeout(forced);
        overall.signal.removeEventListener("abort", terminate);
        record.processMs = monotonicDuration(started, clock);
        record.result = {
          exitCode: code ?? -1, signal: signal ?? null,
          stdout: Buffer.concat(out).toString("utf8").trim(),
          stderr: Buffer.concat(err).toString("utf8").trim(),
        };
        resolveRecord(record);
      });
    });
    if (overall.signal.aborted) terminate();
    return child;
  }
  const logger = {
    info: (line) => captureOperation(line),
    error: (line) => captureOperation(line),
  };
  function captureOperation(line) {
    const record = JSON.parse(line);
    requireDiagnostic(record.event === "clearra.operation" && record.scope === "job" &&
      record.kind === "search", "unexpected_operational_record");
    operations.push(record); // Allow-listed metadata only; no arbitrary logger output.
  }
  const runner = new ClearraCommandRunner(config, {
    spawn: captureSpawn, execFile: dependencies.execFile ?? nodeExecFile, now,
  });
  const service = new ClearraJobService(config, runner, { logger, operationalScope: "job", now });
  let listening = false;
  try {
    await runner.verifyCapabilities(); // Cold/capability work outside every sample.
    requireDiagnostic(!overall.signal.aborted, "diagnostic_deadline");
    const address = await service.listen();
    listening = true;
    const executor = new ClearraJobExecutor({
      endpoint: `http://127.0.0.1:${address.port}/jobs`, authorizationToken: token,
      expectedRuntimeIdentity: runtime, timeoutMs: FIXTURE_TIMEOUT_MS,
      maxOutputBytes: OUTPUT_LIMIT, maxArtifactBytes: OUTPUT_LIMIT, now,
    });
    const fixtures = [];
    for (const fixture of PARITY_FIXTURES) {
      expectedArguments = prepareClearraArguments([...fixture.arguments], {
        workers, useAllLogicalProcessors: true, logicalProcessors: visibleCpus,
        outputFormat: "json", includeSolutionData: true,
      });
      const timings = [];
      let reference = null;
      for (let pair = 0; pair < WARM_PAIRS + MEASURED_PAIRS; pair += 1) {
        const captures = {};
        // Alternate AB/BA order without overlapping jobs or prefetching work.
        const order = pair % 2 === 0 ? ["direct", "service"] : ["service", "direct"];
        for (const route of order) {
          requireDiagnostic(!overall.signal.aborted, "diagnostic_deadline");
          active = null;
          if (route === "direct") {
            captureSpawn(executable, [...expectedArguments], {
              shell: false, windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
              env: directEnvironment(cpus),
            });
            const record = await active.promise;
            captures.direct = { record };
          } else {
            operations.length = 0;
            const httpStart = clock();
            const returned = await executor.execute([...fixture.arguments], {
              jobId: randomUUID(), deadlineUnixMs: now() + FIXTURE_TIMEOUT_MS, signal: overall.signal,
            });
            const httpMs = monotonicDuration(httpStart, clock);
            requireDiagnostic(active !== null, "service_did_not_execute_cli");
            const record = await active.promise;
            requireDiagnostic(operations.length === 1 && operations[0].status === "succeeded" &&
              operations[0].command === canonicalClearraOperationalCommand(fixture.arguments) &&
              Number.isFinite(operations[0].durationMs) && operations[0].durationMs >= 0,
            "missing_successful_job_duration");
            captures.service = { record, returned, httpMs, jobMs: operations[0].durationMs };
          }
          requireDiagnostic(!active.overflow && !active.deadlineExceeded && !active.spawnError && !overall.signal.aborted,
            "failed_or_cancelled_sample");
          active = null;
        }
        const direct = validateParityResult(captures.direct.record.result, fixture, runtime);
        const served = validateParityResult(captures.service.record.result, fixture, runtime);
        requireDiagnostic(JSON.stringify(direct.evidence) === JSON.stringify(served.evidence), "route_result_mismatch");
        const projection = assertDiscordCanonicalOnlyResult(captures.service.record.result);
        requireDiagnostic(captures.service.returned.exitCode === 0 && captures.service.returned.signal === null &&
          jsonDigest(JSON.parse(projection.stdout)) === jsonDigest(JSON.parse(captures.service.returned.stdout)),
        "service_projection_mismatch");
        const projectedDirect = assertDiscordCanonicalOnlyResult(captures.direct.record.result);
        if (fixture.kind === "minimum-pc") {
          requireDiagnostic(jsonDigest(JSON.parse(projectedDirect.stdout)) === jsonDigest(JSON.parse(projection.stdout)),
            "canonical_display_mismatch");
        }
        if (reference !== null) requireDiagnostic(JSON.stringify(reference) === JSON.stringify(direct.evidence),
          "repeat_result_mismatch");
        reference = direct.evidence;
        if (pair >= WARM_PAIRS) timings.push({
          direct_process_ms: captures.direct.record.processMs,
          service_process_ms: captures.service.record.processMs,
          service_job_ms: captures.service.jobMs,
          service_http_wall_ms: captures.service.httpMs,
        });
      }
      fixtures.push({
        fixture_id: fixture.id, argv_sha256: jsonDigest(expectedArguments),
        ...reference, warm_pairs: WARM_PAIRS, measured_pairs: timings.length,
        timings, medians_ms: Object.fromEntries(Object.keys(timings[0]).map((key) =>
          [key, median(timings.map((sample) => sample[key]))])),
      });
    }
    requireDiagnostic(await (dependencies.hashExecutable ?? hashExecutable)(executable) === cliHash,
      "executable_changed_during_diagnostic");
    return {
      schema_id: PARITY_SCHEMA, status: "passed", release_authority: false,
      runtime_identity: runtime, cli_binary_sha256: cliHash,
      cpus, workers, process_visible_cpus: visibleCpus,
      fixture_timeout_ms: FIXTURE_TIMEOUT_MS, total_timeout_ms: TOTAL_TIMEOUT_MS,
      timing_scope: "process-spawn-to-close; job-accept-to-result; loopback-http-wall",
      cold_start_and_capability_excluded: true, pure_solver_timing: false,
      performance_threshold_applied: false, fixtures,
    };
  } finally {
    overall.abort();
    clearTimeout(totalTimer);
    if (active) await active.promise;
    if (listening) await service.close();
  }
}

async function main() {
  try {
    const { values } = parseArgs({ options: {
      executable: { type: "string" }, "source-commit": { type: "string" },
      cpus: { type: "string" }, workers: { type: "string" },
    }, strict: true });
    const report = await benchmarkCloudCliParity({
      executable: values.executable, sourceCommit: values["source-commit"],
      cpus: values.cpus, workers: values.workers,
    });
    process.stdout.write(`${JSON.stringify(report)}\n`);
  } catch (error) {
    // Never print arbitrary errors (which could contain argv, output or URLs).
    process.stderr.write(`${JSON.stringify({ schema_id: PARITY_SCHEMA, status: "failed",
      release_authority: false, code: error instanceof DiagnosticFailure ? error.code : "diagnostic_failed" })}\n`);
    process.exitCode = 2;
  }
}
if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();

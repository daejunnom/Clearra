// A local-only exact mapping qualification, not ABBA timing or release authority.
// Each capture and comparison acquires a native benchmark-search lease. The
// JS orchestrator owns no solver directly and never nests a Linux supervisor.
import { spawn, spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
  createReadStream, existsSync, lstatSync, mkdirSync, readFileSync,
  realpathSync, statfsSync, unlinkSync, writeFileSync,
} from "node:fs";
import { availableParallelism, cpus, hostname, totalmem, version } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";

const SHA = /^[a-f0-9]{64}$/u;
const COMMIT = /^[a-f0-9]{40}$/u;
const RUN_ID = /^\d{16,21}-[1-9]\d*$/u;
const MAX_JSON_BYTES = 4 * 1024 * 1024;
const P7P4_FREE_RESERVE = 33n * 1024n * 1024n * 1024n;
const AXIS = "movement_clear_certificates_mode";
const ARTIFACT = "solution-coverage.clrcov";
const COVERAGE_ROOT = ["_local", "artifacts", "benchmarks", "placement-graph-coverage"];
const MODES = Object.freeze({
  CLEARRA_PLACEMENT_DEPENDENCY: "1",
  CLEARRA_STRUCTURAL_ORDER: "1",
  CLEARRA_STATIC_INFLUENCE: "0",
  CLEARRA_TRANSLATION_INTERVAL: "0",
  CLEARRA_INCREMENTAL_DOMAIN: "0",
  CLEARRA_MINIMUM_DOMAIN_CUTOFF: "0",
  CLEARRA_MINIMUM_DOMAIN_WORK: "0",
  CLEARRA_FIRST_KICK_INTERVAL: "0",
  CLEARRA_CONDITIONAL_ORDER: "0",
  CLEARRA_TERMINAL_IDENTITY_MOVE: "0",
  CLEARRA_ORDERED_KICK_INTERVAL: "0",
  CLEARRA_ORDERED_KICK_GUARD_REUSE: "0",
  CLEARRA_GEOMETRIC_EDGE_REUSE: "0",
  CLEARRA_ILC_LAZY_FRAME: "0",
  CLEARRA_NO_ENTRY_COMPONENTS: "0",
  CLEARRA_SAME_TILE_PARENT_REUSE: "0",
});
const CASES = Object.freeze({
  small: Object.freeze({
    name: "one-I-preflight", universe: "small-i", solutions: 1, queues: 1,
    board: "0x3f0", height: 1, placed: 1, supplied: 1, supply: "I",
    hold: "disabled", defaultTimeout: 120, maxTimeout: 300, comparisonTimeout: 60,
  }),
  p7p4: Object.freeze({
    name: "empty-4L-P7P4-jstris180", universe: "p7p4", solutions: 456_459, queues: 4_233_600,
    board: "0", height: 4, placed: 10, supplied: 11, supply: "P7P4",
    hold: "enabled-empty", defaultTimeout: 1_800, maxTimeout: 2_400, comparisonTimeout: 900,
  }),
});
const REFERENCE_HASH = "cts1:2fcbc950acdffa31";

function requireThat(value, message) {
  if (!value) throw new Error(message);
}

export function parseProofArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index], value = argv[index + 1];
    requireThat(["--case", "--id", "--source-sha", "--timeout-seconds", "--build-profile"].includes(key) &&
      typeof value === "string" && !Object.hasOwn(options, key), "Distinct known option/value pairs required");
    options[key] = value;
  }
  const key = options["--case"];
  const id = options["--id"];
  const sourceCommit = options["--source-sha"];
  requireThat(Object.hasOwn(CASES, key) &&
    /^[a-z0-9][a-z0-9-]{0,47}$/u.test(id ?? "") &&
    COMMIT.test(sourceCommit ?? ""),
  "Usage: run-placement-coverage-proof --case small|p7p4 --id UNIQUE-LEAF --source-sha FULL-HEAD [--timeout-seconds N]");
  const fixture = CASES[key];
  const buildProfile = options["--build-profile"] ?? "release";
  requireThat(buildProfile === "release" || (key === "small" && buildProfile === "debug"),
  "Debug binaries can be used only for the one-I preflight, not P7P4");
  const raw = options["--timeout-seconds"] ?? String(fixture.defaultTimeout);
  const timeout = Number(raw);
  requireThat(/^[1-9]\d*$/u.test(raw) && Number.isSafeInteger(timeout) && timeout <= fixture.maxTimeout,
    "Invalid finite timeout for the selected fixture");
  return { id, sourceCommit, key, fixture, timeout, buildProfile };
}

export function proofSchedule(options, binary, artifactRoot) {
  const arm = (name, value, suffix) => {
    const slot = options.id + suffix;
    return {
      name, slot, value, timeout: options.timeout,
      args: [...(options.key === "small" ? ["--small"] : []), "--coverage-slot", slot],
      artifact: join(artifactRoot, slot, ARTIFACT),
    };
  };
  const reference = arm("reference", "0", "-a");
  const candidate = arm("candidate", "1", "-b");
  return [
    reference, candidate, {
      name: "comparison", value: "0", timeout: options.fixture.comparisonTimeout,
      args: ["--compare-coverage", reference.artifact, candidate.artifact],
    },
  ].map((step) => ({ ...step, command: [binary, ...step.args] }));
}

// Require all receipts to bind to this sequence's concrete invocation. A
// successful process with a timeout/cancellation receipt is not complete.
export function classifyProofRuntime(receipt, execution, step) {
  const outcome = receipt?.outcome;
  requireThat(receipt?.schema_id === "clearra.runtime-receipt.v2" && RUN_ID.test(outcome?.run_id ?? "") &&
    outcome.profile === "benchmark-search" && outcome.producer === "benchmark" &&
    outcome.timeout_seconds === step.timeout && outcome.automatic_retry === false &&
    isDeepStrictEqual(outcome.command, step.command), step.name + ": receipt does not bind this invocation");
  const signal = outcome.child_termination?.kind === "unix-signal" ? outcome.child_termination.signal : null;
  if (["timeout", "memory-limit", "memory-pressure", "output-limit", "process-limit", "cancelled"].includes(outcome.reason) ||
      (outcome.reason === "signal" && [2, 9, 15].includes(signal))) return "censored";
  if (execution.overflow || execution.code !== 0 || execution.signal != null ||
      outcome.return_code !== 0 || outcome.reason !== "normal" || outcome.process_tree_stopped !== true ||
      outcome.error_code !== null || outcome.supervisor_termination_reason !== null ||
      outcome.child_termination?.kind !== "exit" || outcome.child_termination.code !== 0) return "failed";
  requireThat(typeof outcome.memory_pressure_observed === "boolean" &&
    Number.isSafeInteger(outcome.memory_pressure_events) && outcome.memory_pressure_events >= 0,
  step.name + ": missing supervisor pressure evidence");
  return outcome.memory_pressure_observed || outcome.memory_pressure_events > 0 ? "confounded" : "completed";
}

function summaryEquals(value, expected) {
  return value === expected || value === String(expected);
}

export function validatedProofArm(report, receipt, step, options, binarySha, outputMode = "solution-coverage") {
  const fixture = options.fixture;
  const neutralFollowOn = (name) => !Object.hasOwn(report, name) || report[name] === "0";
  requireThat(report?.schema === "clearra.placement-graph-benchmark.v1" &&
    report.case === fixture.name && report.valid === true &&
    Array.isArray(report.validation_errors) && report.validation_errors.length === 0 &&
    report.binary_sha256 === binarySha && report.supervisor_run_id === receipt.outcome.run_id &&
    report.output_mode === outputMode && report[AXIS] === step.value &&
    report.mode === "1" && report.structural_mode === "1" &&
    report.static_influence_mode === "0" && report.translation_interval_mode === "0" &&
    report.incremental_domain_mode === "0" && report.first_kick_interval_mode === "0" &&
    neutralFollowOn("minimum_domain_cutoff_mode") && neutralFollowOn("minimum_domain_work_mode") &&
    report.conditional_order_mode === "0" && report.terminal_identity_move_mode === "0" &&
    (report.ordered_kick_interval_mode ?? "0") === "0" &&
    neutralFollowOn("ordered_kick_guard_reuse_mode") &&
    neutralFollowOn("geometric_edge_reuse_mode") &&
    (report.ilc_lazy_frame_mode ?? "0") === "0" &&
    (report.no_entry_components_mode ?? "0") === "0" &&
    (report.same_tile_parent_reuse_mode ?? "0") === "0",
  step.name + ": unexpected source, mode, or coverage output");
  const input = report.input;
  requireThat(input?.board === fixture.board && input.height === fixture.height &&
    input.placed_pieces === fixture.placed && input.supplied_pieces === fixture.supplied &&
    input.supply === fixture.supply && input.original_supply_count === fixture.queues &&
    input.hold === fixture.hold && input.rule === "jstris-180" && input.count === "unique" &&
    input.queue_knowledge === "full-future-oracle", step.name + ": wrong exact fixture");
  const cpu = report.cpu;
  const summary = report.summary;
  requireThat(cpu?.worker_policy === "auto" && cpu.use_all_logical_processors === true &&
    cpu.expected_vcpus === null && Number.isSafeInteger(cpu.hard_limit) && cpu.hard_limit > 0 &&
    (options.key === "small" || (cpu.hard_limit > 1 && summaryEquals(summary?.workers_used, cpu.hard_limit) &&
      summaryEquals(summary?.parallel_active_workers, cpu.hard_limit) &&
      summaryEquals(summary?.cpu_parallel_execution, true))), step.name + ": automatic parallel placement contract changed");
  const ids = report.materialized_identities;
  const coverage = report.coverage;
  const mapping = report.solution_coverage;
  requireThat(typeof report.backend_contract === "string" && report.backend_contract.length > 0 &&
    report.other_process_configuration && typeof report.other_process_configuration === "object" &&
    !Array.isArray(report.other_process_configuration), step.name + ": missing benchmark runtime configuration");
  requireThat(summaryEquals(summary?.count_complete, true) && summaryEquals(summary?.probability_complete, true) &&
    summaryEquals(summary?.resource_truncated, false) &&
    (options.key === "small" || summary.normalized_solution_set_hash === REFERENCE_HASH) &&
    ids?.count === fixture.solutions && ids.reported_total_count === fixture.solutions &&
    ids.covers_reported_family === true && SHA.test(ids.sha256_ordered_ctk1_v1 ?? "") &&
    coverage?.set_bit_count === fixture.queues && SHA.test(coverage.sha256_le_v1 ?? "") &&
    (outputMode === "global-only" ? mapping == null : (
      outputMode === "solution-coverage" && mapping?.complete === true && SHA.test(mapping.sha256 ?? "") &&
      mapping.solution_count === fixture.solutions && mapping.total_weight === fixture.queues &&
      mapping.covered_weight === fixture.queues && Number.isSafeInteger(mapping.covered_pairs) &&
      mapping.covered_pairs >= Math.max(fixture.solutions, fixture.queues) &&
      mapping.covered_pairs <= fixture.solutions * fixture.queues
    )),
  step.name + ": incomplete or incorrect full per-solution output");
  return {
    input, cpu, ids: ids.sha256_ordered_ctk1_v1, globalCoverage: coverage.sha256_le_v1,
    process: report.other_process_configuration, backend: report.backend_contract,
  };
}

export function assertExactComparison(comparison, reference, candidate, fixture) {
  requireThat(comparison?.schema === "clearra.placement-graph-solution-coverage-comparison.v1" &&
    comparison.equivalent === true && comparison.complete === true && comparison.universe === fixture.universe &&
    comparison.solution_count === fixture.solutions && comparison.total_weight === fixture.queues &&
    comparison.covered_weight === fixture.queues &&
    comparison.covered_pairs === reference.solution_coverage.covered_pairs &&
    comparison.covered_pairs === candidate.solution_coverage.covered_pairs &&
    comparison.left_sha256 === reference.solution_coverage.sha256 &&
    comparison.right_sha256 === candidate.solution_coverage.sha256,
  "Per-solution comparator did not prove the exact complete mappings of both captured artifacts");
}

// Hook injection lets tests cover sequence and cutoff without invoking a bench
// or needing an actual managed worktree or large coverage files.
export async function collectPlacementCoverageProof(options, binarySha, steps, io) {
  const observations = [];
  const reports = {};
  const evidence = {
    schema: "clearra.placement-graph-coverage-proof.v1",
    release_authority: false,
    timing_adoption_authority: false,
    source_commit: options.sourceCommit,
    binary_sha256: binarySha,
    manager_sha256: io.custody?.managerSha,
    host_fingerprint: io.custody?.hostFingerprint,
    case: options.fixture.name,
    axis: AXIS,
    observations,
    mapping: null,
  };
  let referenceContract, active;
  try {
    for (const step of steps) {
      active = { name: step.name, state: "invalid", command: step.command, supervisor: null };
      observations.push(active);
      await io.checkInputs();
      const execution = await io.execute(step);
      const receipt = io.receipt(execution);
      active.state = classifyProofRuntime(receipt, execution, step);
      active.reason = receipt.outcome.reason;
      active.run_id = receipt.outcome.run_id;
      active.child_termination = receipt.outcome.child_termination;
      requireThat(!observations.slice(0, -1).some((earlier) => earlier.run_id === active.run_id),
        step.name + ": reused a prior supervisor run ID");
      active.supervisor = step.name + "-runtime.json";
      io.write(active.supervisor, receipt);
      const reportPath = step.name + ".json";
      if (active.state !== "completed") {
        // A semantic mismatch can print a useful negative report before exit.
        // Preserve it as diagnostic only, not as the verified comparison.
        if (step.name === "comparison") {
          const diagnostic = parseBoundBenchmarkOutput(receipt, execution);
          if (diagnostic?.equivalent === false) io.write("comparison-diagnostic.json", diagnostic);
        }
        throw new Error(step.name + ": " + active.state + " (" + active.reason + ")");
      }
      active.state = "invalid";
      const report = parseBoundBenchmarkOutput(receipt, execution);
      requireThat(report, step.name + ": missing or unbound JSON output");
      if (step.name === "comparison") {
        assertExactComparison(report, reports.reference, reports.candidate, options.fixture);
      } else {
        const contract = validatedProofArm(report, receipt, step, options, binarySha);
        referenceContract ??= contract;
        requireThat(isDeepStrictEqual(referenceContract, contract),
          "Reference and candidate must agree on exact fixture, auto workers, and aggregate contracts");
        io.assertArtifact(step.artifact);
      }
      io.write(reportPath, report);
      reports[step.name] = report;
      active.state = "completed";
    }
    await io.checkInputs();
    evidence.state = options.key === "p7p4" ? "completed-exact-per-solution" : "completed-small-preflight";
    evidence.source_binding = [reports.reference, reports.candidate].every(
      (report) => report.compiled_source_commit === options.sourceCommit)
      ? "compile-time-and-checkout" : "checkout-and-binary-custody-only";
    evidence.mapping = {
      comparison: "comparison.json",
      reference: { benchmark: "reference.json", supervisor: "reference-runtime.json" },
      candidate: { benchmark: "candidate.json", supervisor: "candidate-runtime.json" },
    };
  } catch (error) {
    evidence.state = active?.state === "censored" || active?.state === "confounded" ? active.state : "failed";
    evidence.error = error.message;
  }
  io.write("summary.json", evidence);
  return evidence;
}

export function parseBoundBenchmarkOutput(receipt, execution) {
  try {
    const reported = JSON.parse(receipt.stdout_tail);
    return isDeepStrictEqual(reported, JSON.parse(execution.stdout)) ? reported : null;
  } catch {
    return null;
  }
}

export function benchmarkEnvironment(environment, step) {
  const expected = { ...MODES, CLEARRA_MOVEMENT_CLEAR_CERTIFICATES: step.value };
  const allowed = Object.fromEntries(Object.entries(environment).filter(([key]) =>
    !Object.hasOwn(expected, key.toUpperCase())));
  return { ...allowed, ...expected };
}

export function composeRuntimeArgs(root, step) {
  return ["--root", root, "runtime", "run", "--producer", "benchmark", "--profile", "benchmark-search",
    "--timeout", String(step.timeout), "--", ...step.command];
}

export function captureChild(manager, args, cwd, env) {
  return new Promise((resolvePromise, reject) => {
    // POSIX terminal cancellation belongs to the JS launcher, not the
    // manager. The manager alone owns and enforces the active solver lease.
    const child = spawn(manager, args, { cwd, env, windowsHide: true,
      detached: process.platform !== "win32", stdio: ["ignore", "pipe", "pipe"] });
    const stdout = [], stderr = [];
    let size = 0, overflow = false;
    const collect = (parts) => (chunk) => {
      size += chunk.length;
      if (size <= MAX_JSON_BYTES) parts.push(chunk);
      else overflow = true;
    };
    child.stdout.on("data", collect(stdout));
    child.stderr.on("data", collect(stderr));
    child.once("error", reject);
    child.once("close", (code, signal) => resolvePromise({
      code, signal, overflow, stdout: Buffer.concat(stdout).toString("utf8"),
      stderr: Buffer.concat(stderr).toString("utf8"),
    }));
  });
}

export function stateReceiptDirectory(environment) {
  const root = process.platform === "win32"
    ? join(environment.LOCALAPPDATA ?? "", "Clearra", "state")
    : join(environment.XDG_STATE_HOME ?? join(environment.HOME ?? "", ".local", "state"), "Clearra");
  requireThat(isAbsolute(root), "The supervisor's native receipt directory must resolve to an absolute path");
  return join(root, "receipts", "rust-v2");
}

export function readSupervisorReceipt(execution, receiptDirectory) {
  const slots = [...execution.stderr.matchAll(/^clearra_runtime_receipt=([^\r\n]+)\r?$/gmu)];
  if (slots.length === 0 && execution.stderr.includes("E_CLEARRA_RUNTIME_SLOT_BUSY")) {
    throw new Error("benchmark-search ownership already held; did not retry");
  }
  requireThat(!execution.overflow && slots.length === 1, "Missing unique bounded supervisor receipt");
  const actual = slots[0][1];
  const match = /^(\d{16,21}-[1-9]\d*)-runtime\.json$/u.exec(basename(actual));
  const canonicalDirectory = realpathSync(receiptDirectory);
  requireThat(canonicalDirectory === resolve(receiptDirectory) && isAbsolute(actual) && match &&
    realpathSync(dirname(actual)) === canonicalDirectory,
    "Supervisor receipt must belong to the native Clearra state directory");
  const file = requireOwnedFile(join(canonicalDirectory, basename(actual)));
  requireThat(lstatSync(file).size <= MAX_JSON_BYTES, "Oversized supervisor receipt");
  const receipt = JSON.parse(readFileSync(file, "utf8"));
  requireThat(receipt.outcome?.run_id === match[1], "Receipt filename and run ID do not agree");
  return receipt;
}

export function requireOwnedFile(path, allowCargoHardlink = false) {
  const stat = lstatSync(path);
  requireThat(stat.isFile() && (allowCargoHardlink || stat.nlink === 1) && realpathSync(path) === path,
    "Expected an owned regular file with no link traversal: " + basename(path));
  return path;
}

export function checkedCommand(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, windowsHide: true, encoding: "utf8",
    timeout: 30_000, maxBuffer: MAX_JSON_BYTES });
  requireThat(result.status === 0 && !result.error, "Bounded preflight command failed: " + basename(command));
  return result.stdout;
}

export async function fileSha(path, allowCargoHardlink = true) {
  const sha = createHash("sha256");
  // Cargo can hardlink the canonical output to its own fingerprinted file.
  for await (const chunk of createReadStream(requireOwnedFile(path, allowCargoHardlink))) sha.update(chunk);
  return sha.digest("hex");
}

export function acquireProofOwnership(path, id) {
  const owner = JSON.stringify({ run: id, pid: process.pid, token: randomUUID() }) + "\n";
  try {
    writeFileSync(path, owner, { flag: "wx" });
  } catch (error) {
    if (error.code === "EEXIST") {
      throw new Error("Placement coverage suite already owns this root; did not retry or reclaim its lock");
    }
    throw error;
  }
  const original = lstatSync(path, { bigint: true });
  return () => {
    const current = lstatSync(requireOwnedFile(path), { bigint: true });
    requireThat(original.dev === current.dev && original.ino === current.ino &&
      original.size === current.size && readFileSync(path, "utf8") === owner,
    "Suite lock ownership changed; refusing cleanup");
    unlinkSync(path);
  };
}

export function localPlacementHostFingerprint() {
  // Bind to the same host, OS and CPU presentation without emitting the hostname.
  return createHash("sha256").update(JSON.stringify({
    host: hostname(), version: version(), platform: process.platform, arch: process.arch,
    available: availableParallelism(), cpu: cpus().map(({ model }) => model), totalMemory: totalmem(),
  })).digest("hex");
}

export function requirePlacementEvidenceSlot(manager, root, path) {
  requireThat(!existsSync(path), "Coverage evidence slot is already owned: " + basename(path));
  const verification = JSON.parse(checkedCommand(manager, ["--root", root, "storage", "verify", "--path", path], root));
  requireThat(verification?.schema_id === "clearra.storage-verification.v2" &&
    verification.root_id === "local-artifacts" && resolve(verification.path) === path,
  "Coverage slot is not inside the declared local artifact root");
}

export function readOwnedPlacementJson(path) {
  const file = requireOwnedFile(path);
  requireThat(lstatSync(file).size <= MAX_JSON_BYTES, "Oversized placement suite evidence");
  return JSON.parse(readFileSync(file, "utf8"));
}

export { AXIS, COVERAGE_ROOT };

let interrupted = null;

async function main(argv) {
  const options = parseProofArguments(argv);
  requireThat(process.env.CLEARRA_RUNTIME_SUPERVISED !== "1",
    "Run the orchestrator directly; it starts a separate finite native supervisor for each phase");
  requireThat(!Object.keys(process.env).some((key) => key.toUpperCase() === "CLEARRA_EXPECTED_VCPUS"),
    "Remove CLEARRA_EXPECTED_VCPUS to keep native automatic all-logical worker selection");
  const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), "../.."));
  const rustRoot = join(root, "build", "cargo", "default", options.buildProfile);
  const suffix = process.platform === "win32" ? ".exe" : "";
  const binary = requireOwnedFile(join(rustRoot, "examples", "placement_graph_bench" + suffix), true);
  const manager = requireOwnedFile(join(rustRoot, "clearra-manage" + suffix), true);
  const artifactRoot = join(root, ...COVERAGE_ROOT);
  const proofRoot = join(artifactRoot, options.id + "-proof");
  const lockPath = join(artifactRoot, "proof-transaction.lock");
  const steps = proofSchedule(options, binary, artifactRoot);
  const binarySha = await fileSha(binary);
  const managerSha = await fileSha(manager);
  const hostFingerprint = localPlacementHostFingerprint();
  const checkInputs = async () => {
    requireThat(!interrupted, "Interrupted; stopped before starting another phase");
    requireThat(checkedCommand("git", ["rev-parse", "HEAD"], root).trim() === options.sourceCommit &&
      checkedCommand("git", ["status", "--porcelain=v1", "--untracked-files=normal"], root).trim() === "",
    "The tracked and untracked source must match the exact clean checkout for this proof");
    requireThat(await fileSha(binary) === binarySha && await fileSha(manager) === managerSha,
      "Benchmark or supervisor binary changed during the proof");
    requireThat(localPlacementHostFingerprint() === hostFingerprint, "The measured host changed during the proof");
  };
  await checkInputs();
  const owned = [lockPath, proofRoot, ...steps.slice(0, 2).map((step) => dirname(step.artifact))];
  for (const path of owned) {
    requirePlacementEvidenceSlot(manager, root, path);
  }
  const supervisorDirectory = stateReceiptDirectory(process.env);
  mkdirSync(artifactRoot, { recursive: true });
  requireThat(realpathSync(artifactRoot) === artifactRoot, "Coverage artifact parent is a link or junction");
  if (options.key === "p7p4") {
    const volume = statfsSync(artifactRoot, { bigint: true });
    requireThat(volume.bavail * volume.bsize >= P7P4_FREE_RESERVE,
      "P7P4 requires 33 GiB free for one 16 GiB spill, two 8 GiB outputs, and reserve; no run started");
  }
  const release = acquireProofOwnership(lockPath, options.id);
  let evidence;
  try {
    mkdirSync(proofRoot); // Exclusive ownership; no reopen/resume of a failed proof.
    evidence = await collectPlacementCoverageProof(options, binarySha, steps, {
      custody: { hostFingerprint, managerSha },
      checkInputs,
      execute: (step) => captureChild(manager, composeRuntimeArgs(root, step), root,
        benchmarkEnvironment(process.env, step)),
      receipt: (execution) => readSupervisorReceipt(execution, supervisorDirectory),
      assertArtifact: requireOwnedFile,
      write: (name, value) => writeFileSync(join(proofRoot, name), JSON.stringify(value, null, 2) + "\n", { flag: "wx" }),
    });
  } finally {
    release();
  }
  process.stdout.write(JSON.stringify({
    proof: join(proofRoot, "summary.json"), state: evidence.state, error: evidence.error,
  }) + "\n");
  if (!evidence.mapping) process.exitCode = interrupted === "SIGINT" ? 130 : interrupted === "SIGTERM" ? 143 : 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, () => {
      if (interrupted) return;
      interrupted = signal;
      process.stderr.write("[placement-coverage-proof] cancelled: no further phase will start; " +
        "the current native supervisor, if any, remains responsible for its finite lease\n");
    });
  }
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write("[placement-coverage-proof] " + error.message + "\n");
    process.exitCode = 1;
  });
}

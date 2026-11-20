// Sequential, local-only ABBA + BAAB after complete P7P4 mapping qualification.
import { mkdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";

import { evaluatePlacementGraphAdoption } from "../tools/placement-graph-adoption.mjs";
import {
  acquireProofOwnership, assertExactComparison, AXIS, benchmarkEnvironment, captureChild,
  checkedCommand, classifyProofRuntime, composeRuntimeArgs, COVERAGE_ROOT, fileSha,
  localPlacementHostFingerprint, parseBoundBenchmarkOutput, parseProofArguments, proofSchedule,
  readOwnedPlacementJson, readSupervisorReceipt, requireOwnedFile, requirePlacementEvidenceSlot,
  stateReceiptDirectory, validatedProofArm,
} from "./run-placement-coverage-proof.mjs";

const ORDER = "ABBABAAB";
const INPUT_SCHEMA = "clearra.placement-graph-adoption-input.v1";
const PROOF_SCHEMA = "clearra.placement-graph-coverage-proof.v1";
const ID = /^[a-z0-9][a-z0-9-]{0,47}$/u;
const SOURCE = /^[a-f0-9]{40}$/u;
const DIGEST = /^[a-f0-9]{64}$/u;
const APPROVED_MAPPING = {
  comparison: "comparison.json",
  reference: { benchmark: "reference.json", supervisor: "reference-runtime.json" },
  candidate: { benchmark: "candidate.json", supervisor: "candidate-runtime.json" },
};
const MAPPING_COPY = {
  comparison: "mapping-comparison.json",
  reference: { benchmark: "mapping-reference.json", supervisor: "mapping-reference-runtime.json" },
  candidate: { benchmark: "mapping-candidate.json", supervisor: "mapping-candidate-runtime.json" },
};

const NEUTRAL_ADDITIONAL_AXES = [
  "ordered_kick_interval_mode", "ilc_lazy_frame_mode",
  "no_entry_components_mode", "same_tile_parent_reuse_mode",
];

function isBoundSingleAxisReport(report, sourceCommit) {
  return report?.compiled_source_commit === sourceCommit &&
    NEUTRAL_ADDITIONAL_AXES.every((axis) => report[axis] === "0");
}

function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}

export function parseAdoptionArguments(argv) {
  const args = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index], value = argv[index + 1];
    requireThat(["--id", "--proof-id", "--source-sha", "--time-resolution-ms"].includes(key) &&
      typeof value === "string" && !Object.hasOwn(args, key), "Distinct known option/value pairs required");
    args[key] = value;
  }
  const id = args["--id"], proofId = args["--proof-id"], sourceCommit = args["--source-sha"];
  const rawResolution = args["--time-resolution-ms"] ?? "1";
  const timeResolutionMs = Number(rawResolution);
  requireThat(ID.test(id ?? "") && ID.test(proofId ?? "") && SOURCE.test(sourceCommit ?? "") &&
    /^\d+(?:\.\d+)?$/u.test(rawResolution) && Number.isFinite(timeResolutionMs) &&
    timeResolutionMs >= 0.001 && timeResolutionMs <= 1_000,
  "Usage: run-placement-graph-adoption --id UNIQUE-LEAF --proof-id P7P4-PROOF-LEAF --source-sha FULL-HEAD [--time-resolution-ms N]");
  return { id, proofId, sourceCommit, timeResolutionMs };
}

export function adoptionSchedule(binary, timeout) {
  return Array.from(ORDER, (arm, index) => ({
    name: `trial-${index + 1}-${arm.toLowerCase()}`,
    arm, value: arm === "A" ? "0" : "1", timeout,
    command: [binary],
  }));
}

function interval(outcome) {
  requireThat(Number.isSafeInteger(outcome.duration_ms) && outcome.duration_ms > 0, "Missing runtime duration");
  const start = BigInt(outcome.run_id.split("-")[0]);
  return { start, end: start + BigInt(outcome.duration_ms) * 1_000_000n };
}

function sequentialAfter(previous, receipt) {
  const current = interval(receipt.outcome);
  requireThat(!previous || current.start > previous.end, "Supervisor phases overlapped or their order changed");
  return current;
}

export function validateAdoptionProof(options, binary, artifactRoot, custody, bundle) {
  const { summary, reference, candidate, comparison } = bundle;
  requireThat(summary?.schema === PROOF_SCHEMA && summary.state === "completed-exact-per-solution" &&
    summary.axis === AXIS && summary.source_commit === options.sourceCommit &&
    summary.source_binding === "compile-time-and-checkout" &&
    summary.binary_sha256 === custody.binarySha && summary.manager_sha256 === custody.managerSha &&
    summary.host_fingerprint === custody.hostFingerprint &&
    [custody.binarySha, custody.managerSha, custody.hostFingerprint].every((value) => DIGEST.test(value ?? "")) &&
    summary.release_authority === false && summary.timing_adoption_authority === false &&
    Array.isArray(summary.observations) && summary.observations.length === 3 &&
    isDeepStrictEqual(summary.mapping, APPROVED_MAPPING),
  "A complete same-checkout, same-binary, same-host P7P4 proof for the movement-clear axis is required");

  const proofOptions = parseProofArguments(["--case", "p7p4", "--id", options.proofId,
    "--source-sha", options.sourceCommit, "--timeout-seconds", String(reference.runtime.outcome?.timeout_seconds)]);
  requireThat(summary.case === proofOptions.fixture.name, "The one-I preflight does not qualify P7P4");
  const steps = proofSchedule(proofOptions, binary, artifactRoot);
  let last, referenceContract;
  for (const [index, step] of steps.entries()) {
    const phase = [reference, candidate, comparison][index];
    const runtime = phase?.runtime;
    const observed = summary.observations[index];
    const syntheticExecution = { code: runtime?.outcome?.return_code, signal: null, stdout: JSON.stringify(phase?.report) };
    requireThat(classifyProofRuntime(runtime, syntheticExecution, step) === "completed" &&
      observed?.state === "completed" && observed.name === step.name &&
      observed.run_id === runtime.outcome.run_id && observed.supervisor === step.name + "-runtime.json" &&
      isDeepStrictEqual(observed.command, step.command) &&
      parseBoundBenchmarkOutput(runtime, syntheticExecution),
    step.name + ": stored proof receipt is not bound to this successful phase");
    last = sequentialAfter(last, runtime);
    if (index !== 2) {
      requireThat(isBoundSingleAxisReport(phase.report, options.sourceCommit),
      step.name + ": exact compile-time source or fixed axes missing; comparison-only proof cannot authorize timing");
      const contract = validatedProofArm(phase.report, runtime, step, proofOptions, custody.binarySha);
      referenceContract ??= contract;
      requireThat(isDeepStrictEqual(contract, referenceContract), "Stored proof arms do not agree on the P7P4 contract");
    }
  }
  assertExactComparison(comparison.report, reference.report, candidate.report, proofOptions.fixture);
  return {
    proofOptions, last, contract: referenceContract,
    artifacts: steps.slice(0, 2).map((step, index) => ({
      path: step.artifact, sha256: [reference, candidate][index].report.solution_coverage.sha256,
    })),
    mapping: {
      reference: { benchmark: reference.report, supervisor: reference.runtime },
      candidate: { benchmark: candidate.report, supervisor: candidate.runtime },
      comparison: comparison.report,
    },
  };
}

export async function collectPlacementGraphAdoption(options, custody, proof, schedule, io) {
  const observations = [];
  const manifest = {
    schema: INPUT_SCHEMA, axis: AXIS, output_mode: "global-only",
    time_resolution_ms: options.timeResolutionMs, runs: [], mapping: MAPPING_COPY,
  };
  const evidence = {
    schema: "clearra.placement-graph-adoption-run.v1", release_authority: false,
    source_commit: options.sourceCommit, binary_sha256: custody.binarySha,
    manager_sha256: custody.managerSha, host_fingerprint: custody.hostFingerprint,
    proof_id: options.proofId, order: "ABBA+BAAB", axis: AXIS, output_mode: "global-only",
    observations, decision: null, state: "failed",
  };
  const measured = [];
  let previous = proof.last, active;
  try {
    requireThat(schedule.length === ORDER.length && schedule.every((step, index) =>
      step.arm === ORDER[index] && step.value === (step.arm === "A" ? "0" : "1") &&
      step.timeout === proof.proofOptions.timeout && step.command.length === 1),
    "The timing schedule is not a fixed global-only ABBA+BAAB sequence");
    for (const step of schedule) {
      active = { name: step.name, arm: step.arm, state: "invalid", supervisor: null };
      observations.push(active);
      await io.checkInputs();
      const execution = await io.execute(step);
      const runtime = io.receipt(execution);
      active.state = classifyProofRuntime(runtime, execution, step);
      active.reason = runtime.outcome.reason;
      active.child_termination = runtime.outcome.child_termination;
      active.run_id = runtime.outcome.run_id;
      active.supervisor = step.name + "-runtime.json";
      io.write(active.supervisor, runtime);
      if (active.state !== "completed") throw new Error(step.name + ": " + active.state + " (" + active.reason + ")");
      active.state = "invalid";
      previous = sequentialAfter(previous, runtime);
      const report = parseBoundBenchmarkOutput(runtime, execution);
      requireThat(isBoundSingleAxisReport(report, options.sourceCommit),
      step.name + ": exact compile-time source or fixed axes missing");
      const contract = validatedProofArm(report, runtime, step, proof.proofOptions, custody.binarySha, "global-only");
      requireThat(isDeepStrictEqual(contract, proof.contract),
        step.name + ": timed fixture, workers, coverage or process state differs from the approved proof");
      active.benchmark = step.name + ".json";
      io.write(active.benchmark, report);
      manifest.runs.push({ benchmark: active.benchmark, supervisor: active.supervisor });
      measured.push({ benchmark: report, supervisor: runtime });
      active.state = "completed";
    }
    await io.checkInputs();
    const decision = evaluatePlacementGraphAdoption({ ...manifest, runs: measured, mapping: proof.mapping });
    io.write(MAPPING_COPY.reference.benchmark, proof.mapping.reference.benchmark);
    io.write(MAPPING_COPY.reference.supervisor, proof.mapping.reference.supervisor);
    io.write(MAPPING_COPY.candidate.benchmark, proof.mapping.candidate.benchmark);
    io.write(MAPPING_COPY.candidate.supervisor, proof.mapping.candidate.supervisor);
    io.write(MAPPING_COPY.comparison, proof.mapping.comparison);
    io.write("manifest.json", manifest);
    io.write("decision.json", decision);
    evidence.decision = "decision.json";
    evidence.state = decision.status === "adoptable" ? "completed-adoptable" : "completed-not-adoptable";
  } catch (error) {
    evidence.state = active?.state === "censored" || active?.state === "confounded" ? active.state : "failed";
    evidence.error = error.message;
  }
  io.write("summary.json", evidence);
  return evidence;
}

let interrupted = null;

async function main(argv) {
  const options = parseAdoptionArguments(argv);
  requireThat(process.env.CLEARRA_RUNTIME_SUPERVISED !== "1",
    "Run the timing orchestrator directly; each workload uses its own finite native supervisor");
  requireThat(!Object.keys(process.env).some((key) => key.toUpperCase() === "CLEARRA_EXPECTED_VCPUS"),
    "Remove CLEARRA_EXPECTED_VCPUS to keep native automatic all-logical worker selection");
  const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), "../.."));
  const rustRoot = join(root, "build", "cargo", "default", "release");
  const suffix = process.platform === "win32" ? ".exe" : "";
  const binary = requireOwnedFile(join(rustRoot, "examples", "placement_graph_bench" + suffix), true);
  const manager = requireOwnedFile(join(rustRoot, "clearra-manage" + suffix), true);
  const artifactRoot = join(root, ...COVERAGE_ROOT);
  const proofRoot = join(artifactRoot, options.proofId + "-proof");
  const outputRoot = join(artifactRoot, options.id + "-adoption");
  const lockPath = join(artifactRoot, "proof-transaction.lock");
  requireThat(realpathSync(artifactRoot) === artifactRoot && realpathSync(proofRoot) === proofRoot,
    "Placement evidence roots cannot follow links or junctions");
  const custody = {
    binarySha: await fileSha(binary), managerSha: await fileSha(manager),
    hostFingerprint: localPlacementHostFingerprint(),
  };
  const checkInputs = async () => {
    requireThat(!interrupted, "Interrupted; stopped before starting another timing phase");
    requireThat(checkedCommand("git", ["rev-parse", "HEAD"], root).trim() === options.sourceCommit &&
      checkedCommand("git", ["status", "--porcelain=v1", "--untracked-files=normal"], root).trim() === "",
    "The timing suite requires the exact clean source checkout used for qualification");
    requireThat(await fileSha(binary) === custody.binarySha && await fileSha(manager) === custody.managerSha &&
      localPlacementHostFingerprint() === custody.hostFingerprint,
    "Benchmark, supervisor or measured host changed during the timing sequence");
  };
  await checkInputs();
  requirePlacementEvidenceSlot(manager, root, outputRoot);
  requirePlacementEvidenceSlot(manager, root, lockPath);
  const release = acquireProofOwnership(lockPath, options.id);
  let evidence;
  try {
    const read = (name) => readOwnedPlacementJson(join(proofRoot, name));
    const phase = (name) => ({ report: read(name + ".json"), runtime: read(name + "-runtime.json") });
    const proof = validateAdoptionProof(options, binary, artifactRoot, custody, {
      summary: read("summary.json"), reference: phase("reference"),
      candidate: phase("candidate"), comparison: phase("comparison"),
    });
    // Stream the two exact evidence artifacts once before timing. The original
    // comparator receipt is reused; no solver or comparator is rerun here.
    for (const { path, sha256 } of proof.artifacts) {
      requireThat(await fileSha(path, false) === sha256, "A qualified mapping artifact changed since its comparison");
    }
    const supervisorDirectory = stateReceiptDirectory(process.env);
    const schedule = adoptionSchedule(binary, proof.proofOptions.timeout);
    mkdirSync(outputRoot); // Exclusive slot; no append, retry or reuse.
    evidence = await collectPlacementGraphAdoption(options, custody, proof, schedule, {
      checkInputs,
      execute: (step) => captureChild(manager, composeRuntimeArgs(root, step), root,
        benchmarkEnvironment(process.env, step)),
      receipt: (execution) => readSupervisorReceipt(execution, supervisorDirectory),
      write: (name, value) => writeFileSync(join(outputRoot, name), JSON.stringify(value, null, 2) + "\n", { flag: "wx" }),
    });
  } finally {
    release();
  }
  process.stdout.write(JSON.stringify({
    evidence: join(outputRoot, "summary.json"), state: evidence.state, error: evidence.error,
  }) + "\n");
  if (evidence.state !== "completed-adoptable") process.exitCode = interrupted === "SIGINT" ? 130 : interrupted === "SIGTERM" ? 143 : 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, () => {
      if (interrupted) return;
      interrupted = signal;
      process.stderr.write("[placement-graph-adoption] cancelled: no further trial will start; " +
        "the current native supervisor, if any, remains responsible for its finite lease\n");
    });
  }
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write("[placement-graph-adoption] " + error.message + "\n");
    process.exitCode = 1;
  });
}

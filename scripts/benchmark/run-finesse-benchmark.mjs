import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { finesseSourceSnapshot } from './finesse-source-snapshot.mjs';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const options = parseArgs(process.argv.slice(2));
if (optionEnabled(options.help) || optionEnabled(options.h)) {
  process.stdout.write(usage());
  process.exit(0);
}
if (optionEnabled(options['self-test'])) {
  runSelfTests();
  process.stdout.write('run-finesse-benchmark: self-test passed\n');
  process.exit(0);
}
const dryRun = optionEnabled(options['dry-run']);
const sourceRoot = resolve(options['source-root'] ?? repository);
const reportRoot = dryRun ? null : resolve(required(options, 'report'));
const browserRoot = options['browser-root']
  ? resolve(options['browser-root'])
  : null;
if (
  reportRoot !== null &&
  isWithin(repository, reportRoot) &&
  !optionEnabled(options['allow-workspace-report'])
) {
  throw new Error('--report must be outside the repository (or explicitly allow it)');
}
if (!fs.existsSync(resolve(sourceRoot, 'Cargo.toml'))) {
  throw new Error(`--source-root is not a Clearra source tree: ${sourceRoot}`);
}
if (!dryRun && browserRoot === null) {
  throw new Error('--browser-root is required');
}
if (browserRoot !== null) {
  const browserEntry = resolve(browserRoot, 'index.html');
  if (!fs.existsSync(browserEntry) || !fs.statSync(browserEntry).isFile()) {
    throw new Error(`--browser-root must contain index.html: ${browserEntry}`);
  }
}

const workers = positiveInteger(
  options.workers ?? String(Math.max(1, os.availableParallelism() - 1)),
  'workers'
);
if (workers > os.availableParallelism()) {
  throw new Error(
    `workers (${workers}) exceeds the logical processor hard limit (${os.availableParallelism()})`
  );
}
const phase = options.phase ?? 'candidate';
if (!['baseline', 'reference', 'candidate'].includes(phase)) {
  throw new Error('--phase must be baseline, reference, or candidate');
}
const selected = new Set((options.cases ?? 'small,long,score').split(','));
const selectedModes = new Set(
  (
    options.modes ??
    (phase === 'baseline' ? 'off' : phase === 'reference' ? 'inputs,score' : 'off,inputs,score')
  ).split(',')
);
const smallTimeout = positiveInteger(options['small-timeout'] ?? '300000', 'small-timeout');
const longTimeout = positiveInteger(options['long-timeout'] ?? '1800000', 'long-timeout');
if (![...selectedModes].every((mode) => ['off', 'inputs', 'score'].includes(mode))) {
  throw new Error('--modes accepts only off, inputs, and score');
}
if (
  options['finesse-reference-kind'] &&
  !['reference', 'previous-candidate'].includes(options['finesse-reference-kind'])
) {
  throw new Error('--finesse-reference-kind accepts reference or previous-candidate');
}

// Repetition counts are part of the benchmark contract: cases expected below
// one second run five times, while the deliberately long case runs twice.
const patternScorePlacements = [
  'I:spawn:0:0',
  'O:spawn:0:1',
  'T:spawn:0:3',
  'S:spawn:0:5',
  'Z:spawn:0:7',
  'J:spawn:0:9',
  'L:spawn:0:11',
  'I:spawn:0:13',
  'O:spawn:0:14',
  'T:spawn:0:16',
].join(',');
const cases = [
  benchmarkCase(
    'small-off',
    'small',
    'off',
    5,
    smallTimeout,
    workers,
    `clearra build-probability --base-mask 0 --target-mask 0xffffffffff --height 4 ` +
      `--queue OTSZJLIOTI --no-hold --aggregate buildability --rule srs-plus --no-mirror ` +
      `--max-candidates 100000000 --workers ${workers} --cpu-warmup`
  ),
  benchmarkCase(
    'small-inputs',
    'small',
    'inputs',
    5,
    smallTimeout,
    workers,
    `clearra build-probability --base-mask 0 --target-mask 0xffffffffff --height 4 ` +
      `--queue OOOOOOOOOO --no-hold --aggregate buildability --rule srs-plus --no-mirror ` +
      `--max-candidates 100000000 --finesse inputs --pattern-knowledge both ` +
      `--workers ${workers} --cpu-warmup`
  ),
  benchmarkCase(
    'pattern-inputs',
    'small',
    'inputs',
    5,
    smallTimeout,
    workers,
    `clearra build-probability --base-mask 0 --target-mask 0xe81a06fffbf ` +
      `--height 8 --hold empty --patterns P7P3 --aggregate buildability ` +
      `--rule srs-plus --no-mirror --max-patterns 128 --max-candidates 100000000 ` +
      `--finesse inputs --pattern-knowledge both --workers ${workers} --cpu-warmup`
  ),
  benchmarkCase(
    'long-off',
    'long',
    'off',
    2,
    longTimeout,
    workers,
    `clearra build-probability --base-mask 0 --target-mask 0x000318e3fdffffff ` +
      `--height 5 --hold empty --patterns P7P3 --aggregate buildability ` +
      `--rule srs-plus --no-mirror --max-patterns 2000000 --max-candidates 100000000 ` +
      `--workers ${workers} --cpu-warmup`
  ),
  benchmarkCase(
    'long-inputs',
    'long',
    'inputs',
    2,
    longTimeout,
    workers,
    `clearra build-probability --base-mask 0 --target-mask 0xffffffffff ` +
      `--height 4 --queue OTSZJLIOTI --no-hold --aggregate buildability ` +
      `--rule srs-plus --no-mirror --max-candidates 100000000 ` +
      `--finesse inputs --pattern-knowledge both --workers ${workers} --cpu-warmup`
  ),
  benchmarkCase(
    'pattern-score',
    'score',
    'score',
    5,
    smallTimeout,
    workers,
    `clearra finesse score --initial-mask 0 --height 24 ` +
      `--placements ${patternScorePlacements} --patterns P7P3 --no-hold ` +
      `--rule srs-plus --pattern-knowledge both --max-patterns 65536 ` +
      `--workers ${workers} --cpu-warmup`
  ),
].filter((entry) =>
  (selected.has(entry.group) || selected.has(entry.id)) && selectedModes.has(entry.mode)
);

if (cases.length === 0) throw new Error('--cases selected no benchmark cases');
const baselineReference = !dryRun && phase === 'candidate' && cases.some((entry) => entry.mode === 'off')
  ? loadSummary(required(options, 'baseline-summary'))
  : null;
const finesseReference = !dryRun && phase === 'candidate' &&
    cases.some((entry) => entry.mode === 'inputs' || entry.mode === 'score')
  ? loadSummary(required(options, 'finesse-reference-summary'))
  : null;
const sourceState = await gitSourceState(sourceRoot);
if (
  options['source-revision'] &&
  sourceState.revision !== null &&
  options['source-revision'] !== sourceState.revision
) {
  throw new Error(
    `--source-revision ${options['source-revision']} does not match source HEAD ${sourceState.revision}`
  );
}
if (
  phase === 'baseline' &&
  options['require-clean-source'] !== 'false' &&
  (sourceState.tracked_dirty !== false || sourceState.has_untracked_files !== false)
) {
  throw new Error(
    'baseline must use a clean tracked HEAD worktree (use --require-clean-source false only to diagnose the harness)'
  );
}
const snapshot = finesseSourceSnapshot(sourceRoot);
const harnessSnapshot = snapshotFiles(repository, [
  'scripts/benchmark/finesse-source-snapshot.mjs',
  'scripts/benchmark/run-finesse-benchmark.mjs',
  'scripts/benchmark/wasm-product-browser/benchmark-client.ts',
  'scripts/benchmark/wasm-product-browser/index.html',
  'scripts/benchmark/wasm-product-browser/prewarm-benchmark-worker.ts',
  'scripts/benchmark/wasm-product-browser/vite.config.mjs',
  'scripts/tools/build-clearra-wasm.mjs',
  'scripts/tools/run-wasm-browser-benchmark.mjs',
]);
const metadata = {
  schema_version: 2,
  phase,
  generated_at: new Date().toISOString(),
  workers,
  source_root: sourceRoot,
  source_revision: options['source-revision'] ?? sourceState.revision,
  source_tracked_dirty: sourceState.tracked_dirty,
  source_has_untracked_files: sourceState.has_untracked_files,
  source_snapshot_sha256: snapshot.digest,
  source_files: snapshot.files,
  harness_snapshot_sha256: harnessSnapshot.digest,
  harness_files: harnessSnapshot.files,
  wasm_sha256: browserRoot === null ? null : wasmDigest(browserRoot),
  artifact_provenance: browserRoot === null
    ? null
    : artifactProvenance(browserRoot, snapshot),
  case_contract_sha256: sha256(stableJson(cases.map(caseContract))),
  comparison_references: {
    baseline: referenceDescriptor(baselineReference),
    finesse: referenceDescriptor(finesseReference),
  },
  fixed_score_contract: {
    projection: expectedFixedScoreContract(),
    sha256: sha256(stableJson(expectedFixedScoreContract())),
  },
  acceptance_policy: {
    off_median_regression_limit: 0.02,
    finesse_median_regression_limit: decimalOption(
      options['finesse-regression-limit'] ?? '0.02',
      'finesse-regression-limit'
    ),
    memory_tradeoff_max_regression: 0.10,
    memory_tradeoff_peak_ratio: 0.50,
  },
  host: {
    platform: process.platform,
    architecture: process.arch,
    logical_processors: os.availableParallelism(),
    node: process.version,
  },
  memory_measurement: {
    enabled: options['os-memory-probe'] !== 'off',
    internal: 'solver-retained-byte upper-bound reported as peak_cpu_bytes',
    os: process.platform === 'win32'
      ? 'separate-run aggregate process-tree WorkingSetSize samples (shared pages may overlap)'
      : 'unsupported by this harness',
    os_sample_interval_ms: positiveInteger(
      options['memory-sample-interval'] ?? '250',
      'memory-sample-interval'
    ),
    separated_from_timed_runs: options['os-memory-probe'] !== 'off',
  },
  cases,
};
if (!dryRun && metadata.artifact_provenance?.valid !== true) {
  throw new Error(
    `browser artifact provenance mismatch: ${metadata.artifact_provenance?.reason ?? 'missing'}`
  );
}
if (dryRun) {
  console.log(JSON.stringify({
    dry_run: true,
    metadata,
    expected_fixed_score_contract: expectedFixedScoreContract(),
    required_reference_summaries: phase === 'candidate'
      ? ['--baseline-summary', '--finesse-reference-summary']
      : [],
  }, null, 2));
  process.exit(0);
}
fs.mkdirSync(reportRoot, { recursive: true });
fs.writeFileSync(
  resolve(reportRoot, `${phase}-finesse-metadata.json`),
  JSON.stringify(metadata, null, 2)
);

const runs = [];
const runsPath = resolve(reportRoot, `${phase}-finesse-runs.ndjson`);
fs.writeFileSync(runsPath, '');
for (const entry of cases) {
  for (let repetition = 1; repetition <= entry.repetitions; repetition += 1) {
    const label = `${phase}-${entry.id}-r${repetition}`;
    process.stderr.write(`[finesse-benchmark] start ${label}\n`);
    const rawPath = resolve(reportRoot, `${label}.json`);
    const stderrPath = resolve(reportRoot, `${label}.stderr.txt`);
    const raw = await executeBrowser(entry, rawPath, stderrPath);
    const normalized = normalize(raw, entry, repetition, rawPath);
    runs.push(normalized);
    fs.appendFileSync(
      runsPath,
      `${JSON.stringify(normalized)}\n`
    );
    process.stderr.write(
      `[finesse-benchmark] complete ${label} status=${normalized.status} ` +
        `elapsed_ms=${normalized.elapsed_ms ?? 'censored'}\n`
    );
  }
}

const workerInvariance = await verifyWorkerInvariance(cases);
const memoryProbes = await probeCaseMemory(cases);

const report = {
  metadata,
  runs,
  memory_probes: memoryProbes,
  aggregates: aggregate(runs, memoryProbes),
  worker_invariance: workerInvariance,
};
report.validation = validateReport(report);
if (phase === 'candidate') {
  report.acceptance = compareReferences(
    baselineReference?.report ?? null,
    finesseReference?.report ?? null,
    report
  );
}
report.pass = report.validation.pass && (report.acceptance?.pass ?? true);
const summaryPath = resolve(reportRoot, `${phase}-finesse-summary.json`);
fs.writeFileSync(summaryPath, JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
if (!report.pass) {
  process.exitCode = 1;
}

function benchmarkCase(id, group, mode, repetitions, timeout, workerCount, command) {
  return { id, group, mode, repetitions, timeout, workers: workerCount, command };
}

async function executeBrowser(entry, rawPath, stderrPath, memoryProbe = false) {
  const runner = resolve(repository, 'scripts/tools/run-wasm-browser-benchmark.mjs');
  const stdout = fs.openSync(rawPath, 'w');
  const stderr = fs.openSync(stderrPath, 'w');
  const code = await new Promise((resolveExit, rejectExit) => {
    const args = [
      runner,
      '--root', browserRoot,
      '--command', entry.command,
      '--timeout', String(entry.timeout),
      '--runtime-prewarm-workers', String(entry.workers),
    ];
    if (memoryProbe) {
      args.push(
        '--memory-probe', 'true',
        '--memory-sample-interval', options['memory-sample-interval'] ?? '250'
      );
    }
    const child = spawn(process.execPath, args, {
      cwd: repository,
      stdio: ['ignore', stdout, stderr],
      windowsHide: true,
    });
    child.once('error', rejectExit);
    child.once('exit', (exitCode) => resolveExit(exitCode ?? 1));
  }).finally(() => {
    fs.closeSync(stdout);
    fs.closeSync(stderr);
  });
  if (code !== 0) {
    return {
      runner_failed: true,
      exit_code: code,
      stderr: fs.readFileSync(stderrPath, 'utf8'),
    };
  }
  return JSON.parse(fs.readFileSync(rawPath, 'utf8'));
}

async function verifyWorkerInvariance(entries) {
  if (phase === 'baseline' || options['worker-invariance'] === 'off') {
    return { requested: false, pass: true, comparisons: [] };
  }
  if (workers === 1) {
    return {
      requested: true,
      pass: false,
      error: 'worker invariance requires --workers greater than one',
      comparisons: [],
    };
  }
  const comparisonEntries = [];
  if (selectedModes.has('inputs')) {
    const requestedCase = options['worker-invariance-case'];
    const entry = requestedCase
      ? entries.find((candidate) => candidate.id === requestedCase)
      : benchmarkCase(
          'worker-invariance-inputs',
          'invariance',
          'inputs',
          1,
          smallTimeout,
          workers,
          `clearra build-probability --base-mask 0 --target-mask 0xfc3f3fcff ` +
            `--height 4 --queue OOOOOOO --no-hold --aggregate buildability ` +
            `--rule srs-plus --no-mirror --finesse inputs --pattern-knowledge both ` +
            `--workers ${workers} --cpu-warmup`
        );
    if (!entry || entry.mode !== 'inputs') {
      return {
        requested: true,
        pass: false,
        error: 'worker invariance case must select an inputs-mode case',
        comparisons: [],
      };
    }
    comparisonEntries.push({ entry, parallelExpected: true });
  }
  if (selectedModes.has('score')) {
    const entry = entries.find((candidate) => candidate.id === 'pattern-score');
    if (entry) comparisonEntries.push({ entry, parallelExpected: false });
  }
  const comparisons = [];
  const pairRuns = [];
  for (const request of comparisonEntries) {
    const pair = await executeWorkerPair(request.entry);
    pairRuns.push(pair);
    const serial = pair.serial;
    const parallel = pair.parallel;
    const allStagesActive = [serial, parallel].every((run) =>
      expectedFinesseStages().every((name) =>
        run.finesse_stages.some((stage) =>
          stage.name === name && Number(stage.invocation_count) > 0
        )
      )
    );
    const workerContract = request.parallelExpected
      ? serial.workers_used === 1 &&
        Number(parallel.workers_used) > 1 &&
        Number(parallel.workers_used) <= workers
      : serial.workers_used === 1 && parallel.workers_used === 1;
    const scoreContractExact = request.entry.mode !== 'score' || [serial, parallel].every((run) =>
      run.fixed_score_contract_hash === sha256(stableJson(expectedFixedScoreContract()))
    );
    const comparison = {
      case: request.entry.id,
      mode: request.entry.mode,
      serial_status: serial.status,
      requested_multi_status: parallel.status,
      requested_workers: [1, workers],
      observed_workers: [serial.workers_used, parallel.workers_used],
      multi_worker_path_exercised: request.parallelExpected,
      worker_execution_contract: workerContract,
      solution_identity_match:
        serial.unique_solution_count === parallel.unique_solution_count &&
        serial.solution_hash === parallel.solution_hash,
      finesse_report_match:
        serial.finesse_report_hash !== null &&
        serial.finesse_report_hash === parallel.finesse_report_hash,
      representative_witness_match:
        serial.witness_hash !== null && serial.witness_hash === parallel.witness_hash,
      all_seven_coordinator_stages_active: allStagesActive,
      fixed_score_contract_exact: scoreContractExact,
    };
    comparison.pass =
      serial.status === 'success' &&
      parallel.status === 'success' &&
      comparison.worker_execution_contract &&
      comparison.solution_identity_match &&
      comparison.finesse_report_match &&
      comparison.representative_witness_match &&
      comparison.all_seven_coordinator_stages_active &&
      comparison.fixed_score_contract_exact;
    comparisons.push(comparison);
  }
  return {
    requested: true,
    pass: comparisons.length > 0 && comparisons.every((comparison) => comparison.pass),
    comparisons,
    pair_runs: pairRuns,
  };
}

async function executeWorkerPair(entry) {
  const serialEntry = {
    ...entry,
    id: `${entry.id}-workers-1`,
    repetitions: 1,
    workers: 1,
    command: entry.command.replace(/--workers\s+\d+/, '--workers 1'),
  };
  const parallelEntry = { ...entry, repetitions: 1 };
  const serial = await executeInvarianceRun(serialEntry, 'serial');
  const parallel = await executeInvarianceRun(parallelEntry, 'requested-multi');
  return { serial, parallel };
}

async function executeInvarianceRun(entry, suffix) {
  const label = `${phase}-${entry.id}-${suffix}-invariance`;
  const rawPath = resolve(reportRoot, `${label}.json`);
  const stderrPath = resolve(reportRoot, `${label}.stderr.txt`);
  process.stderr.write(`[finesse-benchmark] start ${label}\n`);
  const run = normalize(
    await executeBrowser(entry, rawPath, stderrPath),
    entry,
    1,
    rawPath
  );
  process.stderr.write(
    `[finesse-benchmark] complete ${label} status=${run.status}\n`
  );
  return run;
}

function normalize(raw, entry, repetition, rawPath) {
  if (raw.timed_out) {
    return {
      phase,
      case: entry.id,
      group: entry.group,
      mode: entry.mode,
      repetition,
      status: 'timeout',
      elapsed_ms: null,
      timeout_ms: raw.timeout_ms,
      peak_cpu_bytes: null,
      finesse_stages: [],
      raw_file: rawPath,
    };
  }
  if (raw.runner_failed) {
    return {
      phase,
      case: entry.id,
      group: entry.group,
      mode: entry.mode,
      repetition,
      status: 'runner-failed',
      elapsed_ms: null,
      peak_cpu_bytes: null,
      finesse_stages: [],
      error: raw.stderr,
      raw_file: rawPath,
    };
  }
  const event = raw.event ?? {};
  const response = event.response ?? {};
  const search = event.search_report ?? {};
  const resources = response.resource_report ?? {};
  const finesseReport = search.finesse_report ?? null;
  const scoreContract = entry.mode === 'score'
    ? fixedScoreContractProjection(finesseReport)
    : null;
  const runtimeEnvironment = runtimeEnvironmentProjection(raw.capabilities);
  return {
    phase,
    case: entry.id,
    group: entry.group,
    mode: entry.mode,
    repetition,
    status: response.status ?? event.event ?? 'unknown',
    elapsed_ms: raw.run_elapsed_ms ?? raw.elapsed_ms ?? null,
    browser_memory_peak_bytes:
      raw.browser_memory_peak_bytes ?? raw.browser_memory_terminal_bytes ?? null,
    browser_memory_terminal_bytes: raw.browser_memory_terminal_bytes ?? null,
    browser_process_tree_peak_working_set_bytes:
      raw.browser_process_tree_peak_working_set_bytes ?? null,
    browser_process_tree_memory_probe: raw.browser_process_tree_memory_probe ?? null,
    retained_peak_bytes: search.peak_cpu_bytes ?? resources.peak_cpu_bytes ?? null,
    // Compatibility alias for schema-v1 summaries. This is solver-retained
    // memory, not the browser process-tree working set.
    peak_cpu_bytes: search.peak_cpu_bytes ?? resources.peak_cpu_bytes ?? null,
    workers_used: search.workers_used ?? null,
    unique_solution_count: search.unique_solution_count ?? null,
    solution_hash: search.normalized_solution_set_hash ?? null,
    finesse_report_hash: finesseReport === null ? null : sha256(stableJson(finesseReport)),
    exact_total_inputs: finesseReport?.exact_total_inputs ?? null,
    materialized_pattern_count: search.materialized_pattern_count ?? null,
    source_sequence_length: search.source_sequence_length ?? null,
    finesse_report_complete: finesseReport?.complete ?? null,
    visible_information_penalty_inputs: finesseReport?.policy_results
      ?.find((policy) => policy?.policy === 'visible-7')
      ?.information_penalty_inputs ?? null,
    witness_hash: finesseReport?.representative_witness === undefined ||
        finesseReport?.representative_witness === null
      ? null
      : sha256(stableJson(finesseReport.representative_witness)),
    fixed_score_contract: scoreContract,
    fixed_score_contract_hash: scoreContract === null ? null : sha256(stableJson(scoreContract)),
    finesse_stages: finesseStages(event.search_profile),
    runtime_environment: runtimeEnvironment,
    runtime_environment_hash: runtimeEnvironment === null
      ? null
      : sha256(stableJson(runtimeEnvironment)),
    raw_file: rawPath,
  };
}

function finesseStages(profile) {
  const stages = Array.isArray(profile) ? profile : profile?.stages;
  if (!Array.isArray(stages)) return [];
  return stages
    .filter((stage) => typeof stage?.name === 'string' && stage.name.startsWith('finesse.'))
    .map((stage) => ({
      name: stage.name,
      duration_ns: finiteOrNull(stage.duration_ns),
      invocation_count: finiteOrNull(stage.invocation_count),
      work_item_count: finiteOrNull(stage.work_item_count),
    }));
}

function aggregate(runs, memoryProbes) {
  const groups = new Map();
  for (const run of runs) {
    const group = groups.get(run.case) ?? [];
    group.push(run);
    groups.set(run.case, group);
  }
  return [...groups.entries()].map(([id, group]) => {
    const contract = cases.find((entry) => entry.id === id);
    const completed = group.filter((run) => run.status === 'success');
    const memoryProbe = memoryProbes.find((probe) => probe.case === id);
    const stageNames = new Set(completed.flatMap((run) => run.finesse_stages.map((stage) => stage.name)));
    return {
      case: id,
      group: group[0]?.group ?? null,
      mode: group[0]?.mode ?? null,
      completed: completed.length,
      expected_repetitions: contract?.repetitions ?? null,
      elapsed_ms: stats(completed.map((run) => run.elapsed_ms)),
      retained_peak_bytes: stats(completed.map((run) => run.retained_peak_bytes)),
      peak_cpu_bytes: stats(completed.map((run) => run.retained_peak_bytes)),
      browser_memory_peak_bytes: stats(
        completed.map((run) => run.browser_memory_peak_bytes)
      ),
      browser_process_tree_peak_working_set_bytes:
        memoryProbe?.browser_process_tree_peak_working_set_bytes ?? null,
      memory_probe_status: memoryProbe?.status ?? null,
      stages: [...stageNames].sort().map((name) => ({
        name,
        duration_ns: stats(completed.flatMap((run) =>
          run.finesse_stages.filter((stage) => stage.name === name).map((stage) => stage.duration_ns)
        )),
        invocation_count: stats(completed.flatMap((run) =>
          run.finesse_stages
            .filter((stage) => stage.name === name)
            .map((stage) => stage.invocation_count)
        )),
        work_item_count: stats(completed.flatMap((run) =>
          run.finesse_stages
            .filter((stage) => stage.name === name)
            .map((stage) => stage.work_item_count)
        )),
      })),
      result_identities: [...new Set(completed.map((run) =>
        `${run.unique_solution_count ?? 'none'}:${run.solution_hash ?? 'none'}:` +
          `${run.finesse_report_hash ?? 'off'}`
      ))],
      solution_identities: [...new Set(completed.map((run) =>
        `${run.unique_solution_count ?? 'none'}:${run.solution_hash ?? 'none'}`
      ))],
      finesse_report_hashes: [...new Set(completed.map((run) => run.finesse_report_hash))],
      witness_hashes: [...new Set(completed.map((run) => run.witness_hash))],
      fixed_score_contract_hashes: [...new Set(
        completed.map((run) => run.fixed_score_contract_hash).filter((value) => value !== null)
      )],
      runtime_environment_identities: [...new Set(
        completed.map((run) => run.runtime_environment_hash).filter((value) => value !== null)
      )],
    };
  });
}

async function probeCaseMemory(entries) {
  if (options['os-memory-probe'] === 'off') return [];
  const probes = [];
  for (const entry of entries) {
    const label = `${phase}-${entry.id}-memory`;
    const rawPath = resolve(reportRoot, `${label}.json`);
    const stderrPath = resolve(reportRoot, `${label}.stderr.txt`);
    process.stderr.write(`[finesse-benchmark] start ${label}\n`);
    const probe = normalize(
      await executeBrowser(entry, rawPath, stderrPath, true),
      entry,
      1,
      rawPath
    );
    probes.push(probe);
    process.stderr.write(
      `[finesse-benchmark] complete ${label} status=${probe.status} ` +
        `process_tree_peak=${probe.browser_process_tree_peak_working_set_bytes ?? 'unavailable'}\n`
    );
  }
  return probes;
}

function validateReport(report) {
  const expectedStages = expectedFinesseStages();
  const requiredActiveStages = expectedStages.slice(0, -1);
  const cases = report.aggregates.map((entry) => {
    const inputMode = entry.mode === 'inputs';
    const scoreMode = entry.mode === 'score';
    const finesseMode = inputMode || scoreMode;
    const caseRuns = report.runs.filter((run) => run.case === entry.case);
    const stageShapeValid = !finesseMode || caseRuns.every((run) =>
      run.status !== 'success' ||
      expectedStages.every((name) => run.finesse_stages.some((stage) => stage.name === name))
    );
    const stageTelemetryValid = !finesseMode || caseRuns.every((run) =>
      run.status !== 'success' ||
      expectedStages.every((name) => run.finesse_stages.some((stage) =>
        stage.name === name &&
        stage.duration_ns !== null &&
        stage.invocation_count !== null &&
        stage.work_item_count !== null
      ))
    );
    const activeStagesRecorded = !finesseMode || caseRuns.every((run) =>
      run.status !== 'success' ||
      (scoreMode ? expectedStages : requiredActiveStages).every((name) =>
        run.finesse_stages.some(
          (stage) => stage.name === name && Number(stage.invocation_count) > 0
        )
      )
    );
    const witnessStageRecorded =
      !['small-inputs', 'pattern-inputs'].includes(entry.case) ||
      caseRuns.every((run) =>
        run.status !== 'success' ||
        run.finesse_stages.some(
          (stage) => stage.name === 'finesse.witness' && Number(stage.invocation_count) > 0
        )
      );
    const adaptivePatternProductExercised = entry.case !== 'pattern-inputs' ||
      caseRuns.every((run) =>
        run.status !== 'success' ||
        (Number(run.materialized_pattern_count) >= 65 &&
          Number(run.source_sequence_length) > 7 &&
          run.finesse_report_complete === false &&
          Number(run.visible_information_penalty_inputs) > 0 &&
          run.witness_hash !== null)
      );
    const disabledStagesInactive =
      finesseMode ||
      caseRuns.every((run) =>
        run.status !== 'success' ||
        (report.metadata.phase === 'baseline' && run.finesse_stages.length === 0) ||
        expectedStages.every((name) => {
          const stage = run.finesse_stages.find((candidate) => candidate.name === name);
          return stage !== undefined && Number(stage.invocation_count) === 0;
        })
      );
    const repetitionsValid = entry.completed === entry.expected_repetitions;
    const identityStable = entry.result_identities.length === 1;
    const durationClassValid =
      entry.group === 'long'
        ? Number.isFinite(entry.elapsed_ms?.median) && entry.elapsed_ms.median > 1_000
        : Number.isFinite(entry.elapsed_ms?.median) && entry.elapsed_ms.median <= 1_000;
    const retainedPeakRecorded = Number.isFinite(entry.retained_peak_bytes?.max);
    const processTreePeakRecorded =
      (entry.memory_probe_status === 'success' &&
        Number.isFinite(entry.browser_process_tree_peak_working_set_bytes)) ||
      options['os-memory-probe'] === 'off';
    const runtimeEnvironmentStable =
      entry.runtime_environment_identities.length === 1;
    const finesseReportShapeValid = caseRuns.every((run) =>
      run.status !== 'success' ||
      (finesseMode
        ? run.finesse_report_hash !== null
        : run.finesse_report_hash === null)
    );
    const expectedScoreHash = sha256(stableJson(expectedFixedScoreContract()));
    const fixedScoreExact = !scoreMode || caseRuns.every((run) =>
      run.status !== 'success' ||
      (run.fixed_score_contract_hash === expectedScoreHash &&
        run.exact_total_inputs === null &&
        Number(run.materialized_pattern_count) === 65_536 &&
        run.finesse_report_complete === false &&
        run.witness_hash !== null)
    );
    const scoreRemainsSerial = !scoreMode || caseRuns.every((run) =>
      run.status !== 'success' || run.workers_used === 1
    );
    return {
      case: entry.case,
      mode: entry.mode,
      repetitions_valid: repetitionsValid,
      identity_stable: identityStable,
      fixed_finesse_stage_shape: stageShapeValid,
      finesse_stage_telemetry_complete: stageTelemetryValid,
      active_coordinator_finesse_stages_recorded: activeStagesRecorded,
      representative_witness_stage_recorded: witnessStageRecorded,
      adaptive_pattern_product_exercised: adaptivePatternProductExercised,
      disabled_finesse_stages_inactive: disabledStagesInactive,
      duration_class_valid: durationClassValid,
      retained_peak_recorded: retainedPeakRecorded,
      process_tree_peak_working_set_recorded: processTreePeakRecorded,
      runtime_environment_stable: runtimeEnvironmentStable,
      finesse_report_shape_valid: finesseReportShapeValid,
      fixed_score_exact_cost_witness_hash: fixedScoreExact,
      score_serial_execution: scoreRemainsSerial,
      pass:
        repetitionsValid &&
        identityStable &&
        stageShapeValid &&
        stageTelemetryValid &&
        activeStagesRecorded &&
        witnessStageRecorded &&
        adaptivePatternProductExercised &&
        disabledStagesInactive &&
        durationClassValid &&
        retainedPeakRecorded &&
        processTreePeakRecorded &&
        runtimeEnvironmentStable &&
        finesseReportShapeValid &&
        fixedScoreExact &&
        scoreRemainsSerial,
    };
  });
  return {
    expected_finesse_stages: expectedStages,
    cases,
    worker_invariance: report.worker_invariance,
    pass:
      cases.length > 0 &&
      cases.every((entry) => entry.pass) &&
      report.worker_invariance.pass,
  };
}

function compareReferences(baseline, finesseReference, candidate) {
  const candidateModes = new Set(candidate.aggregates.map((entry) => entry.mode));
  const offRequired = candidateModes.has('off');
  const finesseRequired = candidateModes.has('inputs') || candidateModes.has('score');
  const off = compareReferenceSet({
    name: 'baseline-head',
    reference: baseline,
    candidate,
    modes: new Set(['off']),
    regressionLimit: 0.02,
    required: offRequired,
    requireBaselineHead: true,
  });
  const finesse = compareReferenceSet({
    name: options['finesse-reference-kind'] ?? 'reference-or-previous-candidate',
    reference: finesseReference,
    candidate,
    modes: new Set(['inputs', 'score']),
    regressionLimit: candidate.metadata.acceptance_policy.finesse_median_regression_limit,
    required: finesseRequired,
    requireBaselineHead: false,
  });
  return {
    schema_version: 2,
    off_median_regression_limit: 0.02,
    finesse_median_regression_limit:
      candidate.metadata.acceptance_policy.finesse_median_regression_limit,
    memory_tradeoff: {
      maximum_speed_regression: 0.10,
      maximum_effective_peak_memory_ratio: 0.50,
      effective_peak_prefers_process_tree_working_set: true,
    },
    baseline_head: off,
    finesse_reference: finesse,
    pass: off.pass && finesse.pass,
  };
}

function compareReferenceSet({
  name,
  reference,
  candidate,
  modes,
  regressionLimit,
  required,
  requireBaselineHead,
}) {
  if (!required) {
    return { name, required: false, pass: true, environment: null, cases: [] };
  }
  if (reference === null) {
    return {
      name,
      required: true,
      pass: false,
      error: requireBaselineHead
        ? '--baseline-summary is required for selected off cases'
        : '--finesse-reference-summary is required for selected inputs/score cases',
      environment: null,
      cases: [],
    };
  }
  const environment = compareEnvironment(reference, candidate, modes);
  const referenceByCase = new Map(
    (reference.aggregates ?? []).map((entry) => [entry.case, entry])
  );
  const compared = candidate.aggregates
    .filter((entry) => modes.has(entry.mode))
    .map((entry) => compareAggregate(
      referenceByCase.get(entry.case),
      entry,
      regressionLimit
    ));
  const baselineHeadValid = !requireBaselineHead || (
    reference.metadata?.phase === 'baseline' &&
    reference.metadata?.source_tracked_dirty === false &&
    reference.metadata?.source_has_untracked_files === false &&
    typeof reference.metadata?.source_revision === 'string' &&
    reference.metadata.source_revision.length > 0
  );
  const baselineSourceDistinct = !requireBaselineHead || (
    typeof reference.metadata?.source_snapshot_sha256 === 'string' &&
    reference.metadata.source_snapshot_sha256 !== candidate.metadata.source_snapshot_sha256
  );
  const referenceValidationPassed = reference.validation?.pass === true;
  const referenceArtifactProvenanceValid =
    reference.metadata?.artifact_provenance?.valid === true;
  return {
    name,
    required: true,
    reference_phase: reference.metadata?.phase ?? null,
    reference_source_revision: reference.metadata?.source_revision ?? null,
    reference_source_snapshot_sha256: reference.metadata?.source_snapshot_sha256 ?? null,
    reference_validation_passed: referenceValidationPassed,
    reference_artifact_provenance_valid: referenceArtifactProvenanceValid,
    baseline_head_contract_valid: baselineHeadValid,
    baseline_source_distinct_from_candidate: baselineSourceDistinct,
    environment,
    cases: compared,
    pass:
      baselineHeadValid &&
      baselineSourceDistinct &&
      referenceValidationPassed &&
      referenceArtifactProvenanceValid &&
      environment.pass &&
      compared.length > 0 &&
      compared.every((entry) => entry.pass),
  };
}

function compareAggregate(reference, candidate, regressionLimit) {
  const referenceMedian = reference?.elapsed_ms?.median;
  const candidateMedian = candidate.elapsed_ms?.median;
  const regression = Number.isFinite(referenceMedian) && Number.isFinite(candidateMedian)
    ? candidateMedian / referenceMedian - 1
    : null;
  const referenceRetained =
    reference?.retained_peak_bytes?.max ?? reference?.peak_cpu_bytes?.max;
  const candidateRetained =
    candidate.retained_peak_bytes?.max ?? candidate.peak_cpu_bytes?.max;
  const retainedPeakRatio = ratio(referenceRetained, candidateRetained);
  const referenceWorkingSet = reference?.browser_process_tree_peak_working_set_bytes;
  const candidateWorkingSet = candidate.browser_process_tree_peak_working_set_bytes;
  const processTreePeakRatio = ratio(referenceWorkingSet, candidateWorkingSet);
  const effectivePeakRatio = processTreePeakRatio ?? retainedPeakRatio;
  const memoryTradeoffException =
    regression !== null &&
    effectivePeakRatio !== null &&
    regression <= 0.10 &&
    effectivePeakRatio <= 0.50;
  const solutionIdentityMatch = singletonEqual(
    reference?.solution_identities,
    candidate.solution_identities
  );
  const finesseReportHashMatch = candidate.mode === 'off'
    ? singletonEqual(reference?.result_identities, candidate.result_identities)
    : singletonEqual(reference?.finesse_report_hashes, candidate.finesse_report_hashes);
  const witnessHashMatch = candidate.mode !== 'score' || singletonEqual(
    reference?.witness_hashes,
    candidate.witness_hashes
  );
  const exactScoreContractMatch = candidate.mode !== 'score' || (
    singletonEqual(
      reference?.fixed_score_contract_hashes,
      candidate.fixed_score_contract_hashes
    ) &&
    candidate.fixed_score_contract_hashes?.[0] ===
      sha256(stableJson(expectedFixedScoreContract()))
  );
  // A solver that retains zero bytes in both runs has a valid measurement,
  // even though the mathematical 0/0 ratio is intentionally undefined. The
  // separate process-tree ratio remains the effective peak comparison.
  const retainedMeasurementsRecorded =
    Number.isFinite(referenceRetained) &&
    Number(referenceRetained) >= 0 &&
    Number.isFinite(candidateRetained) &&
    Number(candidateRetained) >= 0;
  const memoryMeasurementsComplete =
    retainedMeasurementsRecorded &&
    (options['os-memory-probe'] === 'off' || processTreePeakRatio !== null);
  return {
    case: candidate.case,
    mode: candidate.mode,
    reference_median_ms: referenceMedian ?? null,
    candidate_median_ms: candidateMedian ?? null,
    median_regression: regression,
    median_regression_limit: regressionLimit,
    reference_retained_peak_bytes: referenceRetained ?? null,
    candidate_retained_peak_bytes: candidateRetained ?? null,
    retained_peak_ratio: retainedPeakRatio,
    reference_process_tree_peak_working_set_bytes: referenceWorkingSet ?? null,
    candidate_process_tree_peak_working_set_bytes: candidateWorkingSet ?? null,
    process_tree_peak_working_set_ratio: processTreePeakRatio,
    effective_peak_memory_ratio: effectivePeakRatio,
    memory_tradeoff_exception: memoryTradeoffException,
    memory_measurements_complete: memoryMeasurementsComplete,
    solution_identity_match: solutionIdentityMatch,
    finesse_report_hash_match: finesseReportHashMatch,
    witness_hash_match: witnessHashMatch,
    exact_score_contract_match: exactScoreContractMatch,
    pass:
      regression !== null &&
      (regression <= regressionLimit || memoryTradeoffException) &&
      memoryMeasurementsComplete &&
      solutionIdentityMatch &&
      finesseReportHashMatch &&
      witnessHashMatch &&
      exactScoreContractMatch,
  };
}

function compareEnvironment(reference, candidate, modes) {
  const referenceCases = new Map(
    (reference.metadata?.cases ?? []).map((entry) => [entry.id, entry])
  );
  const candidateCases = candidate.metadata.cases.filter((entry) => modes.has(entry.mode));
  const commandsMatch = candidateCases.length > 0 && candidateCases.every((entry) =>
    referenceCases.get(entry.id)?.command === entry.command &&
    referenceCases.get(entry.id)?.repetitions === entry.repetitions &&
    referenceCases.get(entry.id)?.timeout === entry.timeout &&
    referenceCases.get(entry.id)?.workers === entry.workers
  );
  const runtimeEnvironmentsMatch = candidate.aggregates
    .filter((entry) => modes.has(entry.mode))
    .every((entry) => {
      const prior = (reference.aggregates ?? []).find((candidateEntry) =>
        candidateEntry.case === entry.case
      );
      return singletonEqual(
        prior?.runtime_environment_identities,
        entry.runtime_environment_identities
      );
    });
  const hostMatch = stableJson(reference.metadata?.host) === stableJson(candidate.metadata.host);
  const artifactToolchainMatch = stableJson(
    reference.metadata?.artifact_provenance?.toolchain
  ) === stableJson(candidate.metadata?.artifact_provenance?.toolchain);
  const artifactBuildOptionsMatch = stableJson(
    reference.metadata?.artifact_provenance?.build_options
  ) === stableJson(candidate.metadata?.artifact_provenance?.build_options);
  const browserBuildToolchainMatch = stableJson(
    reference.metadata?.artifact_provenance?.browser_build_toolchain
  ) === stableJson(candidate.metadata?.artifact_provenance?.browser_build_toolchain);
  const harnessMatch =
    reference.metadata?.harness_snapshot_sha256 === candidate.metadata.harness_snapshot_sha256;
  const workersMatch = reference.metadata?.workers === candidate.metadata.workers;
  const memoryProbeMatch =
    stableJson(reference.metadata?.memory_measurement) ===
      stableJson(candidate.metadata.memory_measurement);
  return {
    host_match: hostMatch,
    artifact_toolchain_match: artifactToolchainMatch,
    artifact_build_options_match: artifactBuildOptionsMatch,
    browser_build_toolchain_match: browserBuildToolchainMatch,
    harness_snapshot_match: harnessMatch,
    workers_match: workersMatch,
    memory_probe_contract_match: memoryProbeMatch,
    case_commands_and_repetitions_match: commandsMatch,
    browser_runtime_environment_match: runtimeEnvironmentsMatch,
    pass:
      hostMatch &&
      artifactToolchainMatch &&
      artifactBuildOptionsMatch &&
      browserBuildToolchainMatch &&
      harnessMatch &&
      workersMatch &&
      memoryProbeMatch &&
      commandsMatch &&
      runtimeEnvironmentsMatch,
  };
}

function stats(values) {
  const finite = values.filter(Number.isFinite).sort((left, right) => left - right);
  if (finite.length === 0) return null;
  const middle = Math.floor(finite.length / 2);
  const median = finite.length % 2 === 0
    ? (finite[middle - 1] + finite[middle]) / 2
    : finite[middle];
  return {
    min: finite[0],
    median,
    max: finite[finite.length - 1],
  };
}

function fixedScoreContractProjection(report) {
  if (!report || typeof report !== 'object') return null;
  const witness = report.representative_witness;
  return {
    mode: report.mode ?? null,
    metric: report.metric ?? null,
    pattern_knowledge: report.pattern_knowledge ?? null,
    complete: report.complete ?? null,
    exact_total_inputs: report.exact_total_inputs ?? null,
    representative_witness: witness && typeof witness === 'object'
      ? {
          policy: witness.policy ?? null,
          solution_key: witness.solution_key ?? null,
          queue: Array.isArray(witness.queue) ? witness.queue : null,
          total_inputs: witness.total_inputs ?? null,
          input_sequence: Array.isArray(witness.input_sequence)
            ? witness.input_sequence
            : null,
          placements: Array.isArray(witness.placements)
            ? witness.placements.map((placement) => ({
                piece: placement?.piece ?? null,
                rotation: placement?.rotation ?? null,
                x: placement?.x ?? null,
                y: placement?.y ?? null,
              }))
            : null,
        }
      : null,
  };
}

function expectedFixedScoreContract() {
  return {
    mode: 'score',
    metric: 'inputs',
    pattern_knowledge: 'both',
    complete: false,
    exact_total_inputs: null,
    representative_witness: {
      policy: 'oracle',
      solution_key: 'given-operation-sequence',
      queue: ['I', 'O', 'T', 'S', 'Z', 'J', 'L', 'I', 'O', 'T'],
      total_inputs: 20,
      input_sequence: [
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
        'das-left', 'hard-drop',
      ],
      placements: [
        { piece: 'I', rotation: 0, x: 0, y: 0 },
        { piece: 'O', rotation: 0, x: 0, y: 1 },
        { piece: 'T', rotation: 0, x: 0, y: 3 },
        { piece: 'S', rotation: 0, x: 0, y: 5 },
        { piece: 'Z', rotation: 0, x: 0, y: 7 },
        { piece: 'J', rotation: 0, x: 0, y: 9 },
        { piece: 'L', rotation: 0, x: 0, y: 11 },
        { piece: 'I', rotation: 0, x: 0, y: 13 },
        { piece: 'O', rotation: 0, x: 0, y: 14 },
        { piece: 'T', rotation: 0, x: 0, y: 16 },
      ],
    },
  };
}

function runtimeEnvironmentProjection(capabilities) {
  if (!capabilities || typeof capabilities !== 'object') return null;
  return {
    hardware_concurrency: capabilities.hardware_concurrency ?? null,
    cross_origin_isolated: capabilities.cross_origin_isolated ?? null,
    webgpu: capabilities.webgpu ?? null,
    user_agent: capabilities.user_agent ?? null,
  };
}

function expectedFinesseStages() {
  return [
    'finesse.geometry',
    'finesse.target_grouping',
    'finesse.movement_bfs',
    'finesse.annotation_prune',
    'finesse.product_dp',
    'finesse.aggregation',
    'finesse.witness',
  ];
}

function caseContract(entry) {
  return {
    id: entry.id,
    group: entry.group,
    mode: entry.mode,
    repetitions: entry.repetitions,
    timeout: entry.timeout,
    workers: entry.workers,
    command: entry.command,
  };
}

function ratio(reference, candidate) {
  return Number.isFinite(reference) && reference > 0 && Number.isFinite(candidate)
    ? candidate / reference
    : null;
}

function singletonEqual(left, right) {
  return Array.isArray(left) &&
    Array.isArray(right) &&
    left.length === 1 &&
    right.length === 1 &&
    left[0] === right[0];
}

function decimalOption(value, label) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0 || parsed > 1) {
    throw new Error(`${label} must be a decimal between 0 and 1`);
  }
  return parsed;
}

function loadSummary(path) {
  const summaryPath = resolve(path);
  const bytes = fs.readFileSync(summaryPath);
  const value = JSON.parse(bytes.toString('utf8'));
  if (!value || typeof value !== 'object' || !Array.isArray(value.aggregates)) {
    throw new Error(`invalid finesse benchmark summary: ${summaryPath}`);
  }
  return { path: summaryPath, sha256: sha256(bytes), report: value };
}

function referenceDescriptor(reference) {
  if (reference === null) return null;
  return {
    path: reference.path,
    sha256: reference.sha256,
    phase: reference.report.metadata?.phase ?? null,
    source_revision: reference.report.metadata?.source_revision ?? null,
    source_snapshot_sha256: reference.report.metadata?.source_snapshot_sha256 ?? null,
    wasm_sha256: reference.report.metadata?.wasm_sha256 ?? null,
    artifact_provenance_valid:
      reference.report.metadata?.artifact_provenance?.valid === true,
  };
}

async function gitSourceState(root) {
  const revision = await runProcess('git', ['rev-parse', 'HEAD'], root);
  const unstaged = await runProcess(
    'git',
    ['diff', '--quiet', '--ignore-submodules', '--'],
    root
  );
  const staged = await runProcess(
    'git',
    ['diff', '--cached', '--quiet', '--ignore-submodules', '--'],
    root
  );
  const untracked = await runProcess(
    'git',
    ['ls-files', '--others', '--exclude-standard'],
    root
  );
  return {
    revision: revision.code === 0 ? revision.stdout.trim() : null,
    tracked_dirty:
      [0, 1].includes(unstaged.code) && [0, 1].includes(staged.code)
        ? unstaged.code === 1 || staged.code === 1
        : null,
    has_untracked_files: untracked.code === 0
      ? untracked.stdout.trim().length > 0
      : null,
  };
}

async function runProcess(command, args, cwd) {
  return await new Promise((resolveProcess) => {
    let stdout = '';
    const child = spawn(command, args, {
      cwd,
      stdio: ['ignore', 'pipe', 'ignore'],
      windowsHide: true,
    });
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.once('error', () => resolveProcess({ code: -1, stdout: '' }));
    child.once('exit', (code) => resolveProcess({ code: code ?? -1, stdout }));
  });
}

function snapshotFiles(root, paths) {
  const files = paths.map((path) => {
    const sourcePath = resolve(root, path);
    if (!fs.existsSync(sourcePath)) return { path, present: false };
    const bytes = fs.readFileSync(sourcePath);
    return { path, present: true, sha256: sha256(bytes), size: bytes.byteLength };
  });
  const hash = createHash('sha256');
  for (const file of files) {
    hash.update(`${file.path}\0${file.present}\0${file.sha256 ?? ''}\0${file.size ?? ''}\n`);
  }
  return { digest: hash.digest('hex'), files };
}

function artifactProvenance(root, snapshot) {
  const path = resolve(root, 'clearra-finesse-build-provenance.json');
  if (!fs.existsSync(path)) return { valid: false, reason: 'provenance-sidecar-missing' };
  let value;
  try {
    value = JSON.parse(fs.readFileSync(path, 'utf8'));
  } catch {
    return { valid: false, reason: 'provenance-sidecar-invalid-json' };
  }
  const actualWasmSha256 = wasmDigest(root);
  const actualBindingsSha256 = bindingsDigest(root);
  const producerSha256 = sha256(
    fs.readFileSync(resolve(repository, 'scripts/tools/build-clearra-wasm.mjs'))
  );
  const snapshotToolSha256 = sha256(
    fs.readFileSync(resolve(repository, 'scripts/benchmark/finesse-source-snapshot.mjs'))
  );
  const packagerSha256 = sha256(
    fs.readFileSync(resolve(
      repository,
      'scripts/benchmark/wasm-product-browser/vite.config.mjs'
    ))
  );
  const browserBuildToolchain = browserBuildToolchainIdentity(repository);
  const validHash = (candidate) => typeof candidate === 'string' && /^[0-9a-f]{64}$/.test(candidate);
  const validToolchain = (toolchain) => toolchain !== null && typeof toolchain === 'object' &&
    (toolchain.environment === 'native' || toolchain.environment === 'wsl') &&
    typeof toolchain.rustc === 'string' && toolchain.rustc.length > 0 &&
    typeof toolchain.cargo === 'string' && toolchain.cargo.length > 0 &&
    typeof toolchain.wasm_bindgen === 'string' && toolchain.wasm_bindgen.length > 0;
  const reason = value?.schema_version !== 1
    ? 'provenance-schema-mismatch'
    : value.source_snapshot_sha256 !== snapshot.digest
      ? 'provenance-source-snapshot-mismatch'
      : value.source_file_count !== snapshot.files.length
        ? 'provenance-source-file-count-mismatch'
        : !validHash(value.wasm_sha256) || !validHash(actualWasmSha256)
          ? 'provenance-wasm-hash-missing'
          : value.wasm_sha256 !== actualWasmSha256
            ? 'provenance-wasm-hash-mismatch'
            : !validHash(value.bindings_sha256) || !validHash(actualBindingsSha256)
              ? 'provenance-bindings-hash-missing'
              : value.bindings_sha256 !== actualBindingsSha256
                ? 'provenance-bindings-hash-mismatch'
                : value.producer_sha256 !== producerSha256
                  ? 'provenance-producer-hash-mismatch'
                  : value.snapshot_tool_sha256 !== snapshotToolSha256
                    ? 'provenance-snapshot-tool-hash-mismatch'
                    : value.packager_sha256 !== packagerSha256
                      ? 'provenance-packager-hash-mismatch'
                      : stableJson(value.browser_build_toolchain) !==
                          stableJson(browserBuildToolchain)
                        ? 'provenance-browser-toolchain-mismatch'
                        : !validToolchain(value.toolchain)
                          ? 'provenance-toolchain-missing'
                          : value?.build_options?.stage_profiling !== true
                            ? 'provenance-stage-profiling-disabled'
                            : value?.build_options?.environment !== value.toolchain.environment
                              ? 'provenance-build-environment-mismatch'
            : null;
  return {
    valid: reason === null,
    reason,
    sidecar: path,
    source_snapshot_sha256: value?.source_snapshot_sha256 ?? null,
    source_file_count: value?.source_file_count ?? null,
    declared_wasm_sha256: value?.wasm_sha256 ?? null,
    actual_wasm_sha256: actualWasmSha256,
    declared_bindings_sha256: value?.bindings_sha256 ?? null,
    actual_bindings_sha256: actualBindingsSha256,
    declared_producer_sha256: value?.producer_sha256 ?? null,
    actual_producer_sha256: producerSha256,
    declared_snapshot_tool_sha256: value?.snapshot_tool_sha256 ?? null,
    actual_snapshot_tool_sha256: snapshotToolSha256,
    declared_packager_sha256: value?.packager_sha256 ?? null,
    actual_packager_sha256: packagerSha256,
    toolchain: value?.toolchain ?? null,
    browser_build_toolchain: value?.browser_build_toolchain ?? null,
    build_options: value?.build_options ?? null,
  };
}

function browserBuildToolchainIdentity(root) {
  const lockStatus = commandOutput(
    'git',
    ['status', '--porcelain', '--untracked-files=no', '--', 'package-lock.json'],
    root
  );
  if (lockStatus.length > 0) {
    throw new Error('package-lock.json must be clean for a provenance-bound benchmark run');
  }
  return {
    package_manager: 'npm',
    package_lock_git_oid: commandOutput(
      'git',
      ['rev-parse', 'HEAD:package-lock.json'],
      root
    ),
    npm: npmCommandOutput(['--version'], root),
    vite: viteCliVersion(root),
  };
}

function viteCliVersion(root) {
  const output = npmCommandOutput(['exec', '--', 'vite', '--version'], root);
  return /^vite\/([^\s]+)/.exec(output)?.[1] ?? output;
}

function npmCommandOutput(args, cwd) {
  if (process.platform === 'win32') {
    return commandOutput(
      process.env.ComSpec || 'cmd.exe',
      ['/d', '/s', '/c', ['npm', ...args].join(' ')],
      cwd
    );
  }
  return commandOutput('npm', args, cwd);
}

function commandOutput(command, args, cwd) {
  return execFileSync(command, args, {
    cwd,
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
}

function wasmDigest(root) {
  const manifestPath = resolve(root, 'wasm/clearra_wasm.manifest.json');
  if (fs.existsSync(manifestPath)) {
    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    const wasm = resolve(root, 'wasm', manifest?.wasm?.path ?? '');
    return fs.existsSync(wasm) ? sha256(fs.readFileSync(wasm)) : null;
  }
  const direct = resolve(root, 'wasm/clearra_wasm_bg.wasm');
  return fs.existsSync(direct) ? sha256(fs.readFileSync(direct)) : null;
}

function bindingsDigest(root) {
  const manifestPath = resolve(root, 'wasm/clearra_wasm.manifest.json');
  if (fs.existsSync(manifestPath)) {
    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    const bindings = resolve(root, 'wasm', manifest?.bindings?.path ?? '');
    return fs.existsSync(bindings) ? sha256(fs.readFileSync(bindings)) : null;
  }
  const direct = resolve(root, 'wasm/clearra_wasm.js');
  return fs.existsSync(direct) ? sha256(fs.readFileSync(direct)) : null;
}

function stableJson(value) {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${stableJson(value[key])}`
    ).join(',')}}`;
  }
  return JSON.stringify(value);
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function finiteOrNull(value) {
  const number = Number(value);
  return Number.isFinite(number) && number >= 0 ? number : null;
}

function parseArgs(args) {
  const parsed = {};
  const booleanKeys = new Set([
    'help',
    'h',
    'self-test',
    'dry-run',
    'allow-workspace-report',
  ]);
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === '-h') {
      parsed.h = 'true';
      continue;
    }
    if (!argument?.startsWith('--')) {
      throw new Error(`invalid argument ${argument ?? ''}`);
    }
    const equals = argument.indexOf('=');
    if (equals !== -1) {
      parsed[argument.slice(2, equals)] = argument.slice(equals + 1);
      continue;
    }
    const key = argument.slice(2);
    const next = args[index + 1];
    if (booleanKeys.has(key) && (next === undefined || next.startsWith('--'))) {
      parsed[key] = 'true';
      continue;
    }
    if (next === undefined || next.startsWith('--')) {
      throw new Error(`--${key} requires a value`);
    }
    parsed[key] = next;
    index += 1;
  }
  return parsed;
}

function optionEnabled(value) {
  return value === true || ['true', '1', 'yes', 'on'].includes(String(value).toLowerCase());
}

function usage() {
  return `Clearra finesse benchmark gate

Usage:
  node scripts/benchmark/run-finesse-benchmark.mjs --help
  node scripts/benchmark/run-finesse-benchmark.mjs --self-test
  node scripts/benchmark/run-finesse-benchmark.mjs --dry-run --phase candidate
  node scripts/benchmark/run-finesse-benchmark.mjs \\
    --phase <baseline|reference|candidate> --source-root <source> \\
    --browser-root <built-browser> --report <outside-repository-directory> [options]

Phases:
  baseline   Clean HEAD worktree (tracked and untracked); off small 5x/long 2x.
  reference  Finesse reference/previous-candidate snapshot; runs inputs and score.
  candidate  Runs off, inputs, and score. Requires both summaries for acceptance:
             --baseline-summary <baseline-finesse-summary.json>
             --finesse-reference-summary <reference-finesse-summary.json>

Key options:
  --workers <n>                    Default logical processors minus one; hard-capped.
  --cases <small,long,score|ids>   Default small,long,score.
  --modes <off,inputs,score>       Phase-specific defaults shown above.
  --worker-invariance off          Explicitly skip the 1/multi search and score gate.
  --os-memory-probe off            Skip separate process-tree WorkingSet probe.
  --finesse-regression-limit <n>   Default 0.02; max 0.10 with <=0.50 peak memory.
  --finesse-reference-kind <kind>  reference or previous-candidate (report label).
  --require-clean-source false     Diagnostic escape hatch for baseline only.

The report keeps solver-retained peak bytes and browser process-tree peak Working
Set as separate measurements. Build the supplied --browser-root from the supplied
--source-root; source revision, source snapshot, harness, WASM, host, browser,
commands, repetitions, workers, and memory-probe contract are recorded.
Keep CARGO_TARGET_DIR and browser build output outside every source worktree.
`;
}

function runSelfTests() {
  assert.equal(stableJson({ b: 1, a: [2] }), '{"a":[2],"b":1}');
  assert.deepEqual(stats([3, 1, 2, Number.NaN]), { min: 1, median: 2, max: 3 });
  assert.deepEqual(parseArgs(['--help']), { help: 'true' });
  assert.deepEqual(parseArgs(['--phase=candidate', '--workers', '2']), {
    phase: 'candidate',
    workers: '2',
  });
  const exactScore = expectedFixedScoreContract();
  assert.deepEqual(fixedScoreContractProjection({
    ...exactScore,
    representative_witness: {
      ...exactScore.representative_witness,
      pattern_ids: [0],
    },
    policy_results: [{ policy: 'oracle' }],
  }), exactScore);
  const aggregateBase = {
    case: 'small-inputs',
    mode: 'inputs',
    elapsed_ms: { median: 100 },
    retained_peak_bytes: { max: 1000 },
    browser_process_tree_peak_working_set_bytes: 2000,
    solution_identities: ['1:solution'],
    result_identities: ['1:solution:report'],
    finesse_report_hashes: ['report'],
    witness_hashes: ['witness'],
    fixed_score_contract_hashes: [],
  };
  assert.equal(compareAggregate(aggregateBase, { ...aggregateBase }, 0.02).pass, true);
  const zeroRetained = compareAggregate(
    {
      ...aggregateBase,
      retained_peak_bytes: { max: 0 },
    },
    {
      ...aggregateBase,
      retained_peak_bytes: { max: 0 },
    },
    0.02
  );
  assert.equal(zeroRetained.retained_peak_ratio, null);
  assert.equal(zeroRetained.process_tree_peak_working_set_ratio, 1);
  assert.equal(zeroRetained.memory_measurements_complete, true);
  assert.equal(zeroRetained.pass, true);
  const memoryTradeoff = compareAggregate(
    aggregateBase,
    {
      ...aggregateBase,
      elapsed_ms: { median: 109 },
      retained_peak_bytes: { max: 500 },
      browser_process_tree_peak_working_set_bytes: 1000,
    },
    0.02
  );
  assert.equal(memoryTradeoff.memory_tradeoff_exception, true);
  assert.equal(memoryTradeoff.pass, true);
  assert.equal(compareAggregate(
    aggregateBase,
    { ...aggregateBase, elapsed_ms: { median: 111 } },
    0.02
  ).pass, false);
  assert.equal(compareAggregate(
    aggregateBase,
    { ...aggregateBase, finesse_report_hashes: ['changed'] },
    0.02
  ).pass, false);
  const environmentMetadata = {
    workers: 2,
    harness_snapshot_sha256: 'harness',
    artifact_provenance: {
      toolchain: { environment: 'native', rustc: 'r', cargo: 'c', wasm_bindgen: 'w' },
      browser_build_toolchain: {
        package_manager: 'npm',
        package_lock_git_oid: 'lock',
        npm: '1',
        vite: '1',
      },
      build_options: { environment: 'native', stage_profiling: true },
    },
    host: { platform: 'test', architecture: 'x64', logical_processors: 2, node: 'v1' },
    memory_measurement: { enabled: true, os_sample_interval_ms: 250 },
    cases: [{
      id: 'small-inputs',
      mode: 'inputs',
      command: 'clearra example --workers 2',
      repetitions: 5,
      timeout: 100,
      workers: 2,
    }],
  };
  const environmentReport = {
    metadata: environmentMetadata,
    aggregates: [{
      case: 'small-inputs',
      mode: 'inputs',
      runtime_environment_identities: ['browser'],
    }],
  };
  assert.equal(compareEnvironment(
    environmentReport,
    environmentReport,
    new Set(['inputs'])
  ).pass, true);
  assert.equal(compareEnvironment(
    environmentReport,
    {
      ...environmentReport,
      metadata: { ...environmentMetadata, workers: 3 },
    },
    new Set(['inputs'])
  ).pass, false);
  assert.equal(compareEnvironment(
    environmentReport,
    {
      ...environmentReport,
      metadata: {
        ...environmentMetadata,
        artifact_provenance: {
          ...environmentMetadata.artifact_provenance,
          browser_build_toolchain: {
            ...environmentMetadata.artifact_provenance.browser_build_toolchain,
            vite: '2',
          },
        },
      },
    },
    new Set(['inputs'])
  ).pass, false);
  const expectedScoreHash = sha256(stableJson(exactScore));
  const scoreRun = {
    case: 'pattern-score',
    status: 'success',
    finesse_stages: expectedFinesseStages().map((name) => ({
      name,
      duration_ns: 1,
      invocation_count: 1,
      work_item_count: 1,
    })),
    finesse_report_hash: 'full-report',
    fixed_score_contract_hash: expectedScoreHash,
    exact_total_inputs: null,
    materialized_pattern_count: 65_536,
    finesse_report_complete: false,
    witness_hash: 'witness',
    workers_used: 1,
  };
  const scoreAggregate = {
    case: 'pattern-score',
    group: 'score',
    mode: 'score',
    completed: 1,
    expected_repetitions: 1,
    elapsed_ms: { median: 1 },
    retained_peak_bytes: { max: 1 },
    memory_probe_status: 'success',
    browser_process_tree_peak_working_set_bytes: 1,
    result_identities: ['score-result'],
    runtime_environment_identities: ['browser'],
  };
  assert.equal(validateReport({
    metadata: { phase: 'candidate' },
    runs: [scoreRun],
    aggregates: [scoreAggregate],
    worker_invariance: { pass: true },
  }).pass, true);
}

function required(values, key) {
  const value = values[key];
  if (!value) throw new Error(`--${key} is required`);
  return value;
}

function positiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return parsed;
}

function isWithin(parent, candidate) {
  const normalizedParent = resolve(parent).toLocaleLowerCase();
  const normalizedCandidate = resolve(candidate).toLocaleLowerCase();
  return (
    normalizedCandidate === normalizedParent ||
    normalizedCandidate.startsWith(`${normalizedParent}${sep}`)
  );
}

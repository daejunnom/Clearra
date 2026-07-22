import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repository = resolve(scriptDirectory, '../..');
const options = parseArgs(process.argv.slice(2));
const reportRoot = resolve(options.report);
const repetitions = positiveInteger(options.repetitions ?? '3', 'repetitions');
const workers = positiveInteger(
  options.workers ?? String(Math.max(1, os.availableParallelism() - 1)),
  'workers'
);
const workBudget = positiveInteger(options['work-budget'] ?? '8192', 'work-budget');
const browserRoot = resolve(options['browser-root']);
const bindings = resolve(
  repository,
  options.bindings ?? 'apps/clearra-web/static/wasm/clearra_wasm.js'
);
const wasmPath = resolve(dirname(bindings), 'clearra_wasm_bg.wasm');
const sourceTreeSha256 = options['source-tree-sha256'] ?? 'not-supplied';

const problems = [
  {
    name: 'p7p3',
    sourcePieces: 10,
    expectedCount: 208_437,
    expectedHash: 'cts1:938c9254b965d33b',
    expectedResolution: 'projected-terminal-lookahead'
  },
  {
    name: 'p7p4',
    sourcePieces: 11,
    expectedCount: 456_923,
    expectedHash: 'cts1:98ebe8726537b29f',
    expectedResolution: 'materialized-source'
  }
];
const modes = [
  { name: 'cpu-1', backend: 'cpu', workers: 1, multi: false },
  { name: 'cpu-multi', backend: 'cpu', workers, multi: true },
  { name: 'gpu-1', backend: 'gpu', workers: 1, multi: false },
  { name: 'gpu-multi', backend: 'gpu', workers, multi: true }
];
const surfaces = ['local', 'browser'];

fs.mkdirSync(reportRoot, { recursive: true });
const metadata = {
  schema_version: 1,
  generated_at: new Date().toISOString(),
  source_tree_sha256: sourceTreeSha256,
  wasm_sha256: sha256(wasmPath),
  wasm_bytes: fs.statSync(wasmPath).size,
  logical_processors: os.availableParallelism(),
  requested_multi_workers: workers,
  repetitions,
  work_budget: workBudget,
  expected: problems
};
fs.writeFileSync(resolve(reportRoot, 'metadata.json'), JSON.stringify(metadata, null, 2));

const runsPath = resolve(reportRoot, 'runs.ndjson');
const runs = readExistingRuns(runsPath);
const completedRuns = new Set(runs.map(runKey));
for (const problem of problems) {
  for (const surface of surfaces) {
    for (const mode of modes) {
      for (let repetition = 1; repetition <= repetitions; repetition += 1) {
        const label = `${problem.name}-${surface}-${mode.name}-r${repetition}`;
        const key = runKey({ problem: problem.name, surface, mode: mode.name, repetition });
        if (completedRuns.has(key)) {
          process.stderr.write(`[clearra-benchmark] keep ${label}\n`);
          continue;
        }
        process.stderr.write(`[clearra-benchmark] start ${label}\n`);
        const command = commandFor(problem, mode);
        const rawPath = resolve(reportRoot, `${label}.json`);
        const stderrPath = resolve(reportRoot, `${label}.stderr.txt`);
        const raw = await execute(surface, mode, command, rawPath, stderrPath);
        const run = normalize(raw, { problem, surface, mode, repetition, command, rawPath });
        validate(run, problem, surface, mode);
        runs.push(run);
        fs.appendFileSync(runsPath, `${JSON.stringify(run)}\n`);
        completedRuns.add(key);
        process.stderr.write(
          `[clearra-benchmark] pass ${label} elapsed_ms=${run.elapsed_ms} count=${run.solution_count ?? 'unsupported'}\n`
        );
      }
    }
  }
}

const report = { metadata, runs, aggregates: aggregateRuns(runs) };
fs.writeFileSync(resolve(reportRoot, 'summary.json'), JSON.stringify(report, null, 2));
fs.writeFileSync(resolve(reportRoot, 'summary.csv'), toCsv(runs));
console.log(JSON.stringify(report, null, 2));

function commandFor(problem, mode) {
  const warmup = mode.backend === 'gpu' ? '--gpu-warmup' : '--cpu-warmup';
  const fallback = mode.backend === 'gpu' ? '--no-backend-fallback' : '';
  return [
    'clearra pc',
    '--lines 4',
    '--count unique',
    `--source-pieces ${problem.sourcePieces}`,
    `--workers ${mode.workers}`,
    '--max-patterns 5040',
    '--max-candidates 100000000',
    `--backend ${mode.backend}`,
    warmup,
    fallback
  ].filter(Boolean).join(' ');
}

async function execute(surface, mode, command, rawPath, stderrPath) {
  let script;
  let args;
  if (surface === 'browser') {
    script = resolve(repository, 'scripts/tools/run-wasm-browser-benchmark.mjs');
    args = [script, '--root', browserRoot, '--command', command, '--timeout', '1800000'];
  } else if (mode.multi) {
    script = resolve(repository, 'scripts/tools/wasm-pc-distributed-environment-probe.mjs');
    args = [script, bindings, command, String(workBudget)];
  } else {
    script = resolve(repository, 'scripts/tools/wasm-pc-environment-probe.mjs');
    args = [script, bindings, command, String(workBudget), 'summary'];
  }

  const stdout = fs.openSync(rawPath, 'w');
  const stderr = fs.openSync(stderrPath, 'w');
  const exitCode = await new Promise((resolveExit, rejectExit) => {
    const child = spawn(process.execPath, args, {
      cwd: repository,
      stdio: ['ignore', stdout, stderr],
      windowsHide: true
    });
    child.once('error', rejectExit);
    child.once('exit', (code) => resolveExit(code ?? 1));
  }).finally(() => {
    fs.closeSync(stdout);
    fs.closeSync(stderr);
  });
  if (exitCode !== 0) {
    throw new Error(`benchmark child exited ${exitCode}; see ${stderrPath}`);
  }
  return JSON.parse(fs.readFileSync(rawPath, 'utf8'));
}

function normalize(raw, context) {
  const event = context.surface === 'browser' ? raw.event : raw.final_event;
  const direct = context.surface === 'local' && !context.mode.multi ? raw.final : null;
  const search = direct ?? event?.search_report ?? null;
  const response = event?.response ?? null;
  const backendReport = response?.backend_report ?? null;
  const webgpu = event?.webgpu_backend ?? null;
  return {
    problem: context.problem.name,
    surface: context.surface,
    mode: context.mode.name,
    repetition: context.repetition,
    command: context.command,
    raw_file: context.rawPath,
    status: direct?.status ?? response?.status ?? null,
    diagnostics: direct?.diagnostics ?? response?.diagnostics ?? [],
    elapsed_ms: raw.search_elapsed_ms ?? raw.elapsed_ms ?? null,
    load_elapsed_ms: raw.load_elapsed_ms ?? null,
    prepare_elapsed_ms: raw.prepare_elapsed_ms ?? null,
    distributed_mode: raw.distributed_mode ?? null,
    actual_backend: search?.backend_selected ?? backendReport?.backend_selected ?? null,
    workers_used: search?.workers_used ?? null,
    cpu_parallel_execution: search?.cpu_parallel_execution ?? null,
    fallback_used: backendReport?.fallback_used ?? webgpu?.fallback_used ?? false,
    gpu_trust_state: webgpu?.gpu_trust_state ?? null,
    gpu_warmup_performed: webgpu?.gpu_warmup_performed ?? null,
    gpu_session_reused: webgpu?.gpu_session_reused ?? null,
    supply_window_resolution: search?.supply_window_resolution ?? null,
    projects_unplaced_lookahead: search?.projects_unplaced_lookahead ?? null,
    source_sequence_length: search?.source_sequence_length ?? null,
    solution_count: search?.unique_solution_count ?? null,
    solution_hash: search?.normalized_solution_set_hash ?? null,
    count_complete: search?.count_complete ?? null,
    probability_complete: search?.probability_complete ?? null,
    resource_truncated: search?.resource_truncated ?? null,
    peak_cpu_bytes: search?.peak_cpu_bytes ?? null,
    searched_nodes: search?.searched_nodes ?? null,
    peak_frontier_states: search?.peak_frontier_states ?? null,
    local_gpu_capability_unavailable:
      context.surface === 'local' &&
      context.mode.backend === 'gpu' &&
      (direct?.status ?? response?.status) === 'unsupported'
  };
}

function validate(run, problem, surface, mode) {
  if (run.local_gpu_capability_unavailable) {
    const explicit = run.diagnostics.some(
      (diagnostic) => diagnostic.code === 'E_PRODUCT_RUNTIME_UNSUPPORTED'
    );
    if (!explicit) throw new Error(`${run.problem}/${run.mode}: local GPU failure was not explicit`);
    return;
  }
  requireEqual(run.status, 'success', run, 'status');
  requireEqual(run.solution_count, problem.expectedCount, run, 'solution_count');
  requireEqual(run.solution_hash, problem.expectedHash, run, 'solution_hash');
  requireEqual(run.count_complete, true, run, 'count_complete');
  requireEqual(run.resource_truncated, false, run, 'resource_truncated');
  requireEqual(run.source_sequence_length, problem.sourcePieces, run, 'source_sequence_length');
  requireEqual(
    run.supply_window_resolution,
    problem.expectedResolution,
    run,
    'supply_window_resolution'
  );
  requireEqual(
    run.projects_unplaced_lookahead,
    problem.sourcePieces === 10,
    run,
    'projects_unplaced_lookahead'
  );
  requireEqual(run.actual_backend, mode.backend === 'gpu' ? 'webgpu' : 'wasm-cpu', run, 'actual_backend');
  requireEqual(run.fallback_used, false, run, 'fallback_used');
  requireEqual(run.workers_used, mode.workers, run, 'workers_used');
  requireEqual(run.cpu_parallel_execution, mode.multi, run, 'cpu_parallel_execution');
  if (surface === 'browser' && mode.backend === 'gpu') {
    if (!String(run.gpu_trust_state).startsWith('Trusted')) {
      throw new Error(`${run.problem}/${run.mode}: untrusted GPU result ${run.gpu_trust_state}`);
    }
  }
}

function requireEqual(actual, expected, run, field) {
  if (actual !== expected) {
    throw new Error(
      `${run.problem}/${run.surface}/${run.mode}/r${run.repetition}: ${field}=${actual}, expected=${expected}`
    );
  }
}

function aggregateRuns(runs) {
  const groups = new Map();
  for (const run of runs) {
    const key = `${run.problem}/${run.surface}/${run.mode}`;
    const group = groups.get(key) ?? [];
    group.push(run);
    groups.set(key, group);
  }
  return [...groups.entries()].map(([key, group]) => {
    const elapsed = group
      .filter((run) => !run.local_gpu_capability_unavailable)
      .map((run) => run.elapsed_ms)
      .filter(Number.isFinite)
      .sort((left, right) => left - right);
    return {
      key,
      repetitions: group.length,
      status: elapsed.length === 0 ? 'capability-unavailable' : 'measured',
      min_elapsed_ms: elapsed[0] ?? null,
      median_elapsed_ms: elapsed.length === 0 ? null : elapsed[Math.floor(elapsed.length / 2)],
      max_elapsed_ms: elapsed.at(-1) ?? null,
      solution_count: group[0].solution_count,
      solution_hash: group[0].solution_hash
    };
  });
}

function readExistingRuns(path) {
  if (!fs.existsSync(path)) return [];
  return fs.readFileSync(path, 'utf8')
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function runKey(run) {
  return `${run.problem}/${run.surface}/${run.mode}/${run.repetition}`;
}

function toCsv(runs) {
  const columns = [
    'problem', 'surface', 'mode', 'repetition', 'status', 'elapsed_ms', 'actual_backend',
    'workers_used', 'cpu_parallel_execution', 'solution_count', 'solution_hash',
    'supply_window_resolution', 'source_sequence_length', 'count_complete',
    'resource_truncated', 'peak_cpu_bytes', 'searched_nodes', 'peak_frontier_states',
    'gpu_trust_state', 'gpu_warmup_performed', 'gpu_session_reused'
  ];
  const rows = runs.map((run) => columns.map((column) => csvCell(run[column])).join(','));
  return `${columns.join(',')}\n${rows.join('\n')}\n`;
}

function csvCell(value) {
  const text = value == null ? '' : String(value);
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function sha256(path) {
  return createHash('sha256').update(fs.readFileSync(path)).digest('hex');
}

function parseArgs(args) {
  const parsed = {};
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index];
    const value = args[index + 1];
    if (!key?.startsWith('--') || value === undefined) {
      throw new Error(`invalid argument: ${key ?? ''}`);
    }
    parsed[key.slice(2)] = value;
  }
  if (!parsed.report || !parsed['browser-root']) {
    throw new Error('--report and --browser-root are required');
  }
  return parsed;
}

function positiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return parsed;
}

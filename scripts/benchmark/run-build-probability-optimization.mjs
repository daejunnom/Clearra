import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repository = resolve(scriptDirectory, '../..');
const options = parseArgs(process.argv.slice(2));
const reportRoot = resolve(required(options, 'report'));
const browserRoot = resolve(required(options, 'browser-root'));
const browserEntry = resolve(browserRoot, 'index.html');
if (!fs.existsSync(browserEntry) || !fs.statSync(browserEntry).isFile()) {
  throw new Error(
    `browser benchmark root must be a built harness containing index.html: ${browserEntry}`
  );
}
const phase = options.phase ?? 'baseline';
const repetitions = positiveInteger(options.repetitions ?? '2', 'repetitions');
const workers = positiveInteger(
  options.workers ?? String(Math.max(1, os.availableParallelism() - 1)),
  'workers'
);
const selectedCases = new Set((options.cases ?? 'pco,tsar,large').split(','));
const smallTimeout = positiveInteger(options['small-timeout'] ?? '300000', 'small-timeout');
const largeTimeout = positiveInteger(options['large-timeout'] ?? '900000', 'large-timeout');

const cases = [
  {
    id: 'pco',
    size: 'small',
    timeout: smallTimeout,
    command:
      `clearra pc --board-mask 0x000000e0f87e3f87 --height 4 --pieces 4 ` +
      `--hold I --count all --max-patterns 840 --max-candidates 100000000 ` +
      `--backend cpu --workers ${workers} --cpu-warmup`
  },
  {
    id: 'tsar',
    size: 'small',
    timeout: smallTimeout,
    command:
      `clearra pc --board-mask 0x000300c0399e3fdf --height 5 --pieces 6 ` +
      `--hold empty --count all --max-patterns 5040 --max-candidates 100000000 ` +
      `--backend cpu --workers ${workers} --cpu-warmup`
  },
  {
    id: 'p7-not-t-4-build',
    size: 'large',
    timeout: largeTimeout,
    command:
      `clearra build-probability --base-mask 0x0000000000000000 ` +
      `--target-mask 0x000318e3fdffffff --height 5 --hold empty ` +
      `--patterns P7[^T]4 --aggregate buildability --rule srs-plus ` +
      `--include-mirror --workers ${workers} --cpu-warmup`
  }
].filter((entry) => selectedCases.has(entry.id) || selectedCases.has(entry.size));

fs.mkdirSync(reportRoot, { recursive: true });
const sourceSnapshot = sourceSnapshotFor([
  'crates/clearra-coverage/src/pattern/weighted_pattern_set.rs',
  'crates/clearra-supply/src/queue/queue_pattern_expression.rs',
  'crates/clearra-supply/src/pattern_universe/materialized_pattern_universe.rs',
  'crates/clearra-supply/src/pattern_universe/pattern_universe_materializer.rs',
  'crates/clearra-supply/src/pattern_universe/piece_multiset_group.rs',
  'crates/clearra-supply/src/pattern_universe/pattern_piece_position_index.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/build_probability_distributed.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_component.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_dense.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/extended_inverse_catalog.rs',
  'apps/clearra-web/src/workers/DistributedWasmJobRunner.ts',
  'apps/clearra-web/src/workers/ClearraVerifierPool.ts',
  'apps/clearra-web/src/workers/clearraVerifierWorker.ts',
  'scripts/benchmark/wasm-product-browser/benchmark-client.ts',
  'scripts/tools/run-wasm-browser-benchmark.mjs'
]);
const wasmPath = resolve(browserRoot, 'wasm/clearra_wasm_bg.wasm');
const metadata = {
  schema_version: 1,
  phase,
  generated_at: new Date().toISOString(),
  workers,
  repetitions,
  source_snapshot_sha256: sourceSnapshot.digest,
  source_files: sourceSnapshot.files,
  wasm_sha256: fs.existsSync(wasmPath) ? sha256(fs.readFileSync(wasmPath)) : null,
  cases
};
fs.writeFileSync(resolve(reportRoot, `${phase}-metadata.json`), JSON.stringify(metadata, null, 2));

const runs = [];
for (const benchmarkCase of cases) {
  for (let repetition = 1; repetition <= repetitions; repetition += 1) {
    const label = `${phase}-${benchmarkCase.id}-r${repetition}`;
    process.stderr.write(`[build-probability-benchmark] start ${label}\n`);
    const rawPath = resolve(reportRoot, `${label}.json`);
    const stderrPath = resolve(reportRoot, `${label}.stderr.txt`);
    const raw = await executeBrowser(benchmarkCase, rawPath, stderrPath);
    const normalized = normalize(raw, benchmarkCase, repetition, rawPath);
    runs.push(normalized);
    fs.appendFileSync(resolve(reportRoot, `${phase}-runs.ndjson`), `${JSON.stringify(normalized)}\n`);
    process.stderr.write(
      `[build-probability-benchmark] complete ${label} status=${normalized.status} ` +
      `elapsed_ms=${normalized.elapsed_ms ?? 'censored'} memory=${normalized.peak_cpu_bytes ?? 'unknown'}\n`
    );
  }
}

const report = { metadata, runs, aggregates: aggregate(runs) };
fs.writeFileSync(resolve(reportRoot, `${phase}-summary.json`), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));

async function executeBrowser(benchmarkCase, rawPath, stderrPath) {
  const runner = resolve(repository, 'scripts/tools/run-wasm-browser-benchmark.mjs');
  const args = [
    runner,
    '--root',
    browserRoot,
    '--command',
    benchmarkCase.command,
    '--timeout',
    String(benchmarkCase.timeout)
  ];
  const stdout = fs.openSync(rawPath, 'w');
  const stderr = fs.openSync(stderrPath, 'w');
  const code = await new Promise((resolveExit, rejectExit) => {
    const child = spawn(process.execPath, args, {
      cwd: repository,
      stdio: ['ignore', stdout, stderr],
      windowsHide: true
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
      stderr: fs.readFileSync(stderrPath, 'utf8')
    };
  }
  return JSON.parse(fs.readFileSync(rawPath, 'utf8'));
}

function normalize(raw, benchmarkCase, repetition, rawPath) {
  if (raw.timed_out) {
    const telemetry = raw.last_progress?.event?.progress?.telemetry ?? null;
    return {
      phase,
      case: benchmarkCase.id,
      size: benchmarkCase.size,
      repetition,
      status: 'timeout',
      elapsed_ms: null,
      timeout_ms: raw.timeout_ms,
      browser_memory_peak_bytes: raw.last_progress?.browser_memory_peak_bytes ?? null,
      peak_cpu_bytes: null,
      telemetry,
      progress_samples: compactProgressSamples(raw.benchmark_progress_samples),
      raw_file: rawPath
    };
  }
  if (raw.runner_failed) {
    return {
      phase,
      case: benchmarkCase.id,
      size: benchmarkCase.size,
      repetition,
      status: 'runner-failed',
      elapsed_ms: null,
      peak_cpu_bytes: null,
      raw_file: rawPath,
      error: raw.stderr
    };
  }
  const event = raw.event ?? {};
  const response = event.response ?? {};
  const search = event.search_report ?? {};
  const resources = response.resource_report ?? {};
  return {
    phase,
    case: benchmarkCase.id,
    size: benchmarkCase.size,
    repetition,
    status: response.status ?? event.event ?? 'unknown',
    elapsed_ms: raw.elapsed_ms ?? null,
    browser_memory_peak_bytes: raw.browser_memory_peak_bytes ?? null,
    browser_memory_sample_count: raw.browser_memory_sample_count ?? 0,
    peak_cpu_bytes: search.peak_cpu_bytes ?? resources.peak_cpu_bytes ?? null,
    workers_used: search.workers_used ?? null,
    unique_solution_count: search.unique_solution_count ?? null,
    solution_hash: search.normalized_solution_set_hash ?? null,
    packing_candidate_count: search.packing_candidate_count ?? null,
    build_variant_count: search.build_variant_count ?? null,
    covered_pattern_count: search.covered_pattern_count ?? null,
    materialized_pattern_count: search.materialized_pattern_count ?? null,
    coverage_probability: search.coverage_probability ?? null,
    count_complete: search.count_complete ?? resources.count_complete ?? null,
    probability_complete: search.probability_complete ?? resources.probability_complete ?? null,
    resource_truncated: search.resource_truncated ?? resources.truncated ?? null,
    searched_nodes: search.searched_nodes ?? null,
    geometry_nodes: search.geometry_searched_nodes ?? search.searched_nodes ?? null,
    buildup_nodes: search.buildup_searched_nodes ?? null,
    progress_samples: compactProgressSamples(raw.benchmark_progress_samples),
    raw_file: rawPath
  };
}

function compactProgressSamples(samples) {
  if (!Array.isArray(samples)) return [];
  return samples.map((sample) => {
    const telemetry = sample?.event?.progress?.telemetry ?? {};
    return {
      elapsed_ms: sample?.elapsed_ms ?? null,
      browser_memory_peak_bytes: sample?.browser_memory_peak_bytes ?? null,
      phase: telemetry.phase ?? null,
      producer_complete: telemetry.producer_complete ?? null,
      geometry_nodes: telemetry.geometry_nodes ?? null,
      candidates_emitted: telemetry.candidates_emitted ?? null,
      candidates_verified: telemetry.candidates_verified ?? null,
      active_workers: telemetry.active_workers ?? null,
      worker_count: telemetry.worker_count ?? null,
      oldest_batch_ms: telemetry.oldest_batch_ms ?? null,
      pass_index: telemetry.pass_index ?? null,
      pass_count: telemetry.pass_count ?? null
    };
  });
}

function aggregate(runs) {
  const groups = new Map();
  for (const run of runs) {
    const group = groups.get(run.case) ?? [];
    group.push(run);
    groups.set(run.case, group);
  }
  return [...groups.entries()].map(([id, group]) => {
    const completed = group.filter((run) => run.status === 'success');
    return {
      case: id,
      completed: completed.length,
      timed_out: group.filter((run) => run.status === 'timeout').length,
      elapsed_ms: stats(completed.map((run) => run.elapsed_ms)),
      peak_cpu_bytes: stats(completed.map((run) => run.peak_cpu_bytes)),
      browser_memory_peak_bytes: stats(
        group.map((run) => run.browser_memory_peak_bytes).filter(Number.isFinite)
      ),
      result_identities: [...new Set(completed.map((run) =>
        `${run.unique_solution_count ?? 'none'}:${run.solution_hash ?? 'none'}`
      ))]
    };
  });
}

function stats(values) {
  const finite = values.filter(Number.isFinite).sort((left, right) => left - right);
  if (finite.length === 0) return null;
  return {
    min: finite[0],
    median: finite[Math.floor(finite.length / 2)],
    max: finite[finite.length - 1]
  };
}

function sourceSnapshotFor(paths) {
  const files = paths.map((path) => {
    const bytes = fs.readFileSync(resolve(repository, path));
    return { path, sha256: sha256(bytes), size: bytes.byteLength };
  });
  const hash = createHash('sha256');
  for (const file of files) hash.update(`${file.path}\0${file.sha256}\0${file.size}\n`);
  return { digest: hash.digest('hex'), files };
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function parseArgs(args) {
  const parsed = {};
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index];
    const value = args[index + 1];
    if (!key?.startsWith('--') || value === undefined) throw new Error(`invalid argument ${key}`);
    parsed[key.slice(2)] = value;
  }
  return parsed;
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

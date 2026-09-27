// Benchmarks an existing native Clearra binary. It never invokes Cargo,
// downloads assets, or writes outside the declared local benchmark root.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';
import {
  PAIRS, abbaSchedule, assertIsolatedAcceleratorCounters, classifyExecution,
  assertWorkerExecution, commandArgs, semanticIdentity,
  summarizeSamples, validateCases, sampleFitsRemainingLease,
} from './v081-accelerator-abba-core.mjs';
import { PRIVATE_WORKER_MEMORY_SCOPE, readWorkerMemoryAccounting } from './v081-worker-memory-accounting.mjs';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const fixturePath = resolve(repository, 'scripts/benchmark/fixtures/v081-accelerator-cases.json');
const outputParent = resolve(repository, '_local/artifacts/v081-accelerator-abba');
const MAX_STDOUT = 8 * 1024 * 1024;
const MAX_STDERR = 128 * 1024;
const MAX_LEASE_MS = 7_200_000;

try {
  await main(process.argv.slice(2));
} catch (error) {
  process.stderr.write(`[v081-accelerator-abba] ${error.message}\n`);
  process.exitCode = 1;
}

async function main(argv) {
  const runStarted = performance.now();
  const options = parseOptions(argv);
  if (process.env.CLEARRA_RUNTIME_SUPERVISED !== '1' ||
      process.env.CLEARRA_RUNTIME_PROFILE !== 'benchmark-search') {
    throw new Error('run under clearra-manage runtime run --producer benchmark --profile benchmark-search');
  }
  const entry = validateCases(JSON.parse(fs.readFileSync(fixturePath, 'utf8')))
    .find(candidate => candidate.id === options.case);
  if (!entry) throw new Error(`unknown benchmark case: ${options.case}`);
  if (!Object.hasOwn(PAIRS, options.pair)) throw new Error('pair must be legal, conditioned or combined');
  if (options.workers > os.availableParallelism() && !options.allowOversubscription) {
    throw new Error('requested workers exceed visible processors; use --allow-oversubscription explicitly');
  }
  // Preserve the 1800-second sample contract. Pre/postflight work consumes
  // the outer lease; admission below stops before a sample would overrun it.
  if (4 * options.rounds * options.timeoutMs > MAX_LEASE_MS) {
    throw new Error('worst-case run exceeds the supervised 120-minute lease; use smaller batches');
  }
  const binary = fs.realpathSync(options.binary);
  if (!['clearra', 'clearra.exe'].includes(binary.split(/[\\/]/).at(-1)) ||
      !fs.statSync(binary).isFile()) {
    throw new Error('--binary must name an existing clearra executable');
  }
  const roots = {
    legal: checkedExistingDirectory(options.legalRoot),
    conditioned: checkedExistingDirectory(options.conditionedRoot),
  };
  const env = {
    ...process.env,
    CLEARRA_LEGAL_BOARD_DIRECTORY: roots.legal,
    CLEARRA_CONDITIONED_REACHABILITY_DIRECTORY: roots.conditioned,
    // These arms compare accelerators, not ambient local-only memo layouts.
    CLEARRA_STANDARD_BAG_MEMO: 'reference',
    CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: 'flat',
  };
  const binarySha256 = await sha256File(binary);
  const fixtureSha256 = await sha256File(fixturePath);
  await verifyAcceleratorExecutionBackend(binary, env, entry.profile);
  const assets = await verifyInstalledAssets(binary, env, entry.profile);
  const report = options.outputRoot
    ? prepareManagedReportDirectory(options.runId, options.outputRoot, options.manager)
    : prepareReportDirectory(options.runId);
  const manifest = {
    schema: 'clearra.v081.accelerator-abba-receipt.v2',
    generated_at: new Date().toISOString(),
    case: entry,
    pair: options.pair,
    workers_requested: options.workers,
    logical_processors_visible: os.availableParallelism(),
    oversubscribed: options.workers > os.availableParallelism(),
    rounds_requested: options.rounds,
    timeout_ms_per_sample: options.timeoutMs,
    outer_lease_ms: MAX_LEASE_MS,
    postflight_reserve_ms: 120_000,
    executable_sha256: binarySha256,
    fixture_sha256: fixtureSha256,
    assets,
    timing_scope: 'fresh-native-process-start-through-exit',
    stage_profiling: 'not-enabled-by-this-runner',
    validated_cli_output_retained: true,
    process_tree_peak_memory: {
      metric: 'working_set_bytes',
      method: process.platform === 'win32' ? 'sampled-process-tree-working-set' : 'unsupported-platform',
      interval_ms: process.platform === 'win32' ? 250 : null,
      scope: 'timed-solver-child-and-descendants-per-sample',
      observer: process.platform === 'win32' ? 'PowerShell CIM process-tree sampler; included symmetrically in timed wall duration' : 'none',
    },
    accelerator_backend_preflight: 'wasm-cpu',
    memo_selection: { storage: 'reference', layout: 'flat' },
    worker_memory_accounting: {
      scope: PRIVATE_WORKER_MEMORY_SCOPE,
      shared_owner: 'one-request-immutable-standard-bag-owner',
      nested_memo: 'included-in-private-standard-bag-bytes',
      absence: 'no-native-parallel-worker-exit-snapshot-not-zero-memory',
    },
  };
  fs.writeFileSync(join(report, 'manifest.json'), JSON.stringify(manifest, null, 2), { flag: 'wx' });
  const samples = [];
  let expectedIdentity = null;
  let expectedRuntime = null;
  let failure = null;
  for (const slot of abbaSchedule(options.rounds)) {
    if (!sampleFitsRemainingLease(Math.ceil(performance.now() - runStarted),
      options.timeoutMs, MAX_LEASE_MS, 120_000)) {
      failure = 'outer-lease-budget-exhausted-before-sample-admission';
      break;
    }
    const args = commandArgs(entry, options.pair, slot.arm, options.workers);
    const execution = await invoke(binary, args, env, options.timeoutMs, {
      memoryProbeRoot: report,
      memoryProbeId: `${slot.round}-${slot.slot}-${slot.arm}`,
    });
    const classification = classifyExecution(execution);
    const sample = {
      ...slot,
      state: classification.state,
      reason: classification.reason ?? null,
      wall_ms: classification.state === 'complete' ? execution.wallMs : null,
      exit_code: execution.exitCode,
      semantic_identity: null,
      runtime_identity: null,
      logical_peak_cpu_bytes: null,
      reported_workers_used: null,
      requested_workers: options.workers,
      cpu_parallel_execution: null,
      cpu_parallel_decision_reason: null,
      parallel_active_workers: null,
      process_tree_peak_working_set_bytes: execution.processTreePeakWorkingSetBytes,
      process_tree_memory_probe: execution.processTreeMemoryProbe,
      process_tree_memory_sample_interval_ms: execution.processTreeMemorySampleIntervalMs,
      peak_memory_bytes: execution.processTreePeakWorkingSetBytes,
      command_args: args,
      conditioned_complete_hits: null,
      conditioned_misses: null,
      legal_board_verified_negative_prunes: null,
      worker_memory_accounting: null,
      worker_memory_accounting_unavailable: null,
    };
    try {
      if (sample.state === 'complete') {
        const value = JSON.parse(execution.stdout);
        sample.semantic_identity = semanticIdentity(value, entry);
        sample.runtime_identity = value.runtime_identity ?? null;
        if (!sample.runtime_identity || typeof sample.runtime_identity !== 'object') {
          throw new Error('native CLI did not report an authoritative runtime identity');
        }
        sample.logical_peak_cpu_bytes = scalarOrNull(value.summary.resource_peak_cpu_bytes);
        sample.reported_workers_used = nonnegativeCounterOrNull(value.summary.workers_used);
        sample.cpu_parallel_execution = booleanOrNull(value.summary.cpu_parallel_execution);
        sample.cpu_parallel_decision_reason = value.summary.cpu_parallel_decision_reason ?? null;
        sample.parallel_active_workers = nonnegativeCounterOrNull(value.summary.parallel_active_workers);
        if (value.summary.worker_retained_accounting_scope !== undefined) {
          sample.worker_memory_accounting = readWorkerMemoryAccounting(value.summary);
        } else {
          sample.worker_memory_accounting_unavailable = 'no-native-parallel-worker-exit-snapshot';
        }
        sample.conditioned_complete_hits = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_complete_hits);
        sample.conditioned_misses = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_misses);
        sample.conditioned_reachability_requested = booleanOrNull(value.summary.conditioned_reachability_requested);
        sample.conditioned_reachability_policy_enabled = booleanOrNull(value.summary.conditioned_reachability_policy_enabled);
        sample.conditioned_reachability_snapshot_active = booleanOrNull(value.summary.conditioned_reachability_snapshot_active);
        sample.conditioned_lookup_attempts = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_lookup_attempts);
        sample.conditioned_empty_entry_sets = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_empty_entry_sets);
        sample.conditioned_out_of_scope = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_out_of_scope);
        sample.conditioned_unknown = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_unknown);
        sample.conditioned_snapshot_mismatch = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_snapshot_mismatch);
        sample.conditioned_invalid_asset = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_invalid_asset);
        sample.conditioned_no_query_context = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_no_query_context);
        sample.conditioned_cache_short_circuits = nonnegativeCounterOrNull(
          value.summary.reachability_conditioned_cache_short_circuits);
        sample.legal_board_verified_negative_prunes = nonnegativeCounterOrNull(
          value.summary.legal_board_verified_negative_prunes);
        assertIsolatedAcceleratorCounters(sample, options.pair, slot.arm,
          entry.args[0] === 'pc', entry.id.startsWith('pc-p7p4-'));
        assertWorkerExecution(sample, options.workers, entry.args[0] === 'pc');
        expectedIdentity ??= sample.semantic_identity;
        expectedRuntime ??= sample.runtime_identity;
        assert.deepEqual(sample.semantic_identity, expectedIdentity,
          'candidate/result/coverage identity changed between A/B samples');
        assert.deepEqual(sample.runtime_identity, expectedRuntime,
          'runtime identity changed within one same-binary benchmark');
        const outputName = `cli-${slot.round}-${slot.slot}-${slot.arm}.json`;
        fs.writeFileSync(join(report, outputName), execution.stdout, { flag: 'wx' });
        sample.validated_cli_output = outputName;
      }
    } catch (error) {
      sample.state = 'invalid';
      sample.reason = error.message;
      sample.wall_ms = null;
      failure = error.message;
    }
    samples.push(sample);
    fs.appendFileSync(join(report, 'samples.ndjson'), `${JSON.stringify(sample)}\n`);
    process.stderr.write(`[v081-accelerator-abba] ${slot.round}.${slot.slot} ${slot.arm} ${sample.state}\n`);
    // A censored arm cannot establish an ABBA speed ratio. Keep its evidence
    // and stop rather than spending another full lease on the same failure.
    if (sample.state !== 'complete') {
      failure ??= `sample ${slot.round}.${slot.slot} was ${sample.state}: ${sample.reason}`;
      break;
    }
  }
  if (await sha256File(binary) !== binarySha256) failure ??= 'binary changed during A/B';
  try {
    assert.deepEqual(await verifyInstalledAssets(binary, env, entry.profile), assets,
      'installed asset generation changed during A/B');
  } catch (error) {
    failure ??= `asset postflight failed: ${error.message}`;
  }
  const summary = {
    manifest,
    samples,
    aggregate: summarizeSamples(samples),
    semantic_identity: expectedIdentity,
    runtime_identity: expectedRuntime,
    result: failure ? 'invalid-or-censored' : 'complete-local-batch',
    failure,
    // Release gates also require a separately measured BuildUp p95, physical
    // shared peak, and full canonical-order parity. Never infer them here.
    release_gate: 'not-evaluated',
  };
  fs.writeFileSync(join(report, 'summary.json'), JSON.stringify(summary, null, 2), { flag: 'wx' });
  process.stdout.write(`${JSON.stringify({ report, result: summary.result, aggregate: summary.aggregate })}\n`);
  if (failure) process.exitCode = 1;
}

function parseOptions(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith('--') || value === undefined || Object.hasOwn(options, key)) {
      throw new Error('expected distinct --option value pairs');
    }
    options[key] = value;
  }
  const allowed = new Set([
    '--binary', '--case', '--pair', '--workers', '--rounds', '--timeout-ms',
    '--run-id', '--legal-root', '--conditioned-root', '--allow-oversubscription',
    '--output-root', '--manager',
  ]);
  if (Object.keys(options).some(key => !allowed.has(key))) throw new Error('unknown benchmark option');
  if (options['--allow-oversubscription'] !== undefined &&
      !['true', 'false'].includes(options['--allow-oversubscription'])) {
    throw new Error('--allow-oversubscription must be true or false');
  }
  const runId = options['--run-id'];
  if (!/^[a-z0-9][a-z0-9-]{0,47}$/.test(runId ?? '')) {
    throw new Error('--run-id must be a new lowercase, path-safe identifier');
  }
  return {
    binary: requiredAbsolute(options['--binary'], '--binary'),
    case: options['--case'],
    pair: options['--pair'],
    workers: positiveInteger(options['--workers'], '--workers'),
    rounds: positiveInteger(options['--rounds'] ?? '2', '--rounds'),
    timeoutMs: positiveInteger(options['--timeout-ms'] ?? '300000', '--timeout-ms'),
    runId,
    legalRoot: requiredAbsolute(options['--legal-root'], '--legal-root'),
    conditionedRoot: requiredAbsolute(options['--conditioned-root'], '--conditioned-root'),
    outputRoot: options['--output-root'] === undefined ? null : requiredAbsolute(options['--output-root'], '--output-root'),
    manager: options['--output-root'] === undefined ? null : requiredAbsolute(options['--manager'], '--manager'),
    allowOversubscription: options['--allow-oversubscription'] === 'true',
  };
}

function positiveInteger(value, label) {
  if (!/^[1-9][0-9]*$/.test(value ?? '')) throw new Error(`${label} must be a positive integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new Error(`${label} is too large`);
  return parsed;
}

function requiredAbsolute(value, label) {
  if (typeof value !== 'string' || !isAbsolute(value)) throw new Error(`${label} needs an absolute path`);
  return value;
}

function checkedExistingDirectory(path) {
  const physical = fs.realpathSync(path);
  if (!fs.statSync(physical).isDirectory()) throw new Error('asset root must be an existing directory');
  return physical;
}

// A retained implementation worktree may no longer own a declared artifact
// root in the current supervisor policy. Use its existing verification API;
// do not relax that policy or force unmanaged output for a benchmark.
function prepareManagedReportDirectory(runId, parent, manager) {
  const report = resolve(parent, runId);
  const verify = () => {
    const checked = spawnSync(manager, ['storage', 'verify', '--path', report], {
      cwd: process.cwd(), encoding: 'utf8', timeout: 30_000, windowsHide: true,
    });
    if (checked.status !== 0) throw new Error('benchmark output was rejected by the current Rust storage authority');
  };
  verify();
  if (fs.existsSync(report)) throw new Error('benchmark report already exists; use a new run ID');
  fs.mkdirSync(parent, { recursive: true });
  fs.mkdirSync(report);
  verify();
  return report;
}

function prepareReportDirectory(runId) {
  const local = resolve(repository, '_local');
  const artifacts = resolve(repository, '_local/artifacts');
  if (!fs.existsSync(local)) fs.mkdirSync(local);
  if (!fs.statSync(local).isDirectory() || fs.lstatSync(local).isSymbolicLink() ||
      fs.realpathSync(local) !== resolve(fs.realpathSync(repository), '_local')) {
    throw new Error('the repository-owned _local directory must not be linked');
  }
  if (!fs.existsSync(artifacts)) fs.mkdirSync(artifacts);
  if (!fs.statSync(artifacts).isDirectory() || fs.lstatSync(artifacts).isSymbolicLink() ||
      fs.realpathSync(artifacts) !== resolve(fs.realpathSync(repository), '_local/artifacts')) {
    throw new Error('the repository-owned _local/artifacts root must not be linked');
  }
  fs.mkdirSync(outputParent, { recursive: true });
  if (fs.lstatSync(outputParent).isSymbolicLink()) throw new Error('benchmark output root is linked');
  const report = join(outputParent, runId);
  fs.mkdirSync(report);
  return report;
}

async function verifyInstalledAssets(binary, env, profile) {
  const result = {};
  for (const [name, command] of [['legal', 'legal-board'], ['conditioned', 'reachability-pack']]) {
    const execution = await invoke(binary,
      ['--format', 'json', command, 'status', '--profile', profile], env, 60_000);
    if (execution.exitCode !== 0 || execution.timedOut || execution.overflow) {
      throw new Error(`${command} status preflight failed without downloading`);
    }
    const status = JSON.parse(execution.stdout);
    if (status.installed !== true || status.qualified !== true ||
        status.validation !== 'ready' || !status.installed_generation_identity ||
        !status.catalog_identity || !Number.isSafeInteger(status.installed_payload_bytes)) {
      throw new Error(`${command} ${profile} must already be installed and qualified`);
    }
    result[name] = {
      catalog_identity: status.catalog_identity,
      generation_identity: status.installed_generation_identity,
      payload_bytes: status.installed_payload_bytes,
    };
  }
  return result;
}

function scalarOrNull(value) {
  return ['string', 'number', 'boolean'].includes(typeof value) ? value : null;
}

function booleanOrNull(value) {
  if (value === true || value === 'true') return true;
  if (value === false || value === 'false') return false;
  return null;
}

function nonnegativeCounterOrNull(value) {
  if (!/^(0|[1-9][0-9]*)$/.test(String(value ?? ''))) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

async function verifyAcceleratorExecutionBackend(binary, env, profile) {
  // A native-core-only CLI accepts the same switches but does not run either
  // accelerator. This untimed one-piece query prevents a false A/B no-op.
  const execution = await invoke(binary, [
    '--format', 'json', '--lang', 'en', 'pc', '--board-mask', '0x3f',
    '--height', '1', '--pieces', '1', '--lines', '1', '--queue', 'I',
    '--hold', 'empty', '--count', 'unique', '--backend', 'cpu',
    '--no-tablebase', '--no-legal-board', '--no-conditioned-reachability',
    '--rule', profile, '--workers', '1',
  ], env, 60_000);
  let selectedBackend = null;
  if (execution.exitCode === 0 && !execution.timedOut && !execution.overflow) {
    try { selectedBackend = JSON.parse(execution.stdout).summary?.backend_selected; }
    catch { /* report the original CLI output below */ }
  }
  if (execution.exitCode !== 0 || execution.timedOut || execution.overflow ||
      selectedBackend !== 'wasm-cpu') {
    const detail = [execution.stdout, execution.stderr].filter(Boolean).join(' ').trim().slice(0, 512);
    throw new Error(`existing CLI binary must use the accelerator-owning WASM CPU backend` +
      ` (exit=${execution.exitCode}, timeout=${execution.timedOut}, overflow=${execution.overflow},` +
      ` selected=${selectedBackend ?? 'none'})${detail ? `: ${detail}` : ''}`);
  }
}

async function sha256File(path) {
  const hash = createHash('sha256');
  for await (const chunk of fs.createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

async function invoke(binary, args, env, timeoutMs, memoryProbeOptions = null) {
  const started = performance.now();
  return new Promise((resolveResult, rejectResult) => {
    const child = spawn(binary, args, { cwd: repository, env, windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'] });
    const memoryProbe = memoryProbeOptions
      ? startProcessTreeMemoryProbe(child.pid, memoryProbeOptions)
      : null;
    let stdout = '';
    let stderr = '';
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let overflow = false;
    let timedOut = false;
    const timeout = setTimeout(() => { timedOut = true; child.kill(); }, timeoutMs);
    child.stdout.on('data', chunk => {
      stdoutBytes += chunk.length;
      if (stdoutBytes > MAX_STDOUT) {
        overflow = true;
        child.kill();
      } else stdout += chunk.toString('utf8');
    });
    child.stderr.on('data', chunk => {
      stderrBytes += chunk.length;
      if (stderrBytes > MAX_STDERR) { overflow = true; child.kill(); }
      else stderr += chunk.toString('utf8');
    });
    child.once('error', error => { clearTimeout(timeout); rejectResult(error); });
    child.once('close', code => {
      clearTimeout(timeout);
      void finishProcessTreeMemoryProbe(memoryProbe).then(memory => {
        resolveResult({ exitCode: code, stdout, stderr, timedOut, overflow,
          wallMs: performance.now() - started,
          processTreePeakWorkingSetBytes: memory.peakBytes,
          processTreeMemoryProbe: memory.status,
          processTreeMemorySampleIntervalMs: memory.intervalMs });
      }, rejectResult);
    });
  });
}

function startProcessTreeMemoryProbe(rootPid, options) {
  if (process.platform !== 'win32' || !Number.isSafeInteger(rootPid) || rootPid <= 0) {
    return { unsupported: true, intervalMs: null };
  }
  const intervalMs = 250;
  const outputPath = join(options.memoryProbeRoot, `memory-${options.memoryProbeId}.txt`);
  const stopPath = join(options.memoryProbeRoot, `memory-${options.memoryProbeId}.stop`);
  const source = String.raw`
$ErrorActionPreference = 'Stop'
$rootProcessId = [uint32]$env:CLEARRA_MEMORY_PROBE_ROOT_PID
$stopPath = $env:CLEARRA_MEMORY_PROBE_STOP_PATH
$outputPath = $env:CLEARRA_MEMORY_PROBE_OUTPUT_PATH
$intervalMs = [int]$env:CLEARRA_MEMORY_PROBE_INTERVAL_MS
$peakBytes = [long]0
try {
  while (-not (Test-Path -LiteralPath $stopPath)) {
    $rows = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,WorkingSetSize)
    $ids = [System.Collections.Generic.HashSet[uint32]]::new()
    [void]$ids.Add($rootProcessId)
    do {
      $changed = $false
      foreach ($row in $rows) {
        if ($ids.Contains([uint32]$row.ParentProcessId) -and $ids.Add([uint32]$row.ProcessId)) { $changed = $true }
      }
    } while ($changed)
    $workingSetBytes = [long]0
    foreach ($row in $rows) { if ($ids.Contains([uint32]$row.ProcessId)) { $workingSetBytes += [long]$row.WorkingSetSize } }
    if ($workingSetBytes -gt $peakBytes) { $peakBytes = $workingSetBytes }
    Start-Sleep -Milliseconds $intervalMs
  }
} finally { [System.IO.File]::WriteAllText($outputPath, [string]$peakBytes) }
`;
  const child = spawn('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand',
    Buffer.from(source, 'utf16le').toString('base64')], {
    stdio: 'ignore', windowsHide: true,
    env: { ...process.env, CLEARRA_MEMORY_PROBE_ROOT_PID: String(rootPid),
      CLEARRA_MEMORY_PROBE_STOP_PATH: stopPath, CLEARRA_MEMORY_PROBE_OUTPUT_PATH: outputPath,
      CLEARRA_MEMORY_PROBE_INTERVAL_MS: String(intervalMs) },
  });
  return { child, outputPath, stopPath, intervalMs };
}

async function finishProcessTreeMemoryProbe(probe) {
  if (!probe) return { peakBytes: null, status: 'not-requested', intervalMs: null };
  if (probe.unsupported) return { peakBytes: null, status: 'unsupported-platform', intervalMs: null };
  await fs.promises.writeFile(probe.stopPath, '', 'utf8');
  const exited = probe.child.exitCode === null
    ? new Promise(resolveExit => probe.child.once('exit', resolveExit))
    : Promise.resolve(probe.child.exitCode);
  const completed = await Promise.race([exited.then(() => true),
    new Promise(resolveWait => setTimeout(() => resolveWait(false), 5_000))]);
  if (!completed) probe.child.kill('SIGKILL');
  let peakBytes = null;
  try {
    const parsed = Number.parseInt(await fs.promises.readFile(probe.outputPath, 'utf8'), 10);
    if (Number.isSafeInteger(parsed) && parsed > 0) peakBytes = parsed;
  } catch { /* explicitly report a missing sample */ }
  return { peakBytes, status: peakBytes === null ? 'no-sample' : 'sampled-windows-process-tree-working-set',
    intervalMs: probe.intervalMs };
}

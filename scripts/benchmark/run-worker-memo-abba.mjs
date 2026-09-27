// Local-only same-binary storage A/B. The supervisor owns each timed solver
// tree independently; no build, download, asset regeneration or deployment.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import { basename, isAbsolute, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { abbaSchedule, assertWorkerExecution, semanticIdentity, sampleFitsRemainingLease } from './v081-accelerator-abba-core.mjs';
import { PRIVATE_WORKER_MEMORY_SCOPE, readWorkerMemoryAccounting } from './v081-worker-memory-accounting.mjs';
import { assertProductMemoCaseSelection, productMemoAbbaEnvironment, productMemoAbbaSelection, workerMemoBenchmarkCase } from './v081-product-memo-abba-core.mjs';

const BYTE_LIMIT = 8 * 1024 * 1024;
try { await main(process.argv.slice(2)); } catch (error) {
  process.stderr.write(`[worker-memo-abba] ${error.message}\n`);
  process.exitCode = 1;
}

async function main(argv) {
  const started = performance.now();
  // The orchestrator has a separate finite lease. Every solver below acquires
  // the memory-intensive slot itself, never competing with its own parent.
  if (process.env.CLEARRA_RUNTIME_SUPERVISED !== '1' ||
      process.env.CLEARRA_RUNTIME_PROFILE !== 'local-service') {
    throw new Error('the finite orchestrator requires runtime run --profile local-service; samples use benchmark-search');
  }
  const options = {};
  const allowed = new Set(['--binary', '--manager', '--owner-root', '--source-root', '--output', '--workers', '--timeout-seconds', '--candidate', '--case', '--rounds']);
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i], value = argv[i + 1];
    if (!allowed.has(key) || value === undefined || Object.hasOwn(options, key)) throw new Error('distinct known option/value pairs required');
    options[key] = value;
  }
  for (const key of ['--binary', '--manager', '--owner-root', '--source-root', '--output']) {
    if (!isAbsolute(options[key] ?? '')) throw new Error(`${key} requires an absolute path`);
  }
  const workers = Number(options['--workers'] ?? '11');
  const timeout = Number(options['--timeout-seconds'] ?? '1800');
  const candidate = options['--candidate'] ?? 'compact';
  const entry = workerMemoBenchmarkCase(options['--case']);
  const rounds = Number(options['--rounds'] ?? '1');
  if (!Number.isSafeInteger(rounds) || rounds < 1 || rounds > (entry.size === 'small' ? 2 : 1)) {
    throw new Error('finite memo suite supports one large round or up to two small rounds');
  }
  const schedule = abbaSchedule(rounds);
  productMemoAbbaSelection(candidate, 'treatment');
  if (!Number.isSafeInteger(workers) || workers < 1 || workers > os.availableParallelism() ||
      !Number.isSafeInteger(timeout) || timeout < 1 || timeout > 1800) throw new Error('invalid worker count or finite sample timeout');
  if (workers < 2) throw new Error('memo ownership A/B requires native parallel exit accounting; serial memory is not measured by this harness');
  const owner = fs.realpathSync(options['--owner-root']), source = fs.realpathSync(options['--source-root']);
  const binary = fs.realpathSync(options['--binary']), manager = fs.realpathSync(options['--manager']);
  if (!['clearra', 'clearra.exe'].includes(basename(binary)) || !['clearra-manage', 'clearra-manage.exe'].includes(basename(manager))) throw new Error('expected Clearra executable identities');
  const output = resolve(options['--output']);
  checkedCommand(manager, ['storage', 'verify', '--path', output], owner);
  if (fs.existsSync(output)) throw new Error('output directory must be new; never overwrite an earlier receipt');
  fs.mkdirSync(output, { recursive: true });
  const manifest = {
    schema: 'clearra.worker-memo-abba.v4', authority: 'local-only-comparison-not-release',
    source_revision: checkedCommand('git', ['rev-parse', 'HEAD'], source).trim(),
    tracked_patch_sha256: sha(checkedCommand('git', ['diff', '--no-ext-diff', '--binary', '--', 'crates', 'tools/clearra-pc4-qualifier'], source)),
    untracked_source: checkedCommand('git', ['ls-files', '--others', '--exclude-standard', '--', 'crates', 'tools/clearra-pc4-qualifier'], source)
      .trim().split(/\r?\n/).filter(path => path.endsWith('.rs')).map(path => ({ path, sha256: sha(fs.readFileSync(join(source, path))) })),
    binary_sha256: sha(fs.readFileSync(binary)), workers_requested: workers,
    timeout_seconds_per_sample: timeout, input: entry, candidate, rounds,
    order: schedule.map(slot => productMemoAbbaSelection(candidate, slot.arm).label),
    harness_files: ['run-worker-memo-abba.mjs', 'v081-product-memo-abba-core.mjs',
      'v081-accelerator-abba-core.mjs', 'v081-worker-memory-accounting.mjs'].map(name => ({
      path: `scripts/benchmark/${name}`, sha256: sha(fs.readFileSync(new URL(name, import.meta.url))),
    })),
    memo_selections: { baseline: productMemoAbbaSelection(candidate, 'baseline'), treatment: productMemoAbbaSelection(candidate, 'treatment') },
    timing_scope: 'supervisor-start-through-solver-exit',
    peak_metric: process.platform === 'win32' ? 'job-object-aggregate-commit-bytes' : 'supervisor-owned-tree-memory-bytes',
    component_scope: PRIVATE_WORKER_MEMORY_SCOPE,
    shared_component_scope: 'one-request-owned-immutable-standard-bag-tables-not-worker-multiplied',
    nested_component_scope: 'memo-payload-is-included-in-private-standard-bag-owner',
    orchestrator_profile: 'local-service', outer_lease_ms: 7_200_000,
    accelerators: 'both-disabled', tablebase: 'disabled', release_gate: 'not-evaluated',
  };
  write('manifest.json', manifest);
  // An old binary can silently ignore local-only environment selectors. Check
  // the actual backend/layout on a tiny standard-bag request before P7P4.
  for (const arm of ['baseline', 'treatment']) {
    const selected = productMemoAbbaSelection(candidate, arm);
    const args = ['--format', 'json', '--lang', 'en', 'pc', '--board-mask', '0x3c0f03c0f',
      '--height', '4', '--pieces', '6', '--lines', '4', '--patterns', 'P7',
      '--count', 'unique', '--backend', 'cpu', '--no-tablebase', '--rule', 'srs-plus',
      '--workers', '2', '--no-legal-board', '--no-conditioned-reachability'];
    const checked = await invoke(manager, ['runtime', 'run', '--producer', 'benchmark', '--profile', 'benchmark-search',
      '--timeout', '60', '--', binary, ...args], owner, productMemoAbbaEnvironment(process.env, selected));
    if (checked.code !== 0 || checked.overflow) throw new Error(`memo ${arm} preflight failed without starting the timed suite`);
    const value = JSON.parse(checked.stdout.split(/(?:^|\r?\n)clearra_runtime_receipt=/)[0].trim());
    assert.equal(value.summary.backend_selected, 'wasm-cpu');
    semanticIdentity(value, workerMemoBenchmarkCase('pc-existing-field-p7-srs-plus'));
    assertWorkerExecution({ reported_workers_used: Number(value.summary.workers_used),
      parallel_active_workers: Number(value.summary.parallel_active_workers),
      cpu_parallel_execution: value.summary.cpu_parallel_execution === true ||
        value.summary.cpu_parallel_execution === 'true' }, 2, true);
    assertProductMemoCaseSelection(value.summary, selected,
      workerMemoBenchmarkCase('pc-existing-field-p7-srs-plus'));
    const receipt = executionReceipt(checked);
    assertNormalReceipt(checked, receipt.outcome);
    write(`preflight-${arm}.json`, { value, supervisor_receipt: receipt.path,
      outcome: receipt.outcome, command_args: args, excluded_from_timing: true });
  }
  const samples = [];
  let identity, runtimeIdentity, failure;
  for (const slot of schedule) {
    if (!sampleFitsRemainingLease(Math.ceil(performance.now() - started),
      timeout * 1_000, manifest.outer_lease_ms, 30_000)) {
      failure = 'outer-lease-budget-exhausted-before-sample-admission';
      break;
    }
    const selection = productMemoAbbaSelection(candidate, slot.arm);
    const storage = selection.label;
    const args = ['--format', 'json', '--lang', 'en', ...entry.args,
      '--rule', 'srs-plus', '--workers', String(workers), '--no-legal-board', '--no-conditioned-reachability'];
    const sampleId = `${slot.round}-${slot.slot}-${storage}`;
    process.stderr.write(`[worker-memo-abba] ${entry.id} ${sampleId} started\n`);
    const execution = await invoke(manager, ['runtime', 'run', '--producer', 'benchmark', '--profile', 'benchmark-search',
      '--timeout', String(timeout), '--', binary, ...args], owner,
      productMemoAbbaEnvironment(process.env, selection));
    const sample = { ...slot, storage, memo_selection: selection, state: 'invalid', reason: null, wall_ms: execution.wallMs, command_args: args };
    try {
      const receipt = executionReceipt(execution);
      const outcome = receipt.outcome;
      sample.supervisor_receipt = receipt.path;
      sample.peak_memory_bytes = outcome.peak_memory_bytes;
      sample.supervisor_duration_ms = outcome.duration_ms;
      sample.memory_pressure_events = outcome.memory_pressure_events;
      sample.performance_adoption_eligible = outcome.memory_pressure_events === 0;
      try { assertNormalReceipt(execution, outcome); } catch (error) {
        sample.state = outcome.reason === 'timeout' || outcome.reason.includes('memory') ? 'censored' : 'invalid';
        throw error;
      }
      const value = JSON.parse(execution.stdout.split(/(?:^|\r?\n)clearra_runtime_receipt=/)[0].trim());
      sample.semantic_identity = semanticIdentity(value, entry);
      const summary = value.summary;
      sample.reported_workers_used = Number(summary.workers_used);
      sample.parallel_active_workers = Number(summary.parallel_active_workers);
      sample.cpu_parallel_execution = summary.cpu_parallel_execution === true || summary.cpu_parallel_execution === 'true';
      assertWorkerExecution(sample, workers, true);
      assertProductMemoCaseSelection(summary, selection, entry);
      sample.memo_activity = {
        policy: summary.standard_bag_product_memo_policy,
        layout: summary.standard_bag_product_memo_layout,
        promotion_attempts: Number(summary.standard_bag_product_memo_promotion_attempts),
        promotions: Number(summary.standard_bag_product_memo_promotions),
      };
      assert.equal(Number(summary.candidate_digest_retained_bytes), 0);
      assert.equal(Number(summary.legal_board_verified_negative_prunes), 0);
      assert.equal(Number(summary.reachability_conditioned_complete_hits), 0);
      sample.worker_memory_accounting = readWorkerMemoryAccounting(summary);
      identity ??= sample.semantic_identity;
      runtimeIdentity ??= value.runtime_identity;
      assert(runtimeIdentity && typeof runtimeIdentity === 'object', 'missing authoritative native runtime identity');
      assert.deepEqual(sample.semantic_identity, identity);
      assert.deepEqual(value.runtime_identity, runtimeIdentity);
      sample.state = 'complete';
      write(`${sampleId}.json`, value);
    } catch (error) {
      sample.reason = error.message;
      failure = error.message;
    }
    samples.push(sample);
    write('samples.json', samples);
    process.stderr.write(`[worker-memo-abba] ${entry.id} ${sampleId} ${sample.state}\n`);
    if (sample.state !== 'complete') break;
  }
  if (sha(fs.readFileSync(binary)) !== manifest.binary_sha256) failure ??= 'binary changed within A/B';
  const aggregate = Object.fromEntries(['reference', candidate].map(storage => {
    const arm = samples.filter(sample => sample.storage === storage && sample.state === 'complete');
    return [storage, { completed: arm.length, mean_wall_ms: mean(arm.map(sample => sample.wall_ms)),
      clean_completed: arm.filter(sample => sample.performance_adoption_eligible).length,
      mean_peak_memory_bytes: mean(arm.map(sample => sample.peak_memory_bytes)),
      maximum_peak_memory_bytes: arm.length ? Math.max(...arm.map(sample => sample.peak_memory_bytes)) : null }];
  }));
  const report = { manifest, samples, aggregate, semantic_identity: identity, runtime_identity: runtimeIdentity,
    result: failure ? 'invalid-or-censored' : 'complete-local-abba', failure: failure ?? null, release_gate: 'not-evaluated' };
  write('summary.json', report);
  process.stdout.write(`${JSON.stringify({ output, result: report.result, aggregate })}\n`);
  if (failure) process.exitCode = 1;
  function write(name, value) { fs.writeFileSync(join(output, name), `${JSON.stringify(value, null, 2)}\n`); }
}

function sha(value) { return createHash('sha256').update(value).digest('hex'); }
function mean(values) { return values.length ? values.reduce((total, value) => total + value, 0) / values.length : null; }
function executionReceipt(execution) {
  const match = `${execution.stdout}\n${execution.stderr}`.match(/(?:^|\n)clearra_runtime_receipt=([^\r\n]+)/);
  if (!match || execution.overflow) throw new Error('missing supervisor receipt or bounded output overflow');
  const path = match[1].trim();
  return { ...JSON.parse(fs.readFileSync(path, 'utf8')), path };
}
function assertNormalReceipt(execution, outcome) {
  if (execution.code !== 0 || outcome.return_code !== 0 || outcome.reason !== 'normal' || !outcome.process_tree_stopped) {
    throw new Error(`supervisor rejected sample: ${outcome.reason}`);
  }
}
function checkedCommand(command, args, cwd) {
  const run = spawnSync(command, args, { cwd, encoding: 'utf8', timeout: 30_000, maxBuffer: BYTE_LIMIT, windowsHide: true });
  if (run.status !== 0) throw new Error(`preflight failed: ${command} ${args[0]}`);
  return run.stdout;
}
function invoke(command, args, cwd, env) {
  return new Promise((resolvePromise, reject) => {
    const started = performance.now(), child = spawn(command, args, { cwd, env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '', bytes = 0, overflow = false;
    const collect = (stream, append) => stream.on('data', chunk => {
      bytes += chunk.length;
      if (bytes > BYTE_LIMIT) { overflow = true; return; }
      append(chunk.toString('utf8'));
    });
    collect(child.stdout, chunk => { stdout += chunk; });
    collect(child.stderr, chunk => { stderr += chunk; });
    child.on('error', reject);
    child.on('close', code => resolvePromise({ code, stdout, stderr, overflow, wallMs: performance.now() - started }));
  });
}

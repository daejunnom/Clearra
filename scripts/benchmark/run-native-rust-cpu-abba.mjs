// Serial subprocess ABBA with a fresh benchmark-search lease for each arm.
// Never package this runner or its artifacts as acceptance/release evidence.
// --binary selects the candidate; the four revision flags opt into an exact
// Rust-vs-Rust comparison with a separately managed baseline executable.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';

const ARM_ORDER = Object.freeze(['native-c', 'rust-cpu', 'rust-cpu', 'native-c']);
const RUST_REVISION_ORDER = Object.freeze(['rust-baseline', 'rust-candidate', 'rust-candidate', 'rust-baseline']);
const REVISION_FLAGS = ['--baseline-binary', '--baseline-commit', '--baseline-sha256', '--candidate-sha256'];
const COMMIT_HASH = /^[0-9a-f]{40}$/;
const CONTENT_HASH = /^[0-9a-f]{64}$/;
const OUTPUT_BYTES = 1024 * 1024;
const LOCAL_LEASE_MS = 7_200_000;
const GUARD_MS = 30_000;
const CASES = Object.freeze({
  'fixed-2l': { count: 4, hash: null, countPolicy: 'all', patternCount: 1, compareCoverage: true, workers: 1 },
  'existing-p7': { count: 246, hash: 'cts1:cb0b19c391d5003e', countPolicy: 'unique', patternCount: 5040, compareCoverage: false },
  'full-p7p4': { count: 456923, hash: 'cts1:98ebe8726537b29f', countPolicy: 'unique', patternCount: 4233600, compareCoverage: false },
});

const sampleSchema = 'clearra.native-rust-cpu-abba.sample.v1';
const integer = (value, min, max = Number.MAX_SAFE_INTEGER) =>
  Number.isSafeInteger(value) && value >= min && value <= max;

export function compilePlan(argv) {
  const allowed = new Set(['--manager', '--binary', '--output', '--case', '--workers', '--timeout-seconds',
    ...REVISION_FLAGS]);
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const [key, value] = argv.slice(index, index + 2);
    if (!allowed.has(key) || value === undefined || Object.hasOwn(options, key)) {
      throw new Error('expected distinct known option/value pairs');
    }
    options[key] = value;
  }
  for (const key of ['--manager', '--binary', '--output']) {
    assert(path.isAbsolute(options[key] ?? ''), key + ' must be absolute');
  }
  const caseId = options['--case'];
  assert(Object.hasOwn(CASES, caseId), 'unknown fixture');
  const workers = parseIntegerOption(options['--workers'], 1, 256, 'workers');
  assert(CASES[caseId].workers === undefined || workers === CASES[caseId].workers,
    'fixed-2l fixture is qualified only at one worker');
  const timeout = parseIntegerOption(options['--timeout-seconds'], 1, 1800, 'timeout-seconds');
  if (REVISION_FLAGS.some(flag => Object.hasOwn(options, flag))) {
    assert(REVISION_FLAGS.every(flag => Object.hasOwn(options, flag)),
      'Rust revision ABBA requires --baseline-binary, --baseline-commit, --baseline-sha256 and --candidate-sha256');
    assert(path.isAbsolute(options['--baseline-binary']), '--baseline-binary must be absolute');
    assert(COMMIT_HASH.test(options['--baseline-commit']), 'expected exact lowercase baseline commit SHA');
    assert(CONTENT_HASH.test(options['--baseline-sha256']), 'expected exact lowercase baseline binary SHA-256');
    assert(CONTENT_HASH.test(options['--candidate-sha256']), 'expected exact lowercase candidate binary SHA-256');
    assert.notEqual(options['--baseline-sha256'], options['--candidate-sha256'], 'the two binaries must be distinct');
    return { options, caseId, workers, timeout, schedule: RUST_REVISION_ORDER };
  }
  return { options, caseId, workers, timeout, schedule: ARM_ORDER };
}

function parseIntegerOption(text, min, max, name) {
  assert(/^[1-9][0-9]*$/.test(text ?? '') && integer(Number(text), min, max), 'invalid ' + name);
  return Number(text);
}

export function validateSample(sample, caseId, arm, workers) {
  assert.equal(sample.schema, sampleSchema);
  assert.equal(sample.case, caseId);
  assert.equal(sample.arm, arm);
  assert.equal(sample.requested_workers, workers);
  assert(integer(sample.available_parallelism, workers), 'requested width not available inside the solver');
  assert.equal(sample.cpu_warmup, false);
  assert(typeof sample.solver_elapsed_ns === 'string' &&
    /^[1-9][0-9]*$/.test(sample.solver_elapsed_ns) && integer(Number(sample.solver_elapsed_ns), 1),
  'missing or out-of-range monotonic solver duration');
  assert(sample.linux_process_rss_hwm_bytes === null || integer(sample.linux_process_rss_hwm_bytes, 1));
  const { semantic, backend } = sample;
  const expected = CASES[caseId];
  assert.equal(semantic.count_complete, true, 'missing complete count authority');
  assert.equal(semantic.resource_truncated, false, 'resource-truncated result');
  assert.equal(semantic.solution_count, expected.count);
  assert(/^cts1:[0-9a-f]{16}$/.test(semantic.solution_set_hash ?? ''), 'missing canonical set hash');
  if (expected.hash !== null) assert.equal(semantic.solution_set_hash, expected.hash);
  assert(semantic.ordered_key_sha256 === null || /^[0-9a-f]{64}$/.test(semantic.ordered_key_sha256));
  assert(semantic.coverage_sha256 === null || /^[0-9a-f]{64}$/.test(semantic.coverage_sha256));
  assert.equal(semantic.pattern_count, expected.patternCount, 'coverage universe differs from the fixture');
  if (expected.compareCoverage) {
    assert.equal(semantic.probability_complete, true, 'missing complete probability authority');
    assert(integer(semantic.covered_pattern_count, 1, semantic.pattern_count));
  } else {
    // The CountUnique native path intentionally skips probability authority
    // when no probabilities are requested. Rust can retain additional
    // coverage in the same request. Neither presence nor equality of that
    // surplus qualifies probability parity on this benchmark.
    assert(semantic.probability_complete === null || typeof semantic.probability_complete === 'boolean');
    assert(semantic.covered_pattern_count === null ||
      integer(semantic.covered_pattern_count, 0, semantic.pattern_count));
  }
  // The native field counts retained variants; the Rust field can count
  // logical variants. Never make these unequal diagnostics an oracle.
  if (expected.countPolicy === 'all') {
    assert(/^[1-9][0-9]*$/.test(semantic.build_variant_count ?? ''), 'missing positive BuildUp evidence');
  }
  assert.equal(backend.selected, arm === 'native-c' ? 'cpu' : 'wasm-cpu');
  assert.equal(backend.fallback, false, 'backend fell back');
  assert.equal(backend.reported_workers, workers, 'reported worker width differs from requested width');
  let workerEvidence = 'serial';
  if (arm === 'native-c') {
    assert.equal(backend.packing_execution_source, 'native-cpu-packing');
    assert.equal(backend.buildup_execution_source, 'native-cpu-buildup');
    const observed = [backend.native_geometry_dag_workers, backend.native_candidate_workers, backend.native_buildup_workers];
    if (observed.some(count => count !== null)) {
      assert.equal(backend.native_geometry_dag_workers, 1, 'C buildable DAG is a single owned producer');
      assert(integer(backend.native_candidate_workers, 1, workers));
      assert(integer(backend.native_buildup_workers, 0, workers));
      workerEvidence = 'native-separate-stage-participation';
    } else if (workers > 1) {
      // An older reference supplies assigned packing width only.
      workerEvidence = 'native-packing-width-only';
    }
  } else if (workers > 1) {
    assert.equal(backend.parallel, true);
    assert.equal(backend.reported_active_workers, workers, 'Rust useful participation differs from width');
    workerEvidence = 'rust-useful-participation';
  } else {
    assert.equal(backend.parallel, false);
  }
  return {
    workerEvidence,
    identity: {
      solution_count: semantic.solution_count,
      solution_set_hash: semantic.solution_set_hash,
      pattern_count: semantic.pattern_count,
      ...(expected.compareCoverage ? { covered_pattern_count: semantic.covered_pattern_count } : {}),
    },
  };
}

export function validateRevisionSample(sample, caseId, workers, compiledCommit) {
  assert(COMMIT_HASH.test(compiledCommit ?? ''), 'expected exact compiled revision');
  assert.equal(sample.compiled_source_commit, compiledCommit, 'binary is not bound to this revision');
  const accepted = validateSample(sample, caseId, 'rust-cpu', workers);
  // Both arms exercise Rust CPU materialization. A set hash alone
  // is not sufficient to qualify a change to canonical output order.
  assert(CONTENT_HASH.test(sample.semantic.ordered_key_sha256 ?? ''), 'missing ordered normalized-key sequence digest');
  assert.equal(sample.backend.reported_active_workers, workers, 'Rust useful participation differs from width');
  return {
    workerEvidence: accepted.workerEvidence,
    identity: { ...accepted.identity, ordered_key_sha256: sample.semantic.ordered_key_sha256 },
  };
}

export function summarize(samples, failure, schedule = ARM_ORDER) {
  const [baselineLabel, candidateLabel] = schedule;
  const aggregate = {};
  for (const arm of [baselineLabel, candidateLabel]) {
    const completed = samples.filter(sample => sample.arm === arm && sample.state === 'complete');
    const clean = completed.filter(sample => sample.performance_eligible);
    aggregate[arm] = {
      completed: completed.length, clean: clean.length,
      // ns are validated safe integers in the finite benchmark interval.
      mean_solver_ms: clean.length === 2
        ? clean.reduce((sum, sample) => sum + Number(sample.solver.solver_elapsed_ns) / 1e6, 0) / 2
        : null,
      observed_max_solver_ms: clean.length === 2
        ? Math.max(...clean.map(sample => Number(sample.solver.solver_elapsed_ns) / 1e6))
        : null,
      peak_memory_bytes: completed.length
        ? Math.max(...completed.map(sample => sample.peak_memory_bytes)) : null,
      peak_metric: completed[0]?.peak_metric ?? null,
    };
  }
  const complete = !failure && samples.length === schedule.length &&
    samples.every((sample, index) => sample.arm === schedule[index] && sample.state === 'complete');
  const eligible = complete && Object.values(aggregate).every(arm => arm.clean === 2);
  const baseline = aggregate[baselineLabel], candidate = aggregate[candidateLabel];
  return {
    result: complete
      ? (eligible ? 'complete-clean-abba' : 'complete-disturbed-abba')
      : 'incomplete-abba',
    aggregate,
    solver_mean_ratio: eligible ? candidate.mean_solver_ms / baseline.mean_solver_ms : null,
    solver_observed_max_ratio: eligible ? candidate.observed_max_solver_ms / baseline.observed_max_solver_ms : null,
    no_regression_observed: eligible
      ? candidate.mean_solver_ms <= baseline.mean_solver_ms &&
        candidate.observed_max_solver_ms <= baseline.observed_max_solver_ms
      : null,
    inference: 'two-observations-per-arm; illustrative, not a percentile or population bound',
  };
}

async function main(argv) {
  const started = performance.now();
  const hosted = process.env.GITHUB_ACTIONS === 'true' && process.env.RUNNER_OS === 'Linux';
  const supervised = process.env.CLEARRA_RUNTIME_SUPERVISED === '1' &&
    process.env.CLEARRA_RUNTIME_PROFILE === 'local-service';
  assert(hosted || supervised, 'use a finite hosted CI job or the managed local-service orchestrator');
  const { options, caseId, workers, timeout, schedule } = compilePlan(argv);
  const manager = fs.realpathSync(options['--manager']);
  const output = path.resolve(options['--output']);
  assert(/^clearra-manage(?:\.exe)?$/.test(path.basename(manager)), 'expected clearra-manage');
  const rustRevisions = schedule === RUST_REVISION_ORDER;
  const binary = resolveExample(options['--binary'], rustRevisions ? manager : null);
  const sourceRevision = checkedCommand('git', ['rev-parse', 'HEAD']).trim();
  assert(COMMIT_HASH.test(sourceRevision), 'invalid source revision');
  checkedCommand('git', ['diff', '--quiet', 'HEAD', '--']);
  checkedCommand(manager, ['storage', 'verify', '--path', output]);
  const binaryHash = await sha256(binary);
  let variants;
  if (rustRevisions) {
    const baselineRevision = options['--baseline-commit'];
    assert.notEqual(baselineRevision, sourceRevision, 'baseline commit is the candidate checkout');
    assert.equal(checkedCommand('git', ['rev-parse', '--verify', `${baselineRevision}^{commit}`]).trim(),
      baselineRevision, 'baseline is not an exact commit object in this repository');
    const baselineBinary = resolveExample(options['--baseline-binary'], manager);
    assert.notEqual(baselineBinary, binary, 'the two variants resolve to the same executable path');
    const baselineHash = await sha256(baselineBinary);
    assert.equal(baselineHash, options['--baseline-sha256'], 'baseline binary SHA-256 differs from pin');
    assert.equal(binaryHash, options['--candidate-sha256'], 'candidate binary SHA-256 differs from pin');
    variants = {
      'rust-baseline': { binary: baselineBinary, binary_sha256: baselineHash, source_revision: baselineRevision, solver_arm: 'rust-cpu' },
      'rust-candidate': { binary, binary_sha256: binaryHash, source_revision: sourceRevision, solver_arm: 'rust-cpu' },
    };
  }
  // No replacement, including an accidental path to a prior receipt.
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.mkdirSync(output);
  const manifest = {
    schema: variants ? 'clearra.rust-cpu-revision-abba.v1' : 'clearra.native-rust-cpu-abba.v2',
    authority: 'benchmark-only-not-release',
    source_revision: sourceRevision,
    ...(variants ? { variants } : { binary, binary_sha256: binaryHash }),
    host: {
      platform: process.platform, architecture: process.arch,
      logical_capacity: os.availableParallelism(), cpu_model: os.cpus()[0]?.model ?? null,
    },
    case: caseId, fixture: CASES[caseId],
    comparison_scope: CASES[caseId].compareCoverage ? 'normalized-solution-family-and-fixed-queue-coverage' : 'normalized-unique-solution-family-only',
    coverage_note: CASES[caseId].compareCoverage
      ? 'Compare the one fixed queue; build-variant diagnostics are not equivalent.'
      : variants
        ? 'Probabilities were not requested; probability/count/hash fields are diagnostics, not coverage parity.'
        : 'Probabilities were not requested. Rust can do additional coverage work; its probability/count/hash fields are diagnostics, not coverage parity.',
    requested_workers: workers, schedule,
    timeout_seconds_per_sample: timeout,
    tool_profile: 'benchmark-search',
    timing_scope: 'core-solve-through-result-assembly; excludes process startup, compilation, and evidence hashing',
    peak_scope: process.platform === 'linux'
      ? 'Linux kernel solver-process VmHWM (all solver threads, no child workers); independently record manager sampled tree RSS'
      : 'supervisor-owned tree; includes result evidence emission',
    cold_policy: variants
      ? 'fresh child, two pinned binaries, SHA-256 readback before each sample, CPU warmup disabled; system caches not flushed'
      : 'fresh child, same binary, CPU warmup disabled; system caches not flushed',
    accelerators: 'disabled',
    comparison_rule: variants
      ? 'Rust candidate mean and observed maximum must not exceed Rust baseline on two clean samples each'
      : 'Rust mean and observed maximum must not exceed C on two clean samples each',
  };
  write('manifest.json', manifest);
  const samples = [];
  const runIds = new Set();
  const orderedHashes = new Set();
  const coverageHashes = Object.fromEntries(schedule.slice(0, 2).map(arm => [arm, new Set()]));
  let identity, failure, revisionParallelism;
  let missingTreeProof = false;
  for (const [index, arm] of schedule.entries()) {
    if (!hosted && performance.now() - started + timeout * 1_000 + GUARD_MS > LOCAL_LEASE_MS) {
      failure = 'outer-lease-budget-exhausted-before-sample-admission';
      break;
    }
    const slot = index + 1;
    const name = String(slot) + '-' + arm;
    const variant = variants?.[arm];
    if (variant) {
      const readbackError = await verifyReadback(arm, variant);
      if (readbackError) {
        failure ??= readbackError;
        break; // No child was admitted for this slot.
      }
    }
    const command = [variant?.binary ?? binary, variant?.solver_arm ?? arm, caseId, String(workers)];
    const sample = { slot, arm, ...(variant ? { variant } : {}), state: 'invalid', reason: null };
    let processTreeStopped = false;
    process.stderr.write('[native-rust-cpu-abba] ' + caseId + ' ' + name + ' started\n');
    try {
      process.exitCode = 2; // Never await another fixture without a matched tree-stopped receipt.
      const run = await invoke(manager, [
        'runtime', 'run', '--producer', 'benchmark', '--profile', 'benchmark-search',
        '--timeout', String(timeout), '--', ...command,
      ]);
      const outcome = executionReceipt(run);
      sample.supervisor = outcome;
      assert.deepEqual(outcome.command, command, 'receipt command identity mismatch');
      assert.equal(outcome.producer, 'benchmark');
      assert.equal(outcome.profile, 'benchmark-search');
      assert.equal(outcome.timeout_seconds, timeout);
      assert.equal(outcome.automatic_retry, false);
      assert(typeof outcome.run_id === 'string' && outcome.run_id && !runIds.has(outcome.run_id),
        'duplicate or absent supervisor identity');
      runIds.add(outcome.run_id);
      processTreeStopped = outcome.process_tree_stopped === true;
      if (processTreeStopped) process.exitCode = 0;
      assert(processTreeStopped && !run.overflow, 'owned process tree not confirmed stopped or output overflow');
      if (outcome.reason !== 'normal') {
        if (['timeout', 'memory-limit', 'memory-pressure', 'process-limit', 'output-limit', 'cancelled'].includes(outcome.reason) ||
            (outcome.reason === 'signal' && [2, 9, 15].includes(outcome.child_termination?.signal))) {
          sample.state = 'censored';
        }
        throw new Error('supervisor terminated sample: ' + outcome.reason);
      }
      assert(run.code === 0 && outcome.return_code === 0 && outcome.child_termination?.kind === 'exit' &&
        outcome.child_termination.code === 0 && outcome.error_code === null &&
        outcome.supervisor_termination_reason === null, 'solver did not complete: ' + run.stderr.trim());
      const solver = JSON.parse(run.stdout.split(/(?:^|\r?\n)clearra_runtime_receipt=/)[0].trim());
      sample.solver = solver;
      let accepted;
      if (variant) {
        accepted = validateRevisionSample(solver, caseId, workers, variant.source_revision);
        revisionParallelism ??= solver.available_parallelism;
        assert.equal(solver.available_parallelism, revisionParallelism, 'available CPU width changed within ABBA');
      } else {
        assert.equal(solver.compiled_source_commit, sourceRevision, 'binary is not bound to this checkout');
        accepted = validateSample(solver, caseId, arm, workers);
      }
      identity ??= accepted.identity;
      assert.deepEqual(accepted.identity, identity, 'qualified result identity mismatch');
      sample.worker_evidence = accepted.workerEvidence;
      const orderedHash = solver.semantic.ordered_key_sha256;
      if (orderedHash !== null) orderedHashes.add(orderedHash);
      assert(orderedHashes.size <= 1, 'ordered normalized-key sequence disagrees');
      const coverage = solver.semantic.coverage_sha256;
      if (coverage !== null) coverageHashes[arm].add(coverage);
      if (CASES[caseId].compareCoverage) {
        assert(new Set(Object.values(coverageHashes).flatMap(values => [...values])).size <= 1,
          'qualified fixed-queue coverage union words disagree');
      }
      assert(typeof outcome.memory_pressure_observed === 'boolean' &&
        integer(outcome.memory_pressure_events, 0), 'no supervisor pressure evidence');
      sample.peak_memory_bytes = process.platform === 'linux'
        ? solver.linux_process_rss_hwm_bytes : outcome.peak_memory_bytes;
      assert(integer(sample.peak_memory_bytes, 1), 'no comparable peak memory evidence');
      sample.peak_metric = process.platform === 'linux' ? 'linux-solver-process-rss-high-water' : outcome.kernel_memory_peak?.source;
      assert(typeof sample.peak_metric === 'string', 'missing kernel peak provenance');
      sample.performance_eligible = outcome.memory_pressure_events === 0 && !outcome.memory_pressure_observed;
      sample.state = 'complete';
    } catch (error) {
      sample.reason = error.message;
      failure ??= error.message;
    }
    samples.push(sample);
    write(name + '.json', sample);
    process.stderr.write('[native-rust-cpu-abba] ' + caseId + ' ' + name + ' ' + sample.state + '\n');
    // Preserve every planned observation after a result mismatch or censoring
    // if the prior tree is proven stopped. A later slot is not a replacement
    // retry and cannot make the failed comparison eligible.
    if (!processTreeStopped) missingTreeProof = true;
    if (missingTreeProof) break;
  }
  if (variants) {
    for (const [arm, variant] of Object.entries(variants)) {
      const readbackError = await verifyReadback(arm, variant);
      failure ??= readbackError;
    }
  } else if (await sha256(binary) !== binaryHash) {
    failure ??= 'example binary changed within ABBA';
  }
  const summary = {
    manifest, samples, failure: failure ?? null, semantic_identity: identity ?? null,
    all_owned_trees_stopped: !missingTreeProof,
    ordered_key_sequence: {
      observed_slots: samples.filter(sample => sample.solver?.semantic.ordered_key_sha256).length,
      sha256: orderedHashes.size === 1 ? [...orderedHashes][0] : null,
    },
    coverage_observations: {
      compared: CASES[caseId].compareCoverage,
      sha256_by_arm: Object.fromEntries(Object.entries(coverageHashes).map(([arm, hashes]) => [arm, [...hashes]])),
      matrix: samples.filter(sample => sample.solver).map(({ slot, arm, solver: { semantic } }) => ({
        slot, arm, probability_complete: semantic.probability_complete, covered_pattern_count: semantic.covered_pattern_count,
        coverage_sha256: semantic.coverage_sha256,
      })),
    },
    ...summarize(samples, failure, schedule),
  };
  write('summary.json', summary);
  process.stdout.write(JSON.stringify({
    output, result: summary.result, failure: summary.failure,
    comparison_scope: manifest.comparison_scope,
    no_regression_observed: summary.no_regression_observed, aggregate: summary.aggregate,
  }) + '\n');
  if (missingTreeProof) process.exitCode = 2;
  else if (summary.result !== 'complete-clean-abba' || summary.no_regression_observed !== true) process.exitCode = 1;

  function write(name, content) {
    fs.writeFileSync(path.join(output, name), JSON.stringify(content, null, 2) + '\n', { flag: 'wx' });
  }
}

function resolveExample(filename, manager) {
  if (manager) checkedCommand(manager, ['storage', 'verify', '--path', filename]);
  const binary = fs.realpathSync(filename);
  assert(/^native_rust_cpu_abba(?:\.exe)?$/.test(path.basename(binary)), 'expected the ABBA example binary');
  if (manager) {
    if (binary !== filename) checkedCommand(manager, ['storage', 'verify', '--path', binary]);
    assert(fs.statSync(binary).isFile(), 'expected a regular ABBA example file');
  }
  return binary;
}

async function verifyReadback(label, variant) {
  try {
    return await sha256(variant.binary) === variant.binary_sha256 ? null : label + ' binary changed within ABBA';
  } catch {
    return label + ' binary is unavailable for SHA-256 readback';
  }
}

function checkedCommand(command, args) {
  const run = spawnSync(command, args, { encoding: 'utf8', timeout: 30_000, maxBuffer: OUTPUT_BYTES, windowsHide: true });
  assert.equal(run.status, 0, 'preflight failed: ' + command + ' ' + args[0]);
  return run.stdout;
}

function executionReceipt(run) {
  const matches = [...(run.stdout + '\n' + run.stderr)
    .matchAll(/(?:^|\n)clearra_runtime_receipt=([^\r\n]+)/g)];
  assert.equal(matches.length, 1, 'expected exactly one supervisor receipt');
  const filename = matches[0][1].trim();
  assert(path.isAbsolute(filename) && filename.endsWith('-runtime.json'), 'invalid supervisor receipt path');
  assert(fs.statSync(filename).size <= OUTPUT_BYTES, 'unbounded supervisor receipt');
  const receipt = JSON.parse(fs.readFileSync(filename, 'utf8'));
  assert.equal(receipt.schema_id, 'clearra.runtime-receipt.v2');
  assert.equal(receipt.outcome?.receipt, filename, 'receipt path identity mismatch');
  return receipt.outcome;
}

function invoke(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    let stdout = '', stderr = '', bytes = 0, overflow = false;
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    function collect(chunk, append) {
      bytes += Buffer.byteLength(chunk);
      if (bytes > OUTPUT_BYTES) overflow = true;
      else append(chunk);
    }
    child.stdout.on('data', chunk => collect(chunk, value => { stdout += value; }));
    child.stderr.on('data', chunk => collect(chunk, value => { stderr += value; }));
    child.on('error', reject);
    child.on('close', code => resolve({ code, stdout, stderr, overflow }));
  });
}

async function sha256(filename) {
  const hash = createHash('sha256');
  for await (const chunk of fs.createReadStream(filename)) hash.update(chunk);
  return hash.digest('hex');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { await main(process.argv.slice(2)); } catch (error) {
    process.stderr.write('[native-rust-cpu-abba] ' + error.message + '\n');
    if (process.exitCode !== 2) process.exitCode = 1;
  }
}

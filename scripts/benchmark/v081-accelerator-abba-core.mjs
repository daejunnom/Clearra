// Pure, local-only analysis for the v0.8.1 exact-accelerator A/B. This module
// never builds, downloads, installs, or activates an asset.
export const PROFILES = new Set(['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']);
export const PAIRS = Object.freeze({
  legal: { baseline: [false, false], treatment: [true, false] },
  conditioned: { baseline: [false, false], treatment: [false, true] },
  combined: { baseline: [false, false], treatment: [true, true] },
});

const FORBIDDEN_CASE_OPTIONS = new Set([
  '--legal-board', '--no-legal-board', '--conditioned-reachability',
  '--no-conditioned-reachability', '--workers', '--auto-workers',
  '--cpu-threads', '--use-all-cpu-threads', '--rule', '--tablebase',
  '--offline-fallback', '--solution-output', '--include-solution-data',
  '--format', '--lang', '--tiling-only', '--input', '--file', '--fixture',
  '--output', '--target-file', '--target-document', '--solution-document',
  '--kick-profile-json', '--tie-snapshot', '--ties',
  '--deterministic', '--max-candidates',
]);
const MISSING = Symbol('missing');

export function validateCases(document) {
  if (document?.schema !== 'clearra.v081.accelerator-benchmark-cases.v1' ||
      !Array.isArray(document.cases) || document.cases.length === 0) {
    throw new Error('invalid v0.8.1 benchmark case document');
  }
  const ids = new Set();
  for (const entry of document.cases) {
    if (!entry || !/^[a-z0-9][a-z0-9-]{0,63}$/.test(entry.id) || ids.has(entry.id)) {
      throw new Error('benchmark case IDs must be unique, lowercase and path-safe');
    }
    ids.add(entry.id);
    if (!PROFILES.has(entry.profile) || !['large', 'small', 'build'].includes(entry.size)) {
      throw new Error(`${entry.id}: invalid profile or size`);
    }
    if (!Array.isArray(entry.args) || !['pc', 'build', 'setup-finder'].includes(entry.args[0]) ||
        entry.args.some(arg => typeof arg !== 'string' || arg.length === 0 || arg.length > 512 ||
          FORBIDDEN_CASE_OPTIONS.has(arg.split('=')[0]))) {
      throw new Error(`${entry.id}: invalid or benchmark-controlled command argument`);
    }
    if (!Array.isArray(entry.compare_fields) || entry.compare_fields.length === 0 ||
        entry.compare_fields.some(path => !/^summary(?:\.[a-z][a-z0-9_]*)+$/.test(path))) {
      throw new Error(`${entry.id}: explicit summary comparison fields are required`);
    }
    if (entry.expected === undefined ||
        (entry.expected === null || typeof entry.expected !== 'object' ||
          Object.keys(entry.expected).some(path => !entry.compare_fields.includes(path)))) {
      throw new Error(`${entry.id}: explicit completeness oracles must name compared fields`);
    }
  }
  return document.cases;
}

export function abbaSchedule(rounds) {
  if (!Number.isSafeInteger(rounds) || rounds < 1) throw new Error('rounds must be positive');
  return Array.from({ length: rounds }, (_, round) =>
    (round % 2 === 0 ? ['baseline', 'treatment', 'treatment', 'baseline'] :
      ['treatment', 'baseline', 'baseline', 'treatment'])
      .map((arm, slot) => ({ round: round + 1, slot: slot + 1, arm }))).flat();
}

// Keep the requested per-sample timeout intact. An outer finite lease may
// instead prevent admitting another sample; that is not a completed timing.
export function sampleFitsRemainingLease(elapsedMs, sampleTimeoutMs, leaseMs, reserveMs) {
  if (![elapsedMs, sampleTimeoutMs, leaseMs, reserveMs].every(Number.isSafeInteger) ||
      elapsedMs < 0 || sampleTimeoutMs < 1 || leaseMs < 1 || reserveMs < 0) {
    throw new Error('invalid finite benchmark lease accounting');
  }
  return elapsedMs <= leaseMs - sampleTimeoutMs - reserveMs;
}

export function commandArgs(entry, pair, arm, workers) {
  const selected = PAIRS[pair]?.[arm];
  if (!selected || !Number.isSafeInteger(workers) || workers < 1) {
    throw new Error('invalid benchmark pair, arm or worker count');
  }
  return [
    '--format', 'json', '--lang', 'en', ...entry.args,
    '--rule', entry.profile, '--workers', String(workers),
    selected[0] ? '--legal-board' : '--no-legal-board',
    selected[1] ? '--conditioned-reachability' : '--no-conditioned-reachability',
  ];
}

export function semanticIdentity(value, entry) {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
      !value.summary || typeof value.summary !== 'object') {
    throw new Error(`${entry.id}: successful CLI JSON summary is required`);
  }
  const summary = value.summary;
  if (entry.args[0] === 'pc' && summary.backend_selected !== 'wasm-cpu') {
    throw new Error(`${entry.id}: PC benchmark did not use the accelerator-owning WASM CPU backend`);
  }
  // A complete large family may expose only its first solution page. That is
  // not an incomplete count or set hash, so solution_keys_complete is not a
  // global rejection criterion. Each fixture states its own exactness oracle.
  if (summary.resource_truncated === true) {
    throw new Error(`${entry.id}: incomplete result cannot prove A/B parity`);
  }
  const identity = {};
  for (const path of entry.compare_fields) {
    const field = path.slice('summary.'.length);
    const result = path.split('.').slice(1).reduce((owner, segment) =>
      owner !== MISSING && owner !== null && typeof owner === 'object' &&
      Object.hasOwn(owner, segment) ? owner[segment] : MISSING, summary);
    if (result === MISSING || result === null || result === 'not-calculated' ||
        typeof result === 'object') {
      throw new Error(`${entry.id}: ${path} is absent or not authoritative`);
    }
    identity[field] = result;
    if (entry.expected && Object.hasOwn(entry.expected, path) &&
        result !== entry.expected[path] && String(result) !== String(entry.expected[path])) {
      throw new Error(`${entry.id}: ${path} differs from the independent fixture oracle`);
    }
  }
  return identity;
}

export function classifyExecution(execution) {
  if (execution.timedOut) return { state: 'censored', reason: 'timeout' };
  if (execution.overflow) return { state: 'invalid', reason: 'output-limit' };
  if (execution.exitCode === 0) return { state: 'complete' };
  let code = null;
  try { code = JSON.parse(execution.stdout).error?.code ?? null; } catch { /* not JSON */ }
  if (typeof code === 'string' && /(RESOURCE|MEMORY|LIMIT)/i.test(code)) {
    return { state: 'censored', reason: code };
  }
  return { state: 'invalid', reason: code ?? `exit-${execution.exitCode}` };
}

export function assertWorkerExecution(sample, requestedWorkers, requireActiveWorkers = false) {
  if (!Number.isSafeInteger(requestedWorkers) || requestedWorkers < 1) {
    throw new Error('requested worker count must be a positive integer');
  }
  if (!Number.isSafeInteger(sample?.reported_workers_used)) {
    throw new Error('native CLI omitted its actual worker count');
  }
  if (sample.reported_workers_used !== requestedWorkers) {
    throw new Error(`worker underuse: requested ${requestedWorkers}, reported ${sample.reported_workers_used}`);
  }
  if (requestedWorkers > 1 && sample.cpu_parallel_execution !== true) {
    throw new Error('multiworker sample did not report CPU parallel execution');
  }
  if (requireActiveWorkers && (!Number.isSafeInteger(sample.parallel_active_workers) ||
      sample.parallel_active_workers !== requestedWorkers)) {
    throw new Error(`active-worker underuse: requested ${requestedWorkers}, reported ${sample.parallel_active_workers ?? 'missing'}`);
  }
}

export function assertIsolatedAcceleratorCounters(sample, pair, arm, requireCounters,
  requireLegalActivity = false) {
  const enabled = PAIRS[pair]?.[arm];
  if (!enabled) throw new Error('unknown accelerator A/B arm');
  const legal = sample.legal_board_verified_negative_prunes;
  const conditioned = sample.conditioned_complete_hits;
  const misses = sample.conditioned_misses;
  if (requireCounters && (!Number.isSafeInteger(legal) || !Number.isSafeInteger(conditioned) ||
      !Number.isSafeInteger(misses))) {
    throw new Error('PC backend omitted accelerator activity counters');
  }
  if (!enabled[0] && Number.isSafeInteger(legal) && legal !== 0) {
    throw new Error('disabled legal-board still pruned a state');
  }
  if (enabled[0] && arm === 'treatment' && requireLegalActivity && legal === 0) {
    throw new Error('eligible legal-board treatment had zero verified negative prunes; installed status and worker count alone do not prove activation');
  }
  if (!enabled[1] && Number.isSafeInteger(conditioned) && conditioned !== 0) {
    throw new Error('disabled conditioned reachability still completed a relation');
  }
  if (!enabled[1] && Number.isSafeInteger(misses) && misses !== 0) {
    throw new Error('disabled conditioned reachability still missed a relation');
  }
  if (enabled[1] && arm === 'treatment' && requireCounters &&
      Number.isSafeInteger(conditioned) && conditioned === 0) {
    if (sample.conditioned_reachability_requested === null ||
        sample.conditioned_reachability_policy_enabled === null ||
        sample.conditioned_reachability_snapshot_active === null ||
        !Number.isSafeInteger(sample.conditioned_lookup_attempts)) {
      throw new Error('runtime omitted conditioned activation and lookup diagnostics; receipt cannot classify zero hits');
    }
    if (sample.conditioned_reachability_requested !== true) {
      throw new Error('conditioned treatment was not propagated as a request option');
    }
    if (sample.conditioned_reachability_policy_enabled !== true) {
      throw new Error('conditioned treatment was disabled by the local compile-time A/B policy gate');
    }
    if (sample.conditioned_reachability_snapshot_active !== true) {
      throw new Error('conditioned treatment had no active qualified in-process snapshot; installed status alone is insufficient');
    }
    if (sample.conditioned_lookup_attempts === 0) {
      throw new Error(`conditioned snapshot loaded but no provider lookup occurred (no_query_context=${sample.conditioned_no_query_context ?? 'unknown'}, empty_entries=${sample.conditioned_empty_entry_sets ?? 'unknown'}, cache_short_circuits=${sample.conditioned_cache_short_circuits ?? 'unknown'})`);
    }
    if (Number.isSafeInteger(sample.conditioned_out_of_scope) &&
        sample.conditioned_out_of_scope === sample.conditioned_lookup_attempts) {
      throw new Error('all conditioned lookups passed through as OutOfScope; relation data does not cover this query domain');
    }
    throw new Error(`conditioned treatment had zero complete hits after ${sample.conditioned_lookup_attempts ?? 'unknown'} lookups (miss=${sample.conditioned_misses ?? 'unknown'}, out_of_scope=${sample.conditioned_out_of_scope ?? 'unknown'}, unknown=${sample.conditioned_unknown ?? 'unknown'}, snapshot_mismatch=${sample.conditioned_snapshot_mismatch ?? 'unknown'}, invalid_asset=${sample.conditioned_invalid_asset ?? 'unknown'})`);
  }
}

export function summarizeSamples(samples) {
  const arms = {};
  for (const arm of ['baseline', 'treatment']) {
    const selected = samples.filter(sample => sample.arm === arm);
    const completed = selected.filter(sample => sample.state === 'complete')
      .map(sample => sample.wall_ms).sort((a, b) => a - b);
    arms[arm] = {
      complete: completed.length,
      censored: selected.filter(sample => sample.state === 'censored').length,
      invalid: selected.filter(sample => sample.state === 'invalid').length,
      median_ms: percentile(completed, 0.5),
      p95_ms: percentile(completed, 0.95),
      min_ms: completed[0] ?? null,
      max_ms: completed.at(-1) ?? null,
      legal_board_verified_negative_prunes: counterTotal(selected,
        'legal_board_verified_negative_prunes'),
      conditioned_complete_hits: counterTotal(selected, 'conditioned_complete_hits'),
      conditioned_misses: counterTotal(selected, 'conditioned_misses'),
      peak_memory_bytes: selected.reduce((peak, sample) =>
        Number.isSafeInteger(sample.peak_memory_bytes) ? Math.max(peak ?? 0, sample.peak_memory_bytes) : peak, null),
    };
  }
  const enough = arms.baseline.complete >= 20 && arms.treatment.complete >= 20 &&
    arms.baseline.invalid === 0 && arms.treatment.invalid === 0 &&
    arms.baseline.censored === 0 && arms.treatment.censored === 0;
  return {
    arms,
    p95_improvement_fraction: enough && arms.baseline.p95_ms > 0 ?
      1 - arms.treatment.p95_ms / arms.baseline.p95_ms : null,
    decision: enough ? 'eligible-for-gate-review' : 'insufficient-or-censored',
  };
}

export function mergeBatchReceipts(receipts) {
  if (!Array.isArray(receipts) || receipts.length === 0) {
    throw new Error('at least one benchmark receipt is required');
  }
  const first = receipts[0];
  if (first?.manifest?.schema !== 'clearra.v081.accelerator-abba-receipt.v2') {
    throw new Error('unknown benchmark receipt schema');
  }
  const fields = ['executable_sha256', 'fixture_sha256', 'case', 'pair',
    'workers_requested', 'assets', 'timing_scope', 'process_tree_peak_memory', 'worker_memory_accounting', 'memo_selection'];
  for (const receipt of receipts) {
    if (receipt?.result === 'complete-local-batch' && Array.isArray(receipt.samples) &&
        receipt.samples.some(sample => !hasExpectedWorkerCount(sample, receipt.manifest.workers_requested,
          receipt.manifest.case?.args?.[0] === 'pc'))) {
      throw new Error('receipt contains a sample with worker underuse or missing parallel execution evidence');
    }
    if (receipt?.manifest?.schema !== first.manifest.schema || !Array.isArray(receipt.samples) ||
        fields.some(key => JSON.stringify(receipt.manifest[key]) !== JSON.stringify(first.manifest[key])) ||
        JSON.stringify(receipt.semantic_identity) !== JSON.stringify(first.semantic_identity) ||
        JSON.stringify(receipt.runtime_identity) !== JSON.stringify(first.runtime_identity)) {
      throw new Error('mixed binary, fixture, asset generation, worker, or result identity');
    }
  }
  const samples = receipts.flatMap(receipt => receipt.samples);
  const aggregate = summarizeSamples(samples);
  const batchesComplete = receipts.every(receipt => receipt.result === 'complete-local-batch' &&
    receipt.samples.every(sample => hasExpectedWorkerCount(sample, receipt.manifest.workers_requested,
      receipt.manifest.case?.args?.[0] === 'pc')));
  const fraction = batchesComplete ? aggregate.p95_improvement_fraction : null;
  const size = first.manifest.case.size;
  const threshold = size === 'small' ? -0.05 : 0.10;
  return {
    schema: 'clearra.v081.accelerator-abba-analysis.v2',
    case: first.manifest.case.id,
    profile: first.manifest.case.profile,
    pair: first.manifest.pair,
    workers_requested: first.manifest.workers_requested,
    executable_sha256: first.manifest.executable_sha256,
    fixture_sha256: first.manifest.fixture_sha256,
    asset_generations: Object.fromEntries(Object.entries(first.manifest.assets)
      .map(([key, value]) => [key, value.generation_identity])),
    batch_count: receipts.length,
    batches_complete: batchesComplete,
    aggregate,
    mechanism_observation: {
      legal_board_verified_negative_prunes: observedCounter(
        aggregate.arms.treatment.legal_board_verified_negative_prunes),
      conditioned_complete_hits: observedCounter(
        aggregate.arms.treatment.conditioned_complete_hits),
    },
    overall_p95_threshold: threshold,
    peak_memory: {
      unit: 'bytes',
      metric: 'sampled process-tree working set',
      sampling_interval_ms: 250,
      baseline_max_bytes: aggregate.arms.baseline.peak_memory_bytes,
      treatment_max_bytes: aggregate.arms.treatment.peak_memory_bytes,
      baseline_observed_samples: receipts.flatMap(receipt => receipt.samples)
        .filter(sample => sample.arm === 'baseline' && Number.isSafeInteger(sample.peak_memory_bytes)).length,
      treatment_observed_samples: receipts.flatMap(receipt => receipt.samples)
        .filter(sample => sample.arm === 'treatment' && Number.isSafeInteger(sample.peak_memory_bytes)).length,
    },
    overall_p95_result: fraction === null ? 'not-evaluable' :
      fraction >= threshold ? 'pass' : 'fail',
    build_up_p95_result: 'not-measured',
    physical_peak_result: 'not-measured',
    canonical_order_result: 'not-measured',
    release_gate: 'not-evaluated',
  };
}

export function compareWorkerReceipts(receipts, requiredWorkers = [1, 2, 11]) {
  if (!Array.isArray(receipts) || receipts.length === 0) {
    throw new Error('worker comparison needs benchmark receipts');
  }
  const first = receipts[0];
  const common = ['executable_sha256', 'fixture_sha256', 'case', 'pair',
    'assets', 'timing_scope', 'process_tree_peak_memory', 'worker_memory_accounting', 'memo_selection'];
  for (const receipt of receipts) {
    if (Array.isArray(receipt?.samples) && receipt.result === 'complete-local-batch' &&
        receipt.samples.some(sample => !hasExpectedWorkerCount(sample, receipt.manifest.workers_requested,
          receipt.manifest.case?.args?.[0] === 'pc'))) {
      throw new Error('worker receipt contains underused workers or missing parallel execution evidence');
    }
    if (receipt?.manifest?.schema !== 'clearra.v081.accelerator-abba-receipt.v2' ||
        receipt.result !== 'complete-local-batch' ||
        common.some(key => JSON.stringify(receipt.manifest[key]) !== JSON.stringify(first.manifest[key])) ||
        JSON.stringify(receipt.semantic_identity) !== JSON.stringify(first.semantic_identity) ||
        JSON.stringify(receipt.runtime_identity) !== JSON.stringify(first.runtime_identity)) {
      throw new Error('cross-worker comparison has mixed source, data, or result identity');
    }
  }
  const available = [...new Set(receipts.map(receipt => receipt.manifest.workers_requested))]
    .sort((a, b) => a - b);
  return {
    schema: 'clearra.v081.accelerator-worker-parity.v1',
    case: first.manifest.case.id,
    pair: first.manifest.pair,
    available_workers: available,
    missing_workers: requiredWorkers.filter(workers => !available.includes(workers)),
    semantic_identity: first.semantic_identity,
    parity: requiredWorkers.every(workers => available.includes(workers)) ?
      'complete-and-equal' : 'incomplete-worker-matrix',
  };
}

function hasExpectedWorkerCount(sample, requestedWorkers, requireActiveWorkers) {
  return Number.isSafeInteger(requestedWorkers) && requestedWorkers >= 1 &&
    sample?.state === 'complete' && sample.reported_workers_used === requestedWorkers &&
    (requestedWorkers === 1 || sample.cpu_parallel_execution === true) &&
    (!requireActiveWorkers || sample.parallel_active_workers === requestedWorkers);
}

function percentile(sorted, proportion) {
  return sorted.length ? sorted[Math.ceil(proportion * sorted.length) - 1] : null;
}

function counterTotal(samples, field) {
  const complete = samples.filter(sample => sample.state === 'complete');
  if (!complete.length || !complete.every(sample =>
    Number.isSafeInteger(sample[field]) && sample[field] >= 0)) return null;
  const total = complete.reduce((sum, sample) => sum + sample[field], 0);
  return Number.isSafeInteger(total) ? total : null;
}

function observedCounter(total) {
  return total === null ? 'not-reported' : total > 0 ? 'observed' : 'zero-observed';
}

import fs from 'node:fs';
import { dirname, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';
import {
  expectedWasmProbeIdentity,
  validateWasmProbeTerminal,
} from './wasm-product-terminal-contract.mjs';

const [
  bindingsPath,
  commandText,
  workBudgetText = '8192',
  outputMode = 'summary',
  ...contractArguments
] = process.argv.slice(2);
if (!bindingsPath || !commandText) {
  throw new Error(
    'usage: node wasm-pc-environment-probe.mjs CLEARRA_WASM_BINDINGS "clearra pc ..." [WORK_BUDGET] [summary|full] [--product-page] [--manifest PATH --expected-source-commit SHA|unverified-local-build]',
  );
}
if (!['summary', 'full'].includes(outputMode)) {
  throw new Error('WASM probe output mode must be summary or full');
}

const workBudget = positiveInteger(workBudgetText, 'WORK_BUDGET');
const probeOptions = parseProbeArguments(contractArguments);
const contractOptions = parseContractArguments(probeOptions.contractArguments);
const expectedRuntimeIdentity = contractOptions === null
  ? null
  : expectedWasmProbeIdentity(
      JSON.parse(fs.readFileSync(contractOptions.manifestPath, 'utf8')),
      contractOptions.expectedSourceCommit,
    );
const debugLifecycle = process.env.CLEARRA_WASM_PROBE_DEBUG === '1';
const progressInterval = optionalPositiveInteger(
  process.env.CLEARRA_WASM_PROBE_PROGRESS_INTERVAL,
  'CLEARRA_WASM_PROBE_PROGRESS_INTERVAL',
);
const stopMemoryBytes = optionalPositiveInteger(
  process.env.CLEARRA_WASM_PROBE_STOP_MEMORY_BYTES,
  'CLEARRA_WASM_PROBE_STOP_MEMORY_BYTES',
);
const stopAdvanceCalls = optionalPositiveInteger(
  process.env.CLEARRA_WASM_PROBE_STOP_ADVANCE_CALLS,
  'CLEARRA_WASM_PROBE_STOP_ADVANCE_CALLS',
);
const wasmPath = resolve(dirname(bindingsPath), 'clearra_wasm_bg.wasm');
const wasmBytes = fs.readFileSync(wasmPath);
const loadStarted = performance.now();
const bindings = await import(pathToFileURL(resolve(bindingsPath)).href);
const exports = await bindings.default({ module_or_path: wasmBytes });
const loadElapsedMs = performance.now() - loadStarted;
const api = bindRawAbi(exports);

if (api.abiVersion() !== 1) {
  throw new Error(`unsupported Clearra WASM ABI version ${api.abiVersion()}`);
}

const commandBytes = new TextEncoder().encode(commandText);
if (api.inputResize(commandBytes.byteLength) !== 0) {
  throw new Error(readOutput(api));
}
new Uint8Array(api.memory.buffer, api.inputPtr(), commandBytes.byteLength).set(commandBytes);

const profilingEnabled =
  process.env.CLEARRA_WASM_PROFILE !== '0' &&
  typeof api.profileStart === 'function' &&
  typeof api.profileFinish === 'function';
if (profilingEnabled && api.profileStart() !== 0) {
  throw new Error(readOutput(api));
}
if (debugLifecycle) console.error('[wasm-probe] profile-started');
const prepareStarted = performance.now();
const jobId = api.startJob();
const prepareElapsedMs = performance.now() - prepareStarted;
if (jobId === 0) {
  if (profilingEnabled) api.profileFinish();
  throw new Error(readOutput(api));
}
if (debugLifecycle) console.error(`[wasm-probe] job-started id=${jobId}`);

let advanceCalls = 0;
let terminalStatus = 0;
const searchStarted = performance.now();
let progressStarted = searchStarted;
while (jobIsActive(terminalStatus)) {
  if (debugLifecycle && advanceCalls < 4) {
    console.error(`[wasm-probe] advance begin call=${advanceCalls + 1}`);
  }
  terminalStatus = api.advanceJob(jobId, workBudget);
  advanceCalls += 1;
  if (
    progressInterval !== null &&
    (advanceCalls % progressInterval === 0 || jobIsTerminal(terminalStatus))
  ) {
    const now = performance.now();
    console.error(JSON.stringify({
      advance_calls: advanceCalls,
      terminal_status: terminalStatus,
      elapsed_ms: now - searchStarted,
      interval_elapsed_ms: now - progressStarted,
      wasm_memory_bytes: api.memory.buffer.byteLength,
    }));
    progressStarted = now;
  }
  if (
    stopMemoryBytes !== null &&
    api.memory.buffer.byteLength >= stopMemoryBytes
  ) {
    api.cancelJob(jobId);
    throw new Error(
      `diagnostic memory stop reached: ${api.memory.buffer.byteLength} bytes`,
    );
  }
  if (stopAdvanceCalls !== null && advanceCalls >= stopAdvanceCalls) {
    api.cancelJob(jobId);
    throw new Error(`diagnostic advance stop reached: ${advanceCalls} calls`);
  }
  if (debugLifecycle && advanceCalls <= 4) {
    console.error(`[wasm-probe] advance end call=${advanceCalls} status=${terminalStatus}`);
  }
  if (terminalStatus < 0) {
    throw new Error(readOutput(api));
  }
  if (!jobIsActive(terminalStatus) && !jobIsTerminal(terminalStatus)) {
    throw new Error(`invalid Clearra WASM job status ${terminalStatus}`);
  }
}
const searchElapsedMs = performance.now() - searchStarted;

if (api.drainJobEvents(jobId) !== 0) {
  throw new Error(readOutput(api));
}
const events = JSON.parse(readOutput(api));
const finalEvent = validateWasmProbeTerminal({
  events,
  terminalStatus,
  expectedRuntimeIdentity,
});
const searchReport = finalEvent?.search_report ?? null;
let searchProfile = null;
if (profilingEnabled) {
  if (api.profileFinish() !== 0) {
    throw new Error(readOutput(api));
  }
  searchProfile = JSON.parse(readOutput(api));
}
const productPage = probeOptions.productPage
  ? probeCoveragePortfolioPage(api, finalEvent, workBudget)
  : null;

console.log(JSON.stringify({
  runtime: 'wasm-bindgen-web-host',
  bindings_path: bindingsPath,
  wasm_path: wasmPath,
  wasm_file_bytes: wasmBytes.byteLength,
  wasm_memory_bytes: api.memory.buffer.byteLength,
  load_elapsed_ms: loadElapsedMs,
  prepare_elapsed_ms: prepareElapsedMs,
  search_elapsed_ms: searchElapsedMs,
  advance_calls: advanceCalls,
  work_budget: workBudget,
  terminal_status: terminalStatus,
  search_profile: searchProfile,
  command: commandText,
  product_page: productPage,
  final: outputMode === 'full' ? finalEvent : summarizeFinal(finalEvent, searchReport),
}, null, 2));

function summarizeFinal(finalEvent, searchReport) {
  if (!finalEvent) return null;
  return {
    status: finalEvent.response?.status ?? null,
    runtime_identity: finalEvent.response?.runtime_identity ?? null,
    diagnostics: finalEvent.response?.diagnostics ?? [],
    backend_selected: searchReport?.backend_selected ?? null,
    workers_used: searchReport?.workers_used ?? null,
    cpu_parallel_execution: searchReport?.cpu_parallel_execution ?? null,
    cpu_parallel_decision_reason:
      searchReport?.cpu_parallel_decision_reason ?? null,
    cpu_warmup_requested: searchReport?.cpu_warmup_requested ?? null,
    cpu_warmup_performed: searchReport?.cpu_warmup_performed ?? null,
    supply_window_resolution: searchReport?.supply_window_resolution ?? null,
    projects_unplaced_lookahead:
      searchReport?.projects_unplaced_lookahead ?? null,
    source_sequence_length: searchReport?.source_sequence_length ?? null,
    total_possible_pattern_count:
      searchReport?.total_possible_pattern_count ?? null,
    solution_found: searchReport?.solution_found ?? null,
    packing_candidate_count: searchReport?.packing_candidate_count ?? null,
    geometry_candidate_family_count:
      searchReport?.geometry_candidate_family_count ?? null,
    packing_candidate_set_digest: searchReport?.packing_candidate_set_digest ?? null,
    unique_solution_count: searchReport?.unique_solution_count ?? null,
    normalized_solution_set_hash: searchReport?.normalized_solution_set_hash ?? null,
    materialized_pattern_count: searchReport?.materialized_pattern_count ?? null,
    covered_pattern_count: searchReport?.covered_pattern_count ?? null,
    coverage_probability: searchReport?.coverage_probability ?? null,
    probability_complete: searchReport?.probability_complete ?? null,
    count_complete: searchReport?.count_complete ?? null,
    searched_nodes: searchReport?.searched_nodes ?? null,
    geometry_domain_pruned_states:
      searchReport?.geometry_domain_pruned_states ?? null,
    geometry_hall_pruned_states:
      searchReport?.geometry_hall_pruned_states ?? null,
    geometry_column_pruned_states:
      searchReport?.geometry_column_pruned_states ?? null,
    geometry_component_compositions:
      searchReport?.geometry_component_compositions ?? null,
    peak_frontier_states: searchReport?.peak_frontier_states ?? null,
    peak_cpu_bytes: searchReport?.peak_cpu_bytes ?? null,
    peak_build_order_nodes: searchReport?.peak_build_order_nodes ?? null,
    total_build_order_nodes: searchReport?.total_build_order_nodes ?? null,
    coverage_product_words: searchReport?.coverage_product_words ?? null,
    coverage_product_states: searchReport?.coverage_product_states ?? null,
    coverage_product_edge_checks:
      searchReport?.coverage_product_edge_checks ?? null,
    piece_language_coverage_cache_hits:
      searchReport?.piece_language_coverage_cache_hits ?? null,
    piece_language_coverage_cache_misses:
      searchReport?.piece_language_coverage_cache_misses ?? null,
    standard_bag_symbolic_cache_hits:
      searchReport?.standard_bag_symbolic_cache_hits ?? null,
    standard_bag_symbolic_cache_misses:
      searchReport?.standard_bag_symbolic_cache_misses ?? null,
    standard_bag_symbolic_cache_recycles:
      searchReport?.standard_bag_symbolic_cache_recycles ?? null,
    piece_language_node_count:
      searchReport?.piece_language_node_count ?? null,
    piece_language_retained_bytes:
      searchReport?.piece_language_retained_bytes ?? null,
    standard_bag_retained_bytes:
      searchReport?.standard_bag_retained_bytes ?? null,
    reachability_retained_bytes:
      searchReport?.reachability_retained_bytes ?? null,
    realization_feasibility_states:
      searchReport?.realization_feasibility_states ?? null,
    realization_feasibility_rejected_candidates:
      searchReport?.realization_feasibility_rejected_candidates ?? null,
    peak_reachability_states: searchReport?.peak_reachability_states ?? null,
    total_reachability_states: searchReport?.total_reachability_states ?? null,
    reachability_lock_queries: searchReport?.reachability_lock_queries ?? null,
    reachability_harddrop_queries: searchReport?.reachability_harddrop_queries ?? null,
    reachability_harddrop_hits: searchReport?.reachability_harddrop_hits ?? null,
    reachability_cache_reachable_hits:
      searchReport?.reachability_cache_reachable_hits ?? null,
    reachability_cache_unreachable_hits:
      searchReport?.reachability_cache_unreachable_hits ?? null,
    reachability_cache_key_misses:
      searchReport?.reachability_cache_key_misses ?? null,
    reachability_partial_searches:
      searchReport?.reachability_partial_searches ?? null,
    reachability_exhaustive_searches:
      searchReport?.reachability_exhaustive_searches ?? null,
    coverage_row_count: searchReport?.coverage_row_count ?? null,
    pattern_verified_execution_count:
      searchReport?.pattern_verified_execution_count ?? null,
    resource_truncated: searchReport?.resource_truncated ?? null,
    resource_truncation_reason: searchReport?.resource_truncation_reason ?? null,
  };
}

function jobIsActive(status) {
  return status === 0 || status === 4;
}

function jobIsTerminal(status) {
  return status === 1 || status === 2 || status === 3;
}

function bindRawAbi(exports) {
  const required = {
    memory: exports.memory,
    abiVersion: exports.clearra_wasm_abi_version,
    inputResize: exports.clearra_wasm_input_resize,
    inputPtr: exports.clearra_wasm_input_ptr,
    startJob: exports.clearra_wasm_start_job,
    advanceJob: exports.clearra_wasm_advance_job,
    cancelJob: exports.clearra_wasm_cancel_job,
    drainJobEvents: exports.clearra_wasm_drain_job_events,
    outputPtr: exports.clearra_wasm_output_ptr,
    outputLen: exports.clearra_wasm_output_len,
    outputRelease: exports.clearra_wasm_output_release,
  };
  for (const [name, value] of Object.entries(required)) {
    if (value === undefined) {
      throw new Error(`Clearra WASM export missing: ${name}`);
    }
  }
  return {
    ...required,
    profileStart: exports.clearra_wasm_profile_start,
    profileFinish: exports.clearra_wasm_profile_finish,
    productPageAvailable: exports.clearra_wasm_product_page_available,
    productPageNext: exports.clearra_wasm_product_page_next,
    productPageGet: exports.clearra_wasm_product_page_get,
    productPageRelease: exports.clearra_wasm_product_page_release,
  };
}

function probeCoveragePortfolioPage(api, finalEvent, workBudget) {
  for (const name of [
    'productPageAvailable',
    'productPageNext',
    'productPageGet',
    'productPageRelease',
  ]) {
    if (typeof api[name] !== 'function') {
      throw new Error(`Clearra WASM export missing: ${name}`);
    }
  }

  const productResult = finalEvent?.response?.product_result_payload;
  const initialPayload = productResult?.content?.payload;
  if (
    productResult?.contract !== 'pc.minimals' ||
    productResult?.content?.payload_kind !== 'coverage-portfolio' ||
    initialPayload?.page_handle_available !== true
  ) {
    throw new Error('pc.minimals terminal response did not transfer a coverage portfolio page handle');
  }
  validateCanonicalDecimal(initialPayload.alternative_index, 'initial alternative_index');
  for (const [index, member] of initialPayload.members.entries()) {
    validateCanonicalDecimal(member?.candidate_id, `initial members[${index}].candidate_id`);
  }
  if (api.productPageAvailable() !== 1) {
    throw new Error('pc.minimals terminal response advertised a page handle that is unavailable');
  }

  let released = false;
  try {
    if (api.productPageGet(1, 1) !== 0) {
      throw new Error(readOutput(api));
    }
    const loadedPage = JSON.parse(readOutput(api));
    validateCoveragePortfolioRuntimePage(loadedPage);

    if (api.productPageNext(workBudget) !== 0) {
      throw new Error(readOutput(api));
    }
    const nextState = JSON.parse(readOutput(api));
    validateCoveragePortfolioAdvance(nextState);

    if (api.productPageRelease() !== 0) {
      throw new Error(readOutput(api));
    }
    released = true;
    if (api.productPageAvailable() !== 0) {
      throw new Error('coverage portfolio page owner remained available after release');
    }
    return {
      contract: productResult.contract,
      payload_kind: productResult.content.payload_kind,
      initial_alternative_index: initialPayload.alternative_index,
      loaded_alternative_index: loadedPage.page.alternative_index,
      first_candidate_id: loadedPage.page.members[0]?.candidate_id ?? null,
      next_state: nextState.state,
      page_owner_released: true,
    };
  } finally {
    if (!released && api.productPageAvailable() === 1) {
      api.productPageRelease();
    }
  }
}

function validateCoveragePortfolioRuntimePage(value) {
  if (
    value?.schema_version !== 1 ||
    value?.runtime !== 'clearra-wasm' ||
    value?.product_page_kind !== 'coverage-portfolio' ||
    value?.state !== 'page' ||
    !value.page ||
    !Array.isArray(value.page.members) ||
    value.page.members.length === 0
  ) {
    throw new Error('coverage portfolio get did not return a non-empty runtime page');
  }
  validateCanonicalDecimal(value.page.alternative_index, 'loaded alternative_index');
  validateCanonicalDecimal(value.page.member_page_number, 'loaded member_page_number');
  for (const [index, member] of value.page.members.entries()) {
    validateCanonicalDecimal(member?.candidate_id, `loaded members[${index}].candidate_id`);
  }
}

function validateCoveragePortfolioAdvance(value) {
  if (
    value?.schema_version !== 1 ||
    value?.runtime !== 'clearra-wasm' ||
    value?.product_page_kind !== 'coverage-portfolio' ||
    !['page', 'work-budget-exhausted', 'sealed'].includes(value?.state)
  ) {
    throw new Error('coverage portfolio next returned an invalid advance state');
  }
  if (value.state === 'page') {
    validateCoveragePortfolioRuntimePage(value);
  } else {
    validateCanonicalDecimal(value.known_alternative_count, 'known_alternative_count', true);
  }
}

function validateCanonicalDecimal(value, label, allowZero = false) {
  const pattern = allowZero ? /^(0|[1-9][0-9]*)$/ : /^[1-9][0-9]*$/;
  if (typeof value !== 'string' || !pattern.test(value)) {
    throw new Error(`${label} must be a canonical decimal string`);
  }
}

function readOutput(api) {
  try {
    return new TextDecoder().decode(
      new Uint8Array(api.memory.buffer, api.outputPtr(), api.outputLen()),
    );
  } finally {
    api.outputRelease();
  }
}

function positiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return parsed;
}

function optionalPositiveInteger(value, label) {
  if (value === undefined || value === '') return null;
  return positiveInteger(value, label);
}

function parseProbeArguments(argumentsList) {
  let productPage = false;
  const contractArguments = [];
  for (const argument of argumentsList) {
    if (argument === '--product-page') {
      if (productPage) throw new Error('--product-page may only be specified once');
      productPage = true;
    } else {
      contractArguments.push(argument);
    }
  }
  return { productPage, contractArguments };
}

function parseContractArguments(argumentsList) {
  if (argumentsList.length === 0) return null;
  if (
    argumentsList.length !== 4 ||
    argumentsList[0] !== '--manifest' ||
    argumentsList[2] !== '--expected-source-commit' ||
    !argumentsList[1] ||
    !argumentsList[3]
  ) {
    throw new Error(
      'WASM probe identity options require --manifest PATH --expected-source-commit SHA|unverified-local-build',
    );
  }
  return {
    manifestPath: resolve(argumentsList[1]),
    expectedSourceCommit: argumentsList[3],
  };
}

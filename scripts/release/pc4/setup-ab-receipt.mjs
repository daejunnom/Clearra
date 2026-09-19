// SRP: validate and classify one already-observed Setup TB/offline comparison.
// It performs no search, network I/O, qualification minting or activation.

export const PC4_SETUP_AB_RECEIPT_SCHEMA = 'clearra.pc4.setup-ab-receipt.v1';

const PROFILES = new Set(['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']);
const OBJECTIVES = new Set([
  'ranked-joint',
  'ranked-build-probability',
  'ranked-conditional-pc',
  'exact-path-detail'
]);
const TERMINALS = new Set(['complete', 'timeout', 'resource-limit', 'failed', 'cancelled']);
const IDENTITY = /^[A-Za-z0-9][A-Za-z0-9._:+/-]{0,255}$/u;
const SHA256 = /^sha256:[0-9a-f]{64}$/u;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;

export function validatePc4SetupAbReceipt(value) {
  exactKeys(value, [
    'schema',
    'source_commit',
    'query_digest',
    'profile',
    'generation_identity',
    'objective',
    'workers',
    'budget',
    'offline',
    'tablebase',
    'transport'
  ], 'Setup A/B receipt');
  if (value.schema !== PC4_SETUP_AB_RECEIPT_SCHEMA) fail('setup_ab_schema_invalid');
  if (!SOURCE_COMMIT.test(value.source_commit)) fail('setup_ab_source_invalid');
  if (!SHA256.test(value.query_digest)) fail('setup_ab_query_digest_invalid');
  if (!PROFILES.has(value.profile)) fail('setup_ab_profile_invalid');
  identity(value.generation_identity, 'setup_ab_generation_invalid');
  if (!OBJECTIVES.has(value.objective)) fail('setup_ab_objective_invalid');
  positive(value.workers, 'setup_ab_workers_invalid');
  const budget = validateBudget(value.budget);
  const offline = validateArm(value.offline, 'offline');
  const tablebase = validateArm(value.tablebase, 'tablebase');
  const transport = validateTransport(value.transport);
  const classification = classify(offline, tablebase);

  return deepFreeze({
    schema: PC4_SETUP_AB_RECEIPT_SCHEMA,
    source_commit: value.source_commit,
    query_digest: value.query_digest,
    profile: value.profile,
    generation_identity: value.generation_identity,
    objective: value.objective,
    workers: value.workers,
    budget,
    offline,
    tablebase,
    transport,
    classification,
    activation_evidence: classification === 'exact-parity',
    transport_assessment: transportAssessment(transport)
  });
}

function validateBudget(value) {
  exactKeys(value, ['cpu_logical_processors', 'memory_bytes', 'time_ms'], 'Setup A/B budget');
  positive(value.cpu_logical_processors, 'setup_ab_cpu_budget_invalid');
  positive(value.memory_bytes, 'setup_ab_memory_budget_invalid');
  positive(value.time_ms, 'setup_ab_time_budget_invalid');
  return { ...value };
}

function validateArm(value, label) {
  exactKeys(value, [
    'terminal',
    'result_digest',
    'candidate_count',
    'coverage_count',
    'first_result_ms',
    'wall_ms',
    'geometry_nodes',
    'residual_nodes',
    'peak_rss_bytes'
  ], `Setup A/B ${label} arm`);
  if (!TERMINALS.has(value.terminal)) fail(`setup_ab_${label}_terminal_invalid`);
  nonnegative(value.wall_ms, `setup_ab_${label}_wall_invalid`);
  nonnegative(value.geometry_nodes, `setup_ab_${label}_geometry_nodes_invalid`);
  nonnegative(value.residual_nodes, `setup_ab_${label}_residual_nodes_invalid`);
  nonnegative(value.peak_rss_bytes, `setup_ab_${label}_rss_invalid`);
  nullableNonnegative(value.first_result_ms, `setup_ab_${label}_first_result_invalid`);
  if (value.terminal === 'complete') {
    identity(value.result_digest, `setup_ab_${label}_digest_invalid`);
    nonnegative(value.candidate_count, `setup_ab_${label}_candidate_count_invalid`);
    nonnegative(value.coverage_count, `setup_ab_${label}_coverage_count_invalid`);
  } else if (
    value.result_digest !== null ||
    value.candidate_count !== null ||
    value.coverage_count !== null
  ) {
    fail(`setup_ab_${label}_incomplete_authority_invalid`);
  }
  return { ...value };
}

function validateTransport(value) {
  exactKeys(value, [
    'provider',
    'protocol',
    'prewarm_started_before_compute',
    'prewarm_requests',
    'search_requests',
    'search_new_connections',
    'search_reused_connections',
    'transferred_bytes'
  ], 'Setup A/B transport');
  if (value.provider !== 'local' && value.provider !== 'online') {
    fail('setup_ab_transport_provider_invalid');
  }
  if (!['local', 'h2', 'h3', 'http/1.1'].includes(value.protocol)) {
    fail('setup_ab_transport_protocol_invalid');
  }
  if (typeof value.prewarm_started_before_compute !== 'boolean') {
    fail('setup_ab_transport_prewarm_invalid');
  }
  for (const field of [
    'prewarm_requests',
    'search_requests',
    'search_new_connections',
    'search_reused_connections',
    'transferred_bytes'
  ]) {
    nonnegative(value[field], 'setup_ab_transport_count_invalid');
  }
  if (value.provider === 'local') {
    if (
      value.protocol !== 'local' ||
      value.prewarm_started_before_compute ||
      value.prewarm_requests !== 0 ||
      value.search_requests !== 0 ||
      value.search_new_connections !== 0 ||
      value.search_reused_connections !== 0 ||
      value.transferred_bytes !== 0
    ) fail('setup_ab_local_transport_invalid');
  } else if (value.protocol === 'local' || !value.prewarm_started_before_compute) {
    fail('setup_ab_online_transport_invalid');
  }
  return { ...value };
}

function classify(offline, tablebase) {
  if (offline.terminal === 'complete' && tablebase.terminal === 'complete') {
    if (
      offline.result_digest !== tablebase.result_digest ||
      offline.candidate_count !== tablebase.candidate_count ||
      offline.coverage_count !== tablebase.coverage_count
    ) fail('setup_ab_exact_parity_mismatch');
    return 'exact-parity';
  }
  if (
    (offline.terminal === 'timeout' || offline.terminal === 'resource-limit') &&
    tablebase.terminal === 'complete'
  ) return 'feasibility-dominance';
  return 'inconclusive';
}

function transportAssessment(transport) {
  if (transport.provider === 'local') return 'local-no-http';
  if (transport.protocol !== 'h2' && transport.protocol !== 'h3') return 'legacy-http';
  if (transport.search_reused_connections > 0 && transport.search_new_connections === 0) {
    return 'reused-multiplexed';
  }
  return 'no-reuse-proof';
}

function exactKeys(value, expected, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    fail(`${label.replaceAll(' ', '_').toLowerCase()}_invalid`);
  }
  const actual = Object.keys(value).sort();
  const keys = [...expected].sort();
  if (actual.length !== keys.length || actual.some((key, index) => key !== keys[index])) {
    fail(`${label.replaceAll(' ', '_').toLowerCase()}_fields_invalid`);
  }
}

function identity(value, code) {
  if (typeof value !== 'string' || !IDENTITY.test(value)) fail(code);
}

function positive(value, code) {
  if (!Number.isSafeInteger(value) || value <= 0) fail(code);
}

function nonnegative(value, code) {
  if (!Number.isSafeInteger(value) || value < 0) fail(code);
}

function nullableNonnegative(value, code) {
  if (value !== null) nonnegative(value, code);
}

function fail(code) {
  throw Object.assign(new Error(code), { code });
}

function deepFreeze(value) {
  Object.freeze(value);
  for (const child of Object.values(value)) {
    if (child !== null && typeof child === 'object' && !Object.isFrozen(child)) deepFreeze(child);
  }
  return value;
}

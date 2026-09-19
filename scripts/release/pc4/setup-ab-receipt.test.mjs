import assert from 'node:assert/strict';
import test from 'node:test';

import {
  PC4_SETUP_AB_RECEIPT_SCHEMA,
  validatePc4SetupAbReceipt
} from './setup-ab-receipt.mjs';

function arm(terminal = 'complete') {
  const complete = terminal === 'complete';
  return {
    terminal,
    result_digest: complete ? 'css1:0123456789abcdef' : null,
    candidate_count: complete ? 77 : null,
    coverage_count: complete ? 610196 : null,
    first_result_ms: complete ? 100 : null,
    wall_ms: 1000,
    geometry_nodes: 123,
    residual_nodes: 456,
    peak_rss_bytes: 1024
  };
}

function receipt() {
  return {
    schema: PC4_SETUP_AB_RECEIPT_SCHEMA,
    source_commit: 'a'.repeat(40),
    query_digest: `sha256:${'b'.repeat(64)}`,
    profile: 'jstris-180',
    generation_identity: 'hf:immutable-generation',
    objective: 'ranked-joint',
    workers: 8,
    budget: { cpu_logical_processors: 8, memory_bytes: 1024, time_ms: 5000 },
    offline: arm(),
    tablebase: arm(),
    transport: {
      provider: 'online',
      protocol: 'h2',
      prewarm_started_before_compute: true,
      prewarm_requests: 1,
      search_requests: 15,
      search_new_connections: 0,
      search_reused_connections: 1,
      transferred_bytes: 4096
    }
  };
}

test('matching completed arms are exact parity and reusable HTTP/2 is separate evidence', () => {
  const validated = validatePc4SetupAbReceipt(receipt());
  assert.equal(validated.classification, 'exact-parity');
  assert.equal(validated.activation_evidence, true);
  assert.equal(validated.transport_assessment, 'reused-multiplexed');
  assert.ok(Object.isFrozen(validated));
});

test('resource-bounded offline failure is feasibility dominance, never activation evidence', () => {
  const value = receipt();
  value.offline = arm('timeout');
  const validated = validatePc4SetupAbReceipt(value);
  assert.equal(validated.classification, 'feasibility-dominance');
  assert.equal(validated.activation_evidence, false);
});

test('completed result or count mismatch fails instead of manufacturing a speedup', () => {
  for (const change of [
    value => { value.tablebase.result_digest = 'css1:fedcba9876543210'; },
    value => { value.tablebase.candidate_count += 1; },
    value => { value.tablebase.coverage_count += 1; }
  ]) {
    const value = receipt();
    change(value);
    assert.throws(() => validatePc4SetupAbReceipt(value), { code: 'setup_ab_exact_parity_mismatch' });
  }
});

test('incomplete arms cannot publish result authority and online prewarm is explicit', () => {
  const incomplete = receipt();
  incomplete.tablebase = arm('failed');
  incomplete.tablebase.result_digest = 'css1:stale';
  assert.throws(
    () => validatePc4SetupAbReceipt(incomplete),
    { code: 'setup_ab_tablebase_incomplete_authority_invalid' }
  );

  const noPrewarm = receipt();
  noPrewarm.transport.prewarm_started_before_compute = false;
  assert.throws(
    () => validatePc4SetupAbReceipt(noPrewarm),
    { code: 'setup_ab_online_transport_invalid' }
  );
});

test('local installed TB records no synthetic HTTP connection claims', () => {
  const value = receipt();
  value.transport = {
    provider: 'local',
    protocol: 'local',
    prewarm_started_before_compute: false,
    prewarm_requests: 0,
    search_requests: 0,
    search_new_connections: 0,
    search_reused_connections: 0,
    transferred_bytes: 0
  };
  const validated = validatePc4SetupAbReceipt(value);
  assert.equal(validated.transport_assessment, 'local-no-http');

  for (const mutate of [
    transport => { transport.prewarm_started_before_compute = true; },
    transport => { transport.search_requests = 1; },
    transport => { transport.transferred_bytes = 1; }
  ]) {
    const invalid = receipt();
    invalid.transport = { ...value.transport };
    mutate(invalid.transport);
    assert.throws(
      () => validatePc4SetupAbReceipt(invalid),
      { code: 'setup_ab_local_transport_invalid' }
    );
  }
});

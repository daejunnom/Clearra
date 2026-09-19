import test from 'node:test';
import assert from 'node:assert/strict';
import {
  PC4_PC_TERMINAL_SEMANTICS,
  PC4_READER_CONTRACT,
  PC4_SETUP_TARGET_QUALIFICATION_RECEIPT_SCHEMA,
  PC4_SETUP_TERMINAL_SEMANTICS,
  PC4_TARGET_QUALIFICATION_RECEIPT_SCHEMA,
  qualifyPc4UpstreamGeneration,
  createPc4RangeReader
} from './qualify-upstream-generation.mjs';

function le(value, width) { return Uint8Array.from({ length: width }, (_, i) => Math.floor(value / 256 ** i) % 256); }
function data() {
  const header = magic => [...new TextEncoder().encode(magic), ...le(1, 4), ...le(2, 4)];
  const graph = Uint8Array.from([0,0,0,0,0, 1,1,0,0, 0,0,0,0,0,0, 255,255,255,255,255, 0,0,0,0,0,0,0]);
  const files = new Map([
    ['graph.bin', graph],
    ['field_hash_to_id.v1.bin', Uint8Array.from([...header('FHIDIDX1'), ...le(0,5), ...le(0,3), ...le(2**40-1,5), ...le(1,3)])],
    ['graph_offsets.u32.bin', Uint8Array.from([...header('GOFFIDX1'), ...le(0,4), ...le(15,4), ...le(27,4)])]
  ]);
  const discovery = { repository: 'example/pc4', resolved_revision: 'a'.repeat(40), candidates:
    [...files].map(([path, bytes], i) => ({ path, byte_length: bytes.length, content_identity: 'sha256:' + String(i).repeat(64) })) };
  return { discovery, files, reader: { bytes: 0, async read(artifact, offset, length) {
    const bytes = files.get(artifact.path); assert.ok(bytes); return bytes.slice(offset, offset + length);
  } } };
}
function targetReceipt(discovery) {
  return {
    schema: PC4_TARGET_QUALIFICATION_RECEIPT_SCHEMA,
    repository: discovery.repository,
    revision: discovery.resolved_revision,
    profile: 'jstris-180',
    reader_contract: PC4_READER_CONTRACT,
    use_case: 'pc-search',
    target_lines: 4,
    terminal_id: 1,
    terminal_hash: 2 ** 40 - 1,
    terminal_semantics_identity: PC4_PC_TERMINAL_SEMANTICS,
    outgoing_edge_completeness_identity: `sha256:${'0123456789abcdef'.repeat(4)}`,
    known_answer_identity: `sha256:${'123456789abcdef0'.repeat(4)}`,
    offline_exact_parity_identity: `sha256:${'23456789abcdef01'.repeat(4)}`,
  };
}
function setupTargetReceipt(discovery) {
  const receipt = targetReceipt(discovery);
  return {
    ...receipt,
    schema: PC4_SETUP_TARGET_QUALIFICATION_RECEIPT_SCHEMA,
    use_case: 'setup-search',
    terminal_semantics_identity: PC4_SETUP_TERMINAL_SEMANTICS,
    setup_differential_identities: {
      ranked_joint_identity: `sha256:${'3456789abcdef012'.repeat(4)}`,
      ranked_build_probability_identity: `sha256:${'456789abcdef0123'.repeat(4)}`,
      ranked_conditional_pc_identity: `sha256:${'56789abcdef01234'.repeat(4)}`,
      exact_path_detail_identity: `sha256:${'6789abcdef012345'.repeat(4)}`,
    },
  };
}
test('completion declaration and reader qualification do not mint PC target authority', async () => {
  const f = data();
  const result = await qualifyPc4UpstreamGeneration({}, { discover: async () => f.discovery, reader: f.reader });
  assert.equal(result.profiles.length, 5);
  assert.deepEqual(result.profiles.filter(p => p.status === 'ready').map(p => p.profile), ['jstris-180']);
  assert.deepEqual(result.profiles[3].target_lines, [4]);
  assert.deepEqual(result.profiles[3].pc_search_target_lines, []);
  assert.deepEqual(result.profiles[3].setup_search_target_lines, []);
  assert.deepEqual(result.profiles[3].target_qualification_receipts, []);
  assert.equal(result.profiles[0].status, 'unavailable');
});

test('an exact generation-bound receipt alone enables its PC target', async () => {
  const f = data();
  const receipt = targetReceipt(f.discovery);
  const result = await qualifyPc4UpstreamGeneration({}, {
    discover: async () => f.discovery,
    reader: f.reader,
    targetQualificationReceipts: [receipt],
  });
  assert.deepEqual(result.profiles[3].pc_search_target_lines, [4]);
  assert.deepEqual(result.profiles[3].target_qualification_receipts, [receipt]);
});

test('Setup activation is separate and requires every objective differential identity', async () => {
  const f = data();
  const pc = targetReceipt(f.discovery);
  const setup = setupTargetReceipt(f.discovery);
  const result = await qualifyPc4UpstreamGeneration({}, {
    discover: async () => f.discovery,
    reader: f.reader,
    targetQualificationReceipts: [setup, pc],
  });
  assert.deepEqual(result.profiles[3].pc_search_target_lines, [4]);
  assert.deepEqual(result.profiles[3].setup_search_target_lines, [4]);
  assert.deepEqual(result.profiles[3].target_qualification_receipts, [pc, setup]);

  for (const mutate of [
    receipt => { delete receipt.setup_differential_identities.exact_path_detail_identity; },
    receipt => { receipt.setup_differential_identities.ranked_joint_identity = 'placeholder'; },
    receipt => { receipt.schema = PC4_TARGET_QUALIFICATION_RECEIPT_SCHEMA; },
    receipt => { receipt.use_case = 'pc-search'; },
  ]) {
    const candidate = setupTargetReceipt(f.discovery);
    mutate(candidate);
    await assert.rejects(
      qualifyPc4UpstreamGeneration({}, {
        discover: async () => f.discovery,
        reader: f.reader,
        targetQualificationReceipts: [candidate],
      }),
      error => error.code === 'pc4_online_target_qualification_invalid',
    );
  }
});

test('stale, cross-profile, placeholder, duplicate, and terminal-mismatched receipts fail closed', async () => {
  for (const mutate of [
    receipt => { receipt.revision = 'b'.repeat(40); },
    receipt => { receipt.profile = 'srs'; },
    receipt => { receipt.use_case = 'setup-search'; },
    receipt => { receipt.target_lines = 3; },
    receipt => { receipt.reader_contract = 'other-reader'; },
    receipt => { receipt.terminal_semantics_identity = 'placeholder'; },
    receipt => { receipt.offline_exact_parity_identity = 'placeholder'; },
  ]) {
    const f = data();
    const receipt = targetReceipt(f.discovery);
    mutate(receipt);
    await assert.rejects(
      qualifyPc4UpstreamGeneration({}, {
        discover: async () => f.discovery,
        reader: f.reader,
        targetQualificationReceipts: [receipt],
      }),
      error => error.code === 'pc4_online_target_qualification_invalid',
    );
  }
  const duplicate = data();
  const receipt = targetReceipt(duplicate.discovery);
  await assert.rejects(
    qualifyPc4UpstreamGeneration({}, {
      discover: async () => duplicate.discovery,
      reader: duplicate.reader,
      targetQualificationReceipts: [receipt, { ...receipt }],
    }),
    error => error.code === 'pc4_online_target_qualification_invalid',
  );
  const terminal = data();
  const mismatched = targetReceipt(terminal.discovery);
  mismatched.terminal_id = 0;
  const result = await qualifyPc4UpstreamGeneration({}, {
    discover: async () => terminal.discovery,
    reader: terminal.reader,
    targetQualificationReceipts: [mismatched],
  });
  assert.equal(result.profiles[3].status, 'unavailable');
  assert.equal(result.profiles[3].reason, 'pc4_online_target_qualification_invalid');
});

test('qualification batches three dependency stages without dropping any sample evidence', async () => {
  const f = data();
  const baseline = await qualifyPc4UpstreamGeneration({}, { discover: async () => f.discovery, reader: f.reader });
  const stages = [];
  f.reader.readMany = async demands => {
    stages.push(demands.map(d => [d.artifact.path, d.offset, d.length]));
    return Promise.all(demands.map(d => f.reader.read(d.artifact, d.offset, d.length)));
  };
  const result = await qualifyPc4UpstreamGeneration({}, { discover: async () => f.discovery, reader: f.reader });
  assert.deepEqual(result, baseline);
  assert.deepEqual(stages.map(s => s.length), [2, 4, 2]);
  assert.ok(stages[2].every(d => d[0] === 'graph.bin'));
});
test('changed graph or index never retains stale readiness', async () => {
  const f = data();
  f.files.get('graph_offsets.u32.bin')[24] = 26;
  const result = await qualifyPc4UpstreamGeneration({}, { discover: async () => f.discovery, reader: f.reader });
  assert.equal(result.profiles[3].upstream_complete, true);
  assert.equal(result.profiles[3].status, 'unavailable');
  assert.equal(result.profiles[3].reason, 'pc4_online_index_graph_mismatch');
});
test('source bitmap mismatch is not hidden by a completion declaration', async () => {
  const f = data(); f.files.get('graph.bin')[0] = 1;
  const result = await qualifyPc4UpstreamGeneration({}, { discover: async () => f.discovery, reader: f.reader });
  assert.equal(result.profiles[3].reason, 'pc4_online_graph_field_mismatch');
});
test('canonical indices are never lent to another graph profile', async () => {
  const f = data();
  f.discovery.candidates.push({ path: 'graph_srsplus.bin', byte_length: 27, content_identity: 'sha256:' + 'f'.repeat(64) });
  const result = await qualifyPc4UpstreamGeneration({}, { discover: async () => f.discovery, reader: f.reader });
  assert.equal(result.profiles[1].reason, 'missing-profile-specific-index');
});
const generation = { repository: 'example/pc4', revision: 'a'.repeat(40) };
const artifact = { path: 'graph.bin', byte_length: 99, content_identity: 'sha256:' + 'a'.repeat(64) };
function response(status = 206, range = 'bytes 3-5/99', bytes = [1,2,3]) {
  return new Response(Uint8Array.from(bytes), { status, headers: { 'content-range': range } });
}
test('Range cache reuses exact immutable bytes, never mutable returned arrays', async () => {
  let calls = 0;
  const reader = createPc4RangeReader(generation, { fetcher: async (url, options) => {
    calls++; assert.ok(url.includes('/' + 'a'.repeat(40) + '/graph.bin'));
    assert.equal(options.credentials, 'omit'); assert.equal(options.headers.Range, 'bytes=3-5'); return response();
  }});
  const first = await reader.read(artifact, 3, 3); first[0] = 99;
  assert.deepEqual(await reader.read(artifact, 3, 3), Uint8Array.from([1,2,3]));
  assert.equal(calls, 1); assert.equal(reader.bytes, 3);
});
for (const [name, result, code] of [
  ['whole body', () => response(200), 'pc4_online_whole_content_rejected'],
  ['rate limit', () => response(429), 'pc4_online_rate_limited'],
  ['wrong range', () => response(206, 'bytes 0-2/99'), 'pc4_online_range_response_invalid'],
  ['truncation', () => response(206, 'bytes 3-5/99', [1]), 'pc4_online_truncated_range'],
  ['oversized response', () => response(206, 'bytes 3-5/99', [1,2,3,4]), 'pc4_online_response_too_large']
]) test(`rejects ${name} without downloading a file or starting fallback`, async () => {
  const reader = createPc4RangeReader(generation, { fetcher: async () => result() });
  await assert.rejects(reader.read(artifact, 3, 3), error => error.code === code);
});
test('cancellation and transfer budgets apply before new I/O', async () => {
  const controller = new AbortController(); controller.abort();
  const never = async () => { assert.fail('must not fetch'); };
  await assert.rejects(createPc4RangeReader(generation, { signal: controller.signal, fetcher: never }).read(artifact,3,3),
    error => error.code === 'pc4_online_cancelled');
  await assert.rejects(createPc4RangeReader(generation, { maxBytes: 2, fetcher: never }).read(artifact,3,3),
    error => error.code === 'pc4_online_transfer_limit');
});

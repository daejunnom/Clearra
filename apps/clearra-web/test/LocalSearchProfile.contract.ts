import assert from 'node:assert/strict';
import { isLocalSearchProfileMode, localSearchProfileText } from '../src/lib/localSearchProfile';

assert.equal(isLocalSearchProfileMode('local-recovery'), true);
assert.equal(isLocalSearchProfileMode('local-audit'), true);
for (const mode of ['production', 'development', 'test', '']) assert.equal(isLocalSearchProfileMode(mode), false);
assert.equal(localSearchProfileText({ event: 'progress', search_profile: {} }), null);
assert.equal(localSearchProfileText({ event: 'started', job_id: 2 }), '');
// The existing presenter replaces text on a non-null value; a new job clears
// stale online counters even when subsequent ordinary progress has no profile.
let display = '{"pc4_online":{"requests":17}}';
for (const event of [{ event: 'started', job_id: 2 }, { event: 'progress', job_id: 2 }]) {
  const next = localSearchProfileText(event);
  if (next !== null) display = next;
}
assert.equal(display, '');
const online = JSON.parse(localSearchProfileText({ event: 'progress', pc4_online: {
  provider: 'hf-graph', requests: 17, transferred_bytes: 1248, elapsed_ms: 2500,
  logical_reads: 40, cache_hits: 20, joined_requests: 3, cache_bytes: 1024,
  revision: 'a'.repeat(40), input: 'private', arbitrary: 'never include'
} })!);
assert.deepEqual(online.pc4_online, { provider: 'hf-graph', requests: 17,
  transferred_bytes: 1248, logical_reads: 40, cache_hits: 20, joined_requests: 3,
  cache_bytes: 1024, elapsed_ms: 2500, revision: 'a'.repeat(40) });
const local = JSON.parse(localSearchProfileText({ event: 'progress', pc4_online: {
  provider: 'local-graph', requests: 0, transferred_bytes: 0, local_file_reads: 5,
  local_bytes: 4096, logical_reads: 30, elapsed_ms: 100, local_file_access: 'sync-access-handle', input: 'private'
} })!);
assert.deepEqual(local.pc4_online, { provider: 'local-graph', requests: 0,
  transferred_bytes: 0, local_file_reads: 5, local_bytes: 4096, logical_reads: 30, elapsed_ms: 100, local_file_access: 'sync-access-handle' });
assert.deepEqual(JSON.parse(localSearchProfileText({ event: 'progress', pc4_online: {
  provider: 'local-graph', local_file_access: '/private/location'
} })!).pc4_online, { provider: 'local-graph' });
assert.equal(localSearchProfileText({ event: 'final_response', search_profile: { input: 'private' } }), null);
const text = localSearchProfileText({ event: 'final_response', search_profile: {
  input: 'private', verifier_transport: { timings: {
    'consume.prepare': { count: 2, failed: 0, total_ms: 1.23456, max_ms: 1, input: 'private' },
    'unknown': { count: 1 }, 'finish.completed': { count: Infinity, total_ms: -1 }
  } }, minimum_parallel: { wave_count: 200, omitted_wave_count: 72,
    waves: Array.from({ length: 200 }, () => ({ wave: 1, first_receipt_ms: null, query_prepare_ms: NaN, input: 'private' })) }
} });
assert.ok(text);
const parsed = JSON.parse(text!);
assert.equal(parsed.verifier_transport['consume.prepare'].total_ms, 1.235);
assert.deepEqual(parsed.verifier_transport['finish.completed'], {});
assert.equal(parsed.minimum_parallel.waves.length, 128);
assert.deepEqual(parsed.minimum_parallel.waves[0], { wave: 1, first_receipt_ms: null });
assert.equal(text!.includes('private'), false);
assert.equal(text!.includes('unknown'), false);
const hostText = localSearchProfileText({ event: 'final_response', search_profile: {
  host_execution: { source_ms: 1.23456, parse_ms: Infinity, drain_ms: -1, input: 'private', unknown: 7 }
} });
assert.deepEqual(JSON.parse(hostText!).host_execution, { source_ms: 1.235 });

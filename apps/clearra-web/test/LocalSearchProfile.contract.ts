import assert from 'node:assert/strict';
import { isLocalSearchProfileMode, localSearchProfileText } from '../src/lib/localSearchProfile';

assert.equal(isLocalSearchProfileMode('local-recovery'), true);
assert.equal(isLocalSearchProfileMode('local-audit'), true);
for (const mode of ['production', 'development', 'test', '']) assert.equal(isLocalSearchProfileMode(mode), false);
assert.equal(localSearchProfileText({ event: 'progress', search_profile: {} }), null);
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

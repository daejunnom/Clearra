import assert from 'node:assert/strict';
import {
  createDefaultForwardSearchRequest, forwardSearchValidationCodes,
  MAX_REN_QUEUE_PIECES
} from '../src/lib/workspace/forwardSearchModel.ts';

assert.equal(MAX_REN_QUEUE_PIECES, 65_535);
const request = createDefaultForwardSearchRequest('ren');
for (const count of [23, MAX_REN_QUEUE_PIECES]) {
  assert.deepEqual(forwardSearchValidationCodes({ ...request, queue: 'I'.repeat(count) }), []);
}
assert.deepEqual(forwardSearchValidationCodes({ ...request, queue: 'I'.repeat(MAX_REN_QUEUE_PIECES + 1) }),
  ['ren_queue_too_long']);

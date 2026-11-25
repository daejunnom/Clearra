import assert from 'node:assert/strict';
import {
  buildForwardSearchCommandArguments,
  createDefaultForwardSearchRequest,
  forwardSearchValidationCodes,
  MAX_FORWARD_QUEUE_PIECES
} from '../src/lib/workspace/forwardSearchModel.ts';

assert.equal(MAX_FORWARD_QUEUE_PIECES, 65_535);
const spin = createDefaultForwardSearchRequest('spin-finder');
for (const queue of ['P7P2', 'P7P7', 'I'.repeat(MAX_FORWARD_QUEUE_PIECES - 1) + '[OT]']) {
  const request = { ...spin, queue };
  assert.deepEqual(forwardSearchValidationCodes(request), []);
  const arguments_ = buildForwardSearchCommandArguments(request);
  const patternFlag = arguments_.indexOf('--patterns');
  assert.ok(patternFlag >= 0);
  assert.equal(arguments_[patternFlag + 1], queue);
}
assert.deepEqual(
  forwardSearchValidationCodes({ ...spin, queue: 'I'.repeat(MAX_FORWARD_QUEUE_PIECES) + '[OT]' }),
  ['forward_queue_too_long']
);

for (const tool of ['damage', 'spin-finder'] as const) {
  const request = createDefaultForwardSearchRequest(tool);
  assert.deepEqual(forwardSearchValidationCodes({
    ...request, queue: 'I'.repeat(MAX_FORWARD_QUEUE_PIECES)
  }), []);
  assert.deepEqual(forwardSearchValidationCodes({
    ...request, queue: 'I'.repeat(MAX_FORWARD_QUEUE_PIECES + 1)
  }), ['forward_queue_too_long']);
}
for (const tool of ['damage', 'ren'] as const) {
  assert.deepEqual(forwardSearchValidationCodes({
    ...createDefaultForwardSearchRequest(tool), queue: 'P7P7'
  }), ['forward_queue_invalid']);
}

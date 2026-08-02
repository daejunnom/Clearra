import assert from 'node:assert/strict';
import test from 'node:test';

import {
  MAX_FORWARD_CHAIN,
  isValidForwardChain
} from '../src/lib/workspace/forwardSearchLimits.ts';

test('forward chain counters accept the complete u16 boundary', () => {
  assert.equal(MAX_FORWARD_CHAIN, 65_535);
  assert.equal(isValidForwardChain(0), true);
  assert.equal(isValidForwardChain(MAX_FORWARD_CHAIN), true);
});

test('forward chain counters reject values outside the host u16 contract', () => {
  assert.equal(isValidForwardChain(-1), false);
  assert.equal(isValidForwardChain(MAX_FORWARD_CHAIN + 1), false);
  assert.equal(isValidForwardChain(1.5), false);
  assert.equal(isValidForwardChain(Number.NaN), false);
});

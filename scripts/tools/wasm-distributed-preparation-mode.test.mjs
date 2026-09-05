import assert from 'node:assert/strict';
import test from 'node:test';

import {
  decodeWasmDistributedPreparationMode,
  dispatchWasmDistributedPreparation,
} from './wasm-distributed-preparation-mode.mjs';

test('ABI ready preparation bypasses serial replay and distributed workers', async () => {
  const calls = [];
  const mode = decodeWasmDistributedPreparationMode(3);
  const result = await dispatchWasmDistributedPreparation(mode, {
    serial() {
      calls.push('serial');
    },
    distributed() {
      calls.push('distributed');
    },
    ready() {
      calls.push('ready');
      return 'terminal';
    },
  });

  assert.deepEqual(mode, { label: 'ready', route: 'ready' });
  assert.equal(result, 'terminal');
  assert.deepEqual(calls, ['ready']);
});

test('unknown ABI distributed modes fail closed', () => {
  assert.throws(
    () => decodeWasmDistributedPreparationMode(4),
    /invalid Clearra WASM distributed mode 4/u,
  );
});

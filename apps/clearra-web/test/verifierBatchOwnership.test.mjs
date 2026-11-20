import assert from 'node:assert/strict';
import test from 'node:test';
import { takeVerifierBatch } from '../src/workers/verifierBatchOwnership.ts';

test('the verifier transfers one received batch to WASM without retaining it across yields', () => {
  const senderBatch = Uint8Array.of(19, 25, 80).buffer;
  const staged = structuredClone({ requestId: 3, batch: senderBatch }, { transfer: [senderBatch] });
  const received = staged.batch;
  assert.equal(senderBatch.byteLength, 0);
  assert.equal(takeVerifierBatch(staged), received);
  assert.deepEqual(new Uint8Array(received), Uint8Array.of(19, 25, 80));
  assert.equal(staged.batch, null);
  assert.equal(staged.requestId, 3);
  assert.throws(() => takeVerifierBatch(staged), /already consumed/u);
});

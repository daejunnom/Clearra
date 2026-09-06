import assert from 'node:assert/strict';

import { currentWasmArtifactGeneration } from '@clearra/ui/wasm-artifact-generation';

import {
  CLEARRA_WASM_ARTIFACT_SYNC_EVENT,
  CLEARRA_WASM_ARTIFACT_UPDATE_EVENT,
  installWasmArtifactHotUpdate,
  installWasmArtifactPolling,
  parseGeneration
} from '../src/lib/wasmArtifactHotUpdate';

let receive: ((payload: unknown) => void) | null = null;
let reconnect: (() => void) | null = null;
let dispose: (() => void) | null = null;
let offCount = 0;
let syncCount = 0;
const hot = {
  on(event: string, listener: (payload?: unknown) => void) {
    if (event === CLEARRA_WASM_ARTIFACT_UPDATE_EVENT) {
      receive = listener;
    } else {
      assert.equal(event, 'vite:ws:connect');
      reconnect = listener;
    }
  },
  off(event: string, listener: (payload?: unknown) => void) {
    if (event === CLEARRA_WASM_ARTIFACT_UPDATE_EVENT) {
      assert.equal(listener, receive);
    } else {
      assert.equal(event, 'vite:ws:connect');
      assert.equal(listener, reconnect);
    }
    offCount += 1;
  },
  send(event: typeof CLEARRA_WASM_ARTIFACT_SYNC_EVENT) {
    assert.equal(event, CLEARRA_WASM_ARTIFACT_SYNC_EVENT);
    syncCount += 1;
  },
  dispose(listener: () => void) {
    dispose = listener;
  }
};

const remove = installWasmArtifactHotUpdate(hot);
assert.ok(receive);
assert.ok(reconnect);
assert.ok(dispose);
assert.equal(syncCount, 1, 'mount must synchronize a generation broadcast it may have missed');
const initial = currentWasmArtifactGeneration();
(receive as (payload: unknown) => void)({ sourceSha256: 'invalid' });
assert.equal(currentWasmArtifactGeneration(), initial);

const payload = {
  sourceSha256: 'a'.repeat(64),
  bindingsSha256: 'b'.repeat(64),
  wasmSha256: 'c'.repeat(64)
};
assert.deepEqual(parseGeneration(payload), payload);
(receive as (payload: unknown) => void)(payload);
assert.notEqual(currentWasmArtifactGeneration(), initial);

(reconnect as () => void)();
assert.equal(syncCount, 2, 'WebSocket reconnect must replay the accepted server generation');

remove();
(dispose as () => void)();
assert.equal(offCount, 2, 'route and Vite disposal must share an idempotent listener cleanup');

console.log('wasm_artifact_hot_update_contract=passed');

const ticks: (() => void)[] = [];
let requests = 0;
let lastSignal: AbortSignal | undefined;
let finishRequest: ((value: unknown) => void) | undefined;
const removePolling = installWasmArtifactPolling((signal) => {
  lastSignal = signal;
  requests += 1;
  return new Promise((resolve) => { finishRequest = resolve; });
}, (callback) => {
  ticks.push(callback);
  return () => { ticks.length = 0; };
});
assert.equal(requests, 1);
assert.equal(ticks.length, 0, 'a slow endpoint must never create overlapping polls');
finishRequest?.(null);
await Promise.resolve();
assert.equal(ticks.length, 1, 'unavailable endpoint retries without reloading');
ticks.shift()?.();
assert.equal(requests, 2);
const beforeDisposedResponse = currentWasmArtifactGeneration();
removePolling();
assert.equal(lastSignal?.aborted, true);
finishRequest?.({ ...payload, wasmSha256: 'd'.repeat(64) });
await Promise.resolve();
assert.equal(ticks.length, 0);
assert.equal(currentWasmArtifactGeneration(), beforeDisposedResponse,
  'an in-flight response after route disposal cannot invalidate a newer route');
assert.equal(parseGeneration({ ...payload, wasmSha256: 'not-a-sha256' }), null);
console.log('wasm_artifact_polling_contract=passed');

import assert from 'node:assert/strict';

import {
  ClearraWasmRuntimeError,
  withArtifactDeadline
} from '../src/workers/clearraWasmRuntime.ts';

const resolved = await withArtifactDeadline('resolved artifact', 50, async () => 7);
assert.equal(resolved, 7);

let aborted = false;
await assert.rejects(
  withArtifactDeadline('stalled artifact', 20, (signal) =>
    new Promise<never>((_, reject) => {
      signal.addEventListener('abort', () => {
        aborted = true;
        reject(signal.reason);
      });
    })
  ),
  (error: unknown) => {
    assert.ok(error instanceof ClearraWasmRuntimeError);
    assert.equal(error.diagnosticCode, 'E_WASM_MODULE_LOAD_TIMEOUT');
    assert.match(error.message, /stalled artifact timed out after 20 ms/);
    return true;
  }
);
assert.equal(aborted, true);

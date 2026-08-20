import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import {
  assertClearraWasmAvailabilityExactnessExports,
  assertClearraWasmTerminalResponseIdentities,
  CLEARRA_WASM_AVAILABILITY_EXACTNESS_EXPORTS,
  ClearraWasmRuntimeError,
  normalizeWasmU32,
  withArtifactDeadline
} from '../src/workers/clearraWasmRuntime.ts';
import type { ClearraProductBuildIdentity } from '@clearra/ui/wasm';

const productIdentity: ClearraProductBuildIdentity = {
  source_commit: 'a'.repeat(40),
  engine_build_id: 'a'.repeat(40),
  contract_schema_version: 'clearra.search.contract.v2',
  supply_semantics_id: 'clearra.supply.projected-terminal-lookahead.v1',
  artifact_schema_version: 'clearra.solution-data.v1'
};

const terminalEnvelope = (identity: unknown) =>
  JSON.stringify([
    {
      schema_version: 1,
      runtime: 'clearra-wasm',
      job_id: 7,
      event: 'final_response',
      response: { runtime_identity: identity }
    }
  ]);

assert.equal(
  assertClearraWasmTerminalResponseIdentities(
    terminalEnvelope(productIdentity),
    productIdentity
  ),
  terminalEnvelope(productIdentity)
);
for (const key of Object.keys(productIdentity) as Array<keyof ClearraProductBuildIdentity>) {
  assert.throws(
    () =>
      assertClearraWasmTerminalResponseIdentities(
        terminalEnvelope({ ...productIdentity, [key]: `mismatched-${key}` }),
        productIdentity
      ),
    (error) =>
      error instanceof ClearraWasmRuntimeError &&
      error.diagnosticCode === 'E_WASM_RUNTIME_IDENTITY_MISMATCH',
    key
  );
}
assert.doesNotThrow(() =>
  assertClearraWasmTerminalResponseIdentities(
    JSON.stringify([{ event: 'failed', diagnostics: { diagnostics: [] } }]),
    productIdentity
  )
);

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

const completeExports = Object.fromEntries(
  CLEARRA_WASM_AVAILABILITY_EXACTNESS_EXPORTS.map((name) => [name, () => 1])
);

assert.doesNotThrow(() =>
  assertClearraWasmAvailabilityExactnessExports(completeExports)
);
assert.equal(normalizeWasmU32(-1), 0xffff_ffff);

for (const name of CLEARRA_WASM_AVAILABILITY_EXACTNESS_EXPORTS) {
  const incompleteExports = { ...completeExports };
  delete incompleteExports[name];
  assert.throws(
    () => assertClearraWasmAvailabilityExactnessExports(incompleteExports),
    (error) =>
      error instanceof ClearraWasmRuntimeError &&
      error.diagnosticCode === 'E_WASM_CAPABILITY_MISSING' &&
      error.message.includes(name),
    name
  );
}

const runtimeSource = await readFile(
  resolve(
    process.cwd(),
    'apps',
    'clearra-web',
    'src',
    'workers',
    'clearraWasmRuntime.ts'
  ),
  'utf8'
);
assert.match(
  runtimeSource,
  /function wrapRawModule\([\s\S]*assertClearraWasmAvailabilityExactnessExports\(raw\)/u
);

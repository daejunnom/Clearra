import assert from 'node:assert/strict';
import test from 'node:test';

import {
  clearraWasmCapabilitiesSha256,
  productBuildIdentityFromEnvironment,
} from './clearra-wasm-build-contract.mjs';
import {
  expectedWasmProbeIdentity,
  validateWasmProbeTerminal,
} from './wasm-product-terminal-contract.mjs';

const commit = 'a'.repeat(40);
const identity = productBuildIdentityFromEnvironment({
  CLEARRA_SOURCE_COMMIT: commit,
  CLEARRA_ENGINE_BUILD_ID: commit,
});
const manifest = {
  schema_version: 1,
  build: {
    contract_version: 2,
    source_sha256: 'b'.repeat(64),
    source_file_count: 1,
    capabilities_sha256: clearraWasmCapabilitiesSha256(),
    runtime_identity: identity,
  },
};
const successfulEvent = {
  event: 'final_response',
  response: { status: 'success', runtime_identity: identity },
};

test('actual WASM probe accepts exactly one successful manifest-bound final response', () => {
  const expected = expectedWasmProbeIdentity(manifest, commit);
  assert.equal(
    validateWasmProbeTerminal({
      events: [successfulEvent],
      terminalStatus: 1,
      expectedRuntimeIdentity: expected,
    }),
    successfulEvent,
  );
});

test('actual WASM probe rejects non-success terminal states and events', () => {
  for (const terminalStatus of [0, 2, 3, 4]) {
    assert.throws(
      () =>
        validateWasmProbeTerminal({
          events: [successfulEvent],
          terminalStatus,
          expectedRuntimeIdentity: identity,
        }),
      /did not complete successfully/u,
    );
  }
  for (const event of [{ event: 'failed' }, { event: 'cancelled' }]) {
    assert.throws(
      () =>
        validateWasmProbeTerminal({
          events: [event, successfulEvent],
          terminalStatus: 1,
          expectedRuntimeIdentity: identity,
        }),
      /failed or cancelled/u,
    );
  }
});

test('actual WASM probe rejects missing, duplicate, or unsuccessful final responses', () => {
  assert.throws(
    () => validateWasmProbeTerminal({ events: [], terminalStatus: 1 }),
    /exactly one final response/u,
  );
  assert.throws(
    () =>
      validateWasmProbeTerminal({
        events: [successfulEvent, successfulEvent],
        terminalStatus: 1,
      }),
    /exactly one final response/u,
  );
  assert.throws(
    () =>
      validateWasmProbeTerminal({
        events: [{ ...successfulEvent, response: { ...successfulEvent.response, status: 'failed' } }],
        terminalStatus: 1,
      }),
    /status is not success/u,
  );
});

test('actual WASM probe rejects manifest, source, and final identity drift', () => {
  assert.throws(
    () => expectedWasmProbeIdentity(manifest, 'not-a-commit'),
    /expected source/u,
  );
  assert.throws(
    () => expectedWasmProbeIdentity(manifest, 'c'.repeat(40)),
    /does not match the expected source/u,
  );
  assert.throws(
    () =>
      validateWasmProbeTerminal({
        events: [
          {
            ...successfulEvent,
            response: {
              ...successfulEvent.response,
              runtime_identity: {
                ...identity,
                artifact_schema_version: 'clearra.solution-data.v0',
              },
            },
          },
        ],
        terminalStatus: 1,
        expectedRuntimeIdentity: identity,
      }),
    /runtime identity is invalid/u,
  );
  assert.throws(
    () =>
      expectedWasmProbeIdentity(
        {
          ...manifest,
          build: {
            ...manifest.build,
            runtime_identity: { ...identity, extra: true },
          },
        },
        commit,
      ),
    /exactly five fields/u,
  );
});

import {
  CLEARRA_ARTIFACT_SCHEMA_VERSION,
  CLEARRA_CONTRACT_SCHEMA_VERSION,
  CLEARRA_SUPPLY_SEMANTICS_ID,
  CLEARRA_UNVERIFIED_BUILD_ID,
  isClearraWasmBuildContract,
  isProductBuildIdentity,
  productBuildIdentitiesEqual,
} from './clearra-wasm-build-contract.mjs';

const SUCCESS_TERMINAL_STATUS = 1;

export function expectedWasmProbeIdentity(manifest, expectedSourceCommit) {
  const expected = String(expectedSourceCommit ?? '').trim().toLowerCase();
  if (!isCommit(expected) && expected !== CLEARRA_UNVERIFIED_BUILD_ID) {
    throw new Error(
      'WASM probe expected source must be a full lowercase commit or unverified-local-build',
    );
  }
  if (!manifest || typeof manifest !== 'object' || manifest.schema_version !== 1) {
    throw new Error('WASM probe manifest schema is invalid');
  }
  if (!isClearraWasmBuildContract(manifest.build)) {
    throw new Error('WASM probe manifest build contract is invalid');
  }
  const identity = manifest.build.runtime_identity;
  assertExactIdentityShape(identity, 'WASM probe manifest');
  if (identity.source_commit !== expected || identity.engine_build_id !== expected) {
    throw new Error('WASM probe manifest does not match the expected source identity');
  }
  return identity;
}

export function validateWasmProbeTerminal({
  events,
  terminalStatus,
  expectedRuntimeIdentity = null,
}) {
  if (terminalStatus !== SUCCESS_TERMINAL_STATUS) {
    throw new Error(`WASM probe did not complete successfully (status ${terminalStatus})`);
  }
  if (!Array.isArray(events)) {
    throw new Error('WASM probe event payload must be an array');
  }
  const failed = events.filter((event) => event?.event === 'failed');
  const cancelled = events.filter((event) => event?.event === 'cancelled');
  const finals = events.filter((event) => event?.event === 'final_response');
  if (failed.length > 0 || cancelled.length > 0) {
    throw new Error('WASM probe emitted a failed or cancelled terminal event');
  }
  if (finals.length !== 1) {
    throw new Error(`WASM probe must emit exactly one final response (received ${finals.length})`);
  }
  const finalEvent = finals[0];
  if (!finalEvent.response || finalEvent.response.status !== 'success') {
    throw new Error('WASM probe final response status is not success');
  }
  if (expectedRuntimeIdentity !== null) {
    if (!isProductBuildIdentity(expectedRuntimeIdentity)) {
      throw new Error('WASM probe expected runtime identity is invalid');
    }
    assertExactIdentityShape(expectedRuntimeIdentity, 'WASM probe expected');
    const actualIdentity = finalEvent.response.runtime_identity;
    assertExactIdentityShape(actualIdentity, 'WASM probe final response');
    if (!productBuildIdentitiesEqual(actualIdentity, expectedRuntimeIdentity)) {
      throw new Error('WASM probe final response identity does not match its manifest');
    }
  }
  return finalEvent;
}

function assertExactIdentityShape(value, label) {
  if (!isProductBuildIdentity(value)) {
    throw new Error(`${label} runtime identity is invalid`);
  }
  const keys = Object.keys(value).sort();
  const expectedKeys = [
    'artifact_schema_version',
    'contract_schema_version',
    'engine_build_id',
    'source_commit',
    'supply_semantics_id',
  ];
  if (keys.length !== expectedKeys.length || keys.some((key, index) => key !== expectedKeys[index])) {
    throw new Error(`${label} runtime identity must contain exactly five fields`);
  }
  if (
    value.contract_schema_version !== CLEARRA_CONTRACT_SCHEMA_VERSION ||
    value.supply_semantics_id !== CLEARRA_SUPPLY_SEMANTICS_ID ||
    value.artifact_schema_version !== CLEARRA_ARTIFACT_SCHEMA_VERSION
  ) {
    throw new Error(`${label} runtime identity uses an unsupported product contract`);
  }
}

function isCommit(value) {
  return typeof value === 'string' && /^[0-9a-f]{40}$/.test(value);
}

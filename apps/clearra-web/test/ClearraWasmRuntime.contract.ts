import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import {
  assertClearraWasmAvailabilityExactnessExports,
  assertClearraWasmTerminalResponseIdentities,
  CLEARRA_WASM_AVAILABILITY_EXACTNESS_EXPORTS,
  ClearraWasmRuntimeError,
  normalizeWasmU32,
  requireProductPageDecimal,
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

const resourceReport = {
  solver_executed: false,
  memory_status: 'not-executed',
  truncated: false,
  truncation_reason: null,
  peak_frontier_states: 0,
  peak_candidate_rows: 0,
  peak_hash_buckets: 0,
  peak_gpu_bytes: 0,
  peak_cpu_bytes: 0,
  build_worker_backlog_peak: 0,
  coverage_rows_emitted: 0,
  probability_complete: false,
  execution_availability: {
    state: 'unavailable',
    reason: 'dense-pattern-representation-unavailable',
    surface: 'browser-wasm32',
    descriptor_pattern_count: '35384428800',
    dense_pattern_count: '35384428800',
    required_dense_bytes: '4423053600',
    required_memory_bytes: '8846107200'
  },
  result_completeness: 'not-executed'
} as const;
const typedResourceError = ClearraWasmRuntimeError.fromRuntimeOutput(
  JSON.stringify({
    code: 'E_WASM_DISTRIBUTED_START',
    message: 'dense_pattern_representation_unavailable',
    resource_report: resourceReport
  })
);
assert.equal(typedResourceError.diagnosticCode, 'E_WASM_DISTRIBUTED_START');
assert.equal(typedResourceError.resourceReport?.solver_executed, false);
assert.deepEqual(typedResourceError.resourceReport, resourceReport);

const invalidResourceError = ClearraWasmRuntimeError.fromRuntimeOutput(
  JSON.stringify({
    code: 'E_WASM_DISTRIBUTED_START',
    message: 'invalid report must fail closed',
    resource_report: {
      ...resourceReport,
      execution_availability: {
        ...resourceReport.execution_availability,
        required_dense_bytes: '1'
      }
    }
  })
);
assert.equal(invalidResourceError.diagnosticCode, 'E_WASM_RESOURCE_REPORT_INVALID');
assert.equal(invalidResourceError.resourceReport, null);

const legacyRuntimeError = ClearraWasmRuntimeError.fromRuntimeOutput(
  'E_WASM_LEGACY_FAILURE: legacy text remains supported'
);
assert.equal(legacyRuntimeError.diagnosticCode, 'E_WASM_LEGACY_FAILURE');
assert.equal(legacyRuntimeError.resourceReport, null);

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
assert.equal(requireProductPageDecimal('1', 'alternative index'), '1');
assert.equal(
  requireProductPageDecimal('184467440737095516160', 'alternative index'),
  '184467440737095516160'
);
for (const invalidPageNumber of [
  '',
  '0',
  '-1',
  '01',
  '1.5',
  'NaN',
  ' 1',
  '1\n2'
]) {
  assert.throws(
    () => requireProductPageDecimal(invalidPageNumber, 'alternative index'),
    (error) =>
      error instanceof ClearraWasmRuntimeError &&
      error.diagnosticCode === 'E_WASM_PRODUCT_PAGE_RANGE',
    `invalid product page coordinate ${invalidPageNumber}`
  );
}

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
const outputTextReader = runtimeSource.indexOf('const outputText = () => {');
const outputBytesReader = runtimeSource.indexOf('const outputBytes = () => {');
const outputTextRelease = runtimeSource.indexOf(
  'raw.clearra_wasm_output_release();',
  outputTextReader
);
const outputBytesRelease = runtimeSource.indexOf(
  'raw.clearra_wasm_output_release();',
  outputBytesReader
);
assert.ok(
  outputTextReader >= 0 &&
    outputTextRelease > outputTextReader &&
    outputTextRelease < outputBytesReader,
  'text output is released immediately after the host copy'
);
assert.ok(
  outputBytesReader >= 0 && outputBytesRelease > outputBytesReader,
  'binary output is released immediately after the host copy'
);
assert.match(
  runtimeSource.slice(outputTextReader, outputBytesReader),
  /finally\s*\{\s*raw\.clearra_wasm_output_release\(\);\s*\}/u,
  'text output releases even when exactness validation or UTF-8 decoding throws'
);
assert.match(
  runtimeSource.slice(outputBytesReader, runtimeSource.indexOf('const lastPanic = () => {')),
  /finally\s*\{\s*raw\.clearra_wasm_output_release\(\);\s*\}/u,
  'binary output releases even when exactness validation or copying throws'
);
assert.match(
  runtimeSource,
  /if \(status === ABI_OUTPUT_NOT_RELEASED\) \{[\s\S]*?raw\.clearra_wasm_output_release\(\);[\s\S]*?E_WASM_OUTPUT_NOT_RELEASED/u,
  'the host maps the allocation-free outstanding-owner status without decoding stale output'
);
assert.doesNotMatch(
  runtimeSource.slice(
    runtimeSource.indexOf('product_page_get(alternativeIndex, memberPageNumber) {'),
    runtimeSource.indexOf('product_page_release() {')
  ),
  />>>\s*0/u,
  'product page coordinates must never be silently truncated or wrapped'
);
assert.match(
  runtimeSource.slice(
    runtimeSource.indexOf('product_page_get(alternativeIndex, memberPageNumber) {'),
    runtimeSource.indexOf('product_page_release() {')
  ),
  /clearra_wasm_product_page_get_exact\(\)/u,
  'product page requests cross the WASM boundary through exact decimal text'
);

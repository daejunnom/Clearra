import assert from 'node:assert/strict';
import { get } from 'svelte/store';
import type {
  ClearraHostAppResponse,
  ClearraWasmWorkerEvent
} from '../src/lib/wasm/wasmCommandClient';
import {
  deferWasmTerminalResponse,
  formatWasmTerminalLine,
  formatWasmTerminalTranscript
} from '../src/lib/wasm/wasmTerminalTranscript';
import {
  applyWasmWorkerEvent,
  clearWasmTerminalResult,
  wasmWorkerState
} from '../src/lib/wasm/wasmWorkerStore';

// Completion retains the exact response but does not format it until an
// actual terminal consumer requests text. Diagnostics and history keep order.
const initialState = get(wasmWorkerState);
let formatCalls = 0;
const payload = {
  status: 'success',
  diagnostics: [{ code: 'I_TEST', severity: 'info', message: 'diagnostic remains intact' }],
  result: { kind: 'build-probability', fields: [{ key: 'large_count', value: '35384428800' }] },
  resource_report: null
};
const response = {
  ...payload,
  toJSON() {
    formatCalls += 1;
    return payload;
  }
} as unknown as ClearraHostAppResponse;

try {
  const entry = deferWasmTerminalResponse(response);
  assert.equal(formatCalls, 0, 'entry creation cannot eagerly serialize');
  assert.equal(entry.response, response, 'result owner is shared without copying or trimming');
  const expected = JSON.stringify(payload, null, 2);
  assert.equal(formatWasmTerminalLine(entry), expected);
  assert.equal(entry.response, null, 'successful formatting releases the pending response owner');
  assert.equal(formatCalls, 1);
  assert.equal(formatWasmTerminalTranscript(['before', entry, 'after']), `before\n${expected}\nafter`);
  assert.equal(formatCalls, 1, 'later transcript rendering reuses the cached text');

  applyWasmWorkerEvent({
    event: 'final_response', job_id: 901, response, search_report: null, webgpu_backend: null
  } as unknown as ClearraWasmWorkerEvent);
  const completed = get(wasmWorkerState);
  assert.equal(formatCalls, 1, 'product completion must not format terminal-only JSON');
  assert.equal(completed.status, 'completed');
  assert.equal(completed.response, response);
  assert.equal(completed.diagnostics, response.diagnostics);
  assert.equal(formatWasmTerminalLine(completed.terminalLines.at(-1) ?? ''), expected);
  assert.equal(formatCalls, 2, 'the new completion entry formats once when requested');
  const completedEntry = completed.terminalLines.at(-1);
  assert.ok(completedEntry && typeof completedEntry !== 'string');
  assert.equal(completedEntry.response, null);
  assert.equal(completed.response, response, 'releasing transcript ownership cannot clear the product result');
  assert.equal(formatWasmTerminalLine(completed.terminalLines.at(-1) ?? ''), expected);
  assert.equal(formatCalls, 2);

  let failedAttempts = 0;
  const transientResponse = {
    ...payload,
    toJSON() {
      failedAttempts += 1;
      if (failedAttempts === 1) throw new Error('transient presentation failure');
      return payload;
    }
  } as unknown as ClearraHostAppResponse;
  const retryEntry = deferWasmTerminalResponse(transientResponse);
  assert.match(formatWasmTerminalLine(retryEntry), /E_WASM_TERMINAL_FORMAT/);
  assert.equal(retryEntry.response, transientResponse, 'format failure preserves the retry owner');
  assert.equal(get(wasmWorkerState).status, 'completed');
  assert.equal(get(wasmWorkerState).diagnostics, response.diagnostics);
  assert.equal(formatWasmTerminalLine(retryEntry), expected);
  assert.equal(retryEntry.response, null);
  assert.equal(failedAttempts, 2);
  assert.equal(formatWasmTerminalLine(retryEntry), expected);
  assert.equal(failedAttempts, 2, 'successful retry is cached');

  const failedResponse = { ...payload, status: 'execution-failed' } as unknown as ClearraHostAppResponse;
  applyWasmWorkerEvent({
    event: 'failed', job_id: 902, response: failedResponse,
    diagnostics: { diagnostics: failedResponse.diagnostics }
  } as unknown as ClearraWasmWorkerEvent);
  const failed = get(wasmWorkerState);
  assert.equal(failed.status, 'failed');
  assert.equal(failed.response, failedResponse);
  assert.match(failed.error ?? '', /I_TEST: diagnostic remains intact/);
  assert.equal(failed.terminalLines.length, completed.terminalLines.length + 1);
  assert.equal(
    formatWasmTerminalTranscript(failed.terminalLines),
    `${formatWasmTerminalTranscript(completed.terminalLines)}\n${JSON.stringify(failedResponse, null, 2)}`
  );

  clearWasmTerminalResult();
  assert.deepEqual(get(wasmWorkerState).terminalLines, ['clearra web runtime ready']);
  assert.equal(get(wasmWorkerState).response, null);
} finally {
  wasmWorkerState.set(initialState);
}

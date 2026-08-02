import assert from 'node:assert/strict';

import { get } from 'svelte/store';

import { WasmTerminalWorkerController } from '../src/lib/wasm/WasmTerminalWorkerController';
import {
  createWasmWorkerOwnerId,
  listenForWasmOwnerTermination,
  signalWasmOwnerTermination
} from '../src/lib/wasm/wasmWorkerLifecycle';
import {
  applyWasmWorkerEvent,
  clearWasmTerminalResult,
  updateWasmCommandText,
  wasmWorkerState
} from '../src/lib/wasm/wasmWorkerStore';
import type { ClearraWasmWorkerEvent } from '../src/lib/wasm/wasmCommandClient';

const originalState = get(wasmWorkerState);

async function cooperativeCancellationRemainsCancellation() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.run();
  worker.emit(started(11));
  controller.cancel();
  worker.emit(cancelled(11));
  await delay(0);

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'cancelled');
  assert.equal(state.terminationReason, null);
  assert.equal(worker.terminateCount, 1);
}

async function forcedCancellationIsDistinct() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.run();
  worker.emit(started(12));
  controller.cancel();
  await delay(130);

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'terminated');
  assert.equal(state.terminationReason, 'cancel-timeout');
  assert.equal(state.diagnostics[0]?.code, 'E_WASM_FORCED_TERMINATION');
  assert.equal(worker.terminateCount, 1);
}

async function realTerminalEventWinsCancellationRace() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.run();
  worker.emit(started(13));
  controller.cancel();
  worker.emit({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'failed',
    job_id: 13,
    diagnostics: {
      diagnostics: [
        {
          code: 'E_TEST_REAL_FAILURE',
          severity: 'error',
          message: 'real terminal failure'
        }
      ]
    }
  });
  await delay(130);

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'failed');
  assert.equal(state.terminationReason, null);
  assert.equal(state.diagnostics[0]?.code, 'E_TEST_REAL_FAILURE');
  assert.equal(worker.terminateCount, 1);
}

async function ownerDisposalIsForceTermination() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.run();
  worker.emit(started(14));
  controller.dispose();

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'terminated');
  assert.equal(state.terminationReason, 'owner-disposed');
  assert.equal(state.diagnostics[0]?.code, 'E_WASM_OWNER_DISPOSED');
  assert.equal(worker.terminateCount, 1);
}

async function ownerTerminationReachesDescendants() {
  const ownerId = createWasmWorkerOwnerId();
  const reason = new Promise<string>((resolve) => {
    const close = listenForWasmOwnerTermination(ownerId, (received) => {
      close();
      resolve(received);
    });
  });
  signalWasmOwnerTermination(ownerId, 'worker-failure');
  assert.equal(await reason, 'worker-failure');
}

async function duplicateRunIsRejectedBeforePosting() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);

  assert.equal(controller.run(), true);
  assert.equal(controller.run(), false);
  assert.equal(
    worker.messages.filter(
      (message) => (message as { type?: string }).type === 'run_command_text'
    ).length,
    1
  );

  controller.dispose();
}

async function workerCreationFailureBecomesTerminalFailure() {
  resetState();
  const controller = new WasmTerminalWorkerController(() => {
    throw new Error('worker construction failed');
  });

  assert.equal(controller.run(), false);
  const state = get(wasmWorkerState);
  assert.equal(state.status, 'failed');
  assert.equal(state.diagnostics[0]?.code, 'E_WASM_WORKER_CREATE_FAILED');
}

async function nonSuccessFinalResponseRemainsFailure() {
  resetState();
  applyWasmWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'final_response',
    job_id: 17,
    response: {
      command: 'setup-finder',
      status: 'execution-failed',
      result: null,
      diagnostics: [
        {
          code: 'E_TEST_EXECUTION_FAILED',
          severity: 'error',
          message: 'execution failed'
        }
      ]
    },
    search_report: null,
    webgpu_backend: null
  } as unknown as ClearraWasmWorkerEvent);

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'failed');
  assert.equal(state.error, 'E_TEST_EXECUTION_FAILED: execution failed');
  assert.equal(state.response?.status, 'execution-failed');
}

async function clearingTerminalResultReleasesLogPayloads() {
  resetState();
  wasmWorkerState.update((state) => ({
    ...state,
    status: 'completed',
    terminalLines: ['large serialized response']
  }));
  clearWasmTerminalResult();

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'idle');
  assert.deepEqual(state.terminalLines, ['clearra web runtime ready']);
}

function controllerFor(worker: FakeWorker) {
  return new WasmTerminalWorkerController(() => worker as unknown as Worker);
}

function resetState() {
  wasmWorkerState.set({
    ...originalState,
    request: { ...originalState.request },
    jobId: null,
    status: 'idle',
    terminationReason: null,
    progressLabel: '',
    progressDone: 0,
    progressTotal: 0,
    progressTelemetry: null,
    terminalLines: [],
    diagnostics: [],
    response: null,
    searchReport: null,
    webgpuBackend: null,
    error: null
  });
  updateWasmCommandText('clearra verify kicks');
}

function started(jobId: number): ClearraWasmWorkerEvent {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'started',
    job_id: jobId
  };
}

function cancelled(jobId: number): ClearraWasmWorkerEvent {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'cancelled',
    job_id: jobId,
    scope_released: true
  };
}

function delay(milliseconds: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, milliseconds));
}

class FakeWorker {
  onmessage: ((event: MessageEvent<ClearraWasmWorkerEvent>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  messages: unknown[] = [];
  terminateCount = 0;

  postMessage(message: unknown) {
    this.messages.push(message);
  }

  terminate() {
    this.terminateCount += 1;
  }

  emit(event: ClearraWasmWorkerEvent) {
    this.onmessage?.({ data: event } as MessageEvent<ClearraWasmWorkerEvent>);
  }
}

try {
  await cooperativeCancellationRemainsCancellation();
  await forcedCancellationIsDistinct();
  await realTerminalEventWinsCancellationRace();
  await ownerDisposalIsForceTermination();
  await ownerTerminationReachesDescendants();
  await duplicateRunIsRejectedBeforePosting();
  await workerCreationFailureBecomesTerminalFailure();
  await nonSuccessFinalResponseRemainsFailure();
  await clearingTerminalResultReleasesLogPayloads();
} finally {
  wasmWorkerState.set(originalState);
}

console.log(
  JSON.stringify({
    cooperative_cancel: 'cancelled',
    cancel_timeout: 'terminated',
    terminal_race: 'preserved',
    descendant_release_signal: 'delivered',
    duplicate_run: 'rejected',
    worker_creation_failure: 'reported',
    non_success_response: 'failed',
    terminal_log_release: 'cleared'
  })
);

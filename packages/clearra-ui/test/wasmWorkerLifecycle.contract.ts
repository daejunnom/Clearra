import assert from 'node:assert/strict';

// SRP rationale: this executable contract has one change reason: the browser
// WASM worker lifecycle and its terminal-state arbitration semantics change.

import { get } from 'svelte/store';

import { WasmTerminalWorkerController } from '../src/lib/wasm/WasmTerminalWorkerController';
import {
  createWasmWorkerOwnerId,
  listenForWasmOwnerTermination,
  signalWasmOwnerTermination
} from '../src/lib/wasm/wasmWorkerLifecycle';
import {
  announceWasmArtifactGeneration,
  currentWasmArtifactGeneration,
  isCurrentWasmArtifactGeneration
} from '../src/lib/wasm/wasmArtifactGeneration';
import {
  applyWasmWorkerEvent,
  clearWasmTerminalResult,
  runWasmCommand,
  updateWasmCommandText,
  wasmWorkerState
} from '../src/lib/wasm/wasmWorkerStore';
import type {
  ClearraProductPageWorkerEvent,
  ClearraSolutionPageWorkerEvent,
  ClearraWasmWorkerEvent
} from '../src/lib/wasm/wasmCommandClient';
import { workspaceViewFromWasm } from '../src/lib/workspace/workspaceRuntime';

const originalState = get(wasmWorkerState);
assert.doesNotMatch(originalState.request.commandText, /\bverify\b/i);

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

async function boundedProgressWatchdogCoversPreparationAndSerialSearchStalls() {
  resetState();
  const startupWorker = new FakeWorker();
  const startupController = new WasmTerminalWorkerController(
    () => startupWorker as unknown as Worker,
    undefined,
    { preparationProgressStallTimeoutMs: 20, searchProgressStallTimeoutMs: 20 }
  );
  assert.equal(startupController.run(), true);
  startupWorker.emit(started(15));
  await delay(35);
  let state = get(wasmWorkerState);
  assert.equal(state.status, 'terminated');
  assert.equal(state.diagnostics[0]?.code, 'E_WASM_PREPARATION_PROGRESS_STALLED');
  assert.equal(
    startupWorker.terminateCount,
    1,
    'a started runtime that never reaches preparation progress must fail closed'
  );

  resetState();
  const preparationWorker = new FakeWorker();
  const preparationController = new WasmTerminalWorkerController(
    () => preparationWorker as unknown as Worker,
    undefined,
    { preparationProgressStallTimeoutMs: 20, searchProgressStallTimeoutMs: 1_000 }
  );
  assert.equal(preparationController.run(), true);
  preparationWorker.emit(started(16));
  preparationWorker.emit(progress(16, 'preparing', 0));
  await delay(35);

  state = get(wasmWorkerState);
  assert.equal(state.status, 'terminated');
  assert.equal(state.terminationReason, 'worker-failure');
  assert.equal(state.diagnostics[0]?.code, 'E_WASM_PREPARATION_PROGRESS_STALLED');
  assert.equal(preparationWorker.terminateCount, 1);

  resetState();
  const stalledWorker = new FakeWorker();
  const stalledController = new WasmTerminalWorkerController(
    () => stalledWorker as unknown as Worker,
    undefined,
    { searchProgressStallTimeoutMs: 20 }
  );
  assert.equal(stalledController.run(), true);
  stalledWorker.emit(started(17));
  stalledWorker.emit(progress(17, 'searching', 1, 'serial'));
  await delay(35);

  state = get(wasmWorkerState);
  assert.equal(state.status, 'terminated');
  assert.equal(state.terminationReason, 'worker-failure');
  assert.equal(state.diagnostics[0]?.code, 'E_WASM_SEARCH_PROGRESS_STALLED');
  assert.equal(stalledWorker.terminateCount, 1);
}

async function boundedProgressWatchdogRequiresAndAcceptsChangedDistributedWork() {
  resetState();
  const worker = new FakeWorker();
  const controller = new WasmTerminalWorkerController(
    () => worker as unknown as Worker,
    undefined,
    { searchProgressStallTimeoutMs: 30 }
  );
  assert.equal(controller.run(), true);
  worker.emit(started(18));
  worker.emit(progress(18, 'searching', 1, 'distributed'));
  await delay(20);
  worker.emit(progress(18, 'searching', 1, 'distributed'));
  await delay(20);

  assert.equal(
    worker.terminateCount,
    1,
    'an unchanged heartbeat must not renew the bounded-progress lease'
  );

  resetState();
  const progressingWorker = new FakeWorker();
  const progressingController = new WasmTerminalWorkerController(
    () => progressingWorker as unknown as Worker,
    undefined,
    { searchProgressStallTimeoutMs: 30 }
  );
  assert.equal(progressingController.run(), true);
  progressingWorker.emit(started(19));
  progressingWorker.emit(progress(19, 'searching', 1, 'distributed'));
  await delay(20);
  progressingWorker.emit(progress(19, 'searching', 2, 'distributed'));
  await delay(20);
  assert.equal(
    progressingWorker.terminateCount,
    0,
    'changed distributed bounded-work evidence must renew the progress lease'
  );
  progressingController.dispose();
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

async function failedResponsePreservesTypedResourceEvidence() {
  resetState();
  const availability = {
    state: 'unavailable' as const,
    reason: 'dense-pattern-representation-unavailable' as const,
    surface: 'browser-wasm32' as const,
    descriptor_pattern_count: '35384428800',
    dense_pattern_count: '35384428800',
    required_dense_bytes: '4423053600',
    required_memory_bytes: '8846107200'
  };
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
    execution_availability: availability,
    result_completeness: 'not-executed' as const
  };
  applyWasmWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'failed',
    job_id: 18,
    diagnostics: {
      diagnostics: [
        {
          code: 'E_RESOURCE_ADMISSION',
          severity: 'error',
          message: 'dense execution unavailable'
        }
      ]
    },
    resource_report: resourceReport,
    execution_availability: availability,
    result_completeness: 'not-executed',
    response: {
      status: 'execution-failed',
      diagnostics: [
        {
          code: 'E_RESOURCE_ADMISSION',
          severity: 'error',
          message: 'dense execution unavailable'
        }
      ],
      resource_report: resourceReport
    }
  } as unknown as ClearraWasmWorkerEvent);

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'failed');
  assert.equal(state.response?.resource_report.solver_executed, false);
  assert.equal(
    state.response?.resource_report.execution_availability.required_dense_bytes,
    '4423053600'
  );
  assert.equal(
    state.response?.resource_report.execution_availability.required_memory_bytes,
    '8846107200'
  );
  assert.equal(state.response?.resource_report.result_completeness, 'not-executed');
  assert.match(state.terminalLines.at(-1) ?? '', /35384428800/u);
}

async function responseNullFailurePreservesTopLevelResourceAxes() {
  resetState();
  const availability = {
    state: 'exhausted' as const,
    reason: 'memory-budget-exceeded' as const,
    surface: 'browser-wasm32' as const,
    descriptor_pattern_count: '35384428800',
    dense_pattern_count: '35384428800',
    required_dense_bytes: '4423053600',
    required_memory_bytes: '8846107200'
  };
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
    execution_availability: availability,
    result_completeness: 'not-executed' as const
  };
  applyWasmWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'failed',
    job_id: 19,
    diagnostics: { diagnostics: [] },
    response: null,
    resource_report: resourceReport,
    execution_availability: availability,
    result_completeness: 'not-executed'
  });

  let state = get(wasmWorkerState);
  assert.equal(state.status, 'failed');
  assert.equal(state.response, null);
  assert.deepEqual(state.resourceReport, resourceReport);
  assert.deepEqual(state.executionAvailability, availability);
  assert.equal(state.resultCompleteness, 'not-executed');
  assert.deepEqual(workspaceViewFromWasm(state).resourceReport, resourceReport);

  const worker = new FakeWorker();
  runWasmCommand(worker as unknown as Worker);
  state = get(wasmWorkerState);
  assert.equal(state.resourceReport, null);
  assert.equal(state.executionAvailability, null);
  assert.equal(state.resultCompleteness, null);
  assert.equal(workspaceViewFromWasm(state).resourceReport, null);
}

async function inconsistentFailedResourceReportsAreRejected() {
  resetState();
  const availability = {
    state: 'exhausted' as const,
    reason: 'memory-budget-exceeded' as const,
    surface: 'browser-wasm32' as const,
    descriptor_pattern_count: '1058400',
    dense_pattern_count: '1058400',
    required_dense_bytes: '132304',
    required_memory_bytes: '17066704'
  };
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
    execution_availability: availability,
    result_completeness: 'not-executed' as const
  };
  applyWasmWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'failed',
    job_id: 20,
    diagnostics: { diagnostics: [] },
    response: {
      status: 'execution-failed',
      diagnostics: [],
      resource_report: resourceReport
    },
    resource_report: {
      ...resourceReport,
      execution_availability: {
        ...availability,
        required_memory_bytes: '17066705'
      }
    },
    execution_availability: availability,
    result_completeness: 'not-executed'
  } as unknown as ClearraWasmWorkerEvent);

  const state = get(wasmWorkerState);
  assert.equal(state.status, 'failed');
  assert.equal(state.response, null);
  assert.equal(state.resourceReport, null);
  assert.equal(state.executionAvailability, null);
  assert.equal(state.resultCompleteness, 'incomplete');
  assert.equal(state.diagnostics.at(-1)?.code, 'E_WASM_RESOURCE_REPORT_MISMATCH');
  assert.equal(workspaceViewFromWasm(state).resourceReport, null);
}

async function legacyStringFailureRemainsIncomplete() {
  resetState();
  applyWasmWorkerEvent({
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'failed',
    job_id: 21,
    diagnostics: {
      diagnostics: [
        { code: 'E_WASM_LEGACY_FAILURE', severity: 'error', message: 'legacy failure' }
      ]
    }
  });

  const state = get(wasmWorkerState);
  assert.equal(state.resourceReport, null);
  assert.equal(state.executionAvailability, null);
  assert.equal(state.resultCompleteness, 'incomplete');
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

async function abortingSolutionPageRequestReleasesControllerOwnership() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.prewarm(1);
  const abort = new AbortController();
  const request = controller.loadSolutionPage(0, 1, abort.signal);
  const reason = new Error('solution page aborted by test');
  reason.name = 'AbortError';
  abort.abort(reason);

  await assert.rejects(request, reason);
  const message = worker.messages.find(
    (candidate) => (candidate as { type?: string }).type === 'load_solution_page'
  ) as { requestId: number };
  worker.emit({
    type: 'solution_page',
    request_id: message.requestId,
    offset: 0,
    total: 1,
    keys: ['late-key']
  });
  controller.dispose();
}

async function newRunRejectsOutstandingSolutionPageRequest() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.prewarm(1);
  worker.emit({
    type: 'runtime_prewarm',
    phase: 'finished',
    workerCount: 1
  } as unknown as ClearraWasmWorkerEvent);
  const request = controller.loadSolutionPage(0, 1);

  assert.equal(controller.run(), true);
  await assert.rejects(request, /new search replaced/);
  controller.dispose();
}

async function outstandingSolutionPageRequestPreventsWorkerTransfer() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.prewarm(1);
  worker.emit({
    type: 'runtime_prewarm',
    phase: 'finished',
    workerCount: 1
  } as unknown as ClearraWasmWorkerEvent);
  const request = controller.loadSolutionPage(0, 1);

  assert.equal(controller.takeIdleWorker(), null);
  const message = worker.messages.find(
    (candidate) => (candidate as { type?: string }).type === 'load_solution_page'
  ) as { requestId: number };
  worker.emit({
    type: 'solution_page',
    request_id: message.requestId,
    offset: 0,
    total: 1,
    keys: ['only-key']
  });
  assert.deepEqual(await request, { keys: ['only-key'], total: 1 });
  assert.equal(controller.takeIdleWorker(), worker as unknown as Worker);
  worker.terminate();
}

async function mismatchedSolutionPageResponseIsRejected() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.prewarm(1);
  const request = controller.loadSolutionPage(5, 2);
  const message = worker.messages.find(
    (candidate) => (candidate as { type?: string }).type === 'load_solution_page'
  ) as { requestId: number };
  worker.emit({
    type: 'solution_page',
    request_id: message.requestId,
    offset: 4,
    total: 10,
    keys: ['wrong-key']
  });

  await assert.rejects(request, /does not match its request/);
  controller.dispose();
}

async function productPageRequestsReleaseOnCancelAndDispose() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.prewarm(1);
  worker.emit({
    type: 'runtime_prewarm',
    phase: 'finished',
    workerCount: 1
  } as unknown as ClearraWasmWorkerEvent);

  const pending = controller.loadNextProductPage();
  assert.equal(
    worker.messages.some((message) =>
      (message as { type?: string; action?: string }).type === 'load_product_page' &&
      (message as { action?: string }).action === 'next'
    ),
    true
  );
  controller.cancel();
  await assert.rejects(pending, /cancelled/);
  assert.equal(worker.terminateCount, 1);

  const disposeWorker = new FakeWorker();
  const disposeController = controllerFor(disposeWorker);
  disposeController.prewarm(1);
  const member = disposeController.loadProductMemberPage('1', '1');
  disposeController.dispose();
  await assert.rejects(member, /disposed/);
  assert.equal(disposeWorker.terminateCount, 1);

  const abortWorker = new FakeWorker();
  const abortController = controllerFor(abortWorker);
  abortController.prewarm(1);
  const signalController = new AbortController();
  const replay = abortController.loadProductMemberPage(
    '184467440737095516160',
    '1',
    signalController.signal
  );
  signalController.abort();
  await assert.rejects(replay, (error: unknown) => {
    assert.equal((error as Error).name, 'AbortError');
    return true;
  });
  assert.equal(abortWorker.terminateCount, 1);

  const releaseWorker = new FakeWorker();
  const releaseController = controllerFor(releaseWorker);
  releaseController.prewarm(1);
  const released = releaseController.loadNextProductPage();
  releaseController.releaseProductPages();
  await assert.rejects(released, /released/);
  assert.equal(releaseWorker.terminateCount, 1);
}

async function productPageResponsesRemainStringExact() {
  resetState();
  const worker = new FakeWorker();
  const controller = controllerFor(worker);
  controller.prewarm(1);
  const exactAlternativeIndex = '184467440737095516160';
  const request = controller.loadProductMemberPage(exactAlternativeIndex, '1');
  const posted = worker.messages.find(
    (message) =>
      (message as { type?: string; action?: string }).type === 'load_product_page' &&
      (message as { action?: string }).action === 'get'
  ) as { requestId: number; alternativeIndex: string; memberPageNumber: string };
  assert.equal(posted.alternativeIndex, exactAlternativeIndex);
  assert.equal(posted.memberPageNumber, '1');
  assert.equal(
    (posted as { maximumWorkSteps?: number }).maximumWorkSteps,
    10_000,
    'member-page replay is always posted as one bounded slice'
  );
  worker.emit({
    type: 'product_page',
    request_id: posted.requestId,
    payload: {
      schema_version: 1,
      runtime: 'clearra-wasm',
      product_page_kind: 'coverage-portfolio',
      state: 'page',
      page: {
        page_contract: 'portfolio-alternative-page.v1',
        member_page_contract: 'portfolio-member-page.v1',
        set_identity_sha256: 'a'.repeat(64),
        candidate_map_sha256: 'b'.repeat(64),
        alternative_index: exactAlternativeIndex,
        optimal_cardinality: '1',
        known_alternative_count: exactAlternativeIndex,
        total_alternative_count: null,
        enumeration_complete: false,
        member_page_number: '1',
        total_member_pages: '1',
        members: [{ candidate_id: '18446744073709551615', normalized_solution_key: 'k' }]
      }
    }
  });
  const response = await request;
  assert.equal(response.state, 'page');
  if (response.product_page_kind === 'coverage-portfolio' && response.state === 'page') {
    assert.equal(response.page.alternative_index, exactAlternativeIndex);
    assert.equal(response.page.members[0]?.candidate_id, '18446744073709551615');
  }
  controller.dispose();
}

async function productPageStallDeadlineTerminatesAndRejectsItsGeneration() {
  resetState();
  const worker = new FakeWorker();
  const controller = new WasmTerminalWorkerController(
    () => worker as unknown as Worker,
    undefined,
    { productPageStallTimeoutMs: 20 }
  );
  controller.prewarm(1);
  const pending = controller.loadProductMemberPage('2', '1');
  const outcome = pending.then(
    () => null,
    (error: unknown) => error as Error
  );
  await delay(35);
  const error = await outcome;
  assert.match(error?.message ?? '', /did not return within 20 ms/u);
  assert.equal(worker.terminateCount, 1, 'a synchronous page stall releases the worker owner');

  const posted = worker.messages.find(
    (message) =>
      (message as { type?: string; action?: string }).type === 'load_product_page' &&
      (message as { action?: string }).action === 'get'
  ) as { requestId: number };
  worker.emit({
    type: 'product_page',
    request_id: posted.requestId,
    payload: {
      schema_version: 1,
      runtime: 'clearra-wasm',
      product_page_kind: 'coverage-portfolio',
      state: 'work-budget-exhausted',
      known_alternative_count: '2',
      enumeration_complete: false,
      work_steps: 1,
      replay_cursor_alternative_index: '1'
    }
  });
  assert.equal(worker.terminateCount, 1, 'a stale completion cannot reacquire product ownership');
  controller.dispose();
}

async function verifiedArtifactUpdateRotatesAtTheNextRunBoundary() {
  resetState();
  assert.equal(
    announceWasmArtifactGeneration({
      sourceSha256: 'not-a-sha',
      bindingsSha256: 'b'.repeat(64),
      wasmSha256: 'c'.repeat(64)
    }),
    false,
    'an unverified generation must not change worker authority'
  );
  const firstGeneration = {
    sourceSha256: '1'.repeat(64),
    bindingsSha256: '2'.repeat(64),
    wasmSha256: '3'.repeat(64)
  };
  assert.equal(announceWasmArtifactGeneration(firstGeneration), true);
  const firstGenerationIdentity = currentWasmArtifactGeneration();
  assert.equal(isCurrentWasmArtifactGeneration(firstGenerationIdentity), true);
  assert.equal(
    isCurrentWasmArtifactGeneration(null),
    false,
    'a transferred worker without a generation token must not be reused'
  );
  const workers = [new FakeWorker(), new FakeWorker()];
  let created = 0;
  const controller = new WasmTerminalWorkerController(
    () => workers[created++] as unknown as Worker
  );
  controller.prewarm(1);
  workers[0].emit({
    type: 'runtime_prewarm',
    phase: 'finished',
    workerCount: 1
  } as unknown as ClearraWasmWorkerEvent);

  assert.equal(
    announceWasmArtifactGeneration({
      sourceSha256: '4'.repeat(64),
      bindingsSha256: '5'.repeat(64),
      wasmSha256: '6'.repeat(64)
    }),
    true
  );
  assert.match(currentWasmArtifactGeneration(), /:[0-9a-f]{64}:/u);
  assert.equal(isCurrentWasmArtifactGeneration(firstGenerationIdentity), false);

  const retainedPage = controller.loadSolutionPage(0, 1);
  const pageMessage = workers[0].messages.find(
    (candidate) => (candidate as { type?: string }).type === 'load_solution_page'
  ) as { requestId: number };
  workers[0].emit({
    type: 'solution_page',
    request_id: pageMessage.requestId,
    offset: 0,
    total: 1,
    keys: ['retained-old-generation-result']
  });
  assert.deepEqual(await retainedPage, {
    keys: ['retained-old-generation-result'],
    total: 1
  });
  for (const memberPageNumber of ['1', '2']) {
    const retainedProductPage = controller.loadProductMemberPage(
      '0',
      memberPageNumber
    );
    const productPageMessages = workers[0].messages.filter(
      (candidate) =>
        (candidate as { type?: string }).type === 'load_product_page' &&
        (candidate as { memberPageNumber?: string }).memberPageNumber ===
          memberPageNumber
    );
    const productPageMessage = productPageMessages[
      productPageMessages.length - 1
    ] as { requestId: number };
    workers[0].emit({
      type: 'product_page',
      request_id: productPageMessage.requestId,
      payload: {
        schema_version: 1,
        runtime: 'clearra-wasm',
        product_page_kind: 'coverage-portfolio',
        state: 'page',
        page: {
          page_contract: 'portfolio-alternative-page.v1',
          member_page_contract: 'portfolio-member-page.v1',
          set_identity_sha256: 'a'.repeat(64),
          candidate_map_sha256: 'b'.repeat(64),
          alternative_index: '0',
          optimal_cardinality: '2',
          known_alternative_count: '1',
          total_alternative_count: '1',
          enumeration_complete: true,
          member_page_number: memberPageNumber,
          total_member_pages: '2',
          members: [{
            candidate_id: memberPageNumber,
            normalized_solution_key: `retained-member-${memberPageNumber}`
          }]
        }
      }
    });
    const productPage = await retainedProductPage;
    assert.equal(productPage.state, 'page');
    if (
      productPage.product_page_kind === 'coverage-portfolio' &&
      productPage.state === 'page'
    ) {
      assert.equal(productPage.page.member_page_number, memberPageNumber);
    }
  }
  assert.equal(
    workers[0].terminateCount,
    0,
    'solution and multi-page product copy must not be interrupted'
  );

  assert.equal(controller.run(), true);
  assert.equal(workers[0].terminateCount, 1, 'the stale worker is retired at new-run');
  assert.equal(created, 2);
  assert.equal(
    workers[1].messages.some(
      (message) => (message as { type?: string }).type === 'run_command_text'
    ),
    true,
    'the next command must be posted to a worker created for the new generation'
  );
  controller.dispose();
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
    resourceReport: null,
    executionAvailability: null,
    resultCompleteness: null,
    searchReport: null,
    webgpuBackend: null,
    error: null
  });
  updateWasmCommandText('clearra pc path --lines 4 --queue IOTSZ');
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

function progress(
  jobId: number,
  phase: 'preparing' | 'searching',
  done: number,
  executionMode?: 'serial' | 'distributed'
): ClearraWasmWorkerEvent {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'progress',
    job_id: jobId,
    progress: {
      done,
      total: 5,
      label: phase,
      budget_status: { state: 'within-budget', used: 0, limit: null },
      backend_status: {
        backend_requested: 'cpu',
        backend_selected: 'wasm-cpu',
        fallback_used: false,
        fallback_reason: null
      },
      memory_status: {
        state: 'wasm-computation-scope-active',
        raw_pointer_exposed: false
      },
      telemetry: {
        execution_mode: executionMode,
        phase
      }
    }
  } as unknown as ClearraWasmWorkerEvent;
}

function delay(milliseconds: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, milliseconds));
}

class FakeWorker {
  onmessage: ((event: MessageEvent<ClearraWasmWorkerEvent | ClearraSolutionPageWorkerEvent | ClearraProductPageWorkerEvent>) => void) | null = null;
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

  emit(event: ClearraWasmWorkerEvent | ClearraSolutionPageWorkerEvent | ClearraProductPageWorkerEvent) {
    this.onmessage?.({ data: event } as MessageEvent<ClearraWasmWorkerEvent | ClearraSolutionPageWorkerEvent | ClearraProductPageWorkerEvent>);
  }
}

try {
  await cooperativeCancellationRemainsCancellation();
  await forcedCancellationIsDistinct();
  await realTerminalEventWinsCancellationRace();
  await ownerDisposalIsForceTermination();
  await ownerTerminationReachesDescendants();
  await duplicateRunIsRejectedBeforePosting();
  await boundedProgressWatchdogCoversPreparationAndSerialSearchStalls();
  await boundedProgressWatchdogRequiresAndAcceptsChangedDistributedWork();
  await workerCreationFailureBecomesTerminalFailure();
  await nonSuccessFinalResponseRemainsFailure();
  await failedResponsePreservesTypedResourceEvidence();
  await responseNullFailurePreservesTopLevelResourceAxes();
  await inconsistentFailedResourceReportsAreRejected();
  await legacyStringFailureRemainsIncomplete();
  await clearingTerminalResultReleasesLogPayloads();
  await abortingSolutionPageRequestReleasesControllerOwnership();
  await newRunRejectsOutstandingSolutionPageRequest();
  await outstandingSolutionPageRequestPreventsWorkerTransfer();
  await mismatchedSolutionPageResponseIsRejected();
  await productPageRequestsReleaseOnCancelAndDispose();
  await productPageResponsesRemainStringExact();
  await productPageStallDeadlineTerminatesAndRejectsItsGeneration();
  await verifiedArtifactUpdateRotatesAtTheNextRunBoundary();
} finally {
  wasmWorkerState.set(originalState);
}

console.log(
  JSON.stringify({
    cooperative_cancel: 'cancelled',
    cancel_timeout: 'terminated',
    preparation_progress_stall: 'terminated',
    serial_progress_stall: 'terminated',
    distributed_progress_stall: 'terminated-with-changed-progress-renewal',
    terminal_race: 'preserved',
    descendant_release_signal: 'delivered',
    duplicate_run: 'rejected',
    worker_creation_failure: 'reported',
    non_success_response: 'failed',
    typed_failed_response: 'preserved',
    response_null_resource_axes: 'preserved-and-reset',
    resource_report_mismatch: 'rejected',
    legacy_failure_completeness: 'incomplete',
    terminal_log_release: 'cleared',
    solution_page_abort: 'released',
    solution_page_new_run: 'rejected',
    solution_page_worker_transfer: 'guarded',
    solution_page_response_identity: 'validated',
    product_page_release_lifecycle: 'cancelled-and-disposed',
    product_page_decimal_identity: 'string-exact',
    product_page_stall: 'terminated-and-generation-fenced',
    wasm_artifact_hot_update:
      'retained-solution-and-product-pages-then-next-run-rotated'
  })
);

import assert from 'node:assert/strict';

import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

import { WasmJobRunner } from '../src/workers/WasmJobRunner.ts';
import type { ClearraWasmModule } from '../src/workers/clearraWasmRuntime.ts';

const jobId = 117;
let advanceCount = 0;
let advancesSeenByQueuedHostWork = -1;
const pendingEvents: ClearraWasmWorkerEvent[] = [];

const wasm = {
  start_job() {
    return jobId;
  },
  advance_job(actualJobId: number, workBudget: number) {
    assert.equal(actualJobId, jobId);
    assert.equal(workBudget, 2_048);
    advanceCount += 1;
    if (advanceCount === 1) return 'pending' as const;
    assert.equal(advanceCount, 2);
    pendingEvents.push({
      schema_version: 1,
      runtime: 'clearra-wasm',
      event: 'final_response',
      job_id: jobId,
      response: { status: 'success' }
    } as ClearraWasmWorkerEvent);
    return 'completed' as const;
  },
  drain_job_events_json(actualJobId: number) {
    assert.equal(actualJobId, jobId);
    return JSON.stringify(pendingEvents.splice(0));
  },
  cancel_job() {}
} as unknown as ClearraWasmModule;

let nowMs = 0;
const previousNow = Object.getOwnPropertyDescriptor(performance, 'now');
Object.defineProperty(performance, 'now', {
  configurable: true,
  value: () => nowMs
});

try {
  queueMicrotask(() => {
    advancesSeenByQueuedHostWork = advanceCount;
  });
  const terminal = await new WasmJobRunner(wasm).run('clearra pc --lines 4', () => undefined);

  assert.equal(terminal.event, 'final_response');
  assert.equal(advanceCount, 2);
  assert.equal(
    advancesSeenByQueuedHostWork,
    2,
    'cheap pending advances must stay in one bounded wall-clock batch instead of yielding after every ABI call'
  );

  const boundedJobId = 118;
  let boundedAdvanceCount = 0;
  let advancesSeenAtBudgetYield = -1;
  const boundedEvents: ClearraWasmWorkerEvent[] = [];
  const boundedWasm = {
    start_job() {
      return boundedJobId;
    },
    advance_job(actualJobId: number, workBudget: number) {
      assert.equal(actualJobId, boundedJobId);
      assert.equal(workBudget, 2_048);
      boundedAdvanceCount += 1;
      if (boundedAdvanceCount === 1) {
        nowMs = 9;
        return 'pending' as const;
      }
      assert.equal(boundedAdvanceCount, 2);
      boundedEvents.push({
        schema_version: 1,
        runtime: 'clearra-wasm',
        event: 'final_response',
        job_id: boundedJobId,
        response: { status: 'success' }
      } as ClearraWasmWorkerEvent);
      return 'completed' as const;
    },
    drain_job_events_json(actualJobId: number) {
      assert.equal(actualJobId, boundedJobId);
      return JSON.stringify(boundedEvents.splice(0));
    },
    cancel_job() {}
  } as unknown as ClearraWasmModule;

  nowMs = 0;
  queueMicrotask(() => {
    advancesSeenAtBudgetYield = boundedAdvanceCount;
  });
  const keepAlive = setTimeout(() => undefined, 1_000);
  const boundedTerminal = await new WasmJobRunner(boundedWasm)
    .run('clearra pc --lines 4', () => undefined)
    .finally(() => clearTimeout(keepAlive));

  assert.equal(boundedTerminal.event, 'final_response');
  assert.equal(boundedAdvanceCount, 2);
  assert.equal(
    advancesSeenAtBudgetYield,
    1,
    'the serial runner must yield once its wall-clock host budget expires'
  );

  const cancellingJobId = 119;
  let cancellationAdvanceCount = 0;
  let cancellationDrainCount = 0;
  let cancelCallCount = 0;
  const cancellationEvents: ClearraWasmWorkerEvent[] = [];
  let cancellationRunner: WasmJobRunner;
  const cancellationWasm = {
    start_job() {
      return cancellingJobId;
    },
    advance_job(actualJobId: number, workBudget: number) {
      assert.equal(actualJobId, cancellingJobId);
      assert.equal(workBudget, 2_048);
      cancellationAdvanceCount += 1;
      cancellationRunner.cancel();
      return 'pending' as const;
    },
    drain_job_events_json(actualJobId: number) {
      assert.equal(actualJobId, cancellingJobId);
      cancellationDrainCount += 1;
      return JSON.stringify(cancellationEvents.splice(0));
    },
    cancel_job(actualJobId: number) {
      assert.equal(actualJobId, cancellingJobId);
      cancelCallCount += 1;
      queueMicrotask(() => {
        cancellationEvents.push({
          schema_version: 1,
          runtime: 'clearra-wasm',
          event: 'cancelled',
          job_id: cancellingJobId,
          scope_released: true
        });
      });
    }
  } as unknown as ClearraWasmModule;

  nowMs = 0;
  cancellationRunner = new WasmJobRunner(cancellationWasm);
  const cancellationKeepAlive = setTimeout(() => undefined, 1_000);
  const cancellationTerminal = await cancellationRunner
    .run('clearra pc --lines 4', () => undefined)
    .finally(() => clearTimeout(cancellationKeepAlive));

  assert.equal(cancellationTerminal.event, 'cancelled');
  assert.equal(cancellationAdvanceCount, 1, 'cancellation stops new WASM advances immediately');
  assert.equal(cancelCallCount, 1, 'the active Rust job receives exactly one cancellation request');
  assert.ok(
    cancellationDrainCount <= 4,
    `cancellation must yield while awaiting its terminal event, drains=${cancellationDrainCount}`
  );
} finally {
  if (previousNow) Object.defineProperty(performance, 'now', previousNow);
  else delete (performance as unknown as { now?: () => number }).now;
}

import assert from 'node:assert/strict';
import test from 'node:test';

import { createSolutionExportOperation } from '../src/lib/workspace/solutionExportOperation.ts';

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((success, failure) => {
    resolve = success;
    reject = failure;
  });
  return { promise, resolve, reject };
}

function fixture(success = 'copied') {
  const paints = [];
  const resets = [];
  const states = [];
  const failures = [];
  const operation = createSolutionExportOperation({
    success,
    abortMessage: 'owned cancellation',
    onState(state) { states.push(state); },
    onFailure(error) { failures.push(error); }
  }, {
    nextPaint() {
      const next = deferred();
      paints.push(next.resolve);
      return next.promise;
    },
    scheduleReset(delay, callback) {
      const reset = { delay, callback, cancelled: false };
      resets.push(reset);
      return () => { reset.cancelled = true; };
    }
  });
  return { operation, paints, resets, states, failures };
}

test('shows busy before paint and admits only one available export until it settles', async () => {
  const { operation, paints, resets, states } = fixture();
  const effect = deferred();
  let calls = 0;
  const perform = (signal, checkAbort) => {
    calls += 1;
    assert.equal(signal.aborted, false);
    checkAbort();
    return effect.promise;
  };
  await operation.run(false, perform);
  assert.deepEqual(states, []);
  const active = operation.run(true, perform);
  assert.deepEqual(states, ['loading']);
  assert.equal(calls, 0);
  await operation.run(true, perform);
  assert.equal(paints.length, 1);
  paints[0]();
  await Promise.resolve();
  assert.equal(calls, 1);
  await operation.run(true, perform);
  effect.resolve();
  await active;
  assert.deepEqual(states, ['loading', 'copied']);
  assert.equal(resets[0].delay, 1600);
  resets[0].callback();
  assert.deepEqual(states, ['loading', 'copied', 'idle']);
});

test('reports the original error and only the current terminal timer may reset the state', async () => {
  const { operation, paints, resets, states, failures } = fixture('saved');
  const error = new Error('file rejected');
  const first = operation.run(true, () => { throw error; });
  paints[0]();
  await first;
  assert.deepEqual(failures, [error]);
  assert.deepEqual(states, ['loading', 'failed']);
  assert.equal(resets[0].delay, 4000);

  const second = operation.run(true, () => {});
  assert.equal(resets[0].cancelled, true);
  resets[0].callback();
  assert.deepEqual(states, ['loading', 'failed', 'loading']);
  paints[1]();
  await second;
  assert.equal(resets[1].delay, 1600);
  resets[0].callback();
  assert.equal(states.at(-1), 'saved');
  resets[1].callback();
  assert.equal(states.at(-1), 'idle');
});

test('pagehide cancels before and during work without producing a failure or late success', async () => {
  const { operation, paints, resets, states, failures } = fixture();
  const target = new EventTarget();
  const unbind = operation.bindPageHide(target);
  let calls = 0;
  const first = operation.run(true, () => { calls += 1; });
  target.dispatchEvent(new Event('pagehide'));
  paints[0]();
  await first;
  assert.equal(calls, 0);
  assert.deepEqual(states, ['loading', 'idle']);

  const effect = deferred();
  let currentSignal;
  const second = operation.run(true, (signal) => {
    currentSignal = signal;
    return effect.promise;
  });
  paints[1]();
  await Promise.resolve();
  target.dispatchEvent(new Event('pagehide'));
  assert.equal(currentSignal.aborted, true);
  assert.equal(currentSignal.reason.name, 'AbortError');
  assert.equal(currentSignal.reason.message, 'owned cancellation');
  effect.resolve();
  await second;
  assert.deepEqual(states, ['loading', 'idle', 'loading', 'idle']);
  assert.deepEqual(failures, []);
  assert.deepEqual(resets, []);

  unbind();
  const third = operation.run(true, (signal) => {
    assert.equal(signal.aborted, false);
  });
  target.dispatchEvent(new Event('pagehide'));
  paints[2]();
  await third;
  assert.equal(states.at(-1), 'copied');
});

test('destroy aborts pending work and ignores late errors', async () => {
  const { operation, paints, resets, states, failures } = fixture();
  const effect = deferred();
  let currentSignal;
  const active = operation.run(true, (signal) => {
    currentSignal = signal;
    return effect.promise;
  });
  paints[0]();
  await Promise.resolve();
  operation.destroy();
  assert.equal(currentSignal.aborted, true);
  effect.reject(new Error('late source error'));
  await active;
  await operation.run(true, () => assert.fail('destroyed operation ran'));
  assert.deepEqual(states, ['loading']);
  assert.deepEqual(failures, []);
  assert.deepEqual(resets, []);
});

test('destroy cancels the still-active terminal reset', async () => {
  const { operation, paints, resets, states, failures } = fixture();
  const active = operation.run(true, () => {});
  paints[0]();
  await active;
  operation.destroy();
  assert.equal(resets[0].cancelled, true);
  resets[0].callback();
  assert.deepEqual(states, ['loading', 'copied']);
  assert.deepEqual(failures, []);
});

test('downstream AbortError returns to idle without a terminal feedback timer', async () => {
  const { operation, paints, resets, states, failures } = fixture();
  const active = operation.run(true, () => {
    throw new DOMException('downstream cancelled', 'AbortError');
  });
  paints[0]();
  await active;
  assert.deepEqual(states, ['loading', 'idle']);
  assert.deepEqual(failures, []);
  assert.deepEqual(resets, []);
});

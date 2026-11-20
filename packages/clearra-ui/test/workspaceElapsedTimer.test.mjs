import assert from 'node:assert/strict';
import test from 'node:test';

import { createWorkspaceElapsedTimer } from '../src/lib/workspace/workspaceElapsedTimer.ts';

function fakeClock() {
  let time = 0;
  const pending = new Set();
  return {
    timing: {
      now: () => time,
      every: (tick, interval) => {
        const entry = { tick, interval };
        pending.add(entry);
        return () => pending.delete(entry);
      }
    },
    at: (milliseconds) => { time = milliseconds; },
    pending
  };
}

test('workspace elapsed time uses the real elapsed duration on a tick and at termination', () => {
  const { timing, at, pending } = fakeClock();
  const changes = [];
  const timer = createWorkspaceElapsedTimer((...value) => changes.push(value), timing);

  timer.start();
  const [run] = pending;
  assert.equal(run.interval, 100);
  assert.deepEqual(changes, [[0, true]]);

  at(337.5); // browser timer throttling does not change the elapsed time calculation
  run.tick();
  at(363);
  timer.stop();
  assert.equal(pending.size, 0);
  assert.deepEqual(changes, [[0, true], [337.5, true], [363, false]]);

  at(800);
  timer.stop(); // a later teardown does not advance the stopped result
  run.tick(); // even an already queued tick no longer owns the result
  assert.deepEqual(changes.at(-1), [363, false]);
  assert.equal(changes.length, 3);
});

test('restarting or resetting drops the previous run and cannot replay its ticks', () => {
  const { timing, at, pending } = fakeClock();
  const changes = [];
  const timer = createWorkspaceElapsedTimer((...value) => changes.push(value), timing);

  timer.start();
  const [first] = pending;
  at(20);
  timer.start();
  const [second] = pending;
  assert.notEqual(first, second);
  assert.equal(pending.size, 1);

  at(30);
  first.tick();
  second.tick();
  assert.deepEqual(changes, [[0, true], [0, true], [10, true]]);
  timer.reset();
  assert.equal(pending.size, 0);
  assert.deepEqual(changes.at(-1), [0, false]);

  at(200);
  second.tick();
  assert.equal(changes.length, 4);
  assert.deepEqual(changes.at(-1), [0, false]);
});

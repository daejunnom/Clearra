import assert from 'node:assert/strict';
import test from 'node:test';
import { nonnegativeInteger, scheduleSearchDeadline } from './optional-search-deadline.mjs';

function clock() {
  let time = 0;
  let nextId = 0;
  const pending = new Map();
  return {
    pending,
    now: () => time,
    setTimer(callback, delay) {
      assert(delay > 0 && delay <= 2 ** 31 - 1);
      const id = ++nextId;
      pending.set(id, { callback, delay });
      return id;
    },
    clearTimer(id) { pending.delete(id); },
    advance() {
      const [id, next] = pending.entries().next().value;
      pending.delete(id);
      time += next.delay;
      next.callback();
    },
  };
}

test('unlimited search creates no clock reads or timers', () => {
  const unexpected = () => assert.fail('unlimited search cannot schedule work');
  scheduleSearchDeadline(0, unexpected, { now: unexpected, setTimer: unexpected, clearTimer: unexpected })();
});

test('an explicit budget longer than a timer interval fires once at its deadline', () => {
  const timer = clock();
  let fired = 0;
  scheduleSearchDeadline(2 ** 31 + 99, () => { fired += 1; }, timer);
  timer.advance();
  assert.equal(fired, 0);
  assert.equal(timer.pending.size, 1);
  timer.advance();
  assert.equal(fired, 1);
  assert.equal(timer.pending.size, 0);
});

test('completion cancels an explicit pending deadline', () => {
  const timer = clock();
  const cancel = scheduleSearchDeadline(100, () => assert.fail('cancelled'), timer);
  cancel();
  cancel();
  assert.equal(timer.pending.size, 0);
});

test('deadline arguments accept zero and reject negative, fractional and unsafe values', () => {
  assert.equal(nonnegativeInteger('0', 'timeout'), 0);
  assert.equal(nonnegativeInteger('65535', 'timeout'), 65535);
  for (const value of ['-1', '1.5', 'NaN', '', Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => nonnegativeInteger(value, 'timeout'), /nonnegative safe integer/u);
  }
});

import assert from 'node:assert/strict';
import { createWorkerHostYield } from '../src/workers/workerHostYield';

const previousChannel = globalThis.MessageChannel;
const previousTimer = globalThis.setTimeout;
const previousScheduler = Object.getOwnPropertyDescriptor(globalThis, 'scheduler');
const channelTasks: Array<() => void> = [];
const timerTasks: Array<() => void> = [];
const schedulerTasks: Array<() => void> = [];
let channelsCreated = 0;

class PostedMessageChannel {
  port1 = { onmessage: null as (() => void) | null, unref() {} };
  port2 = { postMessage: () => channelTasks.push(() => this.port1.onmessage?.()), unref() {} };
  constructor() { channelsCreated += 1; }
}

try {
  globalThis.MessageChannel = PostedMessageChannel as unknown as typeof MessageChannel;
  globalThis.setTimeout = ((callback: () => void, delay: number) => {
    assert.equal(delay, 0);
    timerTasks.push(callback);
    return timerTasks.length;
  }) as unknown as typeof setTimeout;
  Object.defineProperty(globalThis, 'scheduler', {
    configurable: true,
    value: {
      yield: () => new Promise<void>((resolve) => schedulerTasks.push(resolve))
    }
  });

  const coordinatorYield = createWorkerHostYield('timer');
  assert.equal(channelsCreated, 0, 'the coordinator must not retain a competing message channel');
  for (let quantum = 0; quantum < 17; quantum += 1) {
    let continued = false;
    const pending = coordinatorYield().then(() => { continued = true; });
    await Promise.resolve();
    assert.equal(continued, false, 'a microtask alone cannot resume coordinator compute');
    assert.equal(channelTasks.length, 0);
    assert.equal(timerTasks.length, 0, 'modern coordinators must not depend on a clamped timer');
    assert.equal(schedulerTasks.length, 1, 'every coordinator quantum admits scheduler tasks');
    schedulerTasks.shift()!();
    await pending;
    assert.equal(continued, true);
  }

  delete (globalThis as typeof globalThis & { scheduler?: unknown }).scheduler;
  const fallbackCoordinatorYield = createWorkerHostYield('timer');
  const fallback = fallbackCoordinatorYield();
  assert.equal(timerTasks.length, 1, 'old engines retain the bounded timer fallback');
  timerTasks.shift()!();
  await fallback;
  Object.defineProperty(globalThis, 'scheduler', {
    configurable: true,
    value: {
      yield: () => new Promise<void>((resolve) => schedulerTasks.push(resolve))
    }
  });

  const verifierYield = createWorkerHostYield();
  assert.equal(channelsCreated, 1);
  for (let quantum = 1; quantum <= 24; quantum += 1) {
    const pending = verifierYield();
    const schedulerTurn = quantum % 8 === 0;
    assert.equal(schedulerTasks.length, Number(schedulerTurn));
    assert.equal(timerTasks.length, 0);
    assert.equal(channelTasks.length, Number(!schedulerTurn),
      'remote verifier scheduling remains seven message turns plus one scheduler turn');
    (schedulerTurn ? schedulerTasks : channelTasks).shift()!();
    await pending;
  }
} finally {
  globalThis.MessageChannel = previousChannel;
  globalThis.setTimeout = previousTimer;
  if (previousScheduler) Object.defineProperty(globalThis, 'scheduler', previousScheduler);
  else delete (globalThis as typeof globalThis & { scheduler?: unknown }).scheduler;
}

import assert from 'node:assert/strict';
import { createWorkerHostYield } from '../src/workers/workerHostYield';

const previousChannel = globalThis.MessageChannel;
const previousTimer = globalThis.setTimeout;
const channelTasks: Array<() => void> = [];
const timerTasks: Array<() => void> = [];
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

  const coordinatorYield = createWorkerHostYield('timer');
  assert.equal(channelsCreated, 0, 'the coordinator must not retain a competing message channel');
  for (let quantum = 0; quantum < 17; quantum += 1) {
    let continued = false;
    const pending = coordinatorYield().then(() => { continued = true; });
    await Promise.resolve();
    assert.equal(continued, false, 'a microtask alone cannot resume coordinator compute');
    assert.equal(channelTasks.length, 0);
    assert.equal(timerTasks.length, 1, 'every coordinator quantum admits a timer-lane callback');
    timerTasks.shift()!();
    await pending;
    assert.equal(continued, true);
  }

  const verifierYield = createWorkerHostYield();
  assert.equal(channelsCreated, 1);
  for (let quantum = 1; quantum <= 24; quantum += 1) {
    const pending = verifierYield();
    const timerTurn = quantum % 8 === 0;
    assert.equal(timerTasks.length, Number(timerTurn));
    assert.equal(channelTasks.length, Number(!timerTurn),
      'remote verifier scheduling remains seven message turns plus one timer turn');
    (timerTurn ? timerTasks : channelTasks).shift()!();
    await pending;
  }
} finally {
  globalThis.MessageChannel = previousChannel;
  globalThis.setTimeout = previousTimer;
}

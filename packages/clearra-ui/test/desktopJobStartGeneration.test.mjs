import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import { DesktopJobStartGeneration } from '../src/lib/stores/desktopJobStartGeneration.ts';

test('a Desktop start generation accepts one matching completion', () => {
  const generations = new DesktopJobStartGeneration();
  const token = generations.begin();

  assert.equal(generations.hasPending(), true);
  assert.equal(generations.complete(token), true);
  assert.equal(generations.hasPending(), false);
  assert.equal(generations.complete(token), false);
});

test('disposing a pending Desktop start rejects its late completion', () => {
  const generations = new DesktopJobStartGeneration();
  const stale = generations.begin();

  assert.equal(generations.invalidatePending(), true);
  assert.equal(generations.complete(stale), false);

  const current = generations.begin();
  assert.notEqual(current, stale);
  assert.equal(generations.complete(current), true);
});

test('duplicate pending Desktop starts fail before reaching the host', () => {
  const generations = new DesktopJobStartGeneration();
  generations.begin();

  assert.throws(() => generations.begin(), /already pending/u);
});

test('the Desktop store rejects and cancels a start that resolves after owner disposal', async () => {
  const source = await readFile(
    new URL('../src/lib/stores/desktopJobStore.ts', import.meta.url),
    'utf8'
  );

  assert.match(source, /const startGeneration = desktopJobStartGeneration\.begin\(\)/u);
  assert.match(source, /if \(!desktopJobStartGeneration\.complete\(startGeneration\)\)/u);
  assert.match(source, /await cancelDetachedDesktopJob\(jobId\)/u);
  assert.match(source, /desktopJobStartGeneration\.invalidatePending\(\)/u);
  assert.match(source, /state\.status === 'cancelling'/u);
});

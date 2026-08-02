import assert from 'node:assert/strict';
import test from 'node:test';

import {
  defaultBrowserWorkerCount,
  defaultWorkerCount
} from '../src/lib/workspace/solverWorkspaceModel.ts';

test('browser automatic worker count reserves exactly one logical processor', () => {
  assert.equal(defaultBrowserWorkerCount(1), 1);
  assert.equal(defaultBrowserWorkerCount(8), 7);
  assert.equal(defaultBrowserWorkerCount(64), 63);
});

test('explicit full-CPU selection reaches but never exceeds the logical count', () => {
  assert.equal(defaultBrowserWorkerCount(1, true), 1);
  assert.equal(defaultBrowserWorkerCount(8, true), 8);
  assert.equal(defaultBrowserWorkerCount(64, true), 64);
  assert.equal(defaultWorkerCount(1, true), 1);
  assert.equal(defaultWorkerCount(8, true), 8);
  assert.equal(defaultWorkerCount(64, true), 64);
});

test('invalid hardware reports collapse to the safe one-worker floor', () => {
  assert.equal(defaultWorkerCount(Number.NaN), 1);
  assert.equal(defaultWorkerCount(Number.POSITIVE_INFINITY, true), 1);
  assert.equal(defaultBrowserWorkerCount(0, true), 1);
});

test('native automatic worker count reserves one processor by default', () => {
  assert.equal(defaultWorkerCount(64), 63);
});

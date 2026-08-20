import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  bindSolutionPageLoader,
  createPagedSolutionExportKeySource,
  solutionPageResultIdentity
} from '../src/lib/workspace/solutionPageSource.ts';

test('paged solution export reads every key beyond the materialized first page', async () => {
  const allKeys = Array.from({ length: 101 }, (_, index) => `key-${index}`);
  let identity = 'hash:cts1:test:count:101';
  const loader = bindSolutionPageLoader({
    keyCount: allKeys.length,
    resultIdentity: identity,
    currentResultIdentity: () => identity,
    async loadPage(offset, limit) {
      return {
        keys: allKeys.slice(offset, offset + limit),
        total: allKeys.length
      };
    }
  });
  const source = createPagedSolutionExportKeySource({
    keyCount: allKeys.length,
    loadPage: loader,
    pageSize: 37
  });

  assert.deepEqual(await source.readKeys(0, allKeys.length), allKeys);
});

test('bound page loaders reject totals and ranges that do not match the result', async () => {
  const identity = 'hash:cts1:test:count:3';
  const mismatchedTotal = bindSolutionPageLoader({
    keyCount: 3,
    resultIdentity: identity,
    currentResultIdentity: () => identity,
    async loadPage() {
      return { keys: ['key-0'], total: 4 };
    }
  });
  await assert.rejects(
    mismatchedTotal(0, 1),
    /does not match the completed result/
  );

  const emptyPage = bindSolutionPageLoader({
    keyCount: 3,
    resultIdentity: identity,
    currentResultIdentity: () => identity,
    async loadPage() {
      return { keys: [], total: 3 };
    }
  });
  await assert.rejects(emptyPage(0, 1), /ended before the reported total/);

  const overlongPage = bindSolutionPageLoader({
    keyCount: 3,
    resultIdentity: identity,
    currentResultIdentity: () => identity,
    async loadPage() {
      return { keys: ['key-0', 'key-1'], total: 3 };
    }
  });
  await assert.rejects(
    overlongPage(0, 1),
    /does not match the completed result/
  );
  await assert.rejects(
    mismatchedTotal(3, 1),
    (error) => error instanceof RangeError
  );
});

test('bound page loaders reject a result replaced during an outstanding request', async () => {
  let identity = 'hash:cts1:first:count:2';
  let resolvePage;
  const pendingPage = new Promise((resolve) => {
    resolvePage = resolve;
  });
  const loader = bindSolutionPageLoader({
    keyCount: 2,
    resultIdentity: identity,
    currentResultIdentity: () => identity,
    loadPage: () => pendingPage
  });

  const request = loader(0, 1);
  identity = 'hash:cts1:second:count:2';
  resolvePage({ keys: ['key-0'], total: 2 });
  await assert.rejects(request, /replaced by a newer search/);
});

test('bound page loaders propagate cancellation and skip an already-aborted request', async () => {
  const identity = 'hash:cts1:test:count:2';
  let calls = 0;
  const loader = bindSolutionPageLoader({
    keyCount: 2,
    resultIdentity: identity,
    currentResultIdentity: () => identity,
    async loadPage() {
      calls += 1;
      return { keys: ['key-0'], total: 2 };
    }
  });
  const controller = new AbortController();
  const reason = new Error('cancelled by test');
  reason.name = 'AbortError';
  controller.abort(reason);

  await assert.rejects(loader(0, 1, controller.signal), reason);
  assert.equal(calls, 0);
});

test('solution result identities prefer the canonical set hash', () => {
  assert.equal(
    solutionPageResultIdentity('cts1:abc', 101, ['first']),
    'hash:cts1:abc:count:101'
  );
  assert.notEqual(
    solutionPageResultIdentity('', 101, ['first']),
    solutionPageResultIdentity('', 101, ['second'])
  );
});

test('build probability connects the bounded page source to gallery and full export', () => {
  const resultSource = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityResult.svelte', import.meta.url),
    'utf8'
  );
  const workspaceSource = readFileSync(
    new URL('../src/lib/workspace/BuildProbabilityWorkspace.svelte', import.meta.url),
    'utf8'
  );

  assert.match(resultSource, /workspaceSolutionPageAvailable\(report\)/u);
  assert.match(resultSource, /bindSolutionPageLoader\(\{/u);
  assert.match(resultSource, /createPagedSolutionExportKeySource\(\{/u);
  assert.match(resultSource, /keySource=\{solutionExportKeySource\}/u);
  assert.match(resultSource, /\{solutionCount\}/u);
  assert.match(resultSource, /loadSolutionPage=\{boundSolutionPageLoader\}/u);
  assert.match(
    workspaceSource,
    /loadSolutionPage=\{runtime === 'web'[\s\S]*workerController\.loadSolutionPage\(offset, limit, signal\)/u
  );
});

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const workspace = new URL('../src/lib/workspace/', import.meta.url);

test('aggregate workers have a single stable heading owner, never a stage item', async () => {
  for (const file of ['PcSolverResult.svelte', 'ResultWorkspaceFrame.svelte']) {
    const source = await readFile(new URL(file, workspace), 'utf8');
    assert.ok(source.indexOf('class="worker-metric"') < source.indexOf('class="elapsed-metric"'));
    assert.equal(source.match(/class="worker-metric"/gu)?.length, 1);
    assert.match(source, /workerCapacity/u);
  }
  const model = await readFile(new URL('workspaceProgressModel.ts', workspace), 'utf8');
  assert.doesNotMatch(model, /'progressMetricWorkers'/u);
  const status = await readFile(new URL('WorkspaceProgressStatus.svelte', workspace), 'utf8');
  assert.doesNotMatch(status, /showWorkerMetrics|progressMetricWorkers/u);
});

test('progress surfaces compile in memory after removing duplicate stage-worker controls', async () => {
  for (const file of [
    'WorkspaceProgressStatus.svelte',
    'ResultWorkspaceFrame.svelte',
    'BuildProbabilityResult.svelte'
  ]) {
    const filename = fileURLToPath(new URL(file, workspace));
    const source = await readFile(filename, 'utf8');
    const processed = await preprocess(source, vitePreprocess(), { filename });
    const result = compile(processed.code, { filename, generate: 'client' });
    assert.deepEqual(result.warnings, [], `${file} must compile without warnings`);
  }
});

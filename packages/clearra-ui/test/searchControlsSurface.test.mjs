import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const workspace = new URL('../src/lib/workspace/', import.meta.url);

test('GUI search controls omit data-product toggles and asset managers', async () => {
  for (const file of [
    'SearchControls.svelte',
    'BuildProbabilityControls.svelte',
    'SetupFinderControls.svelte'
  ]) {
    const filename = fileURLToPath(new URL(file, workspace));
    const source = await readFile(filename, 'utf8');
    assert.doesNotMatch(source, /label\('(?:exactLegalBoard|conditionedReachability|tablebase|exactAcceleratorsHelp)'\)/u);
    assert.doesNotMatch(source, /<(?:Pc4DownloadControl|AcceleratorDownloadControl)\b/u);
    // Removing optional data controls must not remove worker policy or supply input.
    assert.match(source, /<WorkerAuthorityStatus\b/u);
    assert.match(source, /label\('useAllThreads'\)/u);
    const processed = await preprocess(source, vitePreprocess(), { filename });
    const result = compile(processed.code, { filename, generate: 'client' });
    assert.deepEqual(result.warnings, [], `${file} must compile without warnings`);
  }
});

test('Web and Desktop no longer mount GUI asset-management panels', async () => {
  for (const app of ['clearra-web', 'clearra-desktop']) {
    const route = await readFile(new URL(`../../../apps/${app}/src/routes/+page.svelte`, import.meta.url), 'utf8');
    assert.doesNotMatch(route, /clearra\.(?:pc4|accelerator)-download-control\.v1/u);
    assert.doesNotMatch(route, /import (?:Pc4DownloadPanel|AcceleratorDownloadPanel|AcceleratorAssetPanel)\b/u);
  }
});

test('both PC entry surfaces prewarm CPU workers without speculative GPU work', async () => {
  for (const file of ['SolverWorkspace.svelte', 'PcSolverStandalone.svelte']) {
    const source = await readFile(new URL(file, workspace), 'utf8');
    assert.match(source, /CPU_ONLY_RUNTIME_WARMUP_POLICY/u);
    assert.doesNotMatch(source, /DEFAULT_RUNTIME_WARMUP_POLICY|backend: 'auto'/u);
  }
});

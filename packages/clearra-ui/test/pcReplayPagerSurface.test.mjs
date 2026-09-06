import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

test('lazy replay pager compiles and retains explicit export and owner lifecycle boundaries', async () => {
  const filename = fileURLToPath(new URL('../src/lib/workspace/ProductResultPager.svelte', import.meta.url));
  const source = await readFile(filename, 'utf8');
  const processed = await preprocess(source, vitePreprocess(), { filename });
  assert.deepEqual(compile(processed.code, { filename, generate: 'client' }).warnings, []);
  assert.match(source, /onDestroy\(\(\) => releaseHandle\(\)\)/u);
  assert.match(source, /abortController\?\.abort\(\)/u);
  assert.match(source, /activePayload = nextPayload/u);
  assert.match(source, /lazyReplayPage === page/u);
  assert.match(source, /function loadVisiblePathPages\(signal\?: AbortSignal\)/u);
  assert.doesNotMatch(source, /\$:[^\n]*collectPcReplayGeometryExportPages/u);
});

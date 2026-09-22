import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const workspace = new URL('../src/lib/workspace/', import.meta.url);

test('boundary recovery and pinned Build selectors compile for browser use', async () => {
  const surfaces = [
    new URL('BoundaryRecoveryWorkspace.svelte', workspace),
    new URL('BuildV2PinnedSelector.svelte', workspace),
    new URL('../../../apps/clearra-web/src/routes/+page.svelte', import.meta.url),
    new URL('../../../apps/clearra-desktop/src/routes/+page.svelte', import.meta.url)
  ];
  for (const surface of surfaces) {
    const filename = fileURLToPath(surface);
    const source = await readFile(filename, 'utf8');
    const processed = await preprocess(source, vitePreprocess(), { filename });
    const result = compile(processed.code, { filename, generate: 'client' });
    assert.deepEqual(result.warnings, [], `${filename} must compile without warnings`);
  }
});

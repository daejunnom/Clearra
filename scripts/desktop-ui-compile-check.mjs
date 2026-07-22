import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { transform } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDirectory, '..');
const svelteSources = [
  'apps/clearra-desktop/src/routes/+page.svelte',
  'packages/clearra-ui/src/lib/components/DesktopHostShell.svelte',
  'packages/clearra-ui/src/lib/render/RenderStatusPanel.svelte'
];
const typescriptSources = [
  'packages/clearra-ui/src/lib/host/clearraDesktopHost.ts',
  'packages/clearra-ui/src/lib/stores/desktopJobStore.ts'
];
const preprocessor = vitePreprocess();
const artifactWrite = false;

for (const relativePath of svelteSources) {
  const filename = path.join(root, relativePath);
  const source = await readFile(filename, 'utf8');
  const processed = await preprocess(source, preprocessor, { filename });
  compile(processed.code, { filename, generate: 'client' });
}

for (const relativePath of typescriptSources) {
  const filename = path.join(root, relativePath);
  const source = await readFile(filename, 'utf8');
  await transform(source, {
    format: 'esm',
    loader: 'ts',
    sourcefile: filename,
    target: 'es2022'
  });
}

console.log(
  `desktop_ui_in_memory_compile=passed svelte=${svelteSources.length} typescript=${typescriptSources.length} artifact_write=${artifactWrite}`
);

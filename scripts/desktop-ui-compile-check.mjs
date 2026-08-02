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
  'packages/clearra-ui/src/lib/render/RenderStatusPanel.svelte',
  'packages/clearra-ui/src/lib/wasm/WasmTerminalShell.svelte',
  'packages/clearra-ui/src/lib/workspace/ProductModeTabs.svelte',
  'packages/clearra-ui/src/lib/workspace/WorkspaceHeader.svelte',
  'packages/clearra-ui/src/lib/workspace/SolverWorkspace.svelte',
  'packages/clearra-ui/src/lib/workspace/SearchControls.svelte',
  'packages/clearra-ui/src/lib/workspace/PcSolverStandalone.svelte',
  'packages/clearra-ui/src/lib/workspace/SetupFinderWorkspace.svelte',
  'packages/clearra-ui/src/lib/workspace/SetupFinderControls.svelte',
  'packages/clearra-ui/src/lib/workspace/SetupFinderResult.svelte',
  'packages/clearra-ui/src/lib/workspace/BuildProbabilityWorkspace.svelte',
  'packages/clearra-ui/src/lib/workspace/BuildProbabilityControls.svelte',
  'packages/clearra-ui/src/lib/workspace/BuildProbabilityResult.svelte',
  'packages/clearra-ui/src/lib/workspace/ForwardSearchWorkspace.svelte',
  'packages/clearra-ui/src/lib/workspace/ForwardSearchControls.svelte',
  'packages/clearra-ui/src/lib/workspace/ForwardSearchResult.svelte',
  'packages/clearra-ui/src/lib/workspace/ResultWorkspace.svelte',
  'packages/clearra-ui/src/lib/workspace/ResultWorkspaceFrame.svelte',
  'packages/clearra-ui/src/lib/workspace/WorkspaceProgressStatus.svelte'
];
const typescriptSources = [
  'packages/clearra-ui/src/lib/host/clearraDesktopHost.ts',
  'packages/clearra-ui/src/lib/stores/desktopJobStore.ts',
  'packages/clearra-ui/src/lib/wasm/WasmTerminalWorkerController.ts',
  'packages/clearra-ui/src/lib/wasm/wasmWorkerStore.ts',
  'packages/clearra-ui/src/lib/workspace/setupFinderModel.ts',
  'packages/clearra-ui/src/lib/workspace/buildProbabilityModel.ts',
  'packages/clearra-ui/src/lib/workspace/forwardSearchModel.ts',
  'packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts',
  'packages/clearra-ui/src/lib/workspace/workspaceRuntime.ts'
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

const desktopEntrySource = await readFile(
  path.join(root, 'apps/clearra-desktop/src/routes/+page.svelte'),
  'utf8'
);
for (const requiredMarker of [
  "'pc'",
  "'setup'",
  "'build-probability'",
  "'damage'",
  "'spin-finder'",
  "'ctk'",
  '<SolverWorkspace runtime="desktop"',
  '<SetupFinderWorkspace runtime="desktop"',
  '<BuildProbabilityWorkspace runtime="desktop"',
  '<ForwardSearchWorkspace tool={selectedTool} runtime="desktop"',
  '<CtkDrawerWorkspace'
]) {
  if (!desktopEntrySource.includes(requiredMarker)) {
    throw new Error(`desktop tool route is missing ${requiredMarker}`);
  }
}

console.log(
  `desktop_ui_in_memory_compile=passed svelte=${svelteSources.length} typescript=${typescriptSources.length} tool_routes=6 artifact_write=${artifactWrite}`
);

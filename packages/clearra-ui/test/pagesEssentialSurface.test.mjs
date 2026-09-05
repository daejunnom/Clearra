import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

function source(path) {
  return readFileSync(new URL(path, import.meta.url), 'utf8');
}

test('Pages exposes the exact v0.7.5 essential navigation without deleting advanced routes', () => {
  const navigation = source('../src/lib/workspace/workspaceNavigation.ts');
  const tabs = source('../src/lib/workspace/ProductModeTabs.svelte');
  const route = source('../../../apps/clearra-web/src/routes/+page.svelte');
  const essentialBlock = navigation.match(
    /PAGES_ESSENTIAL_WORKSPACE_MODES = Object\.freeze\(\[([\s\S]*?)\]\s+satisfies/u
  )?.[1];

  assert.ok(essentialBlock, 'Pages essential navigation declaration is missing');
  assert.deepEqual(
    Array.from(essentialBlock.matchAll(/'([^']+)'/gu), (match) => match[1]),
    ['pc', 'setup', 'build-probability', 'damage', 'spin-finder', 'ctk', 'player']
  );
  assert.match(route, /setContext\(WORKSPACE_MODE_VISIBILITY_CONTEXT, PAGES_ESSENTIAL_WORKSPACE_MODES\)/u);
  assert.match(tabs, /allTabs\.filter\(\(tab\) => visibleModes\.includes\(tab\.mode\)\)/u);

  for (const explicitAdvancedRoute of [
    'setup-score',
    'spin-structure',
    'build',
    'sequence',
    'sequence-dependencies',
    'parity',
    'fumen',
    'render',
    'to-gray',
    'mirror',
    'ren'
  ]) {
    assert.match(route, new RegExp(`'${explicitAdvancedRoute}'`, 'u'));
  }
  assert.match(route, /DocumentUtilityWorkspace tool=\{selectedTool\}/u);
});

test('CTK owns the Pages render entry and executes it through the local browser worker', () => {
  const route = source('../../../apps/clearra-web/src/routes/+page.svelte');
  const ctk = source('../src/lib/workspace/CtkDrawerWorkspace.svelte');
  const documentUtilityModel = source('../src/lib/workspace/documentUtilityModel.ts');
  const wasmRuntime = source('../../../apps/clearra-web/src/workers/clearraWasmRuntime.ts');
  const tablebaseAssets = source('../../../apps/clearra-web/src/workers/pc4TablebaseAssets.ts');

  assert.match(route, /<CtkDrawerWorkspace[\s\S]*?\{workerFactory\}[\s\S]*?\/>/u);
  assert.match(route, /return new Worker\(new URL\('\.\.\/workers\/clearraWorker\.ts'/u);
  assert.match(ctk, /new WasmTerminalWorkerController\([\s\S]*?workerFactory/u);
  assert.match(ctk, /const source = await encodeDocument\('ctk', controller\.signal\)/u);
  assert.match(ctk, /buildDocumentUtilityCommand\(commandInput\)/u);
  assert.doesNotMatch(ctk, /quoteWebCommandToken|clearra utility render/u);
  assert.match(ctk, /updateWasmCommandText/u);
  assert.match(ctk, /renderWorkerController\.run\(\)/u);
  assert.match(documentUtilityModel, /buildDocumentUtilityCommandArguments/u);
  assert.match(documentUtilityModel, /serializeCliCommandArguments\(buildDocumentUtilityCommandArguments\(input\)\)/u);
  assert.match(documentUtilityModel, /input\.artifactFormat === 'png'[\s\S]*?arguments_\.push\('--page', String\(input\.pageNumber\)\)/u);
  assert.match(ctk, /decodeValidatedRenderArtifact/u);
  assert.doesNotMatch(ctk, /\bfetch\s*\(|\bWebSocket\b|\bXMLHttpRequest\b/u);

  assert.match(wasmRuntime, /new URL\(`\$\{wasmRoot\}\/clearra_wasm\.manifest\.json`, self\.location\.origin\)/u);
  assert.match(wasmRuntime, /new URL\(`\$\{wasmRoot\}\/\$\{manifest\.bindings\.path\}`, self\.location\.origin\)/u);
  assert.match(wasmRuntime, /new URL\(`\$\{wasmRoot\}\/\$\{manifest\.wasm\.path\}`, self\.location\.origin\)/u);
  assert.deepEqual(
    Array.from(wasmRuntime.matchAll(/fetch\(([^,\n]+)/gu), (match) => match[1].trim()),
    ['manifestUrl', 'wasmUrl', 'artifactUrl']
  );
  assert.match(tablebaseAssets, /new URL\(`\$\{deploymentBase\}\/\$\{ARTIFACT_PATH\}`, self\.location\.origin\)/u);
  assert.deepEqual(
    Array.from(tablebaseAssets.matchAll(/fetch\(([^,\n]+)/gu), (match) => match[1].trim()),
    ['artifactUrl']
  );
  assert.match(tablebaseAssets, /credentials: 'same-origin'/u);
});

test('Setup path detail preserves results while rotating stale WASM only at its next run', () => {
  const setup = source('../src/lib/workspace/SetupFinderWorkspace.svelte');

  assert.match(setup, /let detailWorkerArtifactGeneration: string \| null = null;/u);
  assert.match(
    setup,
    /if \(detailWorkerBusy\)[\s\S]*?rotateStaleDetailWorkerForNewRun\(\);[\s\S]*?const worker = detailWorker \?\?/u
  );
  assert.match(
    setup,
    /detailWorkerArtifactGeneration = currentWasmArtifactGeneration\(\);/u
  );
  assert.match(
    setup,
    /function rotateStaleDetailWorkerForNewRun\(\)[\s\S]*?isCurrentWasmArtifactGeneration\(detailWorkerArtifactGeneration\)[\s\S]*?disposeDetailWorker\(\);/u
  );
  assert.match(
    setup,
    /function finishDetailWorkerRequest\(worker: Worker\)[\s\S]*?detailWorkerBusy = false;[\s\S]*?activeDetailKey = null;[\s\S]*?\}/u
  );
});

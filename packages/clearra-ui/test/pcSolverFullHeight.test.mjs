import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'esbuild';
import { compile, preprocess } from 'svelte/compiler';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const bundle = await build({
  bundle: true,
  format: 'esm',
  platform: 'node',
  logLevel: 'silent',
  stdin: {
    contents: `
      export * from './src/lib/workspace/pcSolverLinkState.ts';
      export { clearCompletedRows, createDefaultWorkspaceRequest,
        buildWorkspaceCommandArguments } from './src/lib/workspace/solverWorkspaceModel.ts';
      export { decodeInterchangeField } from './src/lib/workspace/fumenFieldImport.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const model = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

test('standalone PC infers long targets while retaining the compact empty default', () => {
  const initial = model.DEFAULT_PC_SOLVER_LINK_STATE;
  assert.equal(model.pcSolverAutomaticTargetLines(initial), 4);
  assert.equal(model.pcSolverDisplayHeight(initial), 4);
  for (const [pieces, lines] of [[5, 2], [10, 4], [14, 4], [15, 6], [16, 6], [20, 8], [60, 24]]) {
    const state = { ...initial, queue: 'O'.repeat(pieces) };
    const target = model.pcSolverAutomaticTargetLines(state);
    assert.equal(target, lines, `${pieces} supplied pieces`);
    assert.equal(model.pcSolverDisplayHeight({ ...state, lines: target }), Math.max(4, lines));
    const path = model.encodePcSolverPath({ ...state, lines: target });
    assert.deepEqual(model.decodePcSolverPath(path), { ...state, lines: target });
  }

  assert.deepEqual(
    model.decodePcSolverPath('v1/4/v115%40vhAAgH/hold/srs-plus/P7P3'),
    { ...initial, queue: 'P7P3' },
    'previous v1 links retain their field, queue, Hold and rule'
  );
});

test('standalone CTK import and sharing preserve high cells and initial line-clear semantics', () => {
  const topO = (3n << 228n) | (3n << 238n);
  const completeRow = 0x3ffn << 60n;
  const boardMask = topO | completeRow;
  const state = { ...model.DEFAULT_PC_SOLVER_LINK_STATE,
    lines: 24, boardMask, queue: 'O'.repeat(59), holdEnabled: false };
  const path = model.encodePcSolverPath(state);
  assert.deepEqual(model.decodePcSolverPath(path), state);
  const imported = model.decodeInterchangeField(decodeURIComponent(path.split('/')[2]), 24);
  assert.equal(imported.boardMask, boardMask);
  assert.equal(imported.occupiedHeight, 24);
  assert.equal(model.pcSolverDisplayHeight({ lines: 4, boardMask: imported.boardMask }), 24);
  assert.equal(model.pcSolverAutomaticTargetLines(state), 24);

  const normalized = model.clearCompletedRows(imported.boardMask, 24);
  assert.equal(normalized.clearedRows, 1);
  assert.equal(normalized.boardMask, topO >> 10n);
  const request = { ...model.createDefaultWorkspaceRequest(), ...state,
    boardMask: normalized.boardMask, lines: model.pcSolverAutomaticTargetLines({
      ...state, boardMask: normalized.boardMask
    }) };
  const args = model.buildWorkspaceCommandArguments(request);
  assert.equal(BigInt(args[args.indexOf('--board-mask') + 1]), topO >> 10n);
  assert.equal(args[args.indexOf('--height') + 1], '24');
  assert.equal(args[args.indexOf('--pieces') + 1], '59');
});

test('share-link transport bounds remain independent of the supported search height', () => {
  const state = { ...model.DEFAULT_PC_SOLVER_LINK_STATE, lines: 24 };
  const path = model.encodePcSolverPath(state);
  assert.equal(model.decodePcSolverPath(path)?.lines, 24);
  assert.equal(model.decodePcSolverPath(path.replace('v1/24/', 'v1/25/')), null);
  assert.equal(model.decodePcSolverPath(model.encodePcSolverPath({ ...state, queue: 'O'.repeat(257) })), null);
  assert.equal(model.decodePcSolverPath(`${path}${'O'.repeat(512)}`), null);
  assert.equal(model.decodePcSolverPath(path.replace(/\/-$/u, '/O%00')), null);
});

test('standalone editor connects imported full-height fields and compiles without warnings', async () => {
  const filename = fileURLToPath(new URL('../src/lib/workspace/PcSolverStandalone.svelte', import.meta.url));
  const source = readFileSync(filename, 'utf8');
  assert.match(source, /on:import=\{\(event\) => setBoardMask\(event\.detail\.boardMask, event\.detail\.lines\)\}/u);
  const processed = await preprocess(source, vitePreprocess(), { filename });
  const result = compile(processed.code, { filename, generate: 'client' });
  assert.deepEqual(result.warnings, []);
});

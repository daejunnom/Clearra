import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export { createPlayerEngine } from './src/lib/workspace/player/playerEngine.ts';
      export { buildPlayerPcQueuePattern, preparePlayerPcFinder, preparePlayerSetupFinder } from './src/lib/workspace/player/playerFinderModel.ts';
      export { buildWorkspaceCommand, workspaceRequestForDesktop } from './src/lib/workspace/solverWorkspaceModel.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const contract = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);
const {
  buildWorkspaceCommand,
  buildPlayerPcQueuePattern,
  createPlayerEngine,
  preparePlayerPcFinder,
  preparePlayerSetupFinder,
  workspaceRequestForDesktop
} = contract;

test('finder state exposes a full six-row PC source without widening the render preview', () => {
  const engine = createPlayerEngine({ seed: 17 });
  const renderQueue = engine.getRenderView().queue;
  const state = engine.getFinderState();

  assert.equal(renderQueue.length, engine.settings.previewCount);
  assert.equal(state.futureQueue.length, 15);
  assert.equal(state.futureQueueBagIds.length, state.futureQueue.length);
  assert.equal(state.activeBagId, state.currentDrawBagId);
  assert.equal(state.setupBagRemainder.length, 7);
  assert.equal(new Set(state.setupBagRemainder).size, 7);

  const prepared = preparePlayerPcFinder(state, 8);
  assert.equal(prepared.ok, true);
  assert.equal(prepared.targetLines, 4);
  assert.match(prepared.request.queue, /^.{7}P4$/);
  assert.equal(prepared.request.holdPiece, 'empty');
  assert.match(buildWorkspaceCommand(prepared.request), /--hold empty .*--patterns/);
});

test('queue-unknown and queue-based modes compile the exact conditioned example', () => {
  const sources = [
    ...'IOSZ'.split('').map(piece => ({ piece, bagId: 1 })),
    ...'SZTIOJL'.split('').map(piece => ({ piece, bagId: 2 }))
  ];
  const shared = {
    sources,
    hold: null,
    currentDrawBagId: 1,
    sourceWindow: 11,
    knownSourceCount: 7
  };
  const unknown = buildPlayerPcQueuePattern({ ...shared, mode: 'queue-unknown' });
  const based = buildPlayerPcQueuePattern({ ...shared, mode: 'queue-based' });

  assert.deepEqual(unknown, {
    source: '[IOSZ]![SZT]![^SZT]!',
    sequenceLength: 11,
    patternCount: 3456
  });
  assert.deepEqual(based, {
    source: 'IOSZSZT[^SZT]!',
    sequenceLength: 11,
    patternCount: 24
  });
});

test('an occupied hold counts toward the seven-piece known Player state', () => {
  const sources = [
    ...'OSZ'.split('').map(piece => ({ piece, bagId: 1 })),
    ...'SZTIOJL'.split('').map(piece => ({ piece, bagId: 2 }))
  ];
  const shared = {
    sources,
    hold: { piece: 'I', bagId: 1 },
    currentDrawBagId: 1,
    sourceWindow: 10,
    knownSourceCount: 6
  };

  assert.equal(
    buildPlayerPcQueuePattern({ ...shared, mode: 'queue-unknown' }).source,
    '[OSZ]![SZT]![^SZT]!'
  );
  assert.equal(
    buildPlayerPcQueuePattern({ ...shared, mode: 'queue-based' }).source,
    'OSZSZT[^SZT]!'
  );
});

test('queue visibility changes decisions only and never rewrites the generated pattern', () => {
  const engine = createPlayerEngine({ seed: 19 });
  const state = engine.getFinderState();
  const oracle = preparePlayerPcFinder(state, {
    hardwareConcurrency: 8,
    queueMode: 'queue-based',
    visibleRangeOnly: false
  });
  const visible = preparePlayerPcFinder(state, {
    hardwareConcurrency: 8,
    queueMode: 'queue-based',
    visibleRangeOnly: true
  });
  const unknown = preparePlayerPcFinder(state, {
    hardwareConcurrency: 8,
    queueMode: 'queue-unknown',
    visibleRangeOnly: true
  });

  assert.equal(oracle.ok, true);
  assert.equal(visible.ok, true);
  assert.equal(unknown.ok, true);
  assert.equal(oracle.request.queue, visible.request.queue);
  assert.equal(oracle.request.queueKnowledge, 'oracle');
  assert.equal(visible.request.queueKnowledge, 'visible-7');
  assert.equal(unknown.request.queueKnowledge, 'oracle');
  assert.match(buildWorkspaceCommand(visible.request), /--patterns .*--queue-knowledge visible-7/);
  const desktop = workspaceRequestForDesktop(visible.request, 'en');
  assert.equal(desktop.patterns, visible.request.queue);
  assert.equal(desktop.queue_knowledge, 'visible-7');
  assert.equal(desktop.pattern_budget, visible.request.maxPatterns);
});

test('occupied Player hold is passed separately after its turn cooldown ends', () => {
  const engine = createPlayerEngine({ seed: 23 });
  const heldPiece = engine.getRenderView().active.piece;
  engine.dispatch('hold');
  const cooldown = preparePlayerPcFinder(engine.getFinderState(), 8);
  assert.deepEqual(cooldown, { ok: false, issue: 'hold-already-used' });

  engine.dispatch('hard-drop');
  const prepared = preparePlayerPcFinder(engine.getFinderState(), 8);
  assert.equal(prepared.ok, true);
  assert.equal(prepared.request.holdPiece, heldPiece);
  assert.match(buildWorkspaceCommand(prepared.request), new RegExp(`--hold ${heldPiece}`));
  assert.equal(workspaceRequestForDesktop(prepared.request, 'en').hold_piece, heldPiece);
  assert.equal(engine.getFinderState().holdBagId !== null, true);
});

test('cross-bag hold swaps preserve active, hold, and draw-stream provenance', () => {
  const engine = createPlayerEngine({ seed: 29 });
  const firstBagId = engine.getFinderState().activeBagId;
  engine.dispatch('hold');
  for (let index = 0; index < 6; index += 1) engine.dispatch('hard-drop');

  const beforeSwap = engine.getFinderState();
  assert.notEqual(beforeSwap.currentDrawBagId, firstBagId);
  assert.equal(beforeSwap.holdBagId, firstBagId);
  const secondBagId = beforeSwap.activeBagId;

  engine.dispatch('hold');
  const swapped = engine.getFinderState();
  assert.equal(swapped.activeBagId, firstBagId);
  assert.equal(swapped.holdBagId, secondBagId);
  assert.equal(swapped.currentDrawBagId, secondBagId);

  engine.dispatch('hard-drop');
  engine.undo();
  const restored = engine.getFinderState();
  assert.equal(restored.activeBagId, swapped.activeBagId);
  assert.equal(restored.holdBagId, swapped.holdBagId);
  assert.equal(restored.currentDrawBagId, swapped.currentDrawBagId);
  assert.deepEqual(restored.futureQueueBagIds, swapped.futureQueueBagIds);
});

test('setup finder uses only a proven standard-bag residue on an empty locked field', () => {
  const randomEngine = createPlayerEngine({ seed: 31 });
  const prepared = preparePlayerSetupFinder(randomEngine.getFinderState());
  assert.equal(prepared.ok, true);
  assert.equal(prepared.request.remaining.length, 7);
  assert.equal(new Set(prepared.request.remaining).size, 7);

  const customEngine = createPlayerEngine({
    seed: 31,
    initialQueue: ['I', 'O', 'T', 'S', 'Z', 'J', 'L']
  });
  assert.deepEqual(preparePlayerSetupFinder(customEngine.getFinderState()), {
    ok: false,
    issue: 'setup-bag-boundary-unknown'
  });
  assert.deepEqual(preparePlayerPcFinder(customEngine.getFinderState(), 8), {
    ok: false,
    issue: 'pc-bag-boundary-unknown'
  });
});

test('finder rejects states the existing exact contracts cannot represent', () => {
  const engine = createPlayerEngine({ seed: 41 });
  const base = engine.getFinderState();

  assert.deepEqual(
    preparePlayerPcFinder({
      ...base,
      settings: { ...base.settings, unlimitedHold: true }
    }, 8),
    { ok: false, issue: 'unlimited-hold-unsupported' }
  );
  assert.deepEqual(
    preparePlayerSetupFinder({ ...base, hold: 'T' }),
    { ok: false, issue: 'setup-hold-unsupported' }
  );

  const highBoard = new Uint16Array(base.rowMasks);
  highBoard[6] = 1;
  assert.deepEqual(
    preparePlayerPcFinder({ ...base, rowMasks: highBoard }, 8),
    { ok: false, issue: 'board-above-pc-limit' }
  );
});

test('undo restores standard-bag provenance together with queue order', () => {
  const engine = createPlayerEngine({ seed: 53 });
  const before = engine.getFinderState();
  engine.dispatch('hard-drop');
  engine.undo();
  const restored = engine.getFinderState();

  assert.equal(restored.active.piece, before.active.piece);
  assert.deepEqual(restored.futureQueue, before.futureQueue);
  assert.deepEqual(restored.futureQueueBagIds, before.futureQueueBagIds);
  assert.equal(restored.activeBagId, before.activeBagId);
  assert.equal(restored.holdBagId, before.holdBagId);
  assert.equal(restored.currentDrawBagId, before.currentDrawBagId);
  assert.deepEqual(restored.setupBagRemainder, before.setupBagRemainder);
});

import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  entryPoints: [
    fileURLToPath(new URL('../src/lib/workspace/ctkAutoColor.ts', import.meta.url))
  ],
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  write: false
});
const autoColorModule = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);
const {
  CTK_PAINT_SHORTCUTS,
  ctkPaintSelectionFromShortcut,
  inferCtkAutoColorPiece
} = autoColorModule;

const pieces = ['I', 'O', 'T', 'S', 'Z', 'J', 'L'];
const baseOffsets = {
  I: [[-1, 0], [0, 0], [1, 0], [2, 0]],
  O: [[0, 0], [1, 0], [0, 1], [1, 1]],
  T: [[-1, 0], [0, 0], [1, 0], [0, 1]],
  S: [[-1, 0], [0, 0], [0, 1], [1, 1]],
  Z: [[-1, 1], [0, 1], [0, 0], [1, 0]],
  J: [[-1, 1], [-1, 0], [0, 0], [1, 0]],
  L: [[1, 1], [-1, 0], [0, 0], [1, 0]]
};
const rotationCounts = { I: 2, O: 1, T: 4, S: 2, Z: 2, J: 4, L: 4 };

test('automatic coloring recognizes every unique tetromino orientation', () => {
  let orientationCount = 0;
  for (const piece of pieces) {
    for (let turns = 0; turns < rotationCounts[piece]; turns += 1) {
      const indexes = baseOffsets[piece].map(([sourceX, sourceY]) => {
        let x = sourceX;
        let y = sourceY;
        for (let turn = 0; turn < turns; turn += 1) [x, y] = [y, -x];
        return (y + 5) * 10 + x + 4;
      });
      assert.equal(
        inferCtkAutoColorPiece(indexes),
        piece,
        `${piece} rotation ${turns}`
      );
      orientationCount += 1;
    }
  }
  assert.equal(orientationCount, 19);
});

test('automatic coloring is translation invariant and rejects board wrapping', () => {
  assert.equal(inferCtkAutoColorPiece([21, 22, 23, 32]), 'T');
  assert.equal(inferCtkAutoColorPiece([67, 68, 69, 78]), 'T');
  assert.equal(inferCtkAutoColorPiece([9, 10, 11, 12]), null);
});

test('automatic coloring supports pieces split by cleared rows', () => {
  const splitT = [11, 12, 13, 42];
  assert.equal(inferCtkAutoColorPiece(splitT), 'T');
  assert.equal(inferCtkAutoColorPiece(splitT, 10, false), null);
});

test('automatic coloring rejects incomplete, duplicate, and invalid groups', () => {
  assert.equal(inferCtkAutoColorPiece([0, 1, 2]), null);
  assert.equal(inferCtkAutoColorPiece([0, 1, 2, 3, 4]), null);
  assert.equal(inferCtkAutoColorPiece([0, 1, 1, 2]), null);
  assert.equal(inferCtkAutoColorPiece([0, 2, 20, 22]), null);
  assert.equal(inferCtkAutoColorPiece([-1, 0, 1, 2]), null);
  assert.equal(inferCtkAutoColorPiece([0, 1, 2, 3], 0), null);
});

test('paint shortcuts use intuitive letters in either case', () => {
  for (const { key, selection } of CTK_PAINT_SHORTCUTS) {
    assert.equal(ctkPaintSelectionFromShortcut(key), selection);
    assert.equal(ctkPaintSelectionFromShortcut(key.toLowerCase()), selection);
  }
  assert.equal(ctkPaintSelectionFromShortcut('Backspace'), null);
  assert.equal(ctkPaintSelectionFromShortcut('Delete'), null);
  assert.equal(ctkPaintSelectionFromShortcut('?'), undefined);
});

test('physical key fallback keeps shortcuts usable outside a Latin layout', () => {
  assert.equal(ctkPaintSelectionFromShortcut('ㅑ', 'KeyI'), 'I');
  assert.equal(ctkPaintSelectionFromShortcut('ㄷ', 'KeyE'), null);
  assert.equal(ctkPaintSelectionFromShortcut('Process', 'KeyA'), 'auto');
});

// SRP rationale: verify the product's existing mandatory-selection and CTK3
// encoder boundary with real full-height drawings, not a fake encoding callback.
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'esbuild';

const bundle = await build({
  bundle: true, format: 'esm', platform: 'node', logLevel: 'silent', write: false,
  stdin: {
    contents: `
      export { emptyMandatorySelection, toggleMandatorySolution }
        from './src/lib/workspace/mandatorySolutionSelection.ts';
      export { encodeCtkSolutionKeySegment } from './src/lib/workspace/solutionExport.ts';
      export { decodeCtk3 } from './src/lib/workspace/ctk3Codec.ts';
    `,
    loader: 'ts', resolveDir: fileURLToPath(new URL('..', import.meta.url))
  }
});
const { emptyMandatorySelection, toggleMandatorySolution, encodeCtkSolutionKeySegment, decodeCtk3 } =
  await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const sourceHash = 'cts1:0123456789abcdef';
const hex = mask => mask.toString(16).padStart(64, '0');
const initial = 1n << 236n;
const verticalI = (1n << 209n) | (1n << 219n) | (1n << 229n) | (1n << 239n);
const key = `ctk2|height=24|initial=${hex(initial)}|placements=I:${hex(verticalI)}`;

test('24L mandatory selection reuses the ordinary drawing encoder and retains cell 239', () => {
  const selected = toggleMandatorySolution(emptyMandatorySelection(), key, sourceHash, encodeCtkSolutionKeySegment);
  assert.deepEqual(selected.pinnedSolutionKeys, [key]);
  assert.equal(selected.pinnedSourceSetHash, sourceHash);
  const drawing = decodeCtk3(selected.pinnedSolutionDocument);
  assert.equal(drawing.pages.length, 1);
  assert.equal(drawing.pages[0].height, 24);
  assert.equal(drawing.pages[0].cells[236], 'G');
  for (const cell of [209, 219, 229, 239]) assert.equal(drawing.pages[0].cells[cell], 'I');
  assert.deepEqual(toggleMandatorySolution(selected, key, sourceHash, encodeCtkSolutionKeySegment), emptyMandatorySelection());
  assert.throws(() => toggleMandatorySolution(selected, key, 'cts1:aaaaaaaaaaaaaaaa', encodeCtkSolutionKeySegment),
    /different result/u);
});

test('extended pins reject mixed-width, out-of-scope and overlapping drawings without changing prior selection', () => {
  const previous = toggleMandatorySolution(emptyMandatorySelection(), key, sourceHash, encodeCtkSolutionKeySegment);
  const snapshot = structuredClone(previous);
  for (const invalid of [
    key.replace(hex(initial), hex(initial).slice(-16)),
    key.replace('height=24', 'height=25'),
    `ctk2|height=24|initial=${hex(1n << 240n)}|placements=I:${hex(verticalI)}`,
    `ctk2|height=24|initial=${hex(1n << 239n)}|placements=I:${hex(verticalI)}`,
    `ctk2|height=24|initial=${hex(initial)}|placements=I:${hex(verticalI).slice(-16)}`,
    'candidate-1'
  ]) {
    assert.throws(() => toggleMandatorySolution(previous, invalid, sourceHash, encodeCtkSolutionKeySegment));
    assert.deepEqual(previous, snapshot);
  }
});

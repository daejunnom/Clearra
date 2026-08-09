import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export { encodeFinesseWitnessCtk } from './src/lib/workspace/solutionExport.ts';
      export { decodeCtk3 } from './src/lib/workspace/ctk3Codec.ts';
    `,
    resolveDir: packageRoot,
    sourcefile: 'finesse-solution-export-test-entry.ts'
  },
  write: false
});
const module = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

test('pattern finesse export keeps colored operations and uses the minimum-average annotation', () => {
  const source = module.encodeFinesseWitnessCtk({
    solutionKey:
      'ctk1|initial=000000000000003f|placements=I:00000000000003c0,O:0000000000300c00',
    totalInputs: 3,
    annotationInputs: '3.5000',
    inputSequence: ['hard-drop', 'tap-left', 'hard-drop'],
    placements: [
      { piece: 'I', rotation: 2, x: 6, y: 0 },
      { piece: 'O', rotation: 3, x: 0, y: 0 }
    ]
  });
  const pages = module.decodeCtk3(source).pages;

  assert.equal(pages.length, 2);
  assert.equal(pages[0].comment, 'F=3.5');
  assert.deepEqual(pages[0].cells.slice(0, 10), [
    'G', 'G', 'G', 'G', 'G', 'G', null, null, null, null
  ]);
  assert.deepEqual(pages.map((page) => page.operation), [
    { piece: 'I', rotation: 'spawn', x: 7, y: 0 },
    { piece: 'O', rotation: 'spawn', x: 0, y: 0 }
  ]);
  assert.equal(pages[1].cells.length, 0);
});

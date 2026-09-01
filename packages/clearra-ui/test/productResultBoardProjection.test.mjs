import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const source = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export {
        solutionBoardPreviewFromKey,
        solutionBoardPreviewFromReplay
      } from './src/lib/workspace/solutionBoardPreview.ts';
    `,
    loader: 'ts',
    resolveDir: packageRoot
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

const solutionKey =
  'ctk1|initial=00000000000003f0|placements=I:000000000000000f';

test('portfolio and score normalized solution keys project to the shared board view', () => {
  for (const family of ['portfolio', 'score']) {
    const preview = production.solutionBoardPreviewFromKey(solutionKey, 4);
    assert.equal(preview.source, 'solution-key', family);
    assert.equal(preview.board?.height, 4, family);
    assert.equal(preview.board?.cells.length, 40, family);
    assert.equal(preview.board?.cells.filter((cell) => cell === 'I').length, 4, family);
    assert.equal(preview.board?.cells.filter((cell) => cell === 'G').length, 6, family);
  }
});

test('path witnesses project their last replay placement to board cells', () => {
  const preview = production.solutionBoardPreviewFromReplay([
    {
      active_piece: 'I',
      placement_mask: '0x000000000000000f',
      board_before_mask: '0x00000000000003f0',
      board_after_placement_mask: '0x00000000000003ff',
      board_after_line_clear_mask: '0x0000000000000000'
    }
  ], 4);
  assert.equal(preview.source, 'replay-last-placement');
  assert.equal(preview.board?.height, 4);
  assert.equal(preview.board?.cells.length, 40);
  assert.equal(preview.board?.cells.filter((cell) => cell === 'I').length, 4);
  assert.equal(preview.board?.cells.filter((cell) => cell === 'G').length, 6);
});

test('product result UI renders boards first and keeps internal keys in collapsed details', () => {
  const pager = source('../src/lib/workspace/ProductResultPager.svelte');
  const preview = source('../src/lib/workspace/SolutionBoardPreview.svelte');
  const gallery = source('../src/lib/workspace/SolutionGallery.svelte');
  const resultWorkspace = source('../src/lib/workspace/ResultWorkspace.svelte');
  const pcResult = source('../src/lib/workspace/PcSolverResult.svelte');

  assert.match(pager, /import SolutionBoardPreview from '\.\/SolutionBoardPreview\.svelte'/u);
  assert.match(pager, /solutionBoardPreviewFromReplay\(witness\.steps, targetLines\)/u);
  assert.doesNotMatch(pager, /<code>\{member\.normalized_solution_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{winner\.normalized_solution_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{winner\.candidate_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{candidate\.candidate_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{witness\.normalized_trace_key\}<\/code>/u);

  const detailsStart = preview.indexOf('<details class="raw-key-details">');
  const rawKeyCode = preview.indexOf('<code data-role="raw-solution-key">{rawKey}</code>');
  assert.ok(detailsStart >= 0 && rawKeyCode > detailsStart);
  assert.match(preview, /\{#each board\.cells as cell\}/u);
  assert.match(preview, /role="img"/u);

  assert.match(gallery, /<SolutionBoardPreview/u);
  assert.match(gallery, /solutionBoardPreviewFromKey\(key, lines\)/u);
  assert.match(resultWorkspace, /<ProductResultPager[\s\S]*?\{targetLines\}/u);
  assert.match(pcResult, /<ProductResultPager[\s\S]*?\{targetLines\}/u);
});

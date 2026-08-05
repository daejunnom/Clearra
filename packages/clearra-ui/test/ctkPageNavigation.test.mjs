import assert from 'node:assert/strict';
import test from 'node:test';

import {
  CTK_PAGE_PREVIEW_RADIUS,
  ctkPageIndexFromArrowKey,
  ctkPageStripItems
} from '../src/lib/workspace/ctkPageNavigation.ts';

test('page previews include up to 100 frames before and after the current frame', () => {
  assert.equal(CTK_PAGE_PREVIEW_RADIUS, 100);
  const items = ctkPageStripItems(500, 250);
  const pages = items
    .filter((item) => item.kind === 'page')
    .map((item) => item.index);

  assert.equal(pages.length, 203);
  assert.deepEqual(pages.slice(0, 3), [0, 150, 151]);
  assert.deepEqual(pages.slice(-3), [349, 350, 499]);
  assert.equal(pages.includes(149), false);
  assert.equal(pages.includes(351), false);
  assert.equal(items.filter((item) => item.kind === 'gap').length, 2);
});

test('short documents and boundary windows do not duplicate endpoint previews', () => {
  assert.deepEqual(
    ctkPageStripItems(201, 100).map((item) => item.kind === 'page' ? item.index : item.kind),
    Array.from({ length: 201 }, (_, index) => index)
  );

  const pages = ctkPageStripItems(500, 50)
    .filter((item) => item.kind === 'page')
    .map((item) => item.index);
  assert.deepEqual(pages.slice(0, 3), [0, 1, 2]);
  assert.deepEqual(pages.slice(-3), [149, 150, 499]);
  assert.equal(pages.length, 152);
});

test('left and right arrows select adjacent frames without wrapping', () => {
  assert.equal(ctkPageIndexFromArrowKey('ArrowLeft', 5, 10), 4);
  assert.equal(ctkPageIndexFromArrowKey('ArrowRight', 5, 10), 6);
  assert.equal(ctkPageIndexFromArrowKey('ArrowLeft', 0, 10), null);
  assert.equal(ctkPageIndexFromArrowKey('ArrowRight', 9, 10), null);
  assert.equal(ctkPageIndexFromArrowKey('ArrowDown', 5, 10), null);
  assert.equal(ctkPageIndexFromArrowKey('ArrowRight', 0, 0), null);
});

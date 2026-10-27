import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';
import {
  decoder as fumenDecoder,
  encoder as fumenEncoder
} from 'tetris-fumen';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export {
        encodeColoredFumenPages,
        SolutionExportError
      } from './src/lib/workspace/solutionExport.ts';
      export {
         decodeFieldDocument,
         decodeFieldInput,
         openFieldDocument,
         decodeFumenWithinPageLimit,
         encodeFieldDocument,
         encodeFieldDocumentAsync,
         encodeFumenPageSourceAsync,
         FUMEN_MAX_PAGES,
         FUMEN_MAX_SOURCE_CHARACTERS,
         inspectFumenPageCount
      } from './src/lib/workspace/fieldInterchange.ts';
      export { CtkDrawerDocument } from './src/lib/workspace/ctkDrawerDocument.ts';
      export { fieldImportFailureMessageKey } from './src/lib/workspace/fieldImportFailure.ts';
      export { workspaceMessage } from './src/lib/workspace/workspaceI18n.ts';
    `,
    resolveDir: packageRoot,
    sourcefile: 'fumen-codec-test-entry.ts'
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

test('fast Fumen export preserves percent, Hangul, and astral comments', () => {
  const comment = '주석 100% 😀 / clearra';
  const encoded = production.encodeColoredFumenPages([emptyPage(comment)]);

  assert.equal(
    encoded,
    'v115@vhAAgWyAlvQSBGFEfEDqG6BFb85AQo78A1no2Al/SS?BTGEfEE4k2AFbMzAFbsiDs4DXEyiBAA'
  );
  assert.equal(fumenDecoder.decode(encoded)[0]?.comment, comment);
});

test('fast Fumen export enforces escaped lengths 4094, 4095, and 4096', () => {
  for (const length of [4094, 4095]) {
    const comment = 'A'.repeat(length);
    const encoded = production.encodeColoredFumenPages([emptyPage(comment)]);
    assert.equal(fumenDecoder.decode(encoded)[0]?.comment, comment);
  }

  assert.throws(
    () => production.encodeColoredFumenPages([emptyPage('A'.repeat(4096))]),
    isFumenError('fumen-comment-too-long')
  );
});

test('fast Fumen export never truncates through a percent escape', () => {
  const boundary = `${'A'.repeat(4092)}%`;
  const encoded = production.encodeColoredFumenPages([emptyPage(boundary)]);
  assert.equal(fumenDecoder.decode(encoded)[0]?.comment, boundary);

  assert.throws(
    () =>
      production.encodeColoredFumenPages([
        emptyPage(`${'A'.repeat(4093)}%`)
      ]),
    isFumenError('fumen-comment-too-long')
  );
});

test('fast Fumen export rejects unpaired UTF-16 surrogates', () => {
  assert.throws(
    () => production.encodeColoredFumenPages([emptyPage('\ud800')]),
    isFumenError('invalid-fumen-comment')
  );
});

test('general Fumen document export applies the same lossless comment contract', () => {
  const comment = '주석 100% 😀';
  const encoded = production.encodeFieldDocument(
    { width: 10, pages: [{ height: 0, cells: [], comment }] },
    'fumen'
  );
  assert.equal(fumenDecoder.decode(encoded)[0]?.comment, comment);

  assert.throws(
    () =>
      production.encodeFieldDocument(
        {
          width: 10,
          pages: [{ height: 0, cells: [], comment: 'A'.repeat(4096) }]
        },
        'fumen'
      ),
    (error) =>
      error instanceof Error && error.message === 'fumen-comment-too-long'
  );
});

test('CTK3 to Fumen keeps comments on their own pages instead of inheriting them', async () => {
  const comments = ['첫 페이지 😀', undefined, '같은 주석', '같은 주석', '', '#Q=[](I)OT', undefined, '마지막', undefined];
  const source = production.encodeFieldDocument({
    width: 10,
    pages: comments.map((comment, index) => ({
      height: 1,
      cells: Array.from({ length: 10 }, (_, x) => x === index ? 'T' : null),
      ...(comment === undefined ? {} : { comment }),
      flags: { lock: false }
    }))
  }, 'ctk');
  const document = production.decodeFieldDocument(source);
  const expected = comments.map(comment => comment ?? '');

  for (const encoded of [
    production.encodeFieldDocument(document, 'fumen'),
    await production.encodeFieldDocumentAsync(document, 'fumen')
  ]) {
    const pages = fumenDecoder.decode(encoded);
    assert.deepEqual(pages.map(page => page.comment), expected);
    assert.deepEqual(production.decodeFieldDocument(encoded).pages.map(page => page.comment ?? ''), expected);
    for (let index = 0; index < pages.length; index += 1) {
      assert.equal(pages[index].field.at(index, 0), 'T');
      assert.equal(pages[index].flags.lock, false);
    }
  }
});

test('newline-separated Fumen chunks open lazily in order with comments and fields intact', async () => {
  const page = { height: 1, cells: ['T', ...Array(9).fill(null)], comment: 'first', flags: { lock: false } };
  const boundary = { height: 1, cells: ['I', ...Array(9).fill(null)], comment: '끝 100% 😀', flags: { lock: false } };
  const next = { height: 1, cells: ['L', ...Array(9).fill(null)], comment: '다음', flags: { lock: false } };
  const first = production.encodeFieldDocument({ width: 10, pages: [...Array(4095).fill(page), boundary] }, 'fumen');
  const second = production.encodeFieldDocument({ width: 10, pages: [next] }, 'fumen');
  const input = `${first}\n${second}`;
  assert.throws(() => production.openFieldDocument(Array(257).fill(first).join('\n')), /fumen-bundle-page-limit/);
  const reader = production.openFieldDocument(input);
  assert.equal(reader.pageCount, 4097);
  const drawer = production.CtkDrawerDocument.fromReader(reader);
  assert.equal(drawer.pageCount, 4097);
  assert.equal((await drawer.readPage(4096)).comment, '다음');
  assert.deepEqual((await drawer.readPages(4095, 2)).map(page => page.cells[0]), ['I', 'L']);
  const recopy = await production.encodeFumenPageSourceAsync(drawer);
  assert.equal(recopy.split('\n').length, 2);
  const reopened = production.openFieldDocument(recopy);
  assert.equal(reopened.pageCount, 4097);
  assert.deepEqual((await reopened.readPages(4095, 2)).map(page => [page.comment, page.cells[0]]), [
    ['끝 100% 😀', 'I'], ['다음', 'L']
  ]);
  reopened.close();
  assert.deepEqual((await reader.readPages(4095, 2)).map(({ comment, cells }) => [comment, cells[0]]), [
    ['끝 100% 😀', 'I'], ['다음', 'L']
  ]);
  assert.equal((await reader.readPage(0)).comment, 'first');
  assert.equal((await reader.readPage(4096)).comment, '다음');
  assert.equal(production.decodeFieldInput(input, 4).boardMask, 1n);
  await assert.rejects(reader.decodeAll(), /fumen-page-limit/);
  assert.throws(() => production.decodeFieldDocument(input), /fumen-page-limit/);
  reader.close();
  await assert.rejects(reader.readPage(0), /closed/);
});

test('bundle preserves duplicate chunks and rejects a malformed following document', async () => {
  const single = production.encodeFieldDocument({ width: 10, pages: [{ height: 0, cells: [], comment: 'same' }] }, 'fumen');
  const onePage = { width: 10, pageCount: 1, async readPages() { return [{ height: 0, cells: [], comment: 'same' }]; } };
  assert.equal(await production.encodeFumenPageSourceAsync(onePage), single);
  const reader = production.openFieldDocument(`${single}\r\n${single}`);
  assert.equal(reader.pageCount, 2);
  assert.deepEqual((await reader.decodeAll()).pages.map(page => page.comment), ['same', 'same']);
  reader.close();
  assert.throws(() => production.openFieldDocument(`${single}\nnot-fumen`), /invalid-fumen-input/);
  assert.throws(() => production.openFieldDocument(`not-fumen\n${single}`), /invalid-fumen-input/);
  assert.throws(() => production.openFieldDocument(`${single}\n${'x'.repeat(production.FUMEN_MAX_SOURCE_CHARACTERS + 1)}`), /fumen-input-too-large/);
  assert.throws(() => production.openFieldDocument(Array(4097).fill(single).join('\n')), /fumen-bundle-page-limit/);
  const abort = new AbortController();
  const cancelled = production.openFieldDocument(`${single}\n${single}`, { signal: abort.signal });
  abort.abort();
  await assert.rejects(cancelled.readPage(0), (error) => error.name === 'AbortError');
  cancelled.close();
  await assert.rejects(production.encodeFumenPageSourceAsync({ width: 10, pageCount: 2, async readPages() { return []; } }), /incomplete/);
  await assert.rejects(production.encodeFumenPageSourceAsync({ width: 10, pageCount: 2, async readPages() { throw new Error('read-failed'); } }), /read-failed/);
  await assert.rejects(production.encodeFumenPageSourceAsync({ width: 10, pageCount: 2, async readPages() { return []; } }, { signal: abort.signal }), (error) => error.name === 'AbortError');
});

test('large bundle uses its separate finite budget while single-field input retains 16 MiB', async () => {
  const single = production.encodeFieldDocument({ width: 10, pages: [{ height: 0, cells: [], comment: 'A'.repeat(4094) }] }, 'fumen');
  const count = Math.floor(production.FUMEN_MAX_SOURCE_CHARACTERS / (single.length + 1)) + 2;
  assert.ok(count < 4096);
  const input = Array(count).fill(single).join('\n');
  assert.ok(input.length > production.FUMEN_MAX_SOURCE_CHARACTERS);
  assert.throws(() => production.decodeFieldInput(input), /fumen-input-too-large/);
  const reader = production.openFieldDocument(input);
  assert.equal(reader.pageCount, count);
  assert.equal((await reader.readPage(count - 1)).comment, 'A'.repeat(4094));
  reader.close();
});

test('bundle bounds use distinct localized guidance from a single document', () => {
  assert.equal(production.fieldImportFailureMessageKey(new Error('fumen-page-limit')), 'fumenPageLimit');
  const pages = production.fieldImportFailureMessageKey(new Error('fumen-bundle-page-limit'));
  const size = production.fieldImportFailureMessageKey(new Error('fumen-bundle-input-too-large'));
  assert.equal(pages, 'fumenBundlePageLimit');
  assert.equal(size, 'fumenBundleInputTooLarge');
  for (const language of ['en', 'ko', 'ja']) {
    assert.match(production.workspaceMessage(language, pages), /1,048,576/);
    assert.match(production.workspaceMessage(language, size), /256\s*MiB/i);
  }
});

test('Fumen page-source export rejects edits, insertions, and removals during reads', async () => {
  const page = { height: 0, cells: [], comment: 'original' };
  for (const mutate of [
    (document) => document.updatePage(0, { ...page, comment: 'changed' }),
    (document) => document.insertPage(1, page),
    (document) => document.removePage(1)
  ]) {
    const document = production.CtkDrawerDocument.fromPages(10, [page, page]);
    const source = {
      width: 10,
      get pageCount() { return document.pageCount; },
      get revision() { return document.revision; },
      async readPages(start, count, signal) {
        const pages = await document.readPages(start, count, signal);
        mutate(document);
        return pages;
      }
    };
    await assert.rejects(production.encodeFumenPageSourceAsync(source), /document-changed-during-export/);
    document.close();
  }
});

test('fast solution Fumen export clears omitted comments and preserves repeated comments', () => {
  const comments = ['해법 1', undefined, undefined, '같음', '같음', '', '해법 7', undefined];
  const encoded = production.encodeColoredFumenPages(comments.map(comment => emptyPage(comment)));
  assert.deepEqual(fumenDecoder.decode(encoded).map(page => page.comment), comments.map(comment => comment ?? ''));
});

test('general Fumen document export enforces its page budget before conversion', () => {
  const page = { height: 0, cells: [] };
  assert.throws(
    () =>
      production.encodeFieldDocument(
        {
          width: 10,
          pages: Array(production.FUMEN_MAX_PAGES + 1).fill(page)
        },
        'fumen'
      ),
    (error) => error instanceof Error && error.message === 'fumen-page-limit'
  );
});

test('solution Fumen export enforces its page budget before encoding', () => {
  const page = emptyPage(undefined);
  assert.throws(
    () =>
      production.encodeColoredFumenPages(
        Array(production.FUMEN_MAX_PAGES + 1).fill(page)
      ),
    isFumenError('fumen-page-limit')
  );
});

test('Fumen ingress rejects oversized raw input before decoding', () => {
  const source = `v115@${'A'.repeat(production.FUMEN_MAX_SOURCE_CHARACTERS)}`;
  assert.throws(
    () => production.decodeFieldDocument(source),
    (error) => error instanceof Error && error.message === 'fumen-input-too-large'
  );
});

test('Fumen ingress rejects compressed documents before third-party page decoding', () => {
  const source = fumenEncoder.encode(
    Array.from({ length: production.FUMEN_MAX_PAGES + 1 }, () => ({}))
  );
  let decoderCalled = false;

  assert.throws(
    () => production.decodeFumenWithinPageLimit(source, () => {
      decoderCalled = true;
      return [];
    }),
    (error) => error instanceof Error && error.message === 'fumen-page-limit'
  );
  assert.equal(decoderCalled, false);

  assert.throws(
    () => production.decodeFieldDocument(source),
    (error) => error instanceof Error && error.message === 'fumen-page-limit'
  );
});

test('bounded Fumen inspection supports v110 and v115 without materializing pages', () => {
  assert.equal(production.inspectFumenPageCount('v110@vhAAgH'), 1);
  assert.equal(production.inspectFumenPageCount('v115@vhAAgH'), 1);
});

function emptyPage(comment) {
  return {
    height: 1,
    initialMask: 0n,
    placements: [],
    comment
  };
}

function isFumenError(code) {
  return (error) =>
    error instanceof production.SolutionExportError && error.code === code;
}

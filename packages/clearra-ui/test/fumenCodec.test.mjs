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
         decodeFumenWithinPageLimit,
         encodeFieldDocument,
         FUMEN_MAX_PAGES,
         FUMEN_MAX_SOURCE_CHARACTERS,
         inspectFumenPageCount
      } from './src/lib/workspace/fieldInterchange.ts';
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

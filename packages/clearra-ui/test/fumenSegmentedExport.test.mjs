import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import { decoder } from 'tetris-fumen';

const root = fileURLToPath(new URL('..', import.meta.url));
const bundle = await build({
  bundle: true, format: 'esm', logLevel: 'silent', platform: 'node', write: false,
  stdin: {
    contents: `export { encodeSolutionKeysForClipboard, encodeSolutionPagesForClipboard, encodeSolutionKeySourceForClipboard } from './src/lib/workspace/solutionExportAsync.ts'; export { inspectFumenPageCount } from './src/lib/workspace/ctk3Codec.ts'; export { SegmentedColoredFumenEncoder } from './src/lib/workspace/fastFumenSolutionEncoder.ts';`,
    loader: 'ts', resolveDir: root
  }
});
// The production Worker URL resolves normally in the browser. The in-memory
// test bundle has a data: URL, so use a synthetic absolute worker URL here.
const source = bundle.outputFiles[0].text.replace(
  "new URL(\"./solutionExportWorker.ts\", import.meta.url)",
  "new URL('file:///fixture-worker')"
);
const api = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
const workerBundle = await build({
  bundle: true, format: 'iife', logLevel: 'silent', platform: 'browser', write: false,
  entryPoints: [`${root}/src/lib/workspace/solutionExportWorker.ts`]
});
const iKey = 'ctk1|initial=0000000000000000|placements=I:000000000000000f';
const oKey = 'ctk1|initial=0000000000000000|placements=O:0000000000000c03';
const page = (index) => ({
  height: 2, initialMask: 0n,
  placements: index % 2 ? [{ piece: 'O', mask: 0xc03n }] : [{ piece: 'I', mask: 0xfn }],
  comment: index % 2 ? 'O 한글' : 'I comment'
});

function checkDocument(output, count, decode = false) {
  const documents = output.split('\n');
  assert.equal(documents.length, Math.ceil(count / 4096));
  const pages = documents.flatMap((document, index) => {
    assert.match(document, /^v115@/u);
    assert.equal(api.inspectFumenPageCount(document), Math.min(4096, count - index * 4096));
    return decode ? decoder.decode(document) : [];
  });
  if (decode) assert.equal(pages.length, count);
  return { documents, pages };
}

test('materialized pages split at 4096 and retain independent boundary fields and comments', async () => {
  for (const count of [4095, 4096, 4097, 8192, 8193]) {
    const output = await api.encodeSolutionPagesForClipboard(Array.from({ length: count }, (_, index) => page(index)), 'fumen');
    const { documents, pages } = checkDocument(output, count, count === 4097);
    for (const index of (count === 4097 ? [0, 4095, 4096] : [])) {
      assert.equal(pages[index].comment, index % 2 ? 'O 한글' : 'I comment');
      assert.equal(pages[index].field.at(0, 0), index % 2 ? 'O' : 'I');
    }
    if (count === 4097) {
      console.log(`fumen_4096_document_utf8_bytes=${Buffer.byteLength(documents[0], 'utf8')}`);
    }
  }
});

test('lazy source retains all keys and comments across segments and honours cancellation', async () => {
  const keys = Array.from({ length: 4097 }, (_, index) => index % 2 ? oKey : iKey);
  const source = {
    keyCount: keys.length,
    commentForKey: (key) => key === iKey ? 'I comment' : 'O 한글',
    readKeys: (start, count) => keys.slice(start, start + count)
  };
  const output = await api.encodeSolutionKeySourceForClipboard(source, 'fumen');
  const { documents } = checkDocument(output, keys.length);
  assert.equal(decoder.decode(documents[1])[0].comment, 'I comment');
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(api.encodeSolutionKeySourceForClipboard(source, 'fumen', { signal: controller.signal }), { name: 'AbortError' });
});

test('worker path and worker failure fallback match single-thread output', async () => {
  const keys = Array.from({ length: 4097 }, (_, index) => index % 2 ? oKey : iKey);
  const expected = await api.encodeSolutionKeysForClipboard(keys, 'fumen');
  const originalWorker = globalThis.Worker;
  let workerStarts = 0;
  let workerMessages = 0;
  class FakeWorker {
    constructor() {
      workerStarts += 1;
      const self = { postMessage: (data) => queueMicrotask(() => this.onmessage?.({ data })) };
      runInNewContext(workerBundle.outputFiles[0].text, { self, Uint8Array, BigInt });
      this.worker = self;
    }
    postMessage(data) { workerMessages += 1; queueMicrotask(() => this.worker.onmessage({ data })); }
    terminate() {}
  }
  try {
    globalThis.Worker = FakeWorker;
    assert.equal(await api.encodeSolutionKeysForClipboard(keys, 'fumen'), expected);
    assert.equal(workerStarts, 1);
    assert.ok(workerMessages > 2);
    let failedWorkerStarts = 0;
    class FailedWorker {
      constructor() { failedWorkerStarts += 1; }
      postMessage() { queueMicrotask(() => this.onerror?.({ message: 'fixture failure' })); }
      terminate() {}
    }
    globalThis.Worker = FailedWorker;
    assert.equal(await api.encodeSolutionKeysForClipboard(keys, 'fumen'), expected);
    assert.equal(failedWorkerStarts, 1);
    let reads = 0;
    class LimitWorker {
      postMessage() { queueMicrotask(() => this.onmessage?.({ data: { type: 'failed', code: 'clipboard-output-too-large' } })); }
      terminate() {}
    }
    globalThis.Worker = LimitWorker;
    await assert.rejects(api.encodeSolutionKeySourceForClipboard({
      keyCount: keys.length,
      readKeys(start, count) { reads += 1; return keys.slice(start, start + count); }
    }, 'fumen'), /clipboard-output-too-large/u);
    assert.equal(reads, 3, 'terminal bundle limit must not retry the source without a worker');
  } finally {
    if (originalWorker === undefined) delete globalThis.Worker;
    else globalThis.Worker = originalWorker;
  }
});

test('worker and no-worker reject invalid materialized pages without hiding the error', async () => {
  const originalWorker = globalThis.Worker;
  let workerStarts = 0;
  let workerFailures = 0;
  class FakeWorker {
    constructor() {
      workerStarts += 1;
      const self = {
        postMessage: (data) => queueMicrotask(() => {
          if (data.type === 'failed') workerFailures += 1;
          this.onmessage?.({ data });
        })
      };
      runInNewContext(workerBundle.outputFiles[0].text, { self, Uint8Array, BigInt });
      this.worker = self;
    }
    postMessage(data) { queueMicrotask(() => this.worker.onmessage({ data })); }
    terminate() {}
  }
  const invalidPages = [
    { height: 2, initialMask: 1n, placements: [{ piece: 'I', mask: 0xfn }] },
    { height: 2, initialMask: 0n, placements: [{ piece: 'I', mask: 0x7n }] },
    { height: 1, initialMask: 1n << 10n, placements: [] },
    { height: 1, initialMask: 0n, placements: [{ piece: 'I', mask: 0xf00n }] }
  ];
  try {
    for (const invalid of invalidPages) {
      const pages = Array.from({ length: 2048 }, (_, index) => index === 1024 ? invalid : page(index));
      delete globalThis.Worker;
      await assert.rejects(api.encodeSolutionPagesForClipboard(pages, 'fumen'), { message: 'invalid-page' });
      globalThis.Worker = FakeWorker;
      await assert.rejects(api.encodeSolutionPagesForClipboard(pages, 'fumen'), { message: 'invalid-page' });
    }
    assert.equal(workerStarts, invalidPages.length);
    assert.equal(workerFailures, invalidPages.length, 'the worker itself must reject every invalid page');
  } finally {
    if (originalWorker === undefined) delete globalThis.Worker;
    else globalThis.Worker = originalWorker;
  }
});

test('long comments split before the per-document 16 MiB import boundary', async () => {
  const comments = ['x'.repeat(4094) + 'a', 'x'.repeat(4094) + 'b'];
  const pages = Array.from({ length: 4096 }, (_, index) => ({ ...page(index), comment: comments[index % 2] }));
  const encoder = new api.SegmentedColoredFumenEncoder();
  for (const item of pages) encoder.append(item);
  const outputs = [encoder.finish(), await api.encodeSolutionPagesForClipboard(pages, 'fumen')];
  for (const output of outputs) {
    const documents = output.split('\n');
    assert.ok(documents.length > 1);
    assert.equal(documents.reduce((total, document) => {
      assert.ok(Buffer.byteLength(document, 'utf8') <= 16 * 1024 * 1024);
      return total + api.inspectFumenPageCount(document);
    }, 0), pages.length);
  }
});

test('bundle limits fail explicitly before reads or clipboard output', async () => {
  let reads = 0;
  const oversizedSource = { keyCount: 1_048_577, readKeys() { reads += 1; return []; } };
  await assert.rejects(api.encodeSolutionKeySourceForClipboard(oversizedSource, 'fumen'), /clipboard-output-too-large/u);
  await assert.rejects(api.encodeSolutionKeysForClipboard(Array(1_048_577).fill(iKey), 'fumen'), /clipboard-output-too-large/u);
  await assert.rejects(api.encodeSolutionPagesForClipboard(Array(1_048_577).fill(page(0)), 'fumen'), /clipboard-output-too-large/u);
  assert.equal(reads, 0);

  // Inject counter state to exercise large limits without allocating hundreds
  // of MiB or generating thousands of individual documents in a unit test.
  const byCharacters = new api.SegmentedColoredFumenEncoder();
  byCharacters.completedCharacters = 256 * 1024 * 1024 - 1;
  assert.throws(() => byCharacters.append(page(0)), /clipboard-output-too-large/u);
  const byDocuments = new api.SegmentedColoredFumenEncoder();
  byDocuments.documents = Array(4095).fill('v115@fixture');
  byDocuments.count = 4096;
  assert.throws(() => byDocuments.append(page(0)), /clipboard-output-too-large/u);
});

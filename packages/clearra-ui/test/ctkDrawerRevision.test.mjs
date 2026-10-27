import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import test from 'node:test';
import { transform } from 'esbuild';

// Exercise the actual component functions with controlled async boundaries.
// This is not a DOM or operating-system clipboard integration test.
const component = await readFile(new URL('../src/lib/workspace/CtkDrawerWorkspace.svelte', import.meta.url), 'utf8');
function section(begin, end) {
  const start = component.indexOf(begin);
  const stop = component.indexOf(end, start + begin.length);
  assert.ok(start >= 0 && stop > start, `missing component section ${begin}`);
  return component.slice(start, stop);
}

const source = [
  section('  async function copyDocument()', '  async function downloadDocument()'),
  section('  function invalidateRenderResult()', '  function revokeRenderArtifact()'),
  section('  function assertDocumentSnapshot(', '  function setCopyState('),
  section('  function throwIfAborted(', '  function handleImportInput('),
  section('  function updateCurrent(', '  function updateComment('),
  section('  function addPage()', '  function duplicatePage()'),
  section('  function removePage()', '  async function selectPage(')
].join('\n');
const compiled = (await transform(source, { loader: 'ts', target: 'es2022' })).code;

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

function fixture(format = 'fumen') {
  const paint = deferred();
  const encode = deferred();
  const started = deferred();
  const context = vm.createContext({ AbortController, Error, Map, Promise });
  vm.runInContext(`
    let copyState = 'idle', copyTimer = 0, copyController = null;
    let downloadController = null, renderDocumentRevision = 0, copyFormat = '${format}';
    let workerFactory = null, renderActive = false, closed = false;
    let pageIndex = 0, pageCount = 2, currentHeight = 4, currentPage = {}, previewPages = new Map();
    let writes = [], encodingCalls = 0;
    const documentModel = {
      pageCount: 2,
      updatePage() {}, insertPage() { this.pageCount++; }, removePage() { this.pageCount--; }
    };
    const window = { clearTimeout() {} };
    const nextPaint = () => paintPromise;
    const encodeDocument = async () => { encodingCalls++; reportStarted(); return encodePromise; };
    const writeClipboardText = async (text) => { writes.push(text); };
    const setCopyState = (next) => { copyState = next; };
    const clonePage = (page) => page;
    const blankPage = () => ({});
    const selectPage = () => {};
    const replaceDocument = () => {};
    const cancelRender = () => {};
    const clearWasmTerminalResult = () => {};
    const clearRenderResult = () => {};
    ${compiled}
    globalThis.fixture = {
      copyDocument, invalidateRenderResult, updateCurrent, addPage, removePage,
      abort: () => copyController?.abort(abortError('user cancelled')),
      get state() { return copyState; }, get writes() { return writes; },
      get encodingCalls() { return encodingCalls; },
      bumpRevision: () => { renderDocumentRevision++; }
    };
  `, Object.assign(context, { paintPromise: paint.promise, encodePromise: encode.promise, reportStarted: started.resolve }));
  return { api: context.fixture, paint, encode, started };
}

for (const format of ['fumen', 'ctk']) {
  test(`${format} normal copy writes the complete encoded value`, async () => {
    const { api, paint, encode, started } = fixture(format);
    const task = api.copyDocument();
    paint.resolve();
    await started.promise;
    encode.resolve('whole-document');
    await task;
    assert.deepEqual(Array.from(api.writes), ['whole-document']);
    assert.equal(api.state, 'copied');
  });

  for (const change of ['updateCurrent', 'addPage', 'removePage']) {
    test(`${format} ${change} during encoding cancels before clipboard write`, async () => {
      const { api, paint, encode, started } = fixture(format);
      const task = api.copyDocument();
      paint.resolve();
      await started.promise;
      assert.equal(api.encodingCalls, 1);
      api[change]({});
      encode.resolve('mixed-document');
      await task;
      assert.equal(api.writes.length, 0);
      assert.equal(api.state, 'idle');
    });
  }

  test(`${format} edit before the first paint never starts encoding`, async () => {
    const { api, paint } = fixture(format);
    const task = api.copyDocument();
    api.updateCurrent({});
    paint.resolve();
    await task;
    assert.equal(api.encodingCalls, 0);
    assert.equal(api.writes.length, 0);
  });

  test(`${format} revision change immediately before write rejects the encoded result`, async () => {
    const { api, paint, encode, started } = fixture(format);
    const task = api.copyDocument();
    paint.resolve();
    await started.promise;
    api.bumpRevision(); // Simulates a revision fence even if a caller missed abort.
    encode.resolve('stale-document');
    await task;
    assert.equal(api.writes.length, 0);
    assert.equal(api.state, 'idle');
  });

  test(`${format} explicit cancellation does not write`, async () => {
    const { api, paint, encode, started } = fixture(format);
    const task = api.copyDocument();
    paint.resolve();
    await started.promise;
    api.abort();
    encode.resolve('cancelled-document');
    await task;
    assert.equal(api.writes.length, 0);
    assert.equal(api.state, 'idle');
  });
}

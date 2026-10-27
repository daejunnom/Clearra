import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';

const root = fileURLToPath(new URL('..', import.meta.url));
const bundle = await build({
  bundle: true, format: 'esm', logLevel: 'silent', platform: 'node', write: false,
  stdin: {
    contents: `export { openFieldDocument, openFieldDocumentAsync, encodeFieldDocument } from './src/lib/workspace/fieldInterchange.ts';`,
    loader: 'ts', resolveDir: root
  }
});
const api = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const page = (comment) => ({ height: 0, cells: [], comment });
const code = api.encodeFieldDocument({ width: 10, pages: [page('첫 번째 😀')] }, 'fumen');
const input = `${code}\n${code}`;

class StubWorker {
  listeners = new Map();
  sent = [];
  terminated = false;
  addEventListener(type, listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(listener);
  }
  removeEventListener(type, listener) { this.listeners.get(type)?.delete(listener); }
  postMessage(message) { this.sent.push(message); }
  terminate() { this.terminated = true; }
  emit(type, event) { for (const listener of this.listeners.get(type) ?? []) listener(event); }
  response(message, value) { this.emit('message', { data: { id: message.id, ok: true, value } }); }
  fail(message, error) { this.emit('message', { data: { id: message.id, ok: false, error } }); }
}

async function openStub(signal) {
  const worker = new StubWorker();
  const pending = api.openFieldDocumentAsync(input, { fumenWorkerFactory: () => worker, signal });
  const request = worker.sent[0];
  assert.equal(request.type, 'open');
  assert.equal(request.input, input);
  worker.response(request, { width: 10, pageCount: 4097 });
  return { reader: await pending, worker };
}

test('small single Fumen retains its legacy open and page behaviour without a worker', async () => {
  const reader = await api.openFieldDocumentAsync(code, { fumenWorkerFactory: () => { throw new Error('unexpected worker'); } });
  assert.equal(reader.pageCount, 1);
  assert.equal((await reader.readPage(0)).comment, '첫 번째 😀');
  reader.close();
});

test('bundle uses a persistent worker, preserves page order and routes concurrent replies by id', async () => {
  const { reader, worker } = await openStub();
  assert.equal(reader.width, 10);
  assert.equal(reader.pageCount, 4097);
  const first = reader.readPage(4095);
  const second = reader.readPage(4096);
  const a = worker.sent[1];
  const b = worker.sent[2];
  assert.equal(a.type, 'readPage');
  assert.equal(b.type, 'readPage');
  worker.response(b, page('boundary-2'));
  worker.response(a, page('boundary-1'));
  assert.equal((await first).comment, 'boundary-1');
  assert.equal((await second).comment, 'boundary-2');
  const range = reader.readPages(4095, 2);
  const c = worker.sent[3];
  assert.equal(c.type, 'readPages');
  worker.response(c, [page('boundary-1'), page('boundary-2')]);
  assert.deepEqual((await range).map((value) => value.comment), ['boundary-1', 'boundary-2']);
  reader.close();
  assert.equal(worker.terminated, true);
});

test('open failures and malformed downstream chunk failures propagate without silently skipping', async () => {
  const worker = new StubWorker();
  const pending = api.openFieldDocumentAsync(input, { fumenWorkerFactory: () => worker });
  worker.fail(worker.sent[0], 'invalid-fumen-input');
  await assert.rejects(pending, /invalid-fumen-input/u);
  assert.equal(worker.terminated, true);
  const other = await openStub();
  const read = other.reader.readPage(4096);
  other.worker.fail(other.worker.sent[1], 'invalid-fumen-input');
  await assert.rejects(read, /invalid-fumen-input/u);
  other.reader.close();
});

test('abort and close terminate owned worker and reject outstanding work', async () => {
  const before = new AbortController();
  before.abort();
  let created = 0;
  await assert.rejects(api.openFieldDocumentAsync(input, { signal: before.signal, fumenWorkerFactory: () => { created++; return new StubWorker(); } }), { name: 'AbortError' });
  assert.equal(created, 0);
  const control = new AbortController();
  const { reader, worker } = await openStub(control.signal);
  const pending = reader.readPage(1);
  control.abort();
  await assert.rejects(pending, { name: 'AbortError' });
  assert.equal(worker.terminated, true);
  const next = await openStub();
  const read = next.reader.readPage(2);
  next.reader.close();
  await assert.rejects(read);
  assert.equal(next.worker.terminated, true);
});

test('worker errors reject open instead of falling back to synchronous bundle scanning', async () => {
  const worker = new StubWorker();
  const pending = api.openFieldDocumentAsync(input, { fumenWorkerFactory: () => worker });
  worker.emit('error', { message: 'fixture-worker-failed' });
  await assert.rejects(pending);
  assert.equal(worker.terminated, true);
});

test('bundle without worker fails closed even when input is short; singleton trailing newline remains compatible', async () => {
  await assert.rejects(api.openFieldDocumentAsync(input, { fumenWorkerFactory: null }), /fumen-worker-unavailable/u);
  const reader = await api.openFieldDocumentAsync(`${code}\n`, { fumenWorkerFactory: null });
  assert.equal(reader.pageCount, 1);
  assert.equal((await reader.readPage(0)).comment, '첫 번째 😀');
  reader.close();
});

test('production worker scans and decodes the 4096/4097 boundary in independent documents', async () => {
  const workerCode = await build({
    bundle: true, format: 'iife', logLevel: 'silent', platform: 'browser', write: false,
    entryPoints: [`${root}/src/lib/workspace/fumenDocumentWorker.ts`]
  });
  class RuntimeWorker extends StubWorker {
    constructor() {
      super();
      const self = {
        addEventListener: (type, callback) => { if (type === 'message') this.receive = callback; },
        postMessage: (data) => queueMicrotask(() => this.emit('message', { data: structuredClone(data) }))
      };
      runInNewContext(workerCode.outputFiles[0].text, { self, TextEncoder, TextDecoder, URL, console });
    }
    postMessage(message) {
      if (!this.terminated) queueMicrotask(() => this.receive({ data: structuredClone(message) }));
    }
  }
  const first = api.encodeFieldDocument({ width: 10, pages: Array.from({ length: 4096 }, () => page('앞')) }, 'fumen');
  const second = api.encodeFieldDocument({ width: 10, pages: [page('뒤 😀')] }, 'fumen');
  const worker = new RuntimeWorker();
  const reader = await api.openFieldDocumentAsync(`${first}\n${second}`, { fumenWorkerFactory: () => worker });
  assert.equal(reader.pageCount, 4097);
  assert.equal((await reader.readPage(4095)).comment, '앞');
  assert.equal((await reader.readPage(4096)).comment, '뒤 😀');
  assert.deepEqual((await reader.readPages(4095, 2)).map((value) => value.comment), ['앞', '뒤 😀']);
  reader.close();
  const bad = new RuntimeWorker();
  await assert.rejects(api.openFieldDocumentAsync(`${first}\nv115@INVALID`, { fumenWorkerFactory: () => bad }));
  assert.equal(bad.terminated, true);
  const badFirst = new RuntimeWorker();
  await assert.rejects(api.openFieldDocumentAsync(`bad-input\n${second}`, { fumenWorkerFactory: () => badFirst }), /invalid-fumen-input/u);
  assert.equal(badFirst.terminated, true);
});

test('multiline single URL and ordinary text preserve synchronous import semantics', async () => {
  const workerCode = await build({
    bundle: true, format: 'iife', logLevel: 'silent', platform: 'browser', write: false,
    entryPoints: [`${root}/src/lib/workspace/fumenDocumentWorker.ts`]
  });
  class RuntimeWorker extends StubWorker {
    constructor() {
      super();
      const self = {
        addEventListener: (type, callback) => { if (type === 'message') this.receive = callback; },
        postMessage: (data) => queueMicrotask(() => this.emit('message', { data: structuredClone(data) }))
      };
      runInNewContext(workerCode.outputFiles[0].text, { self, TextEncoder, TextDecoder, URL, console });
    }
    postMessage(message) {
      if (!this.terminated) queueMicrotask(() => this.receive({ data: structuredClone(message) }));
    }
  }
  const samples = [
    `https://fumen.zui.jp/?${code}\n`,
    `\nhttps://fumen.zui.jp/?${code}\n\n`,
    `reference text\n${code}`,
    `https://example.test/path\n${code}`,
    'ordinary text\nwithout a fumen',
    `${code}\n${code}`,
    `${code}\ninvalid trailing text`
  ];
  async function outcome(open) {
    let reader;
    try {
      reader = await open();
      const value = await reader.readPage(0);
      return { ok: true, count: reader.pageCount, width: reader.width, first: structuredClone(value) };
    } catch (error) {
      return { ok: false, message: error.message };
    } finally { reader?.close(); }
  }
  for (const input of samples) {
    const expected = await outcome(() => api.openFieldDocument(input));
    const actual = await outcome(() => api.openFieldDocumentAsync(input, { fumenWorkerFactory: () => new RuntimeWorker() }));
    assert.deepEqual(actual, expected, `input ${JSON.stringify(input)}`);
  }
});

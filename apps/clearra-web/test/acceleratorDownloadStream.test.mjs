import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';

// No generated build/test artifact or WASM download: execute the actual
// transfer owner in memory through Node's existing TypeScript transform.
const url = new URL('../src/workers/acceleratorDownloadStream.ts', import.meta.url);
const source = stripTypeScriptTypes(await readFile(url, 'utf8'), { mode: 'transform', sourceUrl: url.href });
const { readAcceleratorDownloadResponse } = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);

function response({ chunks = [], status = 200, length, pending = false } = {}) {
  let cancels = 0, controller;
  const body = new ReadableStream({
    start(owner) {
      controller = owner;
      for (const chunk of chunks) owner.enqueue(Uint8Array.from(chunk));
      if (!pending) owner.close();
    },
    cancel() { cancels += 1; }
  });
  const value = new Response(body, { status, headers: length == null ? {} : { 'content-length': String(length) } });
  return { value, body, fail(error) { controller.error(error); }, cancels: () => cancels };
}

test('explicit accelerator transfer preserves exact bytes and progress without cancelling successful body', async () => {
  const fixture = response({ chunks: [[1, 2], [3, 4]], length: 4 });
  const updates = [];
  const bytes = await readAcceleratorDownloadResponse(fixture.value, 4, new AbortController().signal, (...event) => updates.push(event));
  assert.deepEqual(bytes, Uint8Array.of(1, 2, 3, 4));
  assert.deepEqual(updates, [[2, 4], [4, 4]]);
  assert.equal(fixture.cancels(), 0);
  assert.equal(fixture.body.locked, false);
});

test('invalid status, declared length and requested budget cancel the response before admission', async () => {
  for (const [status, length, requested] of [[500, 4, 4], [200, 5, 4], [200, null, 64 * 1024 * 1024 + 1]]) {
    const fixture = response({ status, length, pending: true });
    await assert.rejects(readAcceleratorDownloadResponse(fixture.value, requested, new AbortController().signal, () => {}), /accelerator_download_(?:response_invalid|size_mismatch)/u);
    assert.equal(fixture.cancels(), 1);
    assert.equal(fixture.body.locked, false);
  }
});

test('overflow and progress callback failure cancel a pending body and preserve the original error', async () => {
  const overflow = response({ chunks: [[1, 2, 3, 4, 5]], pending: true });
  await assert.rejects(readAcceleratorDownloadResponse(overflow.value, 4, new AbortController().signal, () => {}), /accelerator_download_size_mismatch/u);
  assert.equal(overflow.cancels(), 1);
  assert.equal(overflow.body.locked, false);
  const failedProgress = response({ chunks: [[1]], pending: true });
  await assert.rejects(readAcceleratorDownloadResponse(failedProgress.value, 4, new AbortController().signal, () => { throw new Error('observer-failed'); }), /observer-failed/u);
  assert.equal(failedProgress.cancels(), 1);
  assert.equal(failedProgress.body.locked, false);
});

test('cancellation during a blocked read settles the owner without waiting for another chunk', async () => {
  const fixture = response({ pending: true });
  const control = new AbortController();
  const promise = readAcceleratorDownloadResponse(fixture.value, 4, control.signal, () => {});
  control.abort();
  await assert.rejects(promise, /accelerator_download_cancelled/u);
  assert.equal(fixture.cancels(), 1);
  assert.equal(fixture.body.locked, false);
});

test('pre-cancelled and truncated transfers cannot return a candidate payload', async () => {
  const control = new AbortController(); control.abort();
  const cancelled = response({ pending: true });
  await assert.rejects(readAcceleratorDownloadResponse(cancelled.value, 4, control.signal, () => {}), /accelerator_download_cancelled/u);
  assert.equal(cancelled.cancels(), 1);
  const short = response({ chunks: [[1, 2]] });
  await assert.rejects(readAcceleratorDownloadResponse(short.value, 4, new AbortController().signal, () => {}), /accelerator_download_size_mismatch/u);
  assert.equal(short.body.locked, false);
});

test('stream errors preserve their cause and release the reader lock', async () => {
  const fixture = response({ pending: true });
  const promise = readAcceleratorDownloadResponse(fixture.value, 4, new AbortController().signal, () => {});
  fixture.fail(new Error('transport-failed'));
  await assert.rejects(promise, /transport-failed/u);
  assert.equal(fixture.body.locked, false);
});

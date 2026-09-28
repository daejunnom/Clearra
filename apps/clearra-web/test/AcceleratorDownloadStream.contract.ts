import assert from 'node:assert/strict';
import { readAcceleratorDownloadResponse } from '../src/workers/acceleratorDownloadStream.ts';

// The standard Web contract runner discovers .contract.ts entries. Exercise
// the real transfer owner without fetching assets or admitting a generation.
function response(chunks: number[][], options: { length?: number; status?: number; pending?: boolean } = {}) {
  let cancels = 0;
  const body = new ReadableStream<Uint8Array<ArrayBuffer>>({
    start(owner) {
      for (const chunk of chunks) owner.enqueue(Uint8Array.from(chunk));
      if (!options.pending) owner.close();
    },
    cancel() { cancels += 1; }
  });
  const headers = options.length == null ? undefined : { 'content-length': String(options.length) };
  return {
    value: new Response(body, { status: options.status ?? 200, headers }),
    body,
    cancels: () => cancels
  };
}

const complete = response([[1, 2], [3, 4]], { length: 4 });
const updates: number[][] = [];
assert.deepEqual(
  await readAcceleratorDownloadResponse(complete.value, 4, new AbortController().signal, (...event) => updates.push(event)),
  Uint8Array.of(1, 2, 3, 4)
);
assert.deepEqual(updates, [[2, 4], [4, 4]]);
assert.equal(complete.cancels(), 0);
assert.equal(complete.body.locked, false);

for (const [status, length, requested] of [[500, 4, 4], [200, 5, 4], [200, undefined, 64 * 1024 * 1024 + 1]]) {
  const fixture = response([], { status, length, pending: true });
  await assert.rejects(
    readAcceleratorDownloadResponse(fixture.value, requested!, new AbortController().signal, () => {}),
    /accelerator_download_(?:response_invalid|size_mismatch)/u
  );
  assert.equal(fixture.cancels(), 1);
  assert.equal(fixture.body.locked, false);
}

const blocked = response([], { pending: true });
const control = new AbortController();
const pending = readAcceleratorDownloadResponse(blocked.value, 4, control.signal, () => {});
control.abort();
await assert.rejects(pending, /accelerator_download_cancelled/u);
assert.equal(blocked.cancels(), 1);
assert.equal(blocked.body.locked, false);

const overflow = response([[1, 2, 3, 4, 5]], { pending: true });
await assert.rejects(
  readAcceleratorDownloadResponse(overflow.value, 4, new AbortController().signal, () => {}),
  /accelerator_download_size_mismatch/u
);
assert.equal(overflow.cancels(), 1);
assert.equal(overflow.body.locked, false);

const failedProgress = response([[1]], { pending: true });
await assert.rejects(
  readAcceleratorDownloadResponse(failedProgress.value, 4, new AbortController().signal, () => { throw new Error('observer-failed'); }),
  /observer-failed/u
);
assert.equal(failedProgress.cancels(), 1);
assert.equal(failedProgress.body.locked, false);

const truncated = response([[1, 2]]);
await assert.rejects(
  readAcceleratorDownloadResponse(truncated.value, 4, new AbortController().signal, () => {}),
  /accelerator_download_size_mismatch/u
);
assert.equal(truncated.body.locked, false);

import type { Ctk3Document, Ctk3Page } from './ctk3Codec';
import type { FieldDocumentReader } from './fieldInterchange';

export type FumenReaderWorkerFactory = () => Worker;
type Response = { id: number; ok: true; value: unknown } | { id: number; ok: false; error: string };

/** Owns one worker and rejects every outstanding request on close or abort. */
export async function openFumenWorkerReader(
  input: string,
  factory: FumenReaderWorkerFactory,
  signal?: AbortSignal
): Promise<FieldDocumentReader> {
  if (signal?.aborted) throw abortError(signal);
  const worker = factory();
  const pending = new Map<number, { resolve(value: unknown): void; reject(error: Error): void }>();
  let nextId = 0;
  let closed = false;
  function shutdown(error: Error) {
    if (closed) return;
    closed = true;
    signal?.removeEventListener('abort', abort);
    worker.removeEventListener('message', receive);
    worker.removeEventListener('error', fail);
    worker.removeEventListener('messageerror', fail);
    worker.terminate();
    for (const waiter of pending.values()) waiter.reject(error);
    pending.clear();
  }
  function abort() { shutdown(abortError(signal)); }
  function fail() { shutdown(new Error('fumen-worker-failed')); }
  function receive(event: MessageEvent<Response>) {
    const response = event.data;
    if (!response || !Number.isSafeInteger(response.id)) return fail();
    const waiter = pending.get(response.id);
    if (!waiter) return;
    pending.delete(response.id);
    if (response.ok === true) waiter.resolve(response.value);
    else waiter.reject(new Error(typeof response.error === 'string' ? response.error : 'fumen-worker-failed'));
  }
  function request(type: string, fields: Record<string, unknown> = {}): Promise<unknown> {
    if (closed) return Promise.reject(new Error('Document reader is closed.'));
    if (signal?.aborted) return Promise.reject(abortError(signal));
    const id = ++nextId;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      try { worker.postMessage({ id, type, ...fields }); }
      catch (error) {
        pending.delete(id);
        reject(error instanceof Error ? error : new Error('fumen-worker-failed'));
      }
    });
  }
  worker.addEventListener('message', receive);
  worker.addEventListener('error', fail);
  worker.addEventListener('messageerror', fail);
  signal?.addEventListener('abort', abort, { once: true });
  try {
    const opened = await request('open', { input }) as { width: number; pageCount: number };
    if (!Number.isSafeInteger(opened?.width) || !Number.isSafeInteger(opened?.pageCount) || opened.pageCount < 1) {
      throw new Error('fumen-worker-failed');
    }
    return {
      width: opened.width,
      pageCount: opened.pageCount,
      originalCtk: null,
      async readPage(index) { return await request('readPage', { index }) as Ctk3Page; },
      async readPages(start, count) { return await request('readPages', { start, count }) as Ctk3Page[]; },
      async decodeAll() { return await request('decodeAll') as Ctk3Document; },
      close() { shutdown(new Error('Document reader is closed.')); }
    };
  } catch (error) {
    shutdown(error instanceof Error ? error : new Error('fumen-worker-failed'));
    throw error;
  }
}

function abortError(signal?: AbortSignal): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error('Document reader was aborted.');
  error.name = 'AbortError';
  return error;
}

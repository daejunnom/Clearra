import { defaultWorkerCount } from './solverWorkspaceModel';
import {
  encodeCtk3PageSourceAsync,
  type Ctk3DecodeWorkerLike
} from './ctk3Codec';
import {
  combineCtkSolutionSegments,
  CTK_SOLUTION_SEGMENT_SIZE,
  encodeColoredFumenSolutionKeys,
  encodeCtkSolutionKeySegment,
  encodeSolutionPages,
  solutionPageToCtk3Page,
  type SolutionCopyFormat,
  type SolutionExportPage
} from './solutionExport';

const WORKER_THRESHOLD = 2048;
const FUMEN_CHUNK_SIZE = 1024;

type ExportWorkerResponse =
  | { type: 'ctk-segment'; taskId: number; encoded: string }
  | { type: 'fumen-ready'; jobId: number }
  | { type: 'fumen-chunk'; jobId: number }
  | { type: 'fumen-finished'; jobId: number; encoded: string }
  | { type: 'failed'; taskId?: number; jobId?: number; code: string };

export type SolutionExportAsyncOptions = {
  signal?: AbortSignal;
};

export async function encodeSolutionPagesForClipboard(
  pages: readonly SolutionExportPage[],
  format: SolutionCopyFormat,
  options: SolutionExportAsyncOptions = {}
): Promise<string> {
  throwIfAborted(options.signal);
  if (!pages.length) throw new Error('invalid-page');
  if (pages.length < WORKER_THRESHOLD || typeof Worker !== 'function') {
    const encoded = encodeSolutionPages(Array.from(pages), format);
    throwIfAborted(options.signal);
    return encoded;
  }
  if (format === 'ctk') {
    return encodeCtk3PageSourceAsync(
      {
        width: 10,
        pageCount: pages.length,
        readPages(start, count) {
          throwIfAborted(options.signal);
          return pages
            .slice(start, start + count)
            .map(solutionPageToCtk3Page);
        }
      },
      {
        workerFactory: createCtkDocumentWorker,
        signal: options.signal
      }
    );
  }
  try {
    return await encodeFumenPagesWithSingleWorker(pages, options.signal);
  } catch (error) {
    rethrowIfAborted(error, options.signal);
    const encoded = encodeSolutionPages(Array.from(pages), format);
    throwIfAborted(options.signal);
    return encoded;
  }
}

export async function encodeSolutionKeysForClipboard(
  keys: readonly string[],
  format: SolutionCopyFormat,
  options: SolutionExportAsyncOptions = {}
): Promise<string> {
  throwIfAborted(options.signal);
  if (!keys.length) throw new Error('invalid-page');
  if (format === 'ctk') {
    if (keys.length <= CTK_SOLUTION_SEGMENT_SIZE) {
      const encoded = encodeCtkSolutionKeySegment(keys);
      throwIfAborted(options.signal);
      return encoded;
    }
    if (typeof Worker !== 'function') {
      return encodeCtkWithoutWorkers(keys, options.signal);
    }
    try {
      return await encodeCtkWithWorkerPool(keys, options.signal);
    } catch (error) {
      rethrowIfAborted(error, options.signal);
      return encodeCtkWithoutWorkers(keys, options.signal);
    }
  }
  if (typeof Worker !== 'function' || keys.length < WORKER_THRESHOLD) {
    const encoded = encodeColoredFumenSolutionKeys(keys);
    throwIfAborted(options.signal);
    return encoded;
  }
  try {
    return await encodeFumenWithSingleWorker(keys, options.signal);
  } catch (error) {
    rethrowIfAborted(error, options.signal);
    const encoded = encodeColoredFumenSolutionKeys(keys);
    throwIfAborted(options.signal);
    return encoded;
  }
}

function encodeCtkWithoutWorkers(
  keys: readonly string[],
  signal?: AbortSignal
): string {
  const segments: string[] = [];
  for (let offset = 0; offset < keys.length; offset += CTK_SOLUTION_SEGMENT_SIZE) {
    throwIfAborted(signal);
    segments.push(
      encodeCtkSolutionKeySegment(
        keys.slice(offset, offset + CTK_SOLUTION_SEGMENT_SIZE)
      )
    );
  }
  throwIfAborted(signal);
  const encoded = combineCtkSolutionSegments(segments);
  throwIfAborted(signal);
  return encoded;
}

async function encodeCtkWithWorkerPool(
  keys: readonly string[],
  signal?: AbortSignal
): Promise<string> {
  const taskCount = Math.ceil(keys.length / CTK_SOLUTION_SEGMENT_SIZE);
  const workerCount = Math.min(
    taskCount,
    defaultWorkerCount(globalThis.navigator?.hardwareConcurrency)
  );
  const workers: Worker[] = [];
  const segments = new Array<string>(taskCount);
  let nextTask = 0;
  let completed = 0;

  return new Promise<string>((resolve, reject) => {
    let settled = false;
    const onAbort = () => finish(() => reject(abortError(signal)));
    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener('abort', onAbort);
      for (const worker of workers) worker.terminate();
      callback();
    };
    const fail = (error: unknown) => {
      finish(() => reject(error));
    };
    const dispatch = (worker: Worker) => {
      if (settled) return;
      if (signal?.aborted) {
        onAbort();
        return;
      }
      if (nextTask >= taskCount) return;
      const taskId = nextTask;
      nextTask += 1;
      const start = taskId * CTK_SOLUTION_SEGMENT_SIZE;
      worker.postMessage({
        type: 'ctk-segment',
        taskId,
        keys: keys.slice(start, start + CTK_SOLUTION_SEGMENT_SIZE)
      });
    };

    try {
      signal?.addEventListener('abort', onAbort, { once: true });
      if (signal?.aborted) {
        onAbort();
        return;
      }
      for (let index = 0; index < workerCount; index += 1) {
        const worker = createExportWorker();
        workers.push(worker);
        worker.onerror = (event) => fail(exportWorkerError(event));
        worker.onmessage = (event: MessageEvent<ExportWorkerResponse>) => {
          const response = event.data;
          if (response.type === 'failed') {
            fail(new Error(response.code));
            return;
          }
          if (response.type !== 'ctk-segment') return;
          segments[response.taskId] = response.encoded;
          completed += 1;
          if (completed === taskCount) {
            finish(() => resolve(combineCtkSolutionSegments(segments)));
            return;
          }
          dispatch(worker);
        };
        dispatch(worker);
      }
    } catch (error) {
      fail(error);
    }
  });
}

async function encodeFumenWithSingleWorker(
  keys: readonly string[],
  signal?: AbortSignal
): Promise<string> {
  throwIfAborted(signal);
  const worker = createExportWorker();
  const jobId = Date.now();
  let offset = 0;

  return new Promise<string>((resolve, reject) => {
    let settled = false;
    const onAbort = () => finish(() => reject(abortError(signal)));
    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener('abort', onAbort);
      worker.terminate();
      callback();
    };
    const fail = (error: unknown) => finish(() => reject(error));
    const sendNext = () => {
      if (settled) return;
      if (signal?.aborted) {
        onAbort();
        return;
      }
      if (offset >= keys.length) {
        worker.postMessage({ type: 'fumen-finish', jobId });
        return;
      }
      const end = Math.min(keys.length, offset + FUMEN_CHUNK_SIZE);
      worker.postMessage({
        type: 'fumen-chunk',
        jobId,
        keys: keys.slice(offset, end)
      });
      offset = end;
    };

    signal?.addEventListener('abort', onAbort, { once: true });
    if (signal?.aborted) {
      onAbort();
      return;
    }
    worker.onerror = (event) => fail(exportWorkerError(event));
    worker.onmessage = (event: MessageEvent<ExportWorkerResponse>) => {
      const response = event.data;
      if (response.type === 'failed') {
        fail(new Error(response.code));
      } else if (
        response.type === 'fumen-ready' ||
        response.type === 'fumen-chunk'
      ) {
        sendNext();
      } else if (response.type === 'fumen-finished') {
        finish(() => resolve(response.encoded));
      }
    };
    worker.postMessage({ type: 'fumen-start', jobId });
  });
}

async function encodeFumenPagesWithSingleWorker(
  pages: readonly SolutionExportPage[],
  signal?: AbortSignal
): Promise<string> {
  throwIfAborted(signal);
  const worker = createExportWorker();
  const jobId = Date.now();
  let offset = 0;

  return new Promise<string>((resolve, reject) => {
    let settled = false;
    const onAbort = () => finish(() => reject(abortError(signal)));
    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener('abort', onAbort);
      worker.terminate();
      callback();
    };
    const fail = (error: unknown) => finish(() => reject(error));
    const sendNext = () => {
      if (settled) return;
      if (signal?.aborted) {
        onAbort();
        return;
      }
      if (offset >= pages.length) {
        worker.postMessage({ type: 'fumen-finish', jobId });
        return;
      }
      const end = Math.min(pages.length, offset + FUMEN_CHUNK_SIZE);
      worker.postMessage({
        type: 'fumen-pages-chunk',
        jobId,
        pages: pages.slice(offset, end)
      });
      offset = end;
    };

    signal?.addEventListener('abort', onAbort, { once: true });
    if (signal?.aborted) {
      onAbort();
      return;
    }
    worker.onerror = (event) => fail(exportWorkerError(event));
    worker.onmessage = (event: MessageEvent<ExportWorkerResponse>) => {
      const response = event.data;
      if (response.type === 'failed') {
        fail(new Error(response.code));
      } else if (
        response.type === 'fumen-ready' ||
        response.type === 'fumen-chunk'
      ) {
        sendNext();
      } else if (response.type === 'fumen-finished') {
        finish(() => resolve(response.encoded));
      }
    };
    worker.postMessage({ type: 'fumen-start', jobId });
  });
}

function createExportWorker(): Worker {
  return new Worker(new URL('./solutionExportWorker.ts', import.meta.url), {
    type: 'module',
    name: 'clearra-solution-export'
  });
}

function createCtkDocumentWorker(): Ctk3DecodeWorkerLike {
  return new Worker(new URL('./ctkDocumentDecodeWorker.ts', import.meta.url), {
    type: 'module',
    name: 'clearra-ctk3-encode'
  }) as unknown as Ctk3DecodeWorkerLike;
}

function exportWorkerError(event: ErrorEvent): Error {
  if (event.error instanceof Error) return event.error;
  const location =
    event.filename && event.lineno
      ? ` (${event.filename}:${event.lineno}:${event.colno})`
      : '';
  return new Error(`${event.message || 'solution-export-worker-failed'}${location}`);
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw abortError(signal);
}

function rethrowIfAborted(error: unknown, signal: AbortSignal | undefined): void {
  if (signal?.aborted || isAbortError(error)) {
    throw signal?.aborted ? abortError(signal) : error;
  }
}

function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === 'AbortError';
}

function abortError(signal: AbortSignal | undefined): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error('Solution export was aborted.');
  error.name = 'AbortError';
  return error;
}

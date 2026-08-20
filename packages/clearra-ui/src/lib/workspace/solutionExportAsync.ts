import {
  automaticWorkerAuthority,
  sharedBrowserHostCapabilitySnapshot,
  type HostCapabilitySnapshot
} from '../wasm/hostCapabilitySnapshot';
import {
  CTK3_MAX_BUNDLE_PAGES,
  encodeCtk3PageSourceAsync,
  type Ctk3DecodeWorkerLike
} from './ctk3Codec';
import {
  combineCtkSolutionSegments,
  CTK_SOLUTION_SEGMENT_SIZE,
  encodeColoredFumenSolutionKeys,
  encodeCtkSolutionKeySegment,
  encodeSolutionPages,
  parseSolutionKey,
  SolutionExportError,
  solutionPageToCtk3Page,
  type SolutionCopyFormat,
  type SolutionExportPage
} from './solutionExport';
import { FastColoredFumenEncoder } from './fastFumenSolutionEncoder';

const WORKER_THRESHOLD = 2048;
const FUMEN_CHUNK_SIZE = 1024;
const LAZY_SOURCE_CHUNK_SIZE = 1000;
const CLIPBOARD_ESTIMATE_SAMPLE_SIZE = 1000;
const MAX_SAFE_CLIPBOARD_PAGES = 1_000_000;
const MAX_SAFE_CLIPBOARD_CHARACTERS = 256 * 1024 * 1024;

type ExportWorkerResponse =
  | { type: 'ctk-segment'; taskId: number; encoded: string }
  | { type: 'fumen-ready'; jobId: number }
  | { type: 'fumen-chunk'; jobId: number }
  | { type: 'fumen-finished'; jobId: number; encoded: string }
  | { type: 'failed'; taskId?: number; jobId?: number; code: string };

export type SolutionExportAsyncOptions = {
  signal?: AbortSignal;
  /** Main-thread capability authority; export workers must not re-probe the host. */
  hostCapabilitySnapshot?: HostCapabilitySnapshot;
};

export type SolutionExportKeySource = {
  readonly keyCount: number;
  commentForKey?(key: string): string | undefined;
  readKeys(
    start: number,
    count: number,
    signal?: AbortSignal
  ): Promise<readonly string[]> | readonly string[];
};

export async function encodeSolutionKeySourceForClipboard(
  source: SolutionExportKeySource,
  format: SolutionCopyFormat,
  options: SolutionExportAsyncOptions = {}
): Promise<string> {
  throwIfAborted(options.signal);
  validateKeySource(source);
  await requireClipboardSizedSource(source, format, options.signal);
  return encodeSolutionKeySource(source, format, options);
}

export async function encodeSolutionKeySource(
  source: SolutionExportKeySource,
  format: SolutionCopyFormat,
  options: SolutionExportAsyncOptions = {}
): Promise<string> {
  throwIfAborted(options.signal);
  validateKeySource(source);
  if (source.keyCount < WORKER_THRESHOLD) {
    const keys = await readSourceKeys(
      source,
      0,
      source.keyCount,
      options.signal
    );
    if (source.commentForKey) {
      return encodeSolutionPagesForClipboard(
        decoratedPages(source, keys),
        format,
        options
      );
    }
    return encodeSolutionKeysForClipboard(keys, format, options);
  }
  if (format === 'ctk') {
    if (source.commentForKey) {
      return encodeCtk3PageSourceAsync(
        {
          width: 10,
          pageCount: source.keyCount,
          async readPages(start, count, signal) {
            const keys = await readSourceKeys(source, start, count, signal);
            return decoratedPages(source, keys).map(solutionPageToCtk3Page);
          }
        },
        {
          workerFactory: createCtkDocumentWorker,
          signal: options.signal,
          workers: exportWorkerCount(source.keyCount, options.hostCapabilitySnapshot)
        }
      );
    }
    if (typeof Worker !== 'function') {
      return encodeCtkKeySourceWithoutWorkers(source, options.signal);
    }
    try {
      return await encodeCtkKeySourceWithWorkerPool(
        source,
        options.signal,
        options.hostCapabilitySnapshot
      );
    } catch (error) {
      rethrowIfAborted(error, options.signal);
      return encodeCtkKeySourceWithoutWorkers(source, options.signal);
    }
  }
  if (typeof Worker !== 'function') {
    return encodeFumenKeySourceWithoutWorker(source, options.signal);
  }
  try {
    return await encodeFumenKeySourceWithSingleWorker(source, options.signal);
  } catch (error) {
    rethrowIfAborted(error, options.signal);
    return encodeFumenKeySourceWithoutWorker(source, options.signal);
  }
}

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
        signal: options.signal,
        workers: exportWorkerCount(pages.length, options.hostCapabilitySnapshot)
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
      return await encodeCtkWithWorkerPool(
        keys,
        options.signal,
        options.hostCapabilitySnapshot
      );
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
  signal?: AbortSignal,
  hostCapabilitySnapshot?: HostCapabilitySnapshot
): Promise<string> {
  const taskCount = Math.ceil(keys.length / CTK_SOLUTION_SEGMENT_SIZE);
  const workerCount = exportWorkerCount(taskCount, hostCapabilitySnapshot);
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

async function encodeCtkKeySourceWithoutWorkers(
  source: SolutionExportKeySource,
  signal?: AbortSignal
): Promise<string> {
  const taskCount = Math.ceil(source.keyCount / LAZY_SOURCE_CHUNK_SIZE);
  const segments = new Array<string>(taskCount);
  for (let taskId = 0; taskId < taskCount; taskId += 1) {
    throwIfAborted(signal);
    const start = taskId * LAZY_SOURCE_CHUNK_SIZE;
    const count = Math.min(
      LAZY_SOURCE_CHUNK_SIZE,
      source.keyCount - start
    );
    const keys = await readSourceKeys(source, start, count, signal);
    segments[taskId] = encodeCtkSolutionKeySegment(keys);
    await yieldToHost();
  }
  throwIfAborted(signal);
  return combineCtkSolutionSegments(segments);
}

async function encodeCtkKeySourceWithWorkerPool(
  source: SolutionExportKeySource,
  signal?: AbortSignal,
  hostCapabilitySnapshot?: HostCapabilitySnapshot
): Promise<string> {
  const taskCount = Math.ceil(source.keyCount / LAZY_SOURCE_CHUNK_SIZE);
  const workerCount = exportWorkerCount(taskCount, hostCapabilitySnapshot);
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
    const fail = (error: unknown) => finish(() => reject(error));
    const dispatch = (worker: Worker) => {
      if (settled) return;
      if (signal?.aborted) {
        onAbort();
        return;
      }
      if (nextTask >= taskCount) return;
      const taskId = nextTask;
      nextTask += 1;
      const start = taskId * LAZY_SOURCE_CHUNK_SIZE;
      const count = Math.min(
        LAZY_SOURCE_CHUNK_SIZE,
        source.keyCount - start
      );
      Promise.resolve()
        .then(() => readSourceKeys(source, start, count, signal))
        .then((keys) => {
          if (settled) return;
          throwIfAborted(signal);
          worker.postMessage({
            type: 'ctk-segment',
            taskId,
            keys: Array.from(keys)
          });
        })
        .catch(fail);
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

async function encodeFumenKeySourceWithSingleWorker(
  source: SolutionExportKeySource,
  signal?: AbortSignal
): Promise<string> {
  throwIfAborted(signal);
  const worker = createExportWorker();
  const jobId = Date.now();
  let offset = 0;

  return new Promise<string>((resolve, reject) => {
    let settled = false;
    let reading = false;
    const onAbort = () => finish(() => reject(abortError(signal)));
    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener('abort', onAbort);
      worker.terminate();
      callback();
    };
    const fail = (error: unknown) => finish(() => reject(error));
    const sendNext = async () => {
      if (settled || reading) return;
      if (signal?.aborted) {
        onAbort();
        return;
      }
      if (offset >= source.keyCount) {
        worker.postMessage({ type: 'fumen-finish', jobId });
        return;
      }
      reading = true;
      try {
        const count = Math.min(
          LAZY_SOURCE_CHUNK_SIZE,
          source.keyCount - offset
        );
        const keys = await readSourceKeys(source, offset, count, signal);
        if (settled) return;
        offset += count;
        worker.postMessage(
          source.commentForKey
            ? {
                type: 'fumen-pages-chunk',
                jobId,
                pages: decoratedPages(source, keys)
              }
            : {
                type: 'fumen-chunk',
                jobId,
                keys: Array.from(keys)
              }
        );
      } finally {
        reading = false;
      }
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
        void sendNext().catch(fail);
      } else if (response.type === 'fumen-finished') {
        finish(() => resolve(response.encoded));
      }
    };
    worker.postMessage({ type: 'fumen-start', jobId });
  });
}

async function encodeFumenKeySourceWithoutWorker(
  source: SolutionExportKeySource,
  signal?: AbortSignal
): Promise<string> {
  const encoder = new FastColoredFumenEncoder();
  for (let offset = 0; offset < source.keyCount; offset += LAZY_SOURCE_CHUNK_SIZE) {
    throwIfAborted(signal);
    const count = Math.min(
      LAZY_SOURCE_CHUNK_SIZE,
      source.keyCount - offset
    );
    const keys = await readSourceKeys(source, offset, count, signal);
    for (const page of decoratedPages(source, keys)) {
      encoder.append(page);
    }
    await yieldToHost();
  }
  throwIfAborted(signal);
  return encoder.finish();
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

export function exportWorkerCount(
  taskCount: number,
  snapshot: HostCapabilitySnapshot = sharedBrowserHostCapabilitySnapshot()
): number {
  const availableTasks = Number.isFinite(taskCount)
    ? Math.max(1, Math.floor(taskCount))
    : 1;
  return Math.min(
    availableTasks,
    automaticWorkerAuthority(snapshot).workersEffective
  );
}

function validateKeySource(source: SolutionExportKeySource): void {
  if (!Number.isSafeInteger(source.keyCount) || source.keyCount < 1) {
    throw new RangeError('Solution export key count is out of range.');
  }
}

async function requireClipboardSizedSource(
  source: SolutionExportKeySource,
  format: SolutionCopyFormat,
  signal?: AbortSignal
): Promise<void> {
  if (
    source.keyCount > MAX_SAFE_CLIPBOARD_PAGES ||
    (format === 'ctk' && source.keyCount > CTK3_MAX_BUNDLE_PAGES)
  ) {
    throw new SolutionExportError('clipboard-output-too-large');
  }
  if (source.keyCount <= CLIPBOARD_ESTIMATE_SAMPLE_SIZE) return;
  const sampleCount = Math.min(
    CLIPBOARD_ESTIMATE_SAMPLE_SIZE,
    source.keyCount
  );
  const lastStart = source.keyCount - sampleCount;
  const starts = Array.from(
    new Set([0, Math.floor(lastStart / 2), lastStart])
  );
  let maximumCharactersPerPage = 0;
  for (const start of starts) {
    const keys = await readSourceKeys(source, start, sampleCount, signal);
    const encoded = source.commentForKey
      ? encodeSolutionPages(decoratedPages(source, keys), format)
      : format === 'ctk'
        ? encodeCtkSolutionKeySegment(keys)
        : encodeColoredFumenSolutionKeys(keys);
    maximumCharactersPerPage = Math.max(
      maximumCharactersPerPage,
      encoded.length / sampleCount
    );
    await yieldToHost();
  }
  const conservativeEstimate =
    maximumCharactersPerPage * source.keyCount * 1.25;
  if (conservativeEstimate > MAX_SAFE_CLIPBOARD_CHARACTERS) {
    throw new SolutionExportError('clipboard-output-too-large');
  }
}

async function readSourceKeys(
  source: SolutionExportKeySource,
  start: number,
  count: number,
  signal?: AbortSignal
): Promise<readonly string[]> {
  throwIfAborted(signal);
  const keys = await source.readKeys(start, count, signal);
  throwIfAborted(signal);
  if (!Array.isArray(keys) || keys.length !== count) {
    throw new RangeError('Solution export source returned an invalid key range.');
  }
  return keys;
}

function decoratedPages(
  source: SolutionExportKeySource,
  keys: readonly string[]
): SolutionExportPage[] {
  return keys.map((key) => {
    const page = parseSolutionKey(key);
    if (!page) throw new Error('invalid-solution-key');
    const comment = source.commentForKey?.(key);
    return comment ? { ...page, comment } : page;
  });
}

function yieldToHost(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
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

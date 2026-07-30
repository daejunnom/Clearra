import {
  CTK3_MAX_BUNDLE_PAGES,
  CTK3_MAX_SEGMENT_PAGES,
  decodeCtk3Segment,
  encodeCtk3Bundle,
  encodeCtk3Compact,
  indexCtk3Segments,
  type Ctk3Document,
  type Ctk3DocumentInfo,
  type Ctk3Page,
} from "./codec.js";
import { segmentOffsets, validateSegment } from "./documentReader.js";

export type Ctk3DecodeWorkerRequest = {
  taskId: number;
} & (
  | {
      type: "decode";
      segment: string;
    }
  | {
      type: "encode";
      document: Ctk3Document;
    }
);

export type Ctk3DecodeWorkerResponse =
  | {
      type: "decoded";
      taskId: number;
      document: Ctk3Document;
    }
  | {
      type: "failed";
      taskId: number;
      message: string;
    }
  | {
      type: "encoded";
      taskId: number;
      encoded: string;
    };

export type Ctk3DecodeWorkerLike = {
  onmessage: ((event: { data: Ctk3DecodeWorkerResponse }) => void) | null;
  onerror: ((event: unknown) => void) | null;
  postMessage(message: Ctk3DecodeWorkerRequest): void;
  terminate(): void;
};

export type Ctk3DecodeWorkerFactory = () => Ctk3DecodeWorkerLike;

export type Ctk3AsyncDecoderOptions = {
  workers?: number;
  cacheSegments?: number;
  workerFactory?: Ctk3DecodeWorkerFactory | null;
  signal?: AbortSignal;
};

export type Ctk3AsyncEncoderOptions = {
  workers?: number;
  segmentPages?: number;
  workerFactory?: Ctk3DecodeWorkerFactory | null;
  signal?: AbortSignal;
};

export type Ctk3AsyncPageSource = {
  readonly width: number;
  readonly pageCount: number;
  readPages(
    start: number,
    count: number,
    signal?: AbortSignal,
  ): Promise<readonly Ctk3Page[]> | readonly Ctk3Page[];
};

type DecodeTask = {
  taskId: number;
  segment: string;
  resolve: (document: Ctk3Document) => void;
  reject: (error: unknown) => void;
};

type WorkerSlot = {
  worker: Ctk3DecodeWorkerLike;
  task: DecodeTask | null;
};

export class Ctk3AsyncDocumentReader {
  readonly info: Ctk3DocumentInfo;
  private readonly cacheLimit: number;
  private readonly offsets: Uint32Array;
  private readonly pool: Ctk3DecodeWorkerPool;
  private readonly segments: string[];
  private readonly cache = new Map<number, Promise<Ctk3Document>>();
  private readonly signal: AbortSignal | undefined;
  private readonly abortListener: (() => void) | null;
  private closed = false;

  constructor(input: string, options: Ctk3AsyncDecoderOptions = {}) {
    throwIfAborted(options.signal);
    const indexed = indexCtk3Segments(input);
    this.info = indexed.info;
    this.segments = indexed.segments;
    this.offsets = segmentOffsets(this.info.segmentPageCounts);
    this.cacheLimit = normalizeCacheLimit(options.cacheSegments);
    this.signal = options.signal;
    const workerCount = normalizeWorkerCount(options.workers, this.segments.length);
    this.pool = new Ctk3DecodeWorkerPool(
      workerCount,
      options.workerFactory === undefined
        ? defaultBrowserWorkerFactory()
        : options.workerFactory,
    );
    this.abortListener = this.signal
      ? () => this.close(abortError(this.signal))
      : null;
    this.signal?.addEventListener("abort", this.abortListener!, { once: true });
    if (this.signal?.aborted) {
      const error = abortError(this.signal);
      this.close(error);
      throw error;
    }
  }

  get width(): number {
    return this.info.width;
  }

  get pageCount(): number {
    return this.info.pageCount;
  }

  async readPage(pageIndex: number): Promise<Ctk3Page> {
    throwIfAborted(this.signal);
    const location = this.locatePage(pageIndex);
    const document = await this.readSegment(location.segmentIndex);
    throwIfAborted(this.signal);
    return document.pages[location.localIndex];
  }

  async readPages(start: number, count: number): Promise<Ctk3Page[]> {
    throwIfAborted(this.signal);
    const end = validateRange(start, count, this.pageCount);
    if (start === end) return [];
    const first = this.locatePage(start).segmentIndex;
    const last = this.locatePage(end - 1).segmentIndex;
    const documents = await Promise.all(
      Array.from({ length: last - first + 1 }, (_, offset) =>
        this.readSegment(first + offset),
      ),
    );
    throwIfAborted(this.signal);
    const pages = new Array<Ctk3Page>(count);
    let outputIndex = 0;
    for (let segmentIndex = first; segmentIndex <= last; segmentIndex += 1) {
      const document = documents[segmentIndex - first];
      const segmentStart = this.offsets[segmentIndex];
      const localStart = Math.max(0, start - segmentStart);
      const localEnd = Math.min(document.pages.length, end - segmentStart);
      for (let index = localStart; index < localEnd; index += 1) {
        pages[outputIndex] = document.pages[index];
        outputIndex += 1;
      }
    }
    return pages;
  }

  prefetchPage(pageIndex: number): void {
    if (this.closed || this.signal?.aborted) return;
    if (pageIndex < 0 || pageIndex >= this.pageCount) return;
    const segmentIndex = this.locatePage(pageIndex).segmentIndex;
    void this.readSegment(segmentIndex).catch(() => undefined);
  }

  async decodeAll(): Promise<Ctk3Document> {
    throwIfAborted(this.signal);
    const documents = await Promise.all(
      this.segments.map((_, index) => this.readSegment(index)),
    );
    throwIfAborted(this.signal);
    const pages = new Array<Ctk3Page>(this.pageCount);
    let offset = 0;
    for (const document of documents) {
      for (const page of document.pages) {
        pages[offset] = page;
        offset += 1;
      }
    }
    return { width: this.width, pages };
  }

  close(error?: unknown) {
    if (this.closed) return;
    this.closed = true;
    if (this.signal && this.abortListener) {
      this.signal.removeEventListener("abort", this.abortListener);
    }
    this.cache.clear();
    this.pool.close(error);
  }

  private locatePage(pageIndex: number): {
    segmentIndex: number;
    localIndex: number;
  } {
    if (!Number.isSafeInteger(pageIndex) || pageIndex < 0 || pageIndex >= this.pageCount) {
      throw new RangeError("CTK3 page index is out of range.");
    }
    let low = 0;
    let high = this.segments.length;
    while (low + 1 < high) {
      const middle = (low + high) >>> 1;
      if (this.offsets[middle] <= pageIndex) low = middle;
      else high = middle;
    }
    return {
      segmentIndex: low,
      localIndex: pageIndex - this.offsets[low],
    };
  }

  private readSegment(segmentIndex: number): Promise<Ctk3Document> {
    throwIfAborted(this.signal);
    if (this.closed) {
      return Promise.reject(new Error("CTK3 decoder is closed."));
    }
    const cached = this.cache.get(segmentIndex);
    if (cached) {
      this.cache.delete(segmentIndex);
      this.cache.set(segmentIndex, cached);
      return cached;
    }
    const pending = this.pool
      .decode(this.segments[segmentIndex])
      .then((document) => {
        validateSegment(document, this.info, segmentIndex);
        return document;
      })
      .catch((error) => {
        this.cache.delete(segmentIndex);
        throw error;
      });
    this.cache.set(segmentIndex, pending);
    while (this.cache.size > this.cacheLimit) {
      const oldest = this.cache.keys().next().value;
      if (oldest === undefined) break;
      this.cache.delete(oldest);
    }
    return pending;
  }
}

export async function decodeCtk3Async(
  input: string,
  options: Ctk3AsyncDecoderOptions = {},
): Promise<Ctk3Document> {
  const reader = new Ctk3AsyncDocumentReader(input, options);
  try {
    return await reader.decodeAll();
  } finally {
    reader.close();
  }
}

export async function encodeCtk3Async(
  document: Ctk3Document,
  options: Ctk3AsyncEncoderOptions = {},
): Promise<string> {
  return encodeCtk3PageSourceAsync(
    {
      width: document.width,
      pageCount: document.pages.length,
      readPages: (start, count) => document.pages.slice(start, start + count),
    },
    options,
  );
}

export async function encodeCtk3PageSourceAsync(
  source: Ctk3AsyncPageSource,
  options: Ctk3AsyncEncoderOptions = {},
): Promise<string> {
  throwIfAborted(options.signal);
  if (
    !Number.isSafeInteger(source.pageCount) ||
    source.pageCount < 1 ||
    source.pageCount > CTK3_MAX_BUNDLE_PAGES
  ) {
    throw new RangeError("CTK3 async page count is out of range.");
  }
  const segmentPages = normalizeSegmentPages(options.segmentPages);
  const taskCount = Math.ceil(source.pageCount / segmentPages);
  const workerCount = normalizeWorkerCount(options.workers, taskCount);
  const workerFactory =
    options.workerFactory === undefined
      ? defaultBrowserWorkerFactory()
      : options.workerFactory;
  if (!workerFactory || workerCount <= 1) {
    const segments = new Array<string>(taskCount);
    for (let taskId = 0; taskId < taskCount; taskId += 1) {
      throwIfAborted(options.signal);
      const start = taskId * segmentPages;
      const count = Math.min(segmentPages, source.pageCount - start);
      const pages = await source.readPages(start, count, options.signal);
      validateSourcePages(pages, count);
      throwIfAborted(options.signal);
      segments[taskId] = encodeCtk3Compact({
        width: source.width,
        pages: Array.from(pages),
      });
    }
    throwIfAborted(options.signal);
    const encoded = encodeCtk3Bundle(segments);
    throwIfAborted(options.signal);
    return encoded;
  }
  return encodePageSourceWithWorkers(
    source,
    workerFactory,
    workerCount,
    segmentPages,
    taskCount,
    options.signal,
  );
}

export function openCtk3DocumentAsync(
  input: string,
  options: Ctk3AsyncDecoderOptions = {},
): Ctk3AsyncDocumentReader {
  return new Ctk3AsyncDocumentReader(input, options);
}

class Ctk3DecodeWorkerPool {
  private closed = false;
  private closeError = new Error("CTK3 decoder was closed.");
  private nextTaskId = 1;
  private readonly queue: DecodeTask[] = [];
  private readonly slots: WorkerSlot[] = [];

  constructor(
    workerCount: number,
    workerFactory: Ctk3DecodeWorkerFactory | null,
  ) {
    if (!workerFactory || workerCount <= 1) return;
    for (let index = 0; index < workerCount; index += 1) {
      const slot: WorkerSlot = {
        worker: workerFactory(),
        task: null,
      };
      slot.worker.onmessage = (event) => this.handleMessage(slot, event.data);
      slot.worker.onerror = (event) => this.handleWorkerFailure(slot, event);
      this.slots.push(slot);
    }
  }

  decode(segment: string): Promise<Ctk3Document> {
    if (this.closed) return Promise.reject(this.closeError);
    if (!this.slots.length) {
      return Promise.resolve().then(() => {
        if (this.closed) throw this.closeError;
        return decodeCtk3Segment(segment);
      });
    }
    return new Promise<Ctk3Document>((resolve, reject) => {
      this.queue.push({
        taskId: this.nextTaskId,
        segment,
        resolve,
        reject,
      });
      this.nextTaskId += 1;
      this.pump();
    });
  }

  close(error?: unknown) {
    if (this.closed) return;
    this.closed = true;
    this.closeError = workerError(error ?? this.closeError);
    for (const task of this.queue.splice(0)) task.reject(this.closeError);
    for (const slot of this.slots) {
      slot.task?.reject(this.closeError);
      slot.task = null;
      slot.worker.terminate();
    }
  }

  private pump() {
    if (this.closed) return;
    for (const slot of this.slots) {
      if (slot.task || !this.queue.length) continue;
      const task = this.queue.shift()!;
      slot.task = task;
      slot.worker.postMessage({
        type: "decode",
        taskId: task.taskId,
        segment: task.segment,
      });
    }
  }

  private handleMessage(slot: WorkerSlot, response: Ctk3DecodeWorkerResponse) {
    if (this.closed) return;
    const task = slot.task;
    if (!task || response.taskId !== task.taskId) {
      this.handleWorkerFailure(slot, new Error("CTK3 worker response is out of order."));
      return;
    }
    slot.task = null;
    if (response.type === "decoded") task.resolve(response.document);
    else if (response.type === "failed") task.reject(new Error(response.message));
    else task.reject(new Error("CTK3 worker returned an unexpected response."));
    this.pump();
  }

  private handleWorkerFailure(slot: WorkerSlot, error: unknown) {
    if (this.closed) return;
    slot.task?.reject(workerError(error));
    slot.task = null;
    slot.worker.terminate();
    const index = this.slots.indexOf(slot);
    if (index >= 0) this.slots.splice(index, 1);
    if (!this.slots.length) {
      while (this.queue.length) {
        const task = this.queue.shift()!;
        Promise.resolve()
          .then(() => decodeCtk3Segment(task.segment))
          .then(task.resolve, task.reject);
      }
      return;
    }
    this.pump();
  }
}

async function encodePageSourceWithWorkers(
  source: Ctk3AsyncPageSource,
  workerFactory: Ctk3DecodeWorkerFactory,
  workerCount: number,
  segmentPages: number,
  taskCount: number,
  signal?: AbortSignal,
): Promise<string> {
  const workers: Ctk3DecodeWorkerLike[] = [];
  const segments = new Array<string>(taskCount);
  let nextTask = 0;
  let completed = 0;
  let settled = false;

  return new Promise<string>((resolve, reject) => {
    const onAbort = () => finish(() => reject(abortError(signal)));
    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", onAbort);
      for (const worker of workers) worker.terminate();
      callback();
    };
    const fail = (error: unknown) => finish(() => reject(workerError(error)));
    const dispatch = (worker: Ctk3DecodeWorkerLike) => {
      if (settled) return;
      if (signal?.aborted) {
        onAbort();
        return;
      }
      if (nextTask >= taskCount) return;
      const taskId = nextTask;
      nextTask += 1;
      const start = taskId * segmentPages;
      const count = Math.min(segmentPages, source.pageCount - start);
      Promise.resolve()
        .then(() => source.readPages(start, count, signal))
        .then((pages) => {
          if (settled) return;
          throwIfAborted(signal);
          validateSourcePages(pages, count);
          worker.postMessage({
            type: "encode",
            taskId,
            document: {
              width: source.width,
              pages: Array.from(pages),
            },
          });
        })
        .catch(fail);
    };

    try {
      signal?.addEventListener("abort", onAbort, { once: true });
      if (signal?.aborted) {
        onAbort();
        return;
      }
      for (let index = 0; index < workerCount; index += 1) {
        const worker = workerFactory();
        workers.push(worker);
        worker.onerror = fail;
        worker.onmessage = (event) => {
          const response = event.data;
          if (response.type === "failed") {
            fail(new Error(response.message));
            return;
          }
          if (response.type !== "encoded") {
            fail(new Error("CTK3 worker returned an unexpected response."));
            return;
          }
          segments[response.taskId] = response.encoded;
          completed += 1;
          if (completed === taskCount) {
            finish(() => resolve(encodeCtk3Bundle(segments)));
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

function validateSourcePages(
  pages: readonly Ctk3Page[],
  expectedCount: number,
): void {
  if (!Array.isArray(pages) || pages.length !== expectedCount) {
    throw new RangeError("CTK3 page source returned an invalid page range.");
  }
}

function defaultBrowserWorkerFactory(): Ctk3DecodeWorkerFactory | null {
  const WorkerConstructor = (
    globalThis as unknown as {
      Worker?: new (
        url: URL,
        options: { type: "module"; name: string },
      ) => Ctk3DecodeWorkerLike;
    }
  ).Worker;
  if (!WorkerConstructor) return null;
  const moduleUrl: string | undefined = import.meta.url;
  if (!moduleUrl) return null;
  const workerPath = "./decodeWorker.js";
  const workerUrl = new URL(workerPath, moduleUrl);
  return () =>
    new WorkerConstructor(workerUrl, {
      type: "module",
      name: "ctk3-decode",
    });
}

function normalizeWorkerCount(value: number | undefined, segmentCount: number): number {
  const fallback = Math.max(
    1,
    ((globalThis as unknown as { navigator?: { hardwareConcurrency?: number } })
      .navigator?.hardwareConcurrency ?? 2) - 1,
  );
  const requested = value === undefined ? fallback : value;
  if (!Number.isSafeInteger(requested) || requested < 1 || requested > 256) {
    throw new RangeError("CTK3 worker count must be between 1 and 256.");
  }
  return Math.min(requested, segmentCount);
}

function normalizeSegmentPages(value: number | undefined): number {
  if (value === undefined) return 1024;
  if (
    !Number.isSafeInteger(value) ||
    value < 1 ||
    value > CTK3_MAX_SEGMENT_PAGES
  ) {
    throw new RangeError(
      `CTK3 segment size must be between 1 and ${CTK3_MAX_SEGMENT_PAGES}.`,
    );
  }
  return value;
}

function normalizeCacheLimit(value: number | undefined): number {
  if (value === undefined) return 3;
  if (!Number.isSafeInteger(value) || value < 1 || value > 64) {
    throw new RangeError("CTK3 segment cache must contain between 1 and 64 segments.");
  }
  return value;
}

function validateRange(start: number, count: number, pageCount: number): number {
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(count) ||
    start < 0 ||
    count < 0 ||
    start + count > pageCount
  ) {
    throw new RangeError("CTK3 page range is out of bounds.");
  }
  return start + count;
}

function workerError(error: unknown): Error {
  if (error instanceof Error) return error;
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return new Error(error.message);
  }
  return new Error("CTK3 decode worker failed.");
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw abortError(signal);
}

function abortError(signal: AbortSignal | undefined): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error("CTK3 operation was aborted.");
  error.name = "AbortError";
  return error;
}

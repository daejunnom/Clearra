import {
  CTK3_MAX_BUNDLE_PAGES,
  FUMEN_MAX_PAGES,
  FUMEN_MAX_SOURCE_CHARACTERS,
  encodeCtk3Async,
  isCtk3,
  type Ctk3AsyncEncoderOptions,
  type Ctk3Document,
  type Ctk3Page
} from './ctk3Codec';
import { FUMEN_BUNDLE_MAX_CHARACTERS, FUMEN_BUNDLE_MAX_DOCUMENTS } from './fumenBundleLimits';
import { openFumenWorkerReader } from './fumenDocumentReader';
import { encodeFieldDocument, openFieldDocument, type FieldDocumentReader, type OpenFieldDocumentOptions } from './fieldInterchangeCore';

export * from './fieldInterchangeCore';

const WIDTH = 10;
const FUMEN_WORKER_THRESHOLD = 256;
const FUMEN_PATTERN = /v11(?:0|5)@[A-Za-z0-9+/?]+/;

/** GUI entry: inspect and decode multiline Fumen in an isolated worker. */
export async function openFieldDocumentAsync(
  input: string,
  options: OpenFieldDocumentOptions = {}
): Promise<FieldDocumentReader> {
  throwIfAborted(options.signal);
  // CTK has its own asynchronous indexed reader. Other multiline inputs need
  // a separate worker: a single bounded Fumen chunk can still be expensive.
  const raw = input.trim();
  if (!isCtk3(raw) && raw.includes('\n')) {
    if (input.length > FUMEN_BUNDLE_MAX_CHARACTERS) {
      throw new Error('fumen-bundle-input-too-large');
    }
    const factory = options.fumenWorkerFactory === undefined
      ? typeof Worker === 'function'
        ? () => new Worker(new URL('./fumenDocumentWorker.ts', import.meta.url), { type: 'module' })
        : null
      : options.fumenWorkerFactory;
    // Even short compressed Fumen can expand to many pages, so input length
    // cannot justify a synchronous Fumen fallback on the UI thread. Preserve
    // the prior bounded handling of multiline non-Fumen text.
    if (!factory) {
      if (raw.length <= 65_536 && !FUMEN_PATTERN.test(raw)) return openFieldDocument(input, options);
      throw new Error('fumen-worker-unavailable');
    }
    return openFumenWorkerReader(input, factory, options.signal);
  }
  return openFieldDocument(input, options);
}

export async function encodeFieldDocumentAsync(
  document: Ctk3Document,
  format: 'fumen' | 'ctk',
  options: Ctk3AsyncEncoderOptions = {}
): Promise<string> {
  if (format === 'ctk') return encodeCtk3Async(document, options);
  throwIfAborted(options.signal);
  if (
    document.pages.length >= FUMEN_WORKER_THRESHOLD &&
    typeof Worker === 'function'
  ) {
    return encodeFumenFieldDocumentWithWorker(document, options.signal);
  }
  const encoded = encodeFieldDocument(document, format);
  throwIfAborted(options.signal);
  return encoded;
}

/** Encode a page source without materializing more than one bounded Fumen. */
export async function encodeFumenPageSourceAsync(
  source: {
    readonly width: number;
    readonly pageCount: number;
    readonly revision?: number;
    readPages(start: number, count: number, signal?: AbortSignal): Promise<Ctk3Page[]>;
  },
  options: Ctk3AsyncEncoderOptions = {}
): Promise<string> {
  throwIfAborted(options.signal);
  if (source.width !== WIDTH || !Number.isSafeInteger(source.pageCount) || source.pageCount < 1) {
    throw new Error('fumen-page-limit');
  }
  if (source.pageCount > CTK3_MAX_BUNDLE_PAGES) throw new Error('fumen-bundle-page-limit');
  const pageCount = source.pageCount;
  const revision = source.revision;
  function checkSource(): void {
    throwIfAborted(options.signal);
    if (source.pageCount !== pageCount || source.revision !== revision) {
      throw new Error('document-changed-during-export');
    }
  }
  const documents: string[] = [];
  let totalCharacters = 0;
  async function appendRange(start: number, count: number): Promise<void> {
    checkSource();
    let pages = await source.readPages(start, count, options.signal);
    checkSource();
    if (pages.length !== count) throw new Error('Fumen page source is incomplete.');
    let encoded = await encodeFieldDocumentAsync({ width: WIDTH, pages }, 'fumen', options);
    checkSource();
    if (encoded.length > FUMEN_MAX_SOURCE_CHARACTERS) {
      if (count === 1) throw new Error('fumen-input-too-large');
      const left = Math.floor(count / 2);
      pages = [];
      encoded = '';
      await appendRange(start, left);
      await appendRange(start + left, count - left);
      return;
    }
    if (documents.length === FUMEN_BUNDLE_MAX_DOCUMENTS) throw new Error('fumen-bundle-page-limit');
    totalCharacters += encoded.length + (documents.length ? 1 : 0);
    if (totalCharacters > FUMEN_BUNDLE_MAX_CHARACTERS) {
      throw new Error('fumen-bundle-input-too-large');
    }
    documents.push(encoded);
  }
  for (let start = 0; start < pageCount; start += FUMEN_MAX_PAGES) {
    await appendRange(start, Math.min(FUMEN_MAX_PAGES, pageCount - start));
  }
  checkSource();
  return documents.join('\n');
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (!signal?.aborted) return;
  if (signal.reason instanceof Error) throw signal.reason;
  const error = new Error('Field document operation was aborted.');
  error.name = 'AbortError';
  throw error;
}

type FumenFieldDocumentWorkerResponse =
  | { type: 'encoded'; encoded: string }
  | { type: 'failed'; message: string };

function encodeFumenFieldDocumentWithWorker(
  document: Ctk3Document,
  signal?: AbortSignal
): Promise<string> {
  throwIfAborted(signal);
  const worker = new Worker(
    new URL('./fieldDocumentExportWorker.ts', import.meta.url),
    {
      type: 'module',
      name: 'clearra-fumen-document-export'
    }
  );
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
    signal?.addEventListener('abort', onAbort, { once: true });
    if (signal?.aborted) {
      onAbort();
      return;
    }
    worker.onerror = (event) => {
      finish(() => reject(event.error ?? new Error(event.message)));
    };
    worker.onmessage = (
      event: MessageEvent<FumenFieldDocumentWorkerResponse>
    ) => {
      const response = event.data;
      if (response.type === 'encoded') {
        finish(() => resolve(response.encoded));
      } else {
        finish(() => reject(new Error(response.message)));
      }
    };
    worker.postMessage(document);
  });
}

function abortError(signal: AbortSignal | undefined): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error('Field document operation was aborted.');
  error.name = 'AbortError';
  return error;
}

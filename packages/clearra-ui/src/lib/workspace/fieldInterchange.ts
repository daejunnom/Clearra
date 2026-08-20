import { decoder, encoder, Field } from 'tetris-fumen';

import {
  type Ctk3AsyncDocumentReader,
  type Ctk3AsyncEncoderOptions,
  type Ctk3DecodeWorkerFactory,
  decodeCtk3,
  decodeFumenWithinPageLimit,
  escapeFumenComment,
  FUMEN_MAX_PAGES,
  FUMEN_MAX_SOURCE_CHARACTERS,
  inspectFumenPageCount,
  encodeCtk3Async,
  encodeCtk3,
  isCtk3,
  openCtk3Document,
  openCtk3DocumentAsync,
  type Ctk3Color,
  type Ctk3Document,
  type Ctk3Operation,
  type Ctk3Page,
} from './ctk3Codec';
import {
  parseSolutionKey,
  solutionPageToCtk3Page,
  type SolutionExportPage
} from './solutionExport';

export type ImportedField = {
  boardMask: bigint;
  occupiedHeight: number;
};

export type FieldDocumentReader = {
  readonly width: number;
  readonly pageCount: number;
  readonly originalCtk: string | null;
  readPage(pageIndex: number): Promise<Ctk3Page>;
  readPages?(start: number, count: number): Promise<Ctk3Page[]>;
  decodeAll(): Promise<Ctk3Document>;
  close(): void;
};

export type OpenFieldDocumentOptions = {
  workers?: number;
  cacheSegments?: number;
  workerFactory?: Ctk3DecodeWorkerFactory | null;
  signal?: AbortSignal;
};

const WIDTH = 10;
const FUMEN_HEIGHT = 23;
const CLEARA_HEIGHT = 24;
const FUMEN_WORKER_THRESHOLD = 256;
const FUMEN_PATTERN = /v11(?:0|5)@[A-Za-z0-9+/?]+/;

export {
  decodeFumenWithinPageLimit,
  FUMEN_MAX_PAGES,
  FUMEN_MAX_SOURCE_CHARACTERS,
  inspectFumenPageCount
};

export function decodeFieldDocument(input: string): Ctk3Document {
  const raw = input.trim();
  if (isCtk3(raw)) {
    return decodeCtk3(raw);
  }
  const source = decodeBoundedFumenInputUrl(raw);
  if (isCtk3(source)) return decodeCtk3(source);

  const legacy = extractLegacyCtk(source);
  if (legacy) {
    const page = parseSolutionKey(legacy);
    if (!page) throw new Error('Legacy CTK solution is invalid.');
    return {
      width: WIDTH,
      pages: [solutionPageToCtk3Page(page)]
    };
  }

  const fumen = source.match(FUMEN_PATTERN)?.[0];
  if (!fumen) throw new Error('No Fumen or CTK value was found.');
  if (fumen.length > FUMEN_MAX_SOURCE_CHARACTERS) {
    throw new Error('fumen-input-too-large');
  }
  const pages = decodeFumenWithinPageLimit(fumen, (bounded) => decoder.decode(bounded));
  if (!pages.length) throw new Error('Fumen has no pages.');
  if (pages.length > FUMEN_MAX_PAGES) {
    throw new Error('fumen-page-limit');
  }
  return {
    width: WIDTH,
    pages: pages.map((page) => {
      const cells: Ctk3Color[] = [];
      let height = 0;
      for (let y = 0; y < FUMEN_HEIGHT; y += 1) {
        for (let x = 0; x < WIDTH; x += 1) {
          const color = fumenColor(page.field.at(x, y));
          cells.push(color);
          if (color !== null) height = y + 1;
        }
      }
      const garbage = Array.from(
        { length: WIDTH },
        (_, x) => fumenGarbageColor(page.field, x)
      );
      const operation = page.operation
        ? {
            piece: page.operation.type,
            rotation: page.operation.rotation,
            x: page.operation.x,
            y: page.operation.y
          } as Ctk3Operation
        : undefined;
      return {
        height,
        cells: cells.slice(0, height * WIDTH),
        comment: page.comment || undefined,
        operation,
        flags: {
          lock: page.flags.lock,
          mirror: page.flags.mirror,
          colorize: page.flags.colorize,
          rise: page.flags.rise,
          quiz: page.flags.quiz
        },
        ...(garbage.some((color) => color !== null) ? { garbage } : {})
      };
    })
  };
}

export function openFieldDocument(
  input: string,
  options: OpenFieldDocumentOptions = {}
): FieldDocumentReader {
  const raw = input.trim();
  const source = isCtk3(raw) ? raw : decodeBoundedFumenInputUrl(raw);
  if (isCtk3(source)) {
    const reader = openCtk3DocumentAsync(source, options);
    return ctkReader(source, reader);
  }
  return eagerReader(decodeFieldDocument(source));
}

export function encodeFieldDocument(
  document: Ctk3Document,
  format: 'fumen' | 'ctk'
): string {
  if (format === 'ctk') return encodeCtk3(document);
  if (document.width !== WIDTH || document.pages.length === 0) {
    throw new Error('Fumen requires a non-empty 10-column document.');
  }
  if (document.pages.length > FUMEN_MAX_PAGES) {
    throw new Error('fumen-page-limit');
  }
  const normalizedPages = document.pages.map((page) =>
    normalizeFumenPage(page, document.width)
  );
  for (const normalized of normalizedPages) {
    if (normalized.comment !== undefined) {
      escapeFumenComment(normalized.comment);
    }
  }
  const pages = normalizedPages.map((normalized, index) => {
    const field = Field.create(
      fumenFieldText(normalized.cells, normalized.height),
      normalized.garbage ? fumenRowText(normalized.garbage) : undefined
    );
    return {
      field,
      ...(normalized.comment ? { comment: normalized.comment } : {}),
      ...(normalized.operation
        ? {
            operation: {
              type: normalized.operation.piece,
              rotation: normalized.operation.rotation,
              x: normalized.operation.x,
              y: normalized.operation.y
            }
          }
        : {}),
      flags: {
        lock: normalized.flags?.lock ?? true,
        mirror: normalized.flags?.mirror ?? false,
        colorize: normalized.flags?.colorize ?? index === 0,
        rise: normalized.flags?.rise ?? false
      }
    };
  });
  return encoder.encode(pages);
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

export function decodeFieldInput(
  input: string,
  maximumHeight = 6
): ImportedField {
  if (
    !Number.isInteger(maximumHeight) ||
    maximumHeight < 1 ||
    maximumHeight > CLEARA_HEIGHT
  ) {
    throw new Error('Field height limit must be between one and 24 rows.');
  }
  const raw = input.trim();
  const source = isCtk3(raw) ? raw : decodeBoundedFumenInputUrl(raw);
  let width: number;
  let page: Ctk3Page | undefined;
  if (isCtk3(source)) {
    const reader = openCtk3Document(source, { cacheSegments: 1 });
    width = reader.width;
    page = reader.readPage(0);
    reader.clearCache();
  } else {
    const document = decodeFieldDocument(source);
    width = document.width;
    page = document.pages[0];
  }
  if (width !== WIDTH) {
    throw new Error('Clearra field import requires a 10-column document.');
  }
  if (!page) throw new Error('The document has no pages.');
  let boardMask = 0n;
  let occupiedHeight = 0;
  for (let index = 0; index < page.cells.length; index += 1) {
    if (page.cells[index] === null) continue;
    const y = Math.floor(index / WIDTH);
    if (y >= maximumHeight) {
      throw new Error(`Field exceeds the ${maximumHeight}-line range.`);
    }
    boardMask |= 1n << BigInt(index);
    occupiedHeight = Math.max(occupiedHeight, y + 1);
  }
  return { boardMask, occupiedHeight };
}

export function documentFromSolutionPages(
  pages: SolutionExportPage[]
): Ctk3Document {
  return {
    width: WIDTH,
    pages: pages.map(solutionPageToCtk3Page)
  };
}

function decodeInputUrl(input: string): string {
  const source = input.trim();
  if (!/%[0-9a-f]{2}/i.test(source)) return source;
  try {
    return decodeURIComponent(source);
  } catch {
    return source;
  }
}

function decodeBoundedFumenInputUrl(input: string): string {
  if (input.length > FUMEN_MAX_SOURCE_CHARACTERS) {
    throw new Error('fumen-input-too-large');
  }
  const source = decodeInputUrl(input);
  if (source.length > FUMEN_MAX_SOURCE_CHARACTERS) {
    throw new Error('fumen-input-too-large');
  }
  return source;
}

function extractLegacyCtk(source: string): string | null {
  const legacy = source.match(/ctk(?:1|2)\|[^\s#&?]+/i)?.[0];
  if (!legacy) return null;
  return legacy
    .toLowerCase()
    .replace(/([iotszjl]):/g, (_, piece) => `${piece.toUpperCase()}:`);
}

function normalizeFumenPage(page: Ctk3Page, width: number): Ctk3Page {
  if (
    !Number.isInteger(page.height) ||
    page.height < 0 ||
    page.height > FUMEN_HEIGHT ||
    page.cells.length !== width * page.height
  ) {
    throw new Error('Fumen supports visible rows 1 through 23.');
  }
  return page;
}

function fumenFieldText(cells: Ctk3Color[], height: number): string {
  let output = '';
  for (let y = height - 1; y >= 0; y -= 1) {
    output += fumenRowText(cells.slice(y * WIDTH, (y + 1) * WIDTH));
  }
  return output;
}

function fumenRowText(cells: Ctk3Color[]): string {
  return cells.map(ctkColorToFumen).join('');
}

function ctkColorToFumen(color: Ctk3Color): string {
  if (color === null) return '_';
  if (color === 'G') return 'X';
  return color;
}

function fumenColor(color: string): Ctk3Color {
  if (color === '_') return null;
  if (color === 'X' || color === 'GRAY') return 'G';
  if (['I', 'O', 'T', 'S', 'Z', 'J', 'L'].includes(color)) {
    return color as Exclude<Ctk3Color, 'G' | null>;
  }
  throw new Error(`Unsupported Fumen color: ${color}`);
}

function fumenGarbageColor(field: Field, x: number): Ctk3Color {
  try {
    return fumenColor(field.at(x, -1));
  } catch {
    return null;
  }
}

function ctkReader(
  source: string,
  reader: Ctk3AsyncDocumentReader
): FieldDocumentReader {
  return {
    width: reader.width,
    pageCount: reader.pageCount,
    originalCtk: source,
    readPage: (pageIndex) => reader.readPage(pageIndex),
    readPages: (start, count) => reader.readPages(start, count),
    decodeAll: () => reader.decodeAll(),
    close: () => reader.close()
  };
}

function eagerReader(document: Ctk3Document): FieldDocumentReader {
  return {
    width: document.width,
    pageCount: document.pages.length,
    originalCtk: null,
    readPage: async (pageIndex) => {
      if (
        !Number.isSafeInteger(pageIndex) ||
        pageIndex < 0 ||
        pageIndex >= document.pages.length
      ) {
        throw new RangeError('Document page index is out of range.');
      }
      return document.pages[pageIndex];
    },
    readPages: async (start, count) => {
      if (
        !Number.isSafeInteger(start) ||
        !Number.isSafeInteger(count) ||
        start < 0 ||
        count < 0 ||
        start + count > document.pages.length
      ) {
        throw new RangeError('Document page range is out of range.');
      }
      return document.pages.slice(start, start + count);
    },
    decodeAll: async () => document,
    close: () => undefined
  };
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

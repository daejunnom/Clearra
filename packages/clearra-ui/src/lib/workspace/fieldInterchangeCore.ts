import { decoder, encoder, Field } from 'tetris-fumen';

import {
  type Ctk3AsyncDocumentReader,
  type Ctk3DecodeWorkerFactory,
  CTK3_MAX_BUNDLE_PAGES,
  decodeCtk3,
  decodeFumenWithinPageLimit,
  escapeFumenComment,
  FUMEN_MAX_PAGES,
  FUMEN_MAX_SOURCE_CHARACTERS,
  inspectFumenPageCount,
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
import { FUMEN_BUNDLE_MAX_CHARACTERS, FUMEN_BUNDLE_MAX_DOCUMENTS } from './fumenBundleLimits';
import type { FumenReaderWorkerFactory } from './fumenDocumentReader';

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
  fumenWorkerFactory?: FumenReaderWorkerFactory | null;
  signal?: AbortSignal;
};

const WIDTH = 10;
const FUMEN_HEIGHT = 23;
const CLEARA_HEIGHT = 24;
const FUMEN_PATTERN = /v11(?:0|5)@[A-Za-z0-9+/?]+/;
// Multi-document clipboard output has its own finite bundle budget. A single
// document still uses the existing 16 MiB / 4096-page Fumen guards.
const EXACT_FUMEN_PATTERN = /^v11(?:0|5)@[A-Za-z0-9+/?]+$/;

type FumenBundlePart = { start: number; end: number; endPage: number };

export {
  decodeFumenWithinPageLimit,
  FUMEN_MAX_PAGES,
  FUMEN_MAX_SOURCE_CHARACTERS,
  inspectFumenPageCount
};

export function decodeFieldDocument(input: string): Ctk3Document {
  const bundle = inspectFumenBundle(input);
  if (bundle) {
    const count = bundle[bundle.length - 1].endPage;
    // This synchronous API returns one fully materialized document. Large
    // bundles must use the bounded, lazy openFieldDocument reader instead.
    if (count > FUMEN_MAX_PAGES) throw new Error('fumen-page-limit');
    return {
      width: WIDTH,
      pages: bundle.flatMap(({ start, end }) =>
        decodeFieldDocument(input.slice(start, end)).pages
      )
    };
  }
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
  throwIfAborted(options.signal);
  const bundle = inspectFumenBundle(input);
  if (bundle) return fumenBundleReader(input, bundle, options.signal);
  const raw = input.trim();
  const source = isCtk3(raw) ? raw : decodeBoundedFumenInputUrl(raw);
  if (isCtk3(source)) {
    const reader = openCtk3DocumentAsync(source, options);
    return ctkReader(source, reader);
  }
  return eagerReader(decodeFieldDocument(source));
}

/** Parse only newline-delimited exact Fumen codes, never loose URL matches. */
function inspectFumenBundle(input: string): FumenBundlePart[] | null {
  if (!input.includes('\n')) return null;
  if (input.length > FUMEN_BUNDLE_MAX_CHARACTERS) {
    throw new Error('fumen-bundle-input-too-large');
  }
  const parts: FumenBundlePart[] = [];
  let cursor = 0;
  let pageCount = 0;
  let recognized = false;
  while (cursor < input.length) {
    const newline = input.indexOf('\n', cursor);
    let end = newline < 0 ? input.length : newline;
    const start = cursor;
    cursor = newline < 0 ? input.length : newline + 1;
    if (end > start && input[end - 1] === '\r') end -= 1;
    if (end === start) continue;
    if (end - start > FUMEN_MAX_SOURCE_CHARACTERS) {
      throw new Error('fumen-input-too-large');
    }
    const line = input.slice(start, end);
    if (!EXACT_FUMEN_PATTERN.test(line)) {
      if (recognized) throw new Error('invalid-fumen-input');
      // A malformed first chunk followed by a plausible Fumen must not fall
      // back to the legacy loose matcher and silently select the later chunk.
      let next = cursor;
      while (next < input.length) {
        if (input.startsWith('v115@', next) || input.startsWith('v110@', next)) {
          throw new Error('invalid-fumen-input');
        }
        const following = input.indexOf('\n', next);
        if (following < 0) break;
        next = following + 1;
      }
      return null;
    }
    recognized = true;
    if (parts.length === FUMEN_BUNDLE_MAX_DOCUMENTS) {
      throw new Error('fumen-bundle-page-limit');
    }
    pageCount += inspectFumenPageCount(line);
    if (pageCount > CTK3_MAX_BUNDLE_PAGES) throw new Error('fumen-bundle-page-limit');
    parts.push({ start, end, endPage: pageCount });
  }
  return parts.length > 1 ? parts : null;
}

function fumenBundleReader(
  source: string,
  parts: FumenBundlePart[],
  signal?: AbortSignal
): FieldDocumentReader {
  const pageCount = parts[parts.length - 1].endPage;
  let cachedPart = -1;
  let cachedPages: Ctk3Page[] = [];
  let closed = false;

  function checkRange(start: number, count: number) {
    throwIfAborted(signal);
    if (closed) throw new Error('Document reader is closed.');
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(count) ||
        start < 0 || count < 0 || start + count > pageCount) {
      throw new RangeError('Document page range is out of range.');
    }
  }

  function pageAt(index: number): Ctk3Page {
    let low = 0;
    let high = parts.length - 1;
    while (low < high) {
      const mid = low + Math.floor((high - low) / 2);
      if (index < parts[mid].endPage) high = mid;
      else low = mid + 1;
    }
    if (cachedPart !== low) {
      const part = parts[low];
      cachedPages = decodeFieldDocument(source.slice(part.start, part.end)).pages;
      cachedPart = low;
    }
    const firstPage = low === 0 ? 0 : parts[low - 1].endPage;
    return cachedPages[index - firstPage];
  }

  return {
    width: WIDTH,
    pageCount,
    originalCtk: null,
    async readPage(index) {
      checkRange(index, 1);
      return pageAt(index);
    },
    async readPages(start, count) {
      checkRange(start, count);
      if (count > FUMEN_MAX_PAGES) throw new Error('fumen-page-limit');
      const pages: Ctk3Page[] = [];
      for (let index = start; index < start + count; index += 1) {
        throwIfAborted(signal);
        pages.push(pageAt(index));
      }
      return pages;
    },
    async decodeAll() {
      checkRange(0, pageCount);
      if (pageCount > FUMEN_MAX_PAGES) throw new Error('fumen-page-limit');
      const pages: Ctk3Page[] = [];
      for (let index = 0; index < pageCount; index += 1) pages.push(pageAt(index));
      return { width: WIDTH, pages };
    },
    close() {
      closed = true;
      cachedPart = -1;
      cachedPages = [];
      source = '';
      parts.length = 0;
    }
  };
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
      // CTK3 comments belong to each page. Fumen omission inherits the
      // previous comment, so an empty CTK3 page must explicitly clear it.
      comment: normalized.comment ?? '',
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
    // A board input intentionally selects one page, unlike document readers.
    // Keep that contract even when a newline-delimited bundle was pasted.
    const bundle = inspectFumenBundle(source);
    const first = bundle ? source.slice(bundle[0].start, bundle[0].end) : source;
    const document = decodeFieldDocument(first);
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

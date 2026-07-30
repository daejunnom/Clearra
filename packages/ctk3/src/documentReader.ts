import {
  Ctk3CodecError,
  decodeCtk3Segment,
  indexCtk3Segments,
  type Ctk3Document,
  type Ctk3DocumentInfo,
  type Ctk3Page,
} from "./codec.js";

export type Ctk3DocumentReaderOptions = {
  cacheSegments?: number;
};

export class Ctk3DocumentReader {
  readonly info: Ctk3DocumentInfo;
  private readonly cacheLimit: number;
  private readonly offsets: Uint32Array;
  private readonly segments: string[];
  private readonly cache = new Map<number, Ctk3Document>();

  constructor(input: string, options: Ctk3DocumentReaderOptions = {}) {
    const indexed = indexCtk3Segments(input);
    this.info = indexed.info;
    this.segments = indexed.segments;
    this.offsets = segmentOffsets(this.info.segmentPageCounts);
    this.cacheLimit = normalizeCacheLimit(options.cacheSegments);
  }

  get width(): number {
    return this.info.width;
  }

  get pageCount(): number {
    return this.info.pageCount;
  }

  readPage(pageIndex: number): Ctk3Page {
    const location = this.locatePage(pageIndex);
    return this.readSegment(location.segmentIndex).pages[location.localIndex];
  }

  readPages(start: number, count: number): Ctk3Page[] {
    const end = validateRange(start, count, this.pageCount);
    const pages = new Array<Ctk3Page>(end - start);
    let outputIndex = 0;
    let pageIndex = start;
    while (pageIndex < end) {
      const location = this.locatePage(pageIndex);
      const document = this.readSegment(location.segmentIndex);
      const take = Math.min(document.pages.length - location.localIndex, end - pageIndex);
      for (let index = 0; index < take; index += 1) {
        pages[outputIndex] = document.pages[location.localIndex + index];
        outputIndex += 1;
      }
      pageIndex += take;
    }
    return pages;
  }

  decodeAll(): Ctk3Document {
    const pages = new Array<Ctk3Page>(this.pageCount);
    let offset = 0;
    for (let index = 0; index < this.segments.length; index += 1) {
      const document = this.readSegment(index);
      for (const page of document.pages) {
        pages[offset] = page;
        offset += 1;
      }
    }
    return { width: this.width, pages };
  }

  clearCache() {
    this.cache.clear();
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

  private readSegment(segmentIndex: number): Ctk3Document {
    const cached = this.cache.get(segmentIndex);
    if (cached) {
      this.cache.delete(segmentIndex);
      this.cache.set(segmentIndex, cached);
      return cached;
    }
    const document = decodeCtk3Segment(this.segments[segmentIndex]);
    validateSegment(document, this.info, segmentIndex);
    this.cache.set(segmentIndex, document);
    while (this.cache.size > this.cacheLimit) {
      const oldest = this.cache.keys().next().value;
      if (oldest === undefined) break;
      this.cache.delete(oldest);
    }
    return document;
  }
}

export function openCtk3Document(
  input: string,
  options: Ctk3DocumentReaderOptions = {},
): Ctk3DocumentReader {
  return new Ctk3DocumentReader(input, options);
}

export function segmentOffsets(pageCounts: readonly number[]): Uint32Array {
  const offsets = new Uint32Array(pageCounts.length + 1);
  for (let index = 0; index < pageCounts.length; index += 1) {
    offsets[index + 1] = offsets[index] + pageCounts[index];
  }
  return offsets;
}

export function validateSegment(
  document: Ctk3Document,
  info: Ctk3DocumentInfo,
  segmentIndex: number,
) {
  if (
    document.width !== info.width ||
    document.pages.length !== info.segmentPageCounts[segmentIndex]
  ) {
    throw new Ctk3CodecError("CTK3 segment metadata does not match its payload.");
  }
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

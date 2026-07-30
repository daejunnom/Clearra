import type { Ctk3Document, Ctk3Page } from './ctk3Codec';
import type { FieldDocumentReader } from './fieldInterchange';

export class CtkDrawerDocument {
  readonly width: number;
  private readonly reader: FieldDocumentReader | null;
  private readonly sourcePages: readonly Ctk3Page[] | null;
  private readonly sourceCount: number;
  private readonly overrides = new Map<number, Ctk3Page>();
  private readonly inserted = new Map<number, Ctk3Page>();
  private order: number[] | null = null;
  private nextInsertedId = -1;
  private changed = false;

  private constructor(
    width: number,
    sourceCount: number,
    sourcePages: readonly Ctk3Page[] | null,
    reader: FieldDocumentReader | null
  ) {
    this.width = width;
    this.sourceCount = sourceCount;
    this.sourcePages = sourcePages;
    this.reader = reader;
  }

  static fromPages(width: number, pages: readonly Ctk3Page[]): CtkDrawerDocument {
    if (!pages.length) throw new Error('CTK3 document has no pages.');
    return new CtkDrawerDocument(width, pages.length, pages, null);
  }

  static fromReader(reader: FieldDocumentReader): CtkDrawerDocument {
    if (reader.pageCount < 1) throw new Error('CTK3 document has no pages.');
    return new CtkDrawerDocument(reader.width, reader.pageCount, null, reader);
  }

  get pageCount(): number {
    return this.order?.length ?? this.sourceCount;
  }

  get originalCtk(): string | null {
    return this.changed ? null : this.reader?.originalCtk ?? null;
  }

  async readPage(pageIndex: number): Promise<Ctk3Page> {
    const id = this.pageId(pageIndex);
    const changed = this.overrides.get(id) ?? this.inserted.get(id);
    if (changed) return changed;
    if (id < 0) throw new Error('Inserted CTK3 page is unavailable.');
    if (this.sourcePages) return this.sourcePages[id];
    if (!this.reader) throw new Error('CTK3 document source is unavailable.');
    return this.reader.readPage(id);
  }

  async readPages(
    start: number,
    count: number,
    signal?: AbortSignal
  ): Promise<Ctk3Page[]> {
    const end = validateRange(start, count, this.pageCount);
    throwIfAborted(signal);
    if (start === end) return [];

    if (!this.order) {
      const pages = await this.readSourcePages(start, count);
      throwIfAborted(signal);
      for (let offset = 0; offset < pages.length; offset += 1) {
        const changed = this.overrides.get(start + offset);
        if (changed) pages[offset] = changed;
      }
      return pages;
    }

    const pages = new Array<Ctk3Page>(count);
    let offset = 0;
    while (offset < count) {
      throwIfAborted(signal);
      const id = this.order[start + offset];
      const changed = this.overrides.get(id) ?? this.inserted.get(id);
      if (changed) {
        pages[offset] = changed;
        offset += 1;
        continue;
      }
      if (id < 0) throw new Error('Inserted CTK3 page is unavailable.');

      let runLength = 1;
      while (offset + runLength < count) {
        const nextId = this.order[start + offset + runLength];
        if (
          nextId !== id + runLength ||
          this.overrides.has(nextId) ||
          this.inserted.has(nextId)
        ) {
          break;
        }
        runLength += 1;
      }
      const source = await this.readSourcePages(id, runLength);
      throwIfAborted(signal);
      for (let index = 0; index < source.length; index += 1) {
        pages[offset + index] = source[index];
      }
      offset += runLength;
    }
    return pages;
  }

  updatePage(pageIndex: number, page: Ctk3Page) {
    const id = this.pageId(pageIndex);
    if (id < 0) this.inserted.set(id, page);
    else this.overrides.set(id, page);
    this.changed = true;
  }

  insertPage(pageIndex: number, page: Ctk3Page) {
    if (
      !Number.isSafeInteger(pageIndex) ||
      pageIndex < 0 ||
      pageIndex > this.pageCount
    ) {
      throw new RangeError('CTK3 insertion index is out of range.');
    }
    const order = this.mutableOrder();
    const id = this.nextInsertedId;
    this.nextInsertedId -= 1;
    this.inserted.set(id, page);
    order.splice(pageIndex, 0, id);
    this.changed = true;
  }

  removePage(pageIndex: number) {
    if (this.pageCount <= 1) {
      throw new Error('The final CTK3 page cannot be removed.');
    }
    const order = this.mutableOrder();
    const [id] = order.splice(pageIndex, 1);
    if (id < 0) this.inserted.delete(id);
    this.changed = true;
  }

  async materialize(signal?: AbortSignal): Promise<Ctk3Document> {
    const pages = new Array<Ctk3Page>(this.pageCount);
    for (let start = 0; start < pages.length; start += MATERIALIZE_BATCH_PAGES) {
      throwIfAborted(signal);
      const count = Math.min(MATERIALIZE_BATCH_PAGES, pages.length - start);
      const batch = await this.readPages(start, count, signal);
      for (let offset = 0; offset < batch.length; offset += 1) {
        pages[start + offset] = batch[offset];
      }
      await yieldControl(signal);
    }
    return { width: this.width, pages };
  }

  close() {
    this.reader?.close();
  }

  private pageId(pageIndex: number): number {
    if (
      !Number.isSafeInteger(pageIndex) ||
      pageIndex < 0 ||
      pageIndex >= this.pageCount
    ) {
      throw new RangeError('CTK3 page index is out of range.');
    }
    return this.order?.[pageIndex] ?? pageIndex;
  }

  private mutableOrder(): number[] {
    if (!this.order) this.order = identityOrder(this.sourceCount);
    return this.order;
  }

  private async readSourcePages(
    start: number,
    count: number
  ): Promise<Ctk3Page[]> {
    if (this.sourcePages) return this.sourcePages.slice(start, start + count);
    if (!this.reader) throw new Error('CTK3 document source is unavailable.');
    if (this.reader.readPages) return this.reader.readPages(start, count);
    return Promise.all(
      Array.from({ length: count }, (_, offset) =>
        this.reader!.readPage(start + offset)
      )
    );
  }
}

const MATERIALIZE_BATCH_PAGES = 1024;

function identityOrder(length: number): number[] {
  return Array.from({ length }, (_, index) => index);
}

function validateRange(start: number, count: number, length: number): number {
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(count) ||
    start < 0 ||
    count < 0 ||
    start + count > length
  ) {
    throw new RangeError('CTK3 page range is out of range.');
  }
  return start + count;
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (!signal?.aborted) return;
  if (signal.reason instanceof Error) throw signal.reason;
  const error = new Error('CTK3 document operation was aborted.');
  error.name = 'AbortError';
  throw error;
}

async function yieldControl(signal: AbortSignal | undefined): Promise<void> {
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  throwIfAborted(signal);
}

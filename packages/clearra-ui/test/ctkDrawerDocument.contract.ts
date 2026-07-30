import assert from 'node:assert/strict';

import { CtkDrawerDocument } from '../src/lib/workspace/ctkDrawerDocument';
import type { Ctk3Document, Ctk3Page } from '../src/lib/workspace/ctk3Codec';
import type { FieldDocumentReader } from '../src/lib/workspace/fieldInterchange';

const pageCount = 456_923;
const reads: number[] = [];
let closed = false;
const reader: FieldDocumentReader = {
  width: 10,
  pageCount,
  originalCtk: 'ctk3b_test',
  async readPage(pageIndex: number): Promise<Ctk3Page> {
    reads.push(pageIndex);
    return page(pageIndex % 7 === 0 ? 'T' : 'L');
  },
  async readPages(start: number, count: number): Promise<Ctk3Page[]> {
    return Array.from({ length: count }, (_, offset) => {
      const pageIndex = start + offset;
      reads.push(pageIndex);
      return page(pageIndex % 7 === 0 ? 'T' : 'L');
    });
  },
  async decodeAll(): Promise<Ctk3Document> {
    throw new Error('lazy document unexpectedly materialized');
  },
  close() {
    closed = true;
  }
};

const document = CtkDrawerDocument.fromReader(reader);
assert.equal(document.pageCount, pageCount);
assert.equal(document.originalCtk, 'ctk3b_test');
assert.equal((await document.readPage(pageCount - 1)).cells[0], 'L');
assert.deepEqual(reads, [pageCount - 1]);

document.updatePage(pageCount - 1, page('S'));
assert.equal((await document.readPage(pageCount - 1)).cells[0], 'S');
assert.deepEqual(reads, [pageCount - 1]);
assert.equal(document.originalCtk, null);

assert.deepEqual(
  (await document.readPages(pageCount - 2, 2)).map((value) => value.cells[0]),
  ['L', 'S']
);

document.close();
assert.equal(closed, true);

const controller = new AbortController();
const abortable = CtkDrawerDocument.fromPages(
  10,
  Array.from({ length: 4096 }, () => page('T'))
);
const pendingMaterialize = abortable.materialize(controller.signal);
controller.abort();
await assert.rejects(
  pendingMaterialize,
  (error: unknown) => error instanceof Error && error.name === 'AbortError'
);

console.log(
  JSON.stringify({
    page_count: pageCount,
    source_reads: reads.length,
    full_decode_calls: 0
  })
);

function page(color: 'T' | 'L' | 'S'): Ctk3Page {
  return {
    height: 1,
    cells: [color, color, color, color, ...Array(6).fill(null)]
  };
}

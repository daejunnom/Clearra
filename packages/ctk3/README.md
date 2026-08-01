# ctk3

`ctk3` is a compact, lossless interchange codec for colored Tetris fields,
multiple pages, comments, operations, garbage, and page flags.

New output uses unpadded Base64url with the `ctk3_` prefix. Existing `ctk3@`
CTK85 documents remain readable.

## Install

```sh
npm install ctk3 tetris-fumen
```

`tetris-fumen` is a runtime dependency of `ctk3`; listing both dependencies is
useful when an application also imports Fumen directly.

## Fumen-compatible API

```ts
import { decoder, encoder, Field, Mino } from "ctk3";

const value = encoder.encode([
  {
    field: Field.create("LLL_______LOO_______"),
    comment: "Opening",
  },
  {
    operation: { type: "T", rotation: "right", x: 4, y: 1 },
  },
]);

const pages = decoder.decode(value);
const first = pages[0];

first.index;
first.comment;
first.operation;
first.flags;
first.refs;
first.field;
first.mino();

first.field instanceof Field; // true
first.mino() instanceof Mino; // true when an operation exists
```

`decoder.decode()` returns the same public page contract as
`tetris-fumen.decoder.decode()`:

- `index`, `operation`, `comment`, `flags`, and `refs`;
- a mutable `Field` copy from the `field` getter;
- a writable `field` property;
- `mino()` returning a `tetris-fumen` `Mino`.

Decoding is direct. It does not convert CTK3 to a Fumen string first.

The compatibility API accepts only Fumen-representable 10-column documents
whose visible cells fit rows 0 through 22. It throws
`Ctk3FumenCompatibilityError` instead of truncating wider or 24-row data.

## Native CTK3 document API

Use the document API for arbitrary CTK3 widths or the 24-row Clearra editor:

```ts
import {
  documentDecoder,
  documentEncoder,
  decodeCtk3,
  encodeCtk3,
} from "ctk3";

const document = documentDecoder.decode(value);
const sameValue = documentEncoder.encode(document);

// The function exports are equivalent.
decodeCtk3(value);
encodeCtk3(document);
```

The native document model uses bottom-up row-major cells and preserves every
CTK3 page property without imposing Fumen's 10x23 limit.

## `.ctk3` files

A `.ctk3` file contains one canonical CTK3 document as BOM-free UTF-8. Its MIME
type is `application/vnd.clearra.ctk3`; the same checksummed codec is therefore
used for text, URLs, clipboard values, and files.

```ts
import {
  CTK3_FILE_EXTENSION,
  CTK3_FILE_MIME_TYPE,
  createCtk3Blob,
  decodeCtk3File,
  encodeCtk3File,
  readCtk3File,
} from "ctk3";

const bytes = encodeCtk3File(document);
decodeCtk3File(bytes);

const blob = createCtk3Blob(document);
await readCtk3File(blob);
```

`decodeCtk3File()` accepts strings, `ArrayBuffer`, and typed-array views.
`readCtk3File()` accepts browser `File` and `Blob` objects.

## Large documents

A single CTK3 segment contains at most 4,096 pages. A `ctk3b_` bundle can
contain up to 1,048,576 pages, regardless of how many non-empty segments are
needed. The 4,096-page value is not a whole-document limit.

For a large document, open it lazily instead of materializing every page:

```ts
const reader = documentDecoder.open(value);

reader.pageCount;
reader.info.segmentCount;
const first = reader.readPage(0);
const distant = reader.readPage(456_922);
reader.clearCache();
```

Browser applications can move segment work off the UI thread. The async reader
decodes only requested segments and keeps a bounded LRU cache:

```ts
const reader = documentDecoder.openAsync(value, {
  workers: Math.max(1, navigator.hardwareConcurrency - 1),
  cacheSegments: 3,
});

const page = await reader.readPage(100_000);
reader.prefetchPage(100_001);
reader.close();
```

Async encoding splits the input into bounded segments and uses browser workers
when available:

```ts
const value = await documentEncoder.encodeAsync(document, {
  workers: Math.max(1, navigator.hardwareConcurrency - 1),
  segmentPages: 1024,
});
```

`decodeAsync()` remains available when every page is genuinely needed. For
drawers, previews, and document inspection, prefer `open()` or `openAsync()` so
page count does not become an up-front memory cost. A custom `workerFactory`
may be supplied by applications whose bundler requires an explicit worker
entry.

## Compatibility notes

- Fields and minos are actual `tetris-fumen` `Field` and `Mino` instances.
- Empty comments are returned as `""`, and every flag is materialized.
- Symmetry-equivalent operation representations are canonicalized by CTK3.
- `refs` are reconstructed canonically from exact field and comment reuse;
  they describe the decoded CTK3 document rather than its original bit layout.
- `encoder.encode()` accepts the same `EncodePages` input as
  `tetris-fumen.encoder.encode()`.

## Package formats

The package publishes:

- ESM through `dist/index.js`;
- CommonJS through `dist/index.cjs`;
- TypeScript declarations through `dist/index.d.ts`.

The binary format specification is maintained in
[`docs/ctk3.md`](https://github.com/daejunnom/Clearra/blob/main/docs/ctk3.md).

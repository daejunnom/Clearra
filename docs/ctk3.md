# CTK3 Interchange Format

CTK3 is Clearra's compact, versioned field interchange format. It represents
colored fields, multiple pages, comments, optional operations, garbage rows,
and page flags without assuming that the cells form standard tetrominoes.

New CTK interchange output uses the URL-safe `ctk3_` header. The compact
`ctk3@` text transport remains readable for existing documents. `ctk1|` and
`ctk2|` remain read-only legacy solution-key formats and are not aliases for
CTK3.

## Canonical Model

A document contains:

- a board width;
- one or more ordered pages.

Each page contains:

- a colored visible field;
- an optional colored garbage row;
- an optional UTF-8 comment;
- an optional piece operation;
- `lock`, `mirror`, `colorize`, `rise`, and `quiz` flags.

Visible cells use bottom-up row order and left-to-right column order. A page is
trimmed to the highest non-empty visible row before encoding. An empty field
therefore has height zero even when an editor displays additional empty rows.

Color codes are:

| Code | Cell       |
| ---: | ---------- |
|    0 | empty      |
|    1 | gray field |
|    2 | I          |
|    3 | O          |
|    4 | T          |
|    5 | S          |
|    6 | Z          |
|    7 | J          |
|    8 | L          |

## npm API

The public `ctk3` package provides two contracts:

```ts
import {
  decoder,
  encoder,
  documentDecoder,
  documentEncoder,
  Field,
  Mino,
} from "ctk3";
```

`decoder.decode()` and `encoder.encode()` mirror the public page contract of
`tetris-fumen`, including `Page.index`, `operation`, `comment`, materialized
flags, `refs`, the mutable-copy `field` accessor, and `mino()`. Returned fields
and minos are actual `tetris-fumen` `Field` and `Mino` instances. This path is
limited to Fumen-representable 10x23 fields and rejects incompatible CTK3
documents instead of truncating them.

`documentDecoder` and `documentEncoder` expose the native CTK3 document model
for arbitrary supported widths and the 24-row Clearra editor. The compatible
decoder reconstructs pages directly from CTK3 bytes; it does not create and
decode an intermediate Fumen string.

## Bitstream

Bits are written least-significant bit first within each byte.

The document header is:

```text
magic                 8 bits, 0xC3
schema revision       3 bits
width minus one       5 bits
page count            UVar
extension-present     1 bit, currently 0
```

Decoders accept revisions 0 through 3. Revision 0 is the original page
encoding, revision 1 is the compact independent/delta encoding, revision 2
adds lossless temporal prediction, and revision 3 adds a document-wide shared
field predictor. Encoders measure complete revision 1, 2, and applicable
revision 3 payloads, including byte padding, and emit only the shortest
admissible payload.

`UVar` uses one of four forms:

```text
0   + 4 value bits
10  + 8 value bits
110 + 16 value bits
111 + 32 value bits
```

Signed coordinates use zigzag conversion followed by `UVar`.

## Revision 2 Temporal Records

Every record starts with a 2-bit mode:

```text
0  independently described or predicted page
1  exact copy of the previous page
2  exact reference to an earlier page
3  run of exact copies of the previous page
```

Exact page modes include cells, colors, comments, operations, flags, and
garbage. A hash or dictionary key is never sufficient for equality; all page
state is represented by the referenced exact page.

A normal temporal page independently compresses flags, visible height, field,
garbage, comment, and operation. Flags can use defaults, the previous flags,
or a literal value. Comments can be empty, copy the previous comment, reference
an earlier identical comment, or contain literal UTF-8. Operations can be
absent, copy the previous operation, delta its coordinates when piece and
rotation are unchanged, or contain a complete operation.

### Field Predictors

The field begins with a 4-bit predictor. The encoder evaluates every applicable
predictor and writes the candidate with the smallest measured bit length:

```text
0  independent field
1  previous field
2  grayscale previous field
3  horizontally mirrored previous field
4  previous field after locking its operation
5  locked previous field, converted to grayscale
6  locked previous field after complete-line removal
7  locked and line-cleared previous field, converted to grayscale
8  recent or exact older field reference
9  horizontally mirrored older field reference
10 document-wide shared field
```

Modes 6 and 7 are the lossless command predictors used by the drawer's
"create the next page after line removal" and combined line/color cleanup
actions. The decoder reconstructs the prior operation, applies it only when it
is locked and placeable, removes complete rows, and optionally converts occupied
cells to gray. Any cells or colored pieces subsequently added to that generated
page are encoded as an exact residual. A failed prediction affects compression
only; it cannot change the decoded page.

Older field references use a bounded recent window, while an exact-field
dictionary can reference an older identical field. Horizontal prediction also
mirrors J/L and S/Z colors.

## Revision 3 Shared Field

Revision 3 stores one non-empty shared field before the temporal page records:

```text
shared field height   UVar
shared field cells    revision 2 cell encoding
```

The encoder constructs this predictor from cells whose exact color is identical
on every page. This represents the unchanged starting field once when PC,
setup, build, damage, or spin results contain many different placements on the
same board. Each page then stores only its exact residual against field
predictor 10. The common field is a compression hint rather than gameplay
metadata: page reconstruction is still exact, and revision 3 is emitted only
when its complete text is shorter than the revision 1 and revision 2
alternatives.

## Cell Residual Encoding

Revision 1 uses a 3-bit cell mode. Revision 2 uses four bits and evaluates both
the revision 1 representations and additional predictor residuals.

```text
0   palette indexes
1   color runs
2   sparse previous/predictor delta
3   occupancy mask plus occupied colors
4   combinatorial multiset ranks
5   tetromino-color ranks
6   exact predictor copy
7   empty field
8   changed-cell mask plus replacement palette
9   consecutive changed runs plus replacement palette
10  changed-cell mask with one replacement color
11  combinatorial changed-position rank plus replacement palette
```

Palette encoding is:

```text
color mask           9 bits
palette indexes      ceil(log2(palette size)) bits per cell
```

Palette order is ascending color-code order.

Color runs are:

```text
run count            UVar
for each run:
  color              4 bits
  run length minus 1 UVar
```

Sparse deltas are:

```text
change count         UVar
for each change:
  index gap          UVar
  replacement color 4 bits
```

The first gap is measured from index `-1`. Missing cells in the previous page
are empty. A page height decrease implicitly removes all cells above the new
height.

Occupancy and change masks each choose between one raw bit per cell and
alternating run lengths. Combinatorial modes store a lexicographic combination
rank and reject ranks outside the exact combination domain.

Predictor-independent palette, run, occupancy, and multiset encodings are
calculated once per page. Predictor candidates evaluate only residual modes,
because repeating an independent encoding behind a predictor can never beat
the independent frame with the same cell payload.

## Text Transports

The packed bytes are followed by a big-endian CRC-16/CCITT-FALSE checksum with
initial value `0xFFFF` and polynomial `0x1021`.

The legacy compact `ctk3@` transport converts the resulting bytes with the
following 85-character alphabet:

```text
0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#
```

Every four bytes form one unsigned big-endian 32-bit value and are written as
five base-85 digits. A final one-, two-, or three-byte group is written as two,
three, or four fixed-width digits respectively. A text payload whose length
modulo five is one is invalid.

Compression happens in the bitstream; CTK85 only provides a compact printable
transport. It remains decode-compatible but is no longer emitted.

CTK85 deliberately uses URL-reserved printable characters to keep copied
values short. A CTK3 value embedded in a URL must therefore be percent-encoded;
plain clipboard and text-file values remain unchanged.

The `ctk3_` transport uses unpadded Base64url:

```text
ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_
```

Standard Base64 has the same density but introduces URL-reserved `+`, `/`, and
padding, so encoders always use unpadded Base64url instead. Relative to CTK85,
the asymptotic text-length increase is about 6.7%; fixing one transport avoids
double encoding and comparison on every copy operation.

### Large Page Bundles

One CTK3 segment contains at most 4,096 pages. Large solution exports use the
URL-safe `ctk3b_` envelope:

```text
ctk3b_<ctk64 payload>.<ctk64 payload>...
```

Each payload is an independently checksummed revision-1 CTK3 document. The
decoder validates segments in order, requires the same board width throughout,
and returns one ordered page list. A bundle may contain up to 1,048,576 pages.
No partial document is returned when a segment fails.

The segmented form lets solution exports encode independent batches in
parallel and release each worker's temporary page data immediately. Bulk copy
uses revision 1 directly instead of constructing revision 1, temporal, and
shared-field candidates simultaneously; ordinary documents still compare all
applicable revisions and emit the shortest form.

## Fumen Mapping

CTK3 preserves the field colors, page order, comments, operation, garbage row,
and page flags exposed by Fumen v115. A Fumen quiz is identified by its quiz
flag and `#Q=` comment convention. Fumen export is limited to 10 columns and 23
visible rows; CTK3 supports Clearra's 24-row editor without that limitation.

Decoders must reject unsupported revisions, invalid indexes, overlong runs,
non-zero trailing bits, and checksum mismatches. They must not return a partial
document after an error.

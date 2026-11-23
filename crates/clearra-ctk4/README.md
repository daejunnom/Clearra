# Experimental CTK4 ordered-ZDD family dialect

This crate stores a finite family of sets as a shared ordered ZDD. It is an
experimental solution-family container, **not a CTK3 revision** and not a public
npm format. The codec has no Tetris, queue, Hold, rotation, kick, score, or
probability rules. Variable descriptors and application metadata are opaque
bytes. Geometry Append/Union/Product graphs are not inputs to this codec.

## What is preserved

An archive contains a variable dictionary, named roots, and one shared node
table. The three symbolic result roots can therefore share their original
nodes while retaining identity/source correlation. No complete member of any
root needs to be enumerated during storage or validation.

Terminal `0` denotes the empty family. Terminal `1` denotes the family containing
the empty set. For a nonterminal `n = (v, low, high)`:

```text
family(n) = family(low) union {S union {v} : S in family(high)}
count(n) = count(low) + count(high)
```

Every child ID is smaller than its parent ID. Every nonterminal child variable
is strictly greater than its parent variable. The high child is nonempty.
These conditions reject cycles and give disjoint low/high branches, so the
checked count recurrence counts distinct sets. Equal low/high children remain
valid: eliminating them would apply a BDD rule with different semantics.

The codec permits duplicate structural node records. It does not promise a
globally minimal or canonical numbering. The manifest digest identifies an
exact artifact; it is neither a CTS1 digest nor a proof that differently
numbered archives denote different families.

## Format

All integers use explicit little-endian encoding; no Rust structure layout is
written to disk. Counts are exact `u128`. Overflow is an error, never saturation.

The first 64 bytes contain:

| Offset | Field |
| ---: | --- |
| 0 | `CTK4ZDD\0`, eight bytes |
| 8 | Dialect revision `u16`, currently 1 |
| 10 | Flags `u16`, currently 0 |
| 12 | Variable count `u32` |
| 16 | Nonterminal count `u64`; IDs begin at 2 |
| 24 | Named-root count `u32` |
| 28 | Block-directory count `u32` |
| 32 | Opaque metadata length `u32` |
| 36 | Node record size `u16`, currently 36 |
| 38 | Reserved `u16`, must be zero |
| 40 | Manifest byte length `u64`, excluding its digest |
| 48 | Complete archive byte length `u64` |
| 56 | Reserved `u64`, must be zero |

The remainder of the manifest contains metadata, roots, and block descriptors.
A root is a nonempty length-prefixed UTF-8 name, a `u64` node ID, and a declared
`u128` count. Names are unique. Each 60-byte block descriptor contains a kind,
three zero reserved bytes, first ID, item count, byte offset, byte length, and
SHA-256 payload digest. Atom blocks precede node blocks, cover their complete
ID ranges without gaps, and occupy contiguous nonoverlapping byte ranges.

The manifest is followed by its 32-byte SHA-256 digest and then the blocks.
Unknown flags, reserved bits, trailing bytes, missing ranges, invalid lengths,
and out-of-range references are rejected.

Atom block entries are a `u32` byte length followed by uninterpreted bytes.
Node records are `(variable:u32, low:u64, high:u64, count:u128)`. Blocks are
independently checked, allowing a reader to load a requested range without
decoding unrelated blocks. This first dialect uses uncompressed block bytes;
it does not promise an optimal compressed size.

## Validation authority

`UnverifiedArchive::open` reads only the header and manifest. The checksum binds
the directory and declared counts but **does not prove those counts**. This
type has no exact count, membership, or paging method.

There are two ways to obtain a `VerifiedArchive`:

1. `verify_all` scans all atom and node blocks, checks references and variable
   order, checks every exact count recurrence and each root count, and rejects
   any malformed or incomplete archive. It uses a bounded block cache and does
   not enumerate family members.
2. `accept_producer_seal` accepts an in-process `ProducerSeal` previously issued
   by the encoder or a successful strict scan. The seal must match the exact
   manifest digest and archive length. It has no public constructor or
   deserialization API. A checksum copied from a document cannot create it.

`write_archive` writes a frozen producer snapshot, then scans the **actual
serialized graph** before returning its seal. This prevents an inconsistent
source implementation from supplying false child counts to the writer. The
validation is a second graph pass; it is not constant-time and is not included
in any claim about solver performance. Both passes operate on nodes rather
than complete solutions. The source may reuse its existing count cache.

This seal is not a persistent signature. A reopened artifact in a new process
must be scanned unless an application has a separate authenticated producer
receipt and implements that authority boundary. The crate intentionally does
not manufacture such an authority from an untrusted header or transport URL.
Solver completeness, gameplay legality, and the meaning of opaque metadata
remain the application's responsibility.

## Lazy loading and ownership

The host implements `ReadAt`. The reader requests a bounded byte range and
verifies its SHA-256 before decoding that block. `SeekReadAt` adapts native
readable/seekable storage; a slice implementation supports already-owned bytes.
The codec does not open files, choose URLs, start background tasks, or own a
network session. A host must keep the source length stable.

A verified reader opened with a matching producer seal fetches no node or atom
block until a query needs it. `page(root, offset, limit)` skips counted branches
and returns only the requested sets. `contains` follows one ordered membership
path. `atom` fetches the dictionary block needed for the requested variable.
The page order is low-before-high in the archived variable order; it is not
legacy CTK1 lexical order. Page cursors should bind the manifest digest, root,
and `u128` rank so that another archive cannot reuse them accidentally.

The pager schedules only children intersecting the requested rank window.
Unrequested high siblings are not retained while following a long low chain.
Every pending branch contributes an output member, so pending traversal frames
are bounded by the requested page size; selected prefixes also respect the
page-variable limit. Metrics expose the peak number of live traversal frames.

Block ownership is local to one reader. Cached bytes are immutable owned
buffers; no reference to an encoder arena or its compaction epoch survives.
Limits govern manifest, block, atom, cache, page, and work representation.
Cancellation and work exhaustion return errors rather than partial success.
Cache size is distinct from peak RSS: the manifest, the in-flight block,
traversal stack, requested output, allocator metadata, and a provider's buffers
also occupy memory. Metrics expose fetched bytes, block fetches, hits, retained
cache bytes, and an uncached block buffer.

`ReadAt` is a synchronous range-provider boundary. A browser/network async
adapter and UI integration are separate work; this crate does not claim to
have shipped those paths.

## Relationship to CTK3

CTK3 stores ordered colored pages and can lazily decode already-encoded
segments. It cannot retain a ZDD's alternatives or an identity/source joint
relation. Existing CTK3 readers also receive the complete encoded string.

An application can use the CTK4 pager to select a bounded identity page, resolve
only the selected placement atoms, and encode that requested view as CTK3.
That conversion is a display projection. A colored field alone need not
preserve distinct partitions made of adjacent pieces of the same color.
The CTK4 catalog atoms and selected row sets remain the exact identity owner.

The archive writer never calls a CTK3 encoder, iterates all solutions, or
converts an arbitrary geometry-product DAG into a ZDD. A request to export all
CTK3 pages would still have output-sized work and the CTK3 page limits.

## Native integration outline

```rust,ignore
let seal = write_archive(
    &mut frozen_zdd_source,
    &named_roots,
    &opaque_application_metadata,
    &mut temporary_readable_file,
    EncodeOptions::default(),
    limits,
    OperationControl::default(),
)?;
// The host flushes/syncs and atomically publishes its owned temporary file.

let raw = UnverifiedArchive::open(range_source, limits, control)?;
let mut reader = raw.accept_producer_seal(&seal)?;
let page = reader.page(identity_root_index, requested_rank, requested_size, control)?;
```

The executor adapter needs a stable dense node-ID view and existing exact node
counts. Its application metadata must bind the board/catalog mapping and, if
claimed, the original source/request provenance. A caller-supplied unrelated
search request must not acquire authority merely by being attached to an
archive whose set graph passed codec validation.

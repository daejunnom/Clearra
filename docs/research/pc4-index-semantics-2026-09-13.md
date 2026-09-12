# PC4 index semantics recovered from source and bounded bytes

This records research evidence, not production qualification. No external source
code was copied, no complete graph/index was downloaded, and no profile was
activated. The HF revision below is an observation identity, not a product pin.

## Sources and meanings

- [Hydra reader](https://github.com/muse918/hydra-optimal/blob/856b67b079ea3e6d2648eb4f4e03025c226130f6/src/graph.rs)
  and its README describe sorted 40-bit occupancy fields and graph edges.
- [ZXCL reader](https://github.com/muse918/zxcl-pc/blob/1f9dffa691b7a519f46028f5d56728873f406ecf/src/graph.rs)
  decodes headerless graph records: u40 BE occupancy, then IJLOSTZ groups of
  u8 degree and u24 LE target IDs. Its in-memory offsets index the decoded
  target array; they are NOT the byte offsets in the HF helper file.
- [ZXCL data documentation](https://github.com/muse918/zxcl-pc/blob/1f9dffa691b7a519f46028f5d56728873f406ecf/docs/DATA.md)
  separates 15,185,706 graph fields from 817,740 layer-0 states. The latter are
  not the cardinality of the graph hash index and are not needed by this runtime.
- Clearra `manifest.rs` and `lookup.rs` in `clearra-pc4-tablebase` implement
  the helper-container contract verified below. Graph payload qualification is
  separate from helper decoding.

Here hash means a reversible 10x4 occupancy bitmap, not a cryptographic hash.
Restoring cells is bit unpacking; hash-to-ID maps that bitmap to a dense graph
node ID. Upstream's cleared-row normalization (cleared rows at the bottom)
must still be reconciled with Clearra coordinates/ILC; raw CTK integers are not
automatically identical. Node count alone does not prove every represented
state can reach a PC under every requested queue or rule profile.

## Observed helper layout

HF resolved revision: `ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`.

| File | Layout | Observed size |
| --- | --- | ---: |
| field_hash_to_id.v1.bin | 16-byte FHIDIDX1/version/count header, then N records of u40 LE bitmap + u24 LE ID | 121,485,664 |
| graph_offsets.u32.bin | 16-byte GOFFIDX1/version/count header, then N+1 u32 LE byte offsets | 60,742,844 |

Both version fields are 1 and both counts are 15,185,706. Size identities are
`16 + 8*N` and `16 + 4*(N+1)`. The first bitmap/ID entries are
`(0,0), (15,1), (30,2), (60,3), (120,4), (240,5)`.
Index bitmaps are little endian while graph record bitmaps are big endian.

To read field ID i, fetch offsets i and i+1 and request the half-open graph
byte range [start,end). ID 1 is [498,912), or HTTP bytes 498-911, 414 bytes.
The terminal sentinel is 510,917,451, exactly the canonical graph file size.
This avoids scanning preceding variable-length records; finding an unknown
bitmap ID with the current sorted-index reader is binary search, not O(1).

## Profile incompatibility is demonstrated, not merely suspected

| Graph | File bytes | Shared offsets at IDs 100 / 10,000 / 1,000,000 |
| --- | ---: | --- |
| graph.bin | 510,917,451 | all three match the indexed bitmap |
| graph_no180.bin | 476,562,450 | all three mismatch |
| graph_nokick.bin | 281,435,322 | all three mismatch |
| graph_srsplus.bin | 512,560,914 | all three mismatch |
| graph_srsx.bin | 825,119,692 | all three mismatch |

For example ID 100 expects bitmap 10,239 at byte 31,695. The four variants
instead yield 2,132,934,681; 34,370,160,640; 358,666,797,056; and
373,846,734,763 at that offset. These are bytes at incorrect record positions,
not assertions about those variants' actual field-100 states. The shared EOF
also differs from every variant. Canonical offsets cannot serve these files.
Empty-field adjacency target IDs also differ between variants; shared field-ID
ordering must be independently established, not inferred from matching prefixes.

SRS-X parsed with 4-byte LE targets yields four consecutive records with
bitmaps 0,15,30,60 at offsets 0,660,1208,1748 (end 2276). Three-byte decoding
instead produces unsorted/inconsistent following bitmaps and degrees. This is
strong sampled evidence for u32 targets, not a complete SRS-X format proof.
Canonical graph uses u24 targets and its first record ends at byte 498.

User mapping `no180 = SRS` is the intended profile identity; no-kick, SRS+ and
SRS-X retain their named identities. The canonical Jstris mapping and all exact
kick arrays still require source/edge differential evidence for these actual
artifacts. A generic gameplay implementation is not provenance for each upload.

## Reproducibility and remaining work

Local-only independent probes and JSON evidence are in
`C:/Users/강민수/AppData/Local/Clearra/reports/pc4-index-semantics-20260913/`:
`probe.mjs`, `result.json`, `crosscheck.mjs`, `crosscheck-result.json`.
They required HTTP 206, exact Content-Range and bounded streamed bodies,
aborted a full-body 200 response, and used per-request timeouts. Total binary
payload read was **28,939 bytes** (20,624 + 8,315); metadata is additional.

Continue without waiting for a reply: reconstruct normalization, verify
source/piece/target transitions against the local exact solver, establish
per-profile index identity and complete-domain evidence, and qualify slots
independently. Canonical sample matches do not qualify its entire graph.
Do not fabricate missing variant indices or publish a Clearra-owned auxiliary
dataset. Missing evidence keeps only that profile disabled. V*, policy and
Krylov are outside the PC/Setup all-solution runtime.

## Implemented follow-up

The lookup state machine now rejects nonzero first offsets and terminal offsets
that end before the graph EOF, in addition to its existing beyond-EOF check.
It uses the already fetched pair and adds no requests. Typed format failures
discard the pending request before graph bytes are fetched. Interior pairs are
not thereby qualified: generation binding and decoded source-hash matching
remain necessary. Existing graph decoding already distinguishes u24/u32.

Managed `cargo test --locked --offline -p clearra-pc4-tablebase --lib` passed
**149 tests**, zero failures, on this follow-up (0.05s test runtime). The two new
tests cover in-bounds wrong first/last offsets for u24/u32 profiles and the
valid single-record file satisfying both boundaries. This is synthetic reader
regression evidence, not whole-dataset or release acceptance.

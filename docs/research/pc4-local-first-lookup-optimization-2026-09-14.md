# PC4 local-first large-search lookup optimization

## Scope and authority

The user explicitly authorized a separate local dataset directory, with local
optimization before HTTP optimization. Restart/fallback-path verification is
deferred. Profiles remain separate; this experiment downloads **Jstris 180 only**.
No policy/V*/Krylov data, remote deployment, main merge or release authority is
introduced. The large reference remains empty field / 4L / P7P4 / Jstris 180,
with the user-reported **456,459 complete solutions**. That total is an acceptance
reference, not an enumeration cap or evidence that the current run completed.

## Explicit local data

- Directory: `%LOCALAPPDATA%/Clearra/tablebase-benchmark/jstris-180`.
- Revision resolved from HF: `ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`.
- Three files: `field_hash_to_id.v1.bin` 121,485,664 B,
  `graph_offsets.u32.bin` 60,742,844 B, `graph.bin` 510,917,451 B.
- Total: **693,145,959 B**. Streamed once; exact size and dynamically discovered
  SHA-256 verified before atomic publication. No hardcoded upstream digest.
- One generation, outside build storage, no copied WASM/native binary or
  implicit GUI/CLI installation. The existing dataset is reused on later probes.
- `scripts/benchmark/pc4-local-dataset.mjs --download --profile jstris-180
  --directory ABSOLUTE_DIRECTORY` uses the shared explicit downloader. Refuses
  to overwrite an existing active benchmark generation. Cleanup removes only
  exclusively created transaction files, never a recursive workspace directory.

## Initial measured prefix (not a completed search)

Real App/WASM source `0932df948e00fcaecd309b06d7ef45fd8ee8badb`, WASM
`9f416207804aa10660853f8562ebaa5a9967eae3ab37c4a2a6d9d1b292700430`.
Node 24.16.0, one WASM runtime, module compilation excluded. Command:

```text
clearra pc --board-mask 0 --height 4 --pieces 10 --lines 4 --count unique --patterns P7P4 --rule jstris-180 --tablebase
```

Every comparison below stops at the **same 30,000 logical reads**, with 30,628
App advances. It is a bounded measurement prefix, not a claim of complete
solution enumeration or end-to-end speedup. Each produced the identical demand
and returned-byte SHA-256:
`76d9a53a36d9c49eb8ae4022df5ef2df437139c30d6361eaebbd4b4ead9fd76f`.

| Reader | Elapsed | Physical reads | Local bytes | I/O time | App compute |
|---|---:|---:|---:|---:|---:|
| Exact-file baseline, warmed OS cache | 8.192 s | 30,000 | 610,111 | 1.313 s | 6.089 s |
| All-file 64 KiB pages, rejected | 8.677 s | 13,033 | 854,072,905 | 1.913 s | 6.060 s |
| All-file 4 KiB pages, rejected | 8.550 s | 14,993 | 61,407,246 | 1.367 s | 6.437 s |
| 4 KiB index pages + exact graph records | 7.748 s | 15,026 | 20,959,851 | 1.077 s | 5.981 s |

These are single samples, not a statistically established performance claim.
An earlier OS-colder baseline needed 20.0 s to reach 29,138 reads (9.434 s I/O),
so comparing only that sample with a warm page-cache run would exaggerate gains.
Its probe initially tried to advance after cancelling a retired job; that
benchmark-owner bug was corrected. It was not a product failure.

A larger prefix, with the same **old** WASM and the index-only local reader,
reached **100,000 logical reads** in **108.219 s**: 45,551 file reads,
51,329,915 local bytes, 99.384 s App compute, 5.367 s I/O, 2.901 s ABI bridge,
47,579,136 B WASM memory. Transcript digest:
`8ec35bc3f553df460923dd4b027b2be4ed526a62eaa752134dd42d51948283cb`.
Thus removing HTTP exposes a substantial local scaling problem too. This is
the baseline for the new graph-cache indexes, not a timing of those indexes.
Both prefix runs stop intentionally and remain non-completion evidence.

The prefix contains 2 field-index reads, 19,999 offset reads and 9,999 graph
reads. Large graph pages create substantial over-read; they are not enabled in
the local product reader. Remote transport policy is not widened by these tests.

## Implemented changes awaiting exact-source Rust CI

1. App decoded graph cache: indexed field-ID and reverse-hash lookups, replacing
   full-vector scans. Admitting R records previously needed quadratic duplicate
   checks, with additional linear scans for adjacency/materialization. Hash
   indexes are used only for lookup; they never determine output order. All
   owners reserve capacity before publication. Conflict, generation, profile,
   terminal and memory-budget guards are unchanged. A 20,000-record scrambled
   insertion/idempotency/conflict test protects both indexes.
2. GUI local reader: bounded 4 KiB index pages (8 MiB aggregate), exact graph
   slices, same typed local admission and shared file lease. Counters distinguish
   logical requests, physical local reads, local bytes, HTTP bytes and cache hits.
3. Native local reader: same index-only paging principle, 2 MiB bound per index
   file, exact graph slices. Generic in-memory tests exercise actual read counts,
   file boundaries, truncation and eviction without disk fixtures.
4. Local-only WASM probe: exact artifact SHA verification, dynamic dataset
   identity, configurable time/read stop, and transcript byte digest. No server
   or port is created. Rust compilation/execution remains in non-publishing CI;
   local UMCI restrictions are not bypassed.

Related JS download, reader, OPFS lease and workflow tests passed locally.
Rust/CLI correctness and the graph-cache algorithm's actual speedup require the
new exact-source CI/WASM. The 4194 WASM has **not** been replaced for these edits.

Implementation commit: `a22dbfffa8bfc65aee6c482a2f989a8023a8c306`.
Non-publishing CI: https://github.com/daejunnom/Clearra/actions/runs/34766442189 .
Source and surface contracts passed at the last observation; Rust, native CLI
and preview WASM were still running. A later host-only follow-up carries the
physical file-read counter through the development receipt and bounds direct
reads as well as cached reads. It does not require another Rust/WASM build.

## Next online stage / remaining evidence

After the local baseline is established, reduce **actual HTTP transactions**,
not just logical reads: carry a bounded known-demand frontier into index/record
batch planning, keep immutable revision/profile admission, and avoid rereading
known metadata. Existing 64 MiB request-budget and 206/Content-Range validation
must not be relaxed to hide a costly query. All-file expansion, background scans
and implicit full downloads remain forbidden. Installed matching-profile data
must remain distinguishable from Range transport.

Still required: complete 456,459-family validation/timing, input/hold-family
scale tests, native/browser parity after these edits, online transaction-count
A/B, independent profile qualification, remaining product contracts and release
acceptance. Do not mark v0.9.0 or the overall goal complete from these prefix tests.

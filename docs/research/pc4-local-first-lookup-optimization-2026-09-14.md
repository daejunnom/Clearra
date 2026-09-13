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

## Implemented local reader and graph-index changes

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

Implementation commit: `a22dbfffa8bfc65aee6c482a2f989a8023a8c306`.
Non-publishing CI https://github.com/daejunnom/Clearra/actions/runs/34766442189
finished with all five jobs successful: source, native CLI, PC4 contracts,
surface contracts, preview WASM. Nonzero evidence includes 91 App tests,
17 native PC4 tests and 2 public WASM-host tests. This is not release acceptance.
The host-only `ec4ab5c` follow-up exposes physical local file reads and bounds
direct pending reads without changing compiler inputs.

Verified a22 WASM: `f0f09bc8d33d530925d44a98cdc81d19866b6c60cf0d5440f850495b4ccf78e2`,
21,488,190 B; source fingerprint
`9c014be368958e894e20767fd6d98a0a832a166b41f5a7594a76d1e6aee6e2cc`.
CI artifact 10321017440, run 34766442189, attempt 1. Exact bytes were published
to 4194 after validation, with no local Rust build or restamping. Four product
generations were retained; the temporary artifact extraction was removed.

A live-development guard previously rejected this artifact after a host/docs-only
descendant commit. It now permits an exact Git ancestor only when the **entire
current compiler fingerprint** still matches, preserving original pins. Changed
Rust inputs and unrelated commits remain rejected; production pin rules are
unchanged. The idle owned 4194 server was restarted without a visible console;
its accepted-generation endpoint and manifest both reported the new hash.
Subsequent Rust edits below need their own CI artifact before publication.

## Large-prefix A/B after graph indexing

The same 100,000 logical reads / 101,696 App advances now take **19.738 s**
(12.379 s compute, 5.251 s I/O, 1.744 s bridge), versus 108.219 s before.
Physical reads and bytes remain exactly **45,551 / 51,329,915**. WASM memory
was 47,906,816 B. A diagnostic repeat took 20.240 s; these remain single local
observations with OS-I/O variation, not a universal speedup or completed search.

Raw ordered demand hashes initially differed. Publication was held until an
exact trace comparison showed four differing positions: two independent
offset/graph record queries exchanged order at ordinals 10009/10010 and
10012/10013. No demand was lost or added. After normalizing only this bounded
recorded prefix, **all 100,000 requests and returned bytes** reproduce the old
digest `8ec35bc3f553df460923dd4b027b2be4ed526a62eaa752134dd42d51948283cb`.
The following unrecorded suffix remains order-sensitive. The comparison retains
the raw hash and reports the trace-coverage boundary; it never sorts away an
arbitrary mismatch. One 362,750 B local trace is retained and reused, not replaced.

The unrestricted-read 120-second probe still did **not** complete. At cancellation:
150,656 logical reads, 50,218 graph records, 67,984 local file reads,
74,601,189 local bytes, 104.855 s compute, 11.051 s I/O, and 425,721,856 B WASM
memory. Progress sometimes consumed many seconds with no new byte demand.
No 456,459-family or full-run speed claim is justified by this evidence.

## Subsequent local accumulator correction

Source inspection found repeated old/old reveal-evidence comparisons on each
small transaction, and full-prefix sorts even for empty or monotonically growing
ledger pages. The old/old invariant has already been checked at atomic commit.
The new check covers only new/old, old/new and new/new intersections; within-side
duplicate/conflict validation and the complete finalization check remain intact.
Sorted delta append skips sorting only for empty or already ordered boundaries;
out-of-order graph memberships still use the exact sort. Allocation/budget,
cancellation and publication contracts are unchanged.

Tests exercise all three new-conflict intersections and 1,000 ascending pages
(64,000 ranks): only 1,998 boundary-key evaluations, rather than repeatedly
scanning accumulated keys. This follow-up requires exact-source Rust CI and a
new WASM measurement; it is not included in the a22 timing above.

## Online transport correction, using the saved real trace

`run-pc4-range-trace.mjs` replays the saved 30,000 real demands through the product
HTTP reader with exact bytes from the verified local dataset as the response
source. It performs **zero network calls, search reruns or builds**. These are
HTTP transaction-policy counts, not internet latency or full-search timings.

| Policy | Completed logical reads | HTTP transactions modeled | Transferred bytes modeled |
|---|---:|---:|---:|
| Exact spans, existing bounded cache | 30,000 | 19,999 | 449,621 |
| Uniform 512 B windows | 30,000 | 19,275 | 9,868,295 |
| Uniform 2 KiB windows | 30,000 | 16,576 | 33,944,771 |
| Uniform 4 KiB windows | 30,000 | 14,993 | 61,407,246 |
| Previous uniform 16 KiB windows | **8,325; transfer limit** | 4,096 | 67,098,631 |
| **4 KiB index pages + exact graph records** | **30,000** | **15,026** | **20,959,851** |

Every completed row matches the original 30k returned-byte digest. The selected
policy reduces requests by 24.9% versus exact spans; compared with uniform 4 KiB
it saves 65.9% of bytes for 33 extra transactions. The former default cannot be
compared as a completed result: it fails at less than a third of the prefix.

The GUI and native CLI now select index-only paging. One-shot raw graph records
do not evict reusable index pages because the App already owns their decoded
records. Qualification's separate bounded explicit-batch reader is unchanged.
The GUI policy selects the exact ready profile; unsupported profiles cannot
borrow another profile's graph. Transfer/request/memory bounds, immutable
revision, real 206/Content-Range checks, cancellation and no implicit retry are
preserved. Local installed readers still perform zero HTTP requests.
The focused JS transport/OPFS/host/comparison suite passed **28 tests** locally;
40 additional qualification/download/workflow/artifact-guard checks passed.
Follow-up implementation is `3ceca8b51c9d6e54a837bc14fa5a6f17bbc8bcba`;
non-publishing CI https://github.com/daejunnom/Clearra/actions/runs/34769318668 .
At this checkpoint source, surface and native CLI jobs succeeded; PC4 contracts
and preview WASM are still running. This exact-source checkpoint supersedes no
release evidence, and the new accumulator timing has not yet been measured.

## Next online stage / remaining evidence

Further reduce **actual HTTP transactions**, not just logical reads: carry a
bounded known-demand frontier into index/record
batch planning, keep immutable revision/profile admission, and avoid rereading
known metadata. Existing 64 MiB request-budget and 206/Content-Range validation
must not be relaxed to hide a costly query. All-file expansion, background scans
and implicit full downloads remain forbidden. Installed matching-profile data
must remain distinguishable from Range transport.

Still required: complete 456,459-family validation/timing, input/hold-family
scale tests, native/browser parity after these edits, real HTTP latency and
frontier-batch A/B, independent profile qualification, remaining product contracts and release
acceptance. Do not mark v0.9.0 or the overall goal complete from these prefix tests.

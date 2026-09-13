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
scanning accumulated keys. This correction passed the 93-test App selection in
the follow-up CI below. It is not included in the a22 timing above.

A new 120-second probe of the exact `3ceca8b` CI artifact still did not complete:
153,776 logical reads, 51,258 graph records, 69,429 file calls, 76,279,410 local
bytes, 105.205 s compute, 10.766 s I/O, 3.130 s bridge and 539,557,888 B WASM
memory. WASM SHA256 `139ebc8d9286b7ca241271ef216db39a12341b692bea1c3fe598efa8682b26b0`;
artifact 10321905713 / run 34769318668 / attempt 1. This is only a small amount
of additional progress versus a22 at the same probe cap, not a demonstrated
end-to-end performance breakthrough. The artifact was used for measurement,
not published to 4194. No 456,459-family completion claim is made.

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
Source, surface and native CLI jobs succeeded. PC4 contracts found one native
unit-test failure: the old tiny fixture expected three whole-file windows,
where the new partial policy made 22 requests. App 93 and public WASM-host 2
tests passed. The failure also exposed a real cache omission: exact reads of
small index files returned before cache admission. The follow-up fixes that
omission and replaces the obsolete count with partial-range bounds, no repeated
contained index fetches, and the original complete local/online solution parity
assertion. It does not simply change the expected count to 22. Fresh Rust CI
is required for this fix and the frontier implementation below.

## First bounded-frontier implementation (superseded transport policy)

On a missing adjacency record, the cooperative traversal now exposes at most
32 unique IDs from its already queued work, inspecting at most 128 pending
entries. A failed page remains uncommitted; the hint does not change DFS order,
suffix facts, candidates, or the next-page cursor. App filters cached, foreign,
and out-of-domain IDs and carries the hint with the existing pending lookup.

Both Web HTTP and native CLI adapters use two dependent byte stages: batched
GOFF pairs, then the graph spans those pairs specify. Only the actual GOFF-pair
phase starts read-ahead, after normal lookup header handling. Known spans merge
only across at most 1,024 gap bytes, up to 64 KiB per transfer. Cached sub-demands
are removed before merging to prevent overlapping frontiers from refetching
their old prefix. Web retains the existing four-HTTP-request concurrency bound;
native curl transfers remain serial but coalesce known spans before launching
processes. Native parallel curl execution is not claimed.

The same bounded 8 MiB / 2,048-entry transport cache retains explicit graph
batches until the exact lookup asks for them. A bucketed containing-span index
avoids scanning every cached Web record. Ordinary one-shot graph reads still
do not evict index pages. Local installed readers **do not prefetch** and make
zero HTTP calls. Profile/revision identity, exact Rust admission, 64 MiB total
transfer budget, cancellation, 206/Content-Range validation and no implicit
retry remain intact. All-file expansion and speculative full-graph scans remain
forbidden.

Initial focused JS transport/OPFS/host validation: **27 tests passed**. A synthetic
32-adjacent-record A/B used 64 serial HTTP requests versus **2 batch requests**
(132 offset bytes + 384 graph bytes), with every exact returned byte equal.
The host integration test separately consumed all 64 genuine pending requests
through normal admission while making those two fetches. This is not evidence
of a 32x whole-search or real-network speedup. New native/core tests require CI.

`run-pc4-local-wasm.mjs --transport http-model --frontier` is the next bounded
measurement path: real WASM-produced frontiers and product HTTP transport with
exact local dataset responses, no network or alternate port. The comparison
without `--frontier` measures the same artifact/input. Old traces contain no
frontier hints and cannot establish this implementation's large-case savings;
future trace reads must not be treated as already known work.

`7601eeda915c926e980651147767d32467c899f1` passed all five non-publishing jobs:
https://github.com/daejunnom/Clearra/actions/runs/34771370952 . Nonzero Rust
evidence includes TB 186, App 94, native TB 22, public WASM host 2, native
process E2E 17 and native contract selection 19. This proves the contracts,
not a speed benefit for the eager transport policy above. Only a tooling
`punycode` deprecation warning was observed in the selected logs; no new Rust
warning or failed selected test was found.

## Real-frontier A/B and final request-conservative policy

The exact 7601 WASM was read from CI artifact 10321957728, run 34771370952,
attempt 1. ZIP digest `61ca9c58a47e8c5bc25187781f66410cb0b4ecf22d11cd26cf178a670fbcceb4`;
WASM digest `bef0f56d12aed2cb0dca349a401cf04d3f99f4195a83dffb118e622cb7b49930`,
21,496,038 B. The benchmark received the checked ZIP entries **in memory**,
without an extra experimental build directory or a 4194 publication. The
read-only helper accepts only the exact integration-preview source/artifact,
bounds the archive, verifies its ZIP and entry SHA256 values, and never reads
credential files. A producer retention-history receipt is allowed but not used
as executable or release authority.

Same empty-board / 4L / Jstris180 / P7P4 command, same WASM, first 30,000 real
logical demands; only host transport policy changed:

| Host policy | Modeled HTTP requests | Modeled HTTP bytes | Decision |
|---|---:|---:|---|
| Index 4 KiB / exact graph, no frontier | 15,026 | 20,959,851 | Baseline |
| Eager exact-pair frontier (7601) | 19,857 | 555,140 | Reject: 32.2% more requests |
| Eager frontier with index pages restored | 16,165 | 25,810,796 | Reject: still more requests |
| Also release consumed exact graph spans | 15,040 | 21,075,795 | Reject: still no request benefit |
| Enlarge only the currently required graph transfer | 14,998 | 21,016,558 | Small benefit, not a broad speed claim |

All rows completed the identical 30k prefix, with the exact same raw ordered
returned-byte digest `c0bcc67bd28c5ba64f5a3200c808fde68f0cb01b44c2ac871a1545e8e78fc65b`.
The older saved trace still differs only by the already documented independent
record swap; its normalized returned-byte digest remains `76d9a53a...76f` above.

Two causes were corrected before accepting a transport policy: eager exact
offset pairs lost the existing index-page reuse, and consumed one-record graph
prefetches occupied entries that reusable index pages needed. More importantly,
most DFS hints are distant queued ancestors/siblings, not a new ready batch at
every missing descendant. Fetching all of them early gives little reduction
in sequential dependency round trips.

The final Web/native policy therefore:

1. Only the current offset-pair request may start index I/O, using ordinary
   index paging. Other hinted offsets must already be cached; otherwise skip.
2. Only a merged graph span containing the **currently required** record and
   at least one other distinct uncached known record may be fetched. Distant
   siblings cause no extra speculative index or graph requests.
3. Keep 64 KiB span / 1,024 B gap / 64 MiB total limits and exact Rust admission.
   Consumed exact one-record graph entries are released; containing multi-record
   spans retain their unread siblings under the existing LRU bound.
4. Reject disconnected optional hints before cache copying/planning. Each
   qualified record has at least 12 bytes, so a gap over 86 field IDs cannot
   bridge the 1,024 B byte-gap limit. Preserve the connected component containing
   the required ID, including chains at the 86-ID boundary. This is only an
   I/O-hint filter, never a PC pruning or completeness claim.

The final larger comparison used **100,000** identical logical demands and
101,696 App advances:

| Host policy | Modeled HTTP requests | Modeled HTTP bytes |
|---|---:|---:|
| No frontier | 45,551 | 51,329,915 |
| Required-transfer-only, before cheap ID filter | 45,219 | 51,832,903 |
| Final connected-ID filter | **45,220** | **51,832,638** |

The final policy saves **331 requests (0.73%)** for about **0.50 MB extra bytes**.
The one-request difference from the unfiltered variant reflects cache access/
retention order, not different logical graph requests. Full ordered returned-byte
digest matches across all three 100k runs:
`8074680fc8fcb67df3044e97156d44edcf3a6839943d123545c123eeb478b6e7`.
Normalizing only the recorded 30k swap reproduces the old 100k digest
`8ec35bc3...82cb`; the unrecorded suffix remains order-sensitive.

Model wall times were 61.383 / 75.633 / 41.121 seconds, with host I/O/planning
25.496 / 40.011 / 10.742 seconds. These use local responses, include OS/runtime
variation, and **are not internet latency or completed PC timings**. The ID
filter removes unnecessary cache-copy/planning work; a universal speed ratio
is not established. The large request-count benefit is modest. Explicit
matching-profile installation remains the path that eliminates all lookup HTTP.

Final local transport/host/OPFS tests: **31 passed**, including cached-only
lookahead, sparse frontiers, index seams, consumed spans, and 86/87-ID bounds.
The native mirror and new tests require the follow-up exact-source CI. No new
App traversal or WASM hint protocol was introduced after the successful 7601
build. The original 3ceca8 temporary extraction remains because its cleanup
command was denied; no workaround deletion or additional extracted build was
performed. All probes are finished, no benchmark port was started, and 4194
still has the earlier a22 artifact.

## Remaining evidence

Still required: complete 456,459-family validation/timing, input/hold-family
scale tests, native/browser parity after these edits, real HTTP latency and
completed-search frontier-batch A/B, independent profile qualification, remaining product contracts and release
acceptance. Do not mark v0.9.0 or the overall goal complete from these prefix tests.

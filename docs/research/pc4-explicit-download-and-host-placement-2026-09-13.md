# Explicit per-profile PC4 files and Discord host placement

This is an implementation/research checkpoint on the v0.9 integration branch,
not release acceptance, a full-dataset download, or a production server change.
It extends the existing plan and preserves previous incomplete performance
observations. The user withdrew the suspected duplicate minimum-set issue.

## Scope and independent profile choices

The requested download covers the selected kick table's complete graph and its
two indexes, not the approximately 778GB V*/policy dataset. CLI and GUI explicitly
offer these five names; no implicit all-profile download or alias is permitted.

| Choice | CLI ID | Graph | Current download availability |
| --- | --- | --- | --- |
| SRS | `srs` | `graph_no180.bin` | Unavailable: independent indexes/qualification absent |
| SRS+ | `srs-plus` | `graph_srsplus.bin` | Unavailable: independent indexes/qualification absent |
| SRS-X | `srs-x` | `graph_srsx.bin` | Unavailable: independent indexes/qualification absent |
| Jstris 180 | `jstris-180` | `graph.bin` | Supported declared-complete graph + qualified index format |
| No kick | `no-kick` | `graph_nokick.bin` | Unavailable: independent indexes/qualification absent |

Other graphs existing on HF does not authorize reuse of the canonical Jstris
indices. Activation remains per-profile. Whole-graph mathematical proof and
Clearra materialization parity remain distinct from bounded format samples.

The live public discovery/qualification observation resolved revision
`ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31` with 15,185,706 field records:

| Artifact | Bytes |
| --- | ---: |
| `field_hash_to_id.v1.bin` | 121,485,664 |
| `graph_offsets.u32.bin` | 60,742,844 |
| `graph.bin` | 510,917,451 |
| Total | **693,145,959** |

That is about 693MB decimal / 661MiB binary. Revision and sizes above are
observations, not compile-time constants. Every explicit update resolves fresh
metadata and pins its own immutable file identities for the transaction.

## Implemented ownership and lifecycle

- CLI: `tablebase check|download|status|remove --profile PROFILE`; optional
  `--directory DIRECTORY`, and `CLEARRA_PC4_DIRECTORY` selects the common native
  data base. All actions require a profile. The command has EN/KO/JA help.
- GUI: a folded download panel, independent profile selector, availability,
  size check, explicit start, cancel, status and delete. Changing profile clears
  the previous size/download selection. No full file request on opening it.
- Shared JS installer: exactly three complete-file GETs after explicit intent;
  bounded stream backpressure and incremental SHA-256, not a 693MB ArrayBuffer.
  Normal Range transport still rejects unexpected HTTP 200 bodies.
- Browser store: OPFS per origin/profile; exclusive update/delete versus shared
  search leases using Web Locks. Active generation + one unpublished staging
  generation. Publication occurs only after all sizes and hashes pass. The
  lock release waits for actual completion so immediate subsequent operations
  cannot incorrectly see the just-finished operation as busy.
- Native store: per-profile OS locks, streamed writes/hash checks, format
  qualification, atomic activation pointer, no recursive deletion of native
  filesystem roots. Symlinks/reparse points are rejected. A failed transaction
  cannot replace the previous active generation.
- Old-file cleanup failure after publication is separate from download failure:
  report new data saved / cleanup pending, not old data preserved. Later
  update/remove retries only that profile's managed generation cleanup.
- Browser download/hash work runs in a dedicated Worker. Native HTTPS transport
  uses system curl without a shell or curl configuration (`-q` first), no
  credential reads, HTTPS-only redirects, bounded bytes/time and hidden Windows
  process windows. The executable requires curl only for check/download.
- Search adapter: bounded local file reads/Blob slices feed the same pure
  reader and App/Core reducers through `verified-local-file` admission. It does
  not fabricate HTTP 206 provenance. Native/GUI input surfaces remain distinct.
  Local byte counters and HTTP request/body counters are separate.
- The v0.9 integration branch enables the new CLI local host by default. The
  released main/v0.8.1 artifacts are unchanged. Native online Range transport,
  Desktop local-storage UI and Discord routing remain independent work.

Downloaded TB use is not `offline-exact` fallback. Missing/corrupt/unqualified
data must not silently choose another kick table or another solver. Existing
partial-network lookup remains a separate optional mode; no runtime search may
trigger an implicit complete download.

## Format-aware request reduction

The graph stores a u40 **big-endian** source field hash before its seven
IJLOSTZ adjacency groups. FHID stores its five-byte hash **little-endian**;
qualified field IDs are sorted-record ordinals. GOFF maps ordinal to an adjacent
pair of u32 little-endian byte offsets. A qualified by-ID lookup can omit the
redundant FHID record and recover the hash from the graph prefix. Opaque formats
retain their old path; truncated graph prefixes are explicit format errors.

Known batch demands now coalesce within the same immutable artifact, bounded
to 512 demands, 64KiB spans and 1KiB default gaps (4KiB absolute maximum). The
qualification pass batches independent headers, then known index samples,
then record slices. A single pending graph traversal demand is not magically
made concurrent by this transport change.

Synthetic 320 x 8-byte clustered demands returned byte-identical data:

| Reader | HTTP requests | Body bytes |
| --- | ---: | ---: |
| Exact single ranges | 320 | 2,560 |
| 16KiB demand windows | 8 | 131,072 |
| Known-demand batches | 8 | 2,560 |

Live qualification used 15 Range requests / 2,628 body bytes in 6,189.7ms wall
time. Reader counters exclude metadata JSON requests/bodies; wall time includes
discovery. This is qualification only, not a completed PC search or benchmark
confidence interval. No complete 693MB download was run during implementation.

## Oracle versus Cloud Run

**Preferred next experiment: a persistent Oracle file store with an isolated,
resource-limited slice service.** Binary search/offset seek needs bounded byte
reads, not a 15-million-entry JavaScript object map. It must not execute graph
traversal on the Gateway Node event loop. Limit CPU, RSS, in-flight requests,
queue length, byte budgets and I/O priority independently.

This is conditional, not a claim that Oracle is currently sufficient. Compare
cold/warm data, update overlap and cancellation with the baseline Gateway
heartbeat, response p95/p99, CPU/RSS/IO and available disk. Tiny node reads are
different from complete graph-family enumeration, replay materialization and
minimum selection. A bounded lookup failure does not authorize silently
launching a heavy Cloud Run job for a claimed TB hit.

**A Cloud Run image can contain a separate read-only data layer.** The tracked
accepted-job Dockerfile already consumes an accepted CLI binary without Rust
compilation. A qualified data layer can follow the same packaging boundary;
data updates must not introduce another Rust build. Actual image/cold-start/RSS
cost still needs measuring and profile-selective data versions need provenance.

Avoid downloading all files into the default writable container filesystem on
every start: it is memory-backed and nonpersistent. File placement alone does
not make full enumeration cheap or prove CLI/Discord timing parity. See the
[Cloud Run container contract](https://docs.cloud.google.com/run/docs/container-contract).

No SSH session, Oracle file installation, Cloud Run image build, production
traffic change or whole-dataset download was performed for this checkpoint.

## Validation boundary and follow-up

Focused stream, hashing, per-profile lifecycle, cancellation, HTTP rejection
and typed local-host tests run in memory with synthetic files: **65 focused
Node tests passed**, the frontend no-emit TypeScript contract passed, and five
affected Svelte components compiled with zero warnings. Rust formatting/parser
validation passed; this does not replace compilation or runtime tests. Native Rust and
new WASM need the isolated non-publishing feature-branch CI; local security
policy is not bypassed. Preview WASM is not a release-authoritative artifact.
The currently loaded 4194 WASM must not be described as updated until exact
artifact replacement is recorded separately.

Remaining measured outcomes: real complete download on user-selected storage,
native/browser full-file query parity, all 456,459 reference solutions, P7P4
whole-family performance, native HTTP/desktop/Discord adapters, Oracle
non-interference and deployment gates. This change does not close them.

## Primary references

- [HF dataset](https://huggingface.co/datasets/muse918/tetris-4lpc-mdp-vstar-policy): public metadata, current per-generation file identities; MIT dataset declared by user/upstream.
- [Hydra optimal](https://github.com/muse918/hydra-optimal): graph format/order; GPL source inspected for format, not vendored into Clearra.
- [OPFS](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system): origin-bound storage and quota.
- [curl](https://curl.se/docs/manpage.html): transport configuration and download limits.

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
  process windows. The original lifecycle path requires curl for check/download;
  the native online continuation below also requires it for uncached Range reads.
- Search adapter: bounded local file reads/Blob slices feed the same pure
  reader and App/Core reducers through `verified-local-file` admission. It does
  not fabricate HTTP 206 provenance. Native/GUI input surfaces remain distinct.
  Local byte counters and HTTP request/body counters are separate.
- The v0.9 integration branch enables the new CLI local host by default. The
  released main/v0.8.1 artifacts are unchanged. Native online Range transport is
  connected in the continuation below; Desktop local-storage UI and Discord
  routing remain independent work.

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

The actual 4194 download-panel audit used a separate tab, preserving the user's
original tab and inputs. All five profile selections were visible. Jstris size
preparation displayed **661.0MiB**; switching through all four other profiles
cleared the old size and the download action. The temporary audit tab was closed.
No full-download button was pressed and no search was run.

First non-publishing CI `34760309467` compiled and passed the pure tablebase
tests (185 passed, two separate opt-in A/B tests also passed) but failed at the
App JSON dependency moved from WASM. The surface job exposed an idle Node
MessageChannel lifetime bug: an outstanding unreferenced yield could exit
before its promise resolved. Follow-up adds the App-only optional dependency,
references the port only while yields are pending, and adds a deterministic
isolated-process test with no timing sleeps. These are corrected-source checks,
not a claim that the failed CI passed. The new local host tests passed (8/8).

Follow-up CI `34760594684` at `5fbcc1d` passed source, native CLI, surface
contracts and preview WASM. Its PC4 job still failed: the native fixture used
legacy `--queue` (observed prefix) while expecting fixed-queue semantics.
Corrected fixtures explicitly use `--fixed`; observed input retains its
disclosure-required result and cannot be reinterpreted as an exhaustive queue.
That run also revealed that the WASM `online_pc4` filter executed zero tests
after configuration ownership moved to App. New synthetic public-worker tests
exercise local/HTTP admission, the same one-I completion, source separation,
stale requests and cancellation. The contract runner now rejects zero-test
success without invoking Cargo a second time. These follow-up changes require
their own CI result; the earlier preview is not their runtime evidence.

The worker test's first run (`34761460337`) caught an incorrect synthetic field
hash: Hydra reverses bits within each ten-cell row, not a column-major packing.
The fixture now uses the existing checked conversion and an independent
constant (`0xffbfeffbfe` Clearra -> `0x7fdff7fdff` Hydra); production conversion
was unchanged. The cancellation/source-separation test already passed. The
contract runner keeps independent test groups running after a failure (fetch
remains prerequisite) and preserves a failing final status. Both the shared
JavaScript download API and its type contract now require a profile explicitly;
even `intent=explicit-download` alone cannot default to Jstris or start I/O.

Remaining measured outcomes: real complete download on user-selected storage,
native/browser full-file query parity, all 456,459 reference solutions, P7P4
whole-family performance, native HTTP/desktop/Discord adapters, Oracle
non-interference and deployment gates. This change does not close them.

## Native online continuation (implementation checkpoint)

CI `34761889108` passed both real WASM worker boundary tests: local and HTTP
responses returned the same one-I solution and cancellation/stale/header fences
held. Its only PC4 failure was the legacy native fixture's empty 4L field with
one fixed piece: the request compiler correctly rejected the wrong area. That
fixture now supplies ten fixed pieces. Source, native process, surface and
preview-WASM jobs passed; the overall run remains failed, not accepted.

The next implementation connects the native CLI without requiring a complete
download. An explicit Jstris `--tablebase` request prefers its installed files;
only absence selects immutable HF Range. Corrupt local metadata/files, busy
leases and another profile cannot silently select network or offline search.
Unsupported profiles and observed-prefix disclosure requirements are checked
before network discovery. Full qualification and exact request/kick validation
still belong to their existing owners.

`tablebase_host_execution.rs` drives the same App for local and online hosts.
`tablebase_download_format.rs` now qualifies through one bounded reader callback
for both transports. `tablebase_http_range.rs` accepts only checked 206 replies
with exact Content-Range/body size, then caches at most 8 MiB in bounded windows;
individual reads are <=64 KiB and per-request transfer allowance is 64 MiB.
These are transfer limits, never partial-family completeness proofs. The cache
is in memory for one immutable generation and writes no dataset files.

The existing curl dependency supplies transport (7.84+ for final header receipt).
It is invoked with configuration disabled, HTTPS-only redirects, hidden windows,
bounded stdout and timeouts. It does not build a shell command or read credentials.
The [curl manual](https://curl.se/docs/manpage.html) explicitly notes that servers
may ignore Range and return whole content; that response is rejected here.
This subprocess transport is not yet a latency parity claim against a persistent
HTTP client. Window reuse avoids repeating a process for adjacent tiny reads;
large native search timing and actual cancellation readback remain to measure.

New synthetic tests cover the one-I complete set via native Range and installed
files, absence versus corruption, zero disk writes for Range, same-window reuse,
cross-artifact isolation, bad headers, whole responses, 429 and byte limits.
They require the next non-publishing Rust CI; formatting alone is not execution.

A separate transport-only check on this Windows host resolved the current
revision to `ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31` and used system curl for
exactly bytes 0-15 of FHID. It returned exit 0, HTTP 206,
`Content-Range: bytes 0-15/121485664` and `FHIDIDX1` (16 payload bytes).
This validates the curl final-header receipt on this host, not native App
execution, full-solution timing or a complete download. No Rust executable was
run through or around the local execution-policy restriction.

## CI follow-up

Run `34762937614` passed source, surface contracts and preview WASM, but both
native-related jobs failed to compile the shared host driver: the range length
is `u32`, whereas the transport boundary deliberately accepts `u64`. The fix is
the lossless `u64::from(range.length())` conversion, not a relaxed transport bound.
The run remains failed; the next exact-source CI must execute the native tests
before this path is considered verified. Its preview has not replaced 4194.

Run `34763603661` compiled and executed the native PC4 group: 12 of 13 tests
passed. The two complete-App tests had omitted the CLI's existing process-wide
execution-resource test guard and raced under `--test-threads=2`; local admission
in the parity test reported `shared_execution_resource_deferred`. Both now use
that guard, while the transport/store-only tests remain parallel. Production
admission limits and failure reporting are unchanged. The online response is
deliberately kept alive while the next local search runs, so the test still
checks release of execution resources between completed requests. The next CI
must confirm this fix; no passing result is inferred from the code change.

The same run passed both public WASM boundary tests, all 90 selected App tests,
185 pure tablebase tests and the independent materializer/replay groups. In the
4194 browser, all five download choices were inspected: four displayed their own
unavailable state, while only Jstris enabled preparation. No full download or
search was started, and the temporary audit tab was closed.

## Exact-source CI and 4194 readback (2026-09-14 KST)

Non-publishing run `34764005219` for
`0932df948e00fcaecd309b06d7ef45fd8ee8badb` passed all five jobs: source,
surface-contracts, native-cli, pc4-contracts and preview-wasm. The native PC4
group executed **13 tests, all passed**. The two public WASM boundary tests
passed too. This confirms the resource-guard correction without changing the
production resource authority or discarding the preceding completed response.
The independent exact-source import checks passed 15 tests; live-generation
guard tests passed 2 tests.

Preview artifact `10319594515` (run 34764005219, attempt 1) was downloaded into
one transient directory inside this source's fixed experimental build slot.
GitHub metadata bound the artifact to that source/run/attempt; the importer
checked the current compile-input fingerprint and the original five artifact
files. It did not restamp the manifest,
run Cargo or confer accepted-release authority. Temporary import files were
removed after publication; three known published WASM generations remain, below
the maximum of five, so no previous published generation was deleted.

The running 4194 Vite listener is this worktree's local-recovery process. It
serves `apps/clearra-web/static/wasm`, not the normal frontend public staging
directory. Both the HTTP manifest and accepted-generation endpoint now report:

- Source commit: `0932df948e00fcaecd309b06d7ef45fd8ee8badb`
- Compile inputs: `480a16beac9e59bb0fddb173245fd0a3195cf49cfb1d75871febc26f155ec258`
- WASM: `9f416207804aa10660853f8562ebaa5a9967eae3ab37c4a2a6d9d1b292700430`, 21,484,910 bytes
- Bindings: `e45cf6b1682171c43b65fa4b86c226dfd359753dd4b55ef0b5d053b918c093e0`, 42,216 bytes

An audit tab was prepared before import with 36 occupied cells, one empty left
column, fixed queue `I`, no hold, Jstris 180, 4L and All solutions. The form and
checked TB option survived publication without reload. After import, two actual
HF Range searches returned **one solution and 100% coverage**, completed all
four stages and displayed the normal solution image/copy controls. No stored
TB or offline fallback was used. UI preparation reported 2.6 KiB and only the
Jstris slot available. Other profiles retained independent unavailable states.

| GUI observation | First search | Same-input repeat |
| --- | ---: | ---: |
| Displayed elapsed | 22.2 s | 16.9 s |
| WASM module preparation | 5,645.8 ms | 0.2 ms |
| PC4 online elapsed | 16,423.5 ms | 16,914.9 ms |
| Worker elapsed to terminal | 22,077.7 ms | 16,915.3 ms |
| Actual Range requests | 19 | 19 |
| Range payload bytes | 301,063 | 301,063 |
| Logical reads / cache hits | 34 / 15 | 34 / 15 |
| Local-file bytes | 0 | 0 |

These are two GUI observations, not a statistical benchmark or full-file/large
family performance claim. The repeat is **module-warm, not dataset-cache-warm**:
it still makes 19 requests. It separates first-module setup from the remaining
online latency, but the counters alone do not attribute each request to a
particular index or graph record. The one-I fixture does not prove the 456,459
empty-field P7P4 family, all kick profiles, Desktop/Discord parity or release
acceptance. Full Jstris download (~661 MiB), installed real-file timing and
Oracle non-interference remain unmeasured; no full download or server deployment
was performed. The temporary audit tab was closed after the two searches.

## Primary references

- [HF dataset](https://huggingface.co/datasets/muse918/tetris-4lpc-mdp-vstar-policy): public metadata, current per-generation file identities; MIT dataset declared by user/upstream.
- [Hydra optimal](https://github.com/muse918/hydra-optimal): graph format/order; GPL source inspected for format, not vendored into Clearra.
- [OPFS](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system): origin-bound storage and quota.
- [curl](https://curl.se/docs/manpage.html): transport configuration and download limits.

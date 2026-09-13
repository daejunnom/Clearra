# PC4 compact input admission: exact structure instead of Cartesian unranking

This is an input-admission optimization, not a graph DP completion, enabled
HF profile, or end-to-end PC-search performance result. Product semantics,
candidate qualification, original ordinal weights, and all existing source
freshness checks stay unchanged.

## Source bottleneck and change

The v2 binding unranked every compiled queue once during preparation and again
when admitting it to the Core product. For N original ordinals of effective
length L this costs O(NL), despite Core already retaining many inputs as a
compact ranked expression. A P7 source therefore performed 5,040 lazy reads
at each boundary. Two bag atoms amplify that cost multiplicatively.

`02c0880e3aa9a08daba59ea7504c373b7dd124bc` adds read-only views of the actual
sequence representation. They are derived from private storage, not from the
descriptive `structure` enum, numeric IDs, the input string alone, or samples:

- Standard seven-bag: actual ranked length and count, plus the piece order
  used by its unranker.
- Factorized expression: actual atom order, ordered choices, draw count,
  variant count, full length, visible prefix length, and original count.
- A compact path is admitted only when the actual weight storage is Uniform
  and exactly equal to Core's `1.0/N` model. An explicit vector, terminal
  remainder, or different uniform weight does not acquire this fast path.

The input binding schema is v3 with distinct compact/exhaustive encodings.
The existing header still binds original syntax, source kind, observation
policy, lookahead and complete count. No dataset revision or artifact hash is
changed or pinned. The new schema deliberately does not claim byte equality
with the old expanded-queue digest. Independently compiled actual storage
must have the same representation to share this binding; an unrelated source
with the same coarse IDs remains inadmissible.

Preparation still cannot seal before an advance. One compact advance proves
all represented ordinals without reading any queue; cancellation after the
last atom discards the staged digest and permanently prevents sealing. Core
admission uses the same encoder. Explicit storage keeps the existing paged
exhaustive preparation and per-ordinal admission checks.

Let S be the input text size, A the number of atoms and C the total number of
stored choices. Compact admission costs O(S+A+C), not O(NL). It allocates no
second source universe and does not retain per-ordinal hashes. Prefix
projection still preserves N even when many visible queues coincide.
Changing an unranking algorithm's interpretation requires reviewing/versioning
this structural encoding; this is not a generic hash of an arbitrary object.

## Validation coverage

- Existing P7 test still compares all 5,040 queues and original weight bits
  against the compiler owner and checks unchanged retained capacity. It now
  requires a single compact advance with four cancellation observations,
  instead of 79 exhaustive 64-row advances.
- P7 projected to one piece retains 5,040 ordinals: seven visible pieces,
  each repeated 720 times, all with original weight bits. Prefix changes
  change input identity, and late cancellation cannot commit a certificate.
- Supply tests verify the view follows actual atom order even when the
  original source string is unchanged, ignores misleading descriptive
  structure metadata, and rejects explicit or non-normalized weight storage.
- A representation-only P7P7P2 test checks the 1,066,867,200-ordinal compact
  source in three atoms. It performs no product search and does not extend
  PC4's placement horizon beyond four lines.
- The full existing Range/Core product matrix remains enabled in the same
  non-publishing CI. This does not stand in for a large compact-pattern
  Range/reducer performance test; that remains a separate requirement.

The ignored `pc4_compact_input_admission_ab` runs explicitly once in the
non-publishing integration checker, not in product builds or ordinary tests.
It compares old exhaustive binding and compact admission in ABBA order,
four cycles (eight executions of each arm), for P7, one-piece-prefix P7 and
the standard seven-bag source. It reports nanoseconds, actual baseline queue
reads and compact guard work, with no timing-dependent pass threshold.
Equality/weight tests carry correctness; timings do not grant authority.

Non-publishing run
[34748490523](https://github.com/daejunnom/Clearra/actions/runs/34748490523)
completed successfully on the exact code SHA above. All four jobs passed:
source, PC4 contracts, native CLI and surface contracts. The PC4 job passed
Core 7+5, Tablebase 167, Replay 20, Postprocess 41, App 87 (one A/B ignored in
the regular selection), the three focused supply tests, the explicit A/B
test, and App replay 16. The 1,090 existing paired product cases remain in
that selection. Native CLI passed 17 real process tests; its complete log has
no LNK4098/LNK2038. Hosted Actions `punycode` deprecations remain unrelated.
No release workflow, merge to main or 4194 replacement occurred.

### ABBA measurements

Linux CI, Cargo test profile with `CARGO_PROFILE_TEST_DEBUG=0`, eight
executions of each arm. The table divides the recorded sums by eight; these
are input-admission measurements, not release/WASM or end-to-end timings.

| Source | Exhaustive mean ms | Compact mean ms | Baseline queue reads per execution | Compact queue reads |
| --- | ---: | ---: | ---: | ---: |
| P7, effective 7 pieces | 12.031226 | 0.016990 | 5,040 | 0 |
| P7, effective 1-piece prefix | 9.835633 | 0.013589 | 5,040 | 0 |
| Standard seven-bag | 13.905349 | 0.011023 | 5,040 | 0 |

Raw nanosecond sums (baseline / compact) are 96,249,811 / 135,923;
78,685,065 / 108,711; and 111,242,795 / 88,185. Compact cancellation checks
over eight executions were 32, 32 and 24 respectively. Original retained
source capacity stayed unchanged. No timings were used to weaken an exactness
test or qualify a dataset profile. These results show elimination of the
Cartesian identity audit; they do not prove an equally large search speedup.

## Work still required

The observation frontier still expands reveal/hold histories and starts
separate fixed-queue graph traversals. The shared adjacency cache avoids
duplicate HTTP reads, but it does not eliminate repeated suffix traversal or
materialization. A bounded graph/hold/source DP must preserve distinct input
ordinals, controllable hold choices, all witnesses and zero-hit mass; this
admission improvement does not close that item. Actual profile qualification,
public adapters and fallback/release gates also remain open.

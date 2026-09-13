# Bounded PC4 Range windows: transport A/B and integrated GUI observations

This continues [profile activation](pc4-online-profile-activation-2026-09-13.md).
It does not replace the previous exact-fragment observation or close v0.9.0.
The existing Rust/WASM engine is unchanged: `6ca1dde572979b1fd4201e045fc34af80a2503de`,
WASM SHA256 `12019d54adaaf175029484f1beb5df83e2976fb93e7de01e3e32b914b7e661c9`.
The host transport changes are in this commit. Tests run with no local Rust
compilation or additional build root, and no production deployment.

## Transport implementation

`scripts/release/pc4/pc4-range-reader.mjs` now owns HTTP, byte reservation,
cancellation, the in-flight request map and LRU. Qualification remains in its
existing independent module.

- Fetch only the 16KiB windows intersecting an actual read request. There is no
  background graph/index scan and no speculative frontier traversal.
- Return an independent copy of the exact requested subrange to Rust. The
  consumer's requested offsets, lengths and candidate authority are unchanged.
- Share the same in-flight window and reuse containing cached window bytes.
- Small artifacts keep exact partial reads rather than expanding to a whole
  file. Window seams and EOF are reconstructed from validated individual ranges.
- At most four HTTP requests run simultaneously; at most 512 wait in the queue.
- Reserve the shared 64MiB transfer allowance before starting I/O, so parallel
  requests cannot independently spend the same remaining allowance. The
  original 100,000-request cap is retained.
- Retain at most 8MiB and 2,048 cache entries. Cache identity includes the fixed
  repository/revision lifetime and each artifact's identity, path and length.
- Reject 200 full bodies, mismatched ranges, 416, 429, short/oversized bodies;
  cancellation rejects queued work, aborts active HTTP and clears retained data.
- Observability separates logical reads, HTTP requests, cache hits, in-flight
  joins and retained bytes. A job start clears prior job observations, fixing
  stale online counters during a subsequent offline search.

## Deterministic byte-trace A/B

The synthetic test issues 320 unique eight-byte requests in eight clusters.
All returned bytes match between exact-range and windowed readers.

| Reader | HTTP calls | Received bytes | Cache hits |
| --- | ---: | ---: | ---: |
| Exact, windows off | 320 | 2,560 | 0 |
| Demand windows | 8 | 131,072 | 312 |

This establishes a latency/transfer trade-off and byte equivalence, not a
40-fold search speedup. The additional bytes are bounded locality reads.

## Live HF browser A/B

4194 was idle before reloading the new JavaScript. The same existing WASM was
retained. HF revision in both observations:
`ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`.

### One-piece completed request

Same input as the preceding record: four rows, columns 2..10 filled, column 1
empty; `I`; hold off; 4L; Jstris 180; all solutions; PC4 enabled.

| Observation | Exact fragments (preceding run) | Demand windows |
| --- | ---: | ---: |
| GUI result | 1 solution | 1 solution |
| Online elapsed | 30,143.2 ms | 18,107.6 ms |
| Module preparation | 14,146.1 ms | 10,113.4 ms |
| Host elapsed to terminal | 44,298.1 ms | 28,224.4 ms |
| GUI elapsed | 44.4 s | 28.3 s |
| HTTP calls | 31 | 20 |
| Body bytes | 275 | 315,751 |
| Logical reads | not recorded | 38 |
| Window cache hits | not recorded | 18 |

This is one real before/after observation, not ABBA or a latency confidence
interval. Network variability and preparation variation are not attributed to
the algorithm. The result rendered successfully and did not fall back offline.

### Empty / P7P4 incomplete observation

Empty field, 4L, Jstris 180, P7P4, hold on, all supplied queue visible, all
solutions, PC4 enabled. The host advertised 11 ordinary compute slots; this
network path is not an 11-worker Geometry benchmark.

At 118.4 s: 233 logical reads, 122 HTTP requests, 111 cache hits, 1,998,848 bytes.
Manually cancelled at **135.2 s**: 263 logical reads, 140 HTTP requests,
123 cache hits, 2,277,376 body bytes and 135,230.8 ms online elapsed. Zero
in-flight joins were observed in this serial demand stream. Retained bytes
became zero on cancellation, and the GUI reached `cancelled`.

No complete universe was returned. The user's **456,459** solution reference
is still unverified. The cancelled time is not a successful search time.

## Additional algorithm boundary

Source inspection confirms that `Pc4ObservationFrontierFamily::next_page`
composes concrete reveal queues and expands their hold paths.
`Pc4ObservationGraphFamily::next_page` starts a fixed-queue traversal for each
frontier entry, sharing a suffix memo but retaining a single active traversal.
The host publishes one pending Range at a time. Window caching cannot make
those serial graph demands concurrent by itself.

Remaining work is structural: a bounded frontier of independent known node
requests and a graph/compiled-pattern composition that preserves queue/hold
provenance without repeatedly expanding equivalent work. Duplicate paths cannot
simply be dropped because probability and replay use their provenance. These
are source-backed next investigation points, not a measured breakdown of all
P7P4 CPU cost and not an implemented structural pattern-DP claim.

## v0.8.1 GUI checkpoint and residuals

On the same integrated source/WASM with online TB explicitly off:
`ctk3_w0kCQBjwwAMPPAD37g`, P7, 4L, Jstris 180, hold on, minimum solutions.

- GUI first result completed at **16.4 s**; minimum cardinality **25**, coverage
  100%, and 25 members rendered with existing selected-set copy controls.
- Geometry: 3,303 nodes and 2,260/2,260 candidates; BuildUp: 11,858 nodes;
  verification: 46,650 checks. Displayed result-family solution count: 246.
- Host source time: 368.9 ms; finalization: 15,943 ms; total to terminal:
  16,442.3 ms. Eight minimum waves reported a sampled maximum of 11 active
  workers. Overlapping worker timing sums are not wall time.
- The initial next-page browser action did not return a successful control
  receipt; a later view showed alternative 40 with navigation disabled. This
  observation does not establish a one-click 1-to-40 application bug. No page
  teardown was performed. Subsequent completed 40/41 navigation and the user's
  suspected duplicate-set report are recorded in
  [the focused minimum-set audit](minimum-portfolio-identity-audit-2026-09-13.md).
- Stale prior-HF counters were observed at the start of this offline job. The
  local observation formatter now returns empty text on `started`, with a
  regression covering online-to-offline transitions.

This is a first-result browser check only: canonical set identity comparison,
lazy continuation, Build, native CLI and deployed Discord parity retain their
own gates. v0.8.1 and v0.9.0 completion checkboxes are not closed by this run.

## Validation

- 59 focused Node tests passed, including nine new window transport tests and
  the selected-portfolio export regressions.
- Frontend TypeScript contract graph passed a no-emit typecheck.
- Product-pager, local-profile and ordinary WASM host-yield contracts ran in
  memory and passed.
- Existing successful Rust CI/artifact evidence is unchanged; no duplicate Rust
  or WASM build is needed for this host-only change.

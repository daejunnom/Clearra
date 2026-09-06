# PC / Build GUI elapsed boundary audit

Date: 2026-09-06. Scope: the Web `?tool=pc` and `?tool=build-probability` product surfaces, compared with the v0.7.4 Build command contract. The initial audit snapshot predates the lazy-transcript change recorded below; its eager-formatting description is historical evidence, not the post-change state.

This is a **source audit, not a runtime benchmark or a performance acceptance result**. No browser measurement, product implementation change, or release authorization is supplied by this document. Line numbers below describe the audited worktree and can move in later edits.

## Recorded elapsed time

| Boundary | Source evidence | Consequence |
| --- | --- | --- |
| Start | `packages/clearra-ui/src/lib/workspace/SolverWorkspace.svelte:211,225–239`; `BuildProbabilityWorkspace.svelte:170,209–223` | The timer starts after `workerController.run()` returns. Command normalization and synchronous worker acquisition/posting are earlier; asynchronous worker preparation is later. |
| Worker completion | `apps/clearra-web/src/workers/DistributedWasmJobRunner.ts:686,714`; `clearraWorker.ts:775` | App finalization, result serialization, worker-side JSON parsing, and terminal-message transport precede the GUI stop. |
| Main-thread completion | `packages/clearra-ui/src/lib/wasm/wasmWorkerStore.ts:249–275` | The reducer formats the entire response with `JSON.stringify(response, null, 2)` at line 273 before publishing terminal status, even when the developer terminal is not displayed. |
| Stop | `SolverWorkspace.svelte:88,234–239`; `BuildProbabilityWorkspace.svelte:84,218–223` | The parent component reacts to terminal status and samples `performance.now()` again. The 100ms display interval does not mandate an additional 100ms delay. |

Thus the displayed duration is a GUI job-completion interval, neither pure Geometry/BuildUp CPU time nor a full click-to-painted-result measurement. One-decimal display rounding is separate from the captured duration. Scheduling delay before the terminal reducer/component update remains included.

## Result-visible work is a separate boundary

The result components receive the completed response after the parent terminal transition. Their normal initial field preparation, lazy page loading, DOM creation, and subsequent paint are not the work interval intentionally captured by the stopped timer.

- `SolutionGallery.svelte:107–119` prepares up to 200 preview records while exposing 100 cards. The initial lazy path at lines 151–177 can request two sequential 100-member batches before setting the initial visible count.
- `solutionBoardPreview.ts:36–43` parses a solution key and creates board data. `SolutionBoardPreview.svelte:21` renders those cells as a DOM grid; ordinary field previews are not PNG generation.
- The result adapters also map/sort materialized solution metadata before passing gallery props. Their visible-ready cost must not be attributed to recorded search time without an instrumented boundary measurement.

Possible narrow optimizations are lazy developer-terminal formatting and preparing only the initial 100 previews before yielding/prefetching the next batch. These are candidates, not measured savings. They must preserve complete selected-set copy/export semantics and existing terminal diagnostics.

## v0.7.4 Build versus current All Solutions

The old `crates/clearra-web-command/src/web_build_probability_input.rs` and current `crates/clearra-cli-command/src/web_build_probability_input.rs` retain the ordinary Build core query's `CountUnique`, retained trace limit 1, target piece count, and hold configuration.

The current GUI explicitly sends `--result-mode all-solutions`, but that does not request all replay paths, scoring, or a minimum cover:

- `buildProbabilityModel.ts:94,101,285,295`: All Solutions is the default result mode; per-solution probabilities default to false and are requested only explicitly.
- `cooperative_execution.rs:3504`: ordinary All Solutions does not retain an additional result-product command. `materialize_build_probability_result_mode_evidence` returns unchanged when that command is absent.
- `commands/build_probability_app_command.rs:126,142`: replay evidence expansion is specific to Complete Replay Paths, while All Solutions adds no product payload/page owner.
- The current App boundary additionally validates the query-owned Build result through `build_solution_probability_result.rs:340–409`; finite-memory guards and distributed transport are separate work from product-mode selection.

The hypothesis that selecting ordinary All Solutions automatically expands complete replay/score evidence is therefore unsupported by the audited code.

### Follow-up source comparison: internal results were not probability-only

This additional inspection **corrects the earlier explanation that the v0.7.4 probability GUI did not generate the complete solution identity set internally**. What the GUI displayed and what the core computed/retained are distinct. This is a source-contract correction, not a new historical timing measurement.

1. In `v0.7.4:crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs`, `CompactBuildProbabilitySession` already retained `buildable_tilings: ExactHashSet<StandardBoard64TilingIdentity>`. Its result finalization at lines 1715–1730 collected and sorted those identities, computed the normalized set hash, and created normalized solution keys. `merge_symmetry_results` already merged global coverage, identity/key sets, and available page stores; line 404 requested the initial 100 identities from a merged page store. The normal Build path was not merely a scalar-probability computation.

2. The v0.7.4 App did not unconditionally discard these results. `v0.7.4:crates/clearra-app/src/search_output_surface_postprocess.rs`, `finalize_coverage_summary_public_surface`, lines 10–25, stripped private solution authority only for `search_output_policy == "coverage-summary"` or an invalid declared contract. Normal Build symmetry merge selected the distinct policy `"summary"` at `build_probability.rs:435–439` and preserved the set contract. Therefore the existence of a generic summary-pruning helper is not evidence that ordinary Build dropped all solutions. Explicit coverage-summary and malformed-contract paths must remain distinguished.

3. Neither default version builds the full solution-by-pattern coverage matrix merely for ordinary All Solutions. v0.7.4 allocated `solution_coverage` only when execution constraints requested it (`build_probability.rs:962–966`). Current code additionally permits an explicit per-solution-probability request (`build_probability.rs:2479–2481`), but that GUI option is off by default. Both versions select `CandidateWitnessMode` using global coverage already known and whether per-solution coverage is required. The old source already used global covered-pattern union and a unique buildable-field set.

4. There is additional current boundary work independent of the All Solutions label. The current distributed verifier preserves caller/raw/sibling owners through pre/post memory admission in `build_probability_distributed.rs:1007–1043`; the old `consume` at lines 444 onward activated a pass and processed the candidate without this whole-live accounting. Current merge similarly admits validation/absorb and clone peaks before mutation (`:1377–1484`). Both versions retain verifier sessions and shared supply catalogs rather than inherently rebuilding a verifier for every candidate. The new guards are a concrete profiling boundary, not proof of their share of elapsed time or permission to remove them.

5. Current Build coverage metadata is query-bound and validated more strictly. `build_probability.rs:1719–1940` checks source/universe/weight-model identity and aggregation/denominator/completeness, then reconstructs success/failure coverage aggregates before accepting a worker result. The old `absorb_distributed_result` read the pattern count, reconstructed the bitset, and OR-merged it without the same authority checks. Current App `build_solution_probability_result.rs:340–409` additionally authorizes the completed Build response against its query. Importantly, its per-solution report validator returns early when that option is not requested (`:1051–1060`); default All Solutions does not pay a full per-solution probability reconstruction there.

6. The browser dispatch protocol also changed. `git diff v0.7.4 -- apps/clearra-web/src/workers/ClearraVerifierPool.ts` shows that current initialization/consume/finish operations add durable delegation offers, acceptance/start notices, payload/result hashes and result-commit state; v0.7.4 did not import `DurableDelegationJournal` or use those intermediate protocol states. These checks protect ownership and exactly-once result application. They add asynchronous transaction/transport boundaries even when a candidate batch computes quickly. This is a concrete added-cost boundary, not a measured attribution of the whole elapsed difference, and is not permission to bypass fencing or result validation.

These differences support measuring distributed validation, owner accounting, and transport separately. They do not establish that any one boundary causes the reported difference, nor do they make a PC source-search probe equivalent to a Build-probability product run.

## Narrow implementation follow-up: lazy terminal transcript

After auditing every `terminalLines` consumer, response formatting was moved out of the product-completion reducer into `packages/clearra-ui/src/lib/wasm/wasmTerminalTranscript.ts`. `WasmTerminalShell.svelte` is the sole product text consumer. Ordinary string log entries remain ordered; success and typed-failure responses are retained as deferred entries and formatted only when the terminal requests text. Each formatted entry is cached to prevent repeated serialization on later terminal updates. Successful formatting releases that entry's pending response reference so historical entries do not keep both the object and its serialized text; the product store's current response reference remains intact. A presentation-only formatting failure emits fixed terminal text, retains the pending owner for the next display retry, and never changes execution status or diagnostics. The structured response, diagnostics/resource fields, selected-set copy/export, and page-owner lifecycle are unchanged.

Focused evidence: the new `wasmTerminalTranscript.contract.ts` and existing `wasmWorkerLifecycle.contract.ts` passed; UI contract TypeScript checking reported no diagnostics. This proves the tested lifecycle/text semantics, not a measured elapsed-time improvement. The gallery's 200-preview preparation was left unchanged because its visible-ready cost has not been measured.

An isolated, opt-in harness is available at `scripts/tools/benchmark-wasm-terminal-response.mjs`:

```powershell
node scripts/tools/benchmark-wasm-terminal-response.mjs --response-json <saved-response.json> --iterations 25
```

It accepts a saved App response or terminal-event JSON, limits input to 8 MiB and iterations to 100, and enforces a 30-second sampling budget. It reports numeric summaries for eager completion formatting, deferred entry creation, first terminal display, and cached terminal display. Response contents are never printed. Module preparation/file parsing are outside the samples; results are an isolated Node microbenchmark and must not be equated with browser search wall time.

### Actual saved-response microbenchmark

In a separately authorized CPU-idle window, the harness was run for 25 samples using `_local/clearra-stage-ab-1788689522366.json`, an artifact from the existing WASM candidate. The input artifact is 90,519 bytes, but only `terminal`'s `final_response.response` was measured: its pretty-printed text is 10,202 characters. The surrounding stage profile, command, progress event, and search report were excluded. Each phase had three untimed warmups.

| Isolated phase | Median (ms) | p95 (ms) | Maximum (ms) |
| --- | ---: | ---: | ---: |
| Previous eager completion JSON formatting | 0.0095 | 0.0150 | 0.0220 |
| Deferred completion entry creation | 0.0004 | 0.0024 | 0.0041 |
| First requested terminal display | 0.0112 | 0.0151 | 0.0267 |
| Cached terminal display | 0.0002 | 0.0005 | 0.0006 |

For this saved response, eager formatting costs approximately **0.01ms**, not hundreds of milliseconds. The lazy-transcript change removes unnecessary product work and avoids retaining both formatted text and its historical response object, but this measurement does **not** identify it as a principal fix for the reported approximately 0.3s difference. The sub-microsecond deferred/cached medians are close to timer granularity and should not be converted into a headline speedup ratio. These are isolated Node samples of one response shape, not browser end-to-end measurements or proof that every product's response costs the same.

## Interpretation of existing measurements

The reported raw source value near 0.129s was captured with a separately prepared Node/WASM PC-minimum source harness, batch 128, core budget 8192, and without the GUI IndexedDB transport path or worker preload/initialization interval. The reported GUI value near 0.4s and historical v0.7.4 Build-probability value near 0.1s do not establish an equal-input, equal-mode, equal-start/stop comparison by themselves.

Existing transport observations can guide where to measure next, but overlapping journal/transport phase totals must not be added as independent wall-time contributions. In particular, this audit does **not** assign the apparent difference to JSON formatting, rendering, or an algorithm regression. A matched comparison should separately record preparation, source search, verification/merge transport, App finalization/serialization, main-thread terminal handling, and first-result-visible time.

## Follow-up priority without changing acceptance

The user accepts approximately0.4s for ordinary PC/Build and prioritizes minimum
cover performance. That acceptance remains in force: this audit does not quietly
restore the old0.1s release gate. It also does not establish0.4s as an algorithmic
lower bound or justify a scalar-only shortcut that drops the complete solution
set. The source findings justify a targeted wrapper-overhead follow-up:

- Measure one matched Build fixture/mode on each artifact, separately cold and
  warm, with identical worker policy and exact terminal timestamps. Keep the PC
  source-only128.9ms diagnostic as a separate measure.
- Prioritize worker reuse and critical-path durable protocol waits, then profile
  repeated immutable-input validation/owner projections and terminal formatting.
  Optimize repeated work only while preserving query binding, finite-memory
  admission, stale-result rejection and exactly-once merge.
- Keep first-visible field preparation separate from search elapsed. Lazy
  diagnostics and first100 preview preparation are possible UI improvements,
  not yet demonstrated causes of the historical0.1-to0.4 difference.

No product source changes, new A/B timing run, WASM publication, release gate or
deployment were performed for this follow-up audit.

## Subsequent implementation and measured host boundaries

The following is later work requested by the user, separate from the read-only
audit above. An opt-in local host profile now captures module/preparation,
producer/dispatch, final drain, verifier finalization, App finalization and JSON
parse intervals. Nested producer/enqueue/merge totals are explicitly not added
to their enclosing stage. Only bounded numeric fields reach the local panel.

On published512 WASM with the current profiling/lazy-transcript TypeScript,
the corrected Build fixture (base16, target24, P7, SRS+, All Solutions,
solution-probabilities off) returned all246 solutions and5,040/5,040 success in
each observed run. The new audit tab is independent of previously closed tabs.
No Cargo or agent test process overlapped these measurements. Ordinary user
applications and OS background work remain outside the benchmark's control.

| Host interval, ms | Cold auto11 | Warm auto11 | Repeat auto11 | Full12 |
| --- | ---: | ---: | ---: | ---: |
| Module/setup | 177.9 | 0.2 | 0.1 | 0.1 |
| Product preparation | 13.0 | 4.4 | 4.6 | 2.2 |
| Producer and dispatch stage | 403.6 | 625.3 | 647.0 | 593.0 |
| Final consume drain | 46.7 | 70.7 | 67.1 | 83.4 |
| Verifier finalization | 68.8 | 86.7 | 94.0 | 93.9 |
| App finalization | 18.3 | 3.2 | 3.2 | 4.9 |
| JSON parse | 0.2 | 0.0 | 0.1 | 0.1 |
| Worker start to terminal emission, enclosing total | 729.3 | 790.6 | 816.3 | 777.7 |
| Producer synchronous calls, nested | 119.0 | 209.8 | 233.8 | 193.3 |
| Enqueue/ready wait, nested | 224.6 | 333.6 | 331.9 | 325.1 |
| Raw partial merge, nested across stages | 5.7 | 5.4 | 3.8 | 4.2 |

Cold and warm GUI displays read0.8s; the source itself also varied between runs,
so these are not a demonstrated improvement over the historical0.4s samples.
The full-processor option was restored to auto after measurement. The native
source-only128.9ms remains a different PC diagnostic boundary.

Warm auto reused8 workers and initialized2 more (93.4ms maximum initialization
prewarm). Full12 reused10 and initialized1 more. Forty auto consume batches had
99ms summed actual run-grant-to-reply time in the warm run, while the producer's
ready/enqueue wait alone was333.6ms. Those worker totals overlap, but the host's
sequential wait establishes a concrete non-rendering cost. Terminal parse and
warm App finalization are too small in these samples to explain the observed
gap by themselves. This motivates batching/reuse/protocol critical-path work,
not removing exact result or resource safeguards.

### Grow-only prewarm correction

The idle prewarm hint previously trimmed a ten-worker ready cache to eight.
`prewarm` now only grows the cache; actual `initialize(size)` still enforces
the current request's exact worker count and host/resource admission. Pool
contracts cover cache reuse, explicit 10→11→10→3 resizing, lifecycle-owner
changes, cancellation, and zero active authority for idle cached workers.

With the same published512 WASM and corrected TypeScript, a new audit tab's
cold and warm auto11 runs again returned all246 solutions and5,040/5,040.
The warm run reused all10 remote workers (no new worker initialization),
worker-start-to-terminal575.4ms, source473.6ms, nested producer144.8ms,
nested enqueue287.1ms, drain43.5ms, verifier finish52.0ms, App finalization2.4ms,
parse0.1ms. The GUI displayed0.6s (cold0.9s). This confirms removal of the
specific repeated cold-worker creation, not achievement of0.4s or elimination
of journal/lease waits. No Cargo/agent CPU test overlapped this pair.

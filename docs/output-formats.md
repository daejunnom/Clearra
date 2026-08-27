# Output Formats

Output writers consume render models and diagnostic reports. Search crates should
not know text, JSON, CSV, fumen-like codec, or bitmap rendering details.
`clearra-output` owns output envelopes and format dispatch. `clearra-fumen` owns
fumen-like reader/writer/trace codec and replay adapters. `clearra-render` owns
skin manifests, PNG atlas metadata, render options, and bitmap render capability
reports.

I18N applies to UI labels and human-facing messages, not to output contract
keys. JSON fields such as `total_solution_count`, `backend_fallback_reason`,
and `coverage_probability` remain stable English identifiers in every language.
GUI and CLI display layers may use `clearra-i18n` translation keys with English
fallback labels when presenting those fields.

## Text Output Profiles

Text output has explicit field profiles owned by `clearra-output`, not by CLI
command handlers.

Default text is a human-sized summary. It may show `kind`, `status`, preset,
lines, queue, hold, rule, backend, solution counts, `coverage_probability`,
continuation fields, and warning/fallback summaries. It must not dump
`executor_flow`, compact problem descriptors, GPU/hybrid internals, score
internals, coverage row internals, raw coverage exports, or backend report
objects.

`--verbose` selects the verbose text profile and may show compact descriptors,
backend metrics, C/Rust bridge details, score internals, coverage row internals,
and executor flow fields.

`--format json` remains the full stable contract and must preserve all contract
fields regardless of text verbosity.

PC/search output consumes typed product result models: `SearchExecutionReport`, `PackingResult`, `PackingCandidateView`, `BuildUpResult`, `CoverageRowView`, `CoverageResult`, `ObjectiveResult`, `ReplayTrace`, and `BackendReport`. Writers must keep `total_solution_count`, `unique_solution_count`, `retained_trace_count`, `count_complete`, `trace_retention_truncated`, `coverage_probability`, and backend fallback fields distinct. Packing candidate counts are not solution counts.

Capacity and partial-result safety is part of the output contract. BuildUp
count/enumeration output preserves `total_variant_count`, `count_complete`,
`trace_retained`, `retained_variant_count`, and `truncation_reason` at the
C boundary. Product output keeps `total_solution_count`,
`unique_solution_count`, `retained_trace_count`, `count_complete`,
`count_truncated_reason`, `trace_retention_truncated`,
`trace_retention_reason`, `coverage_probability`, and
`probability_complete` separate. `retained_trace_count` is never a substitute
for `total_solution_count`.

Scoring output is an overlay on replay/objective results. It exposes
`score_accuracy_level`, `score_profile_accuracy_mode`, `score_event_basis`,
`score_b2b_chain_rule`, `score_all_clear_b2b_extra_increment`,
`score_hard_drop_included`, `score_soft_drop_included`,
`score_evaluation_basis`, `score_evaluation_scope`,
`score_does_not_change_probability_union`,
`score_field_average_score`, `score_failed_pc_pattern_count`,
`score_failed_pc_pattern_score`, `score_unconditional_expected_score`, and
`score_covered_pattern_conditional_average_score` without changing
`coverage_probability`.

Score summary selects the highest legal movement trace for each
`(candidate_id, pattern_id)` by integer score alone, then retains every
highest-scoring candidate for each pattern before applying the requested
reducer. Attack cannot break any score tie or alter ordering. Patterns without
a PC execution contribute score zero to `score_field_average_score`. Missing or
partial score evidence reports an incomplete matrix rather than inventing a
score. `objective_min_cover_selected_rows` remains the ordinary minimum set
whose PatternBitSet union covers the requested PC patterns;
`pc.score-minimals` and typed `max-score-cover` use separate score-optimal row
and exact portfolio contracts.

## Portfolio Alternative Output

Portfolio tie output is used only when the public result unit is a portfolio.
`portfolio-alternative-set.v1` carries query/source/profile/universe/build and
candidate-map identity, proven optimal cardinality,
`known_alternative_count_decimal`, nullable
`total_alternative_count_decimal`, `enumeration_complete`, and restart evidence.
`portfolio-alternative-page.v1` carries one outer portfolio and opaque
previous/next cursors. `portfolio-member-page.v1` reads that result's candidate
dictionary in pages of exactly 100.

GUI result stores prepare automatically but enumerate only for navigation or
bounded prefetch. They keep current/previous/next state and release it on new
search, cancellation, navigation away, disposal, or app exit. They do not put
snapshot state in share URLs or carry it across restarts in v0.8.

CLI tie enumeration is opt-in:

```text
clearra <original-command> ... --ties --tie-snapshot PATH
clearra continue --tie-snapshot PATH --tie-cursor TOKEN
```

The initial path must be new. Snapshot output is versioned, query/build bound,
exclusively locked, no-replace, and rejects symlink/reparse targets. Default CLI
output remains unchanged without the flags. Discord does not expose tie flags,
counts, pages, cursors, buttons, explanations, or attachments and accepts only
the canonical first portfolio in the existing result shape.

Normal solution or witness families, including `pc.score`, `pc.allspin-sol`,
Setup rankings, Forward outcomes, and operation-order lists, keep their own
family paging and never acquire portfolio tie metadata merely because multiple
members exist.

## Native Solution-Set CTK3 And Fumen

`solution-set-artifact.v2` requires every solution-bearing native CLI command to
support `--format text|json|ctk3|fumen` and
`--solution-artifact-format compact|json|ctk3|fumen`. Existing text and compact
defaults do not change. A selected portfolio encodes all of its members, not
only the visible 100-member page; a normal family follows its capability paging
contract.

Native CTK3 uses a Rust codec conforming to the language-neutral CTK3 document
contract and the same KAT fixtures as the browser/Discord TypeScript package.
The native path must not launch Node/JavaScript or call a network codec. Large
documents stream through CTK3 segment/bundle framing. Native Fumen writes a
compatible multi-page document and rejects CTK3-only information that cannot be
represented without loss. Cancellation, codec failure, and publication failure
must not leave a successful partial stdout/file; native file publication keeps
atomic-new-file, no-overwrite, and symlink rejection.

Spin target output is a coverage result. `SpinProbabilityResult` fields include
`spin_target_id`, `spin_target_name`, `covered_pattern_count`, `pattern_count`,
`pattern_universe_id`, `pattern_weight_model_id`, `probability`,
`probability_complete`, `materialized_probability_mass`, `renormalized`,
`truncation_reason`, `spin_accuracy`, `trace_completeness`, and
`score_profile_id`.

When BuildUp enumeration or coverage matrix budgets truncate evidence, output
must keep `probability_complete=false`, `truncation_reason`, and
`materialized_probability_mass`. It must not renormalize a partial observed
universe or present a budgeted spin/score matrix as complete.
Observed queue truncation keeps `materialized_pattern_count`,
`total_possible_pattern_count_or_unknown`, `materialized_probability_mass`,
`renormalized=false`, `truncation_reason`, and
`probability_complete=false`.

Score output must distinguish evaluation scopes. `ScoreResult` fields include
`score_profile_id`, `score_accuracy`, `trace_completeness`, `evaluation_scope`,
`retained_trace_average_score`, `covered_pattern_conditional_average_score`,
`unconditional_expected_score`, `best_score_by_pattern_available`, and
`score_does_not_change_probability_union`. A retained trace average must never
be labeled as an unconditional expected score.

Special spin diagnostics are user-visible when exact classification is disabled
or incomplete. `SpecialSpinDiagnosticOutput` fields include
`special_spin_case_id`, `verification_state`, `kick_evidence_required`,
`kick_evidence_available`, `classification_accuracy`, and `disabled_reason`.

Memory/FFI diagnostics that affect output completeness must surface as ordinary
diagnostics rather than disappear into logs. User-visible fields should include
the relevant budget evidence such as `row_count`, `row_limit`,
`word_count`, `word_limit`, `variant_limit`, `kick_evidence_count`, and
`kick_evidence_limit` when those values are available.

Replay output consumes `clearra-replay::ReplayTrace`, not raw executor strings.
The same trace can render through `TextWriter::replay_trace`,
`JsonContract::from_replay_trace`, `JsonWriter::write_replay_trace`,
`clearra-fumen::FumenLikeWriter::write_replay_trace`, or
`RenderFormatDispatcher::render_replay_trace`. Replay JSON preserves colored
cell ownership, line clear events, canonical trace key, and the
representative/sample marker.

Bitmap output is connected through `ReplayTrace -> RenderScene -> SkinAtlas ->
RGBA frames -> PNG/GIF encoder`. PNG and GIF report `supported=true` and
`render_exact=true` only after the default atlas, replay ownership pixels, and
encoded-byte goldens pass. `RenderExactOutputGate::render_replay_trace` returns
typed binary output; it does not synthesize a frame from fixture data.

Runtime raw SVG rendering is forbidden. External SVG art is accepted only by
the feature-gated build-time importer, which applies resource limits,
structure-aware rejection and deterministic sanitization before `resvg`
rasterization. Runtime code consumes only the generated PNG atlas, manifest,
and provenance hashes. `tests/golden/render/render_capability_exact.json` pins
the connected capability:

```json
{
  "render_exact": true,
  "supported": true,
  "runtime_asset_format": "png-atlas"
}
```

Setup output renders raw exports instead of interpreted advice. Supported fields include `setup_raw_metrics`, `setup_raw_coverage_export`, `shape_family_id`, `tiling_variant_count`, `build_variant_count`, `covered_pattern_count`, `coverage_probability`, `post_pc_solution_count`, `backend_report`, `score_basis`, `raw_coverage_export_path`, `diagnostic_evidence`, `coverage_overlap_report`, `build_variant_metrics`, and `raw_condition_data`. JSON setup output groups X3 fields under `raw_metrics` and `raw_coverage` as well as preserving flat summary fields for CLI compatibility.

## Backend And Memory Reports

PC JSON output exposes stable `backend_report` and `memory_report` objects in
addition to the flat summary fields defined by the current schema.

`backend_report` includes `backend_requested`, `backend_selected`,
`gpu_trust_state`, `fallback_reason`, `cpu_confirm_required`, and
`deterministic_reference_matched`.

Phase 8 backend output nests the GPU worker contract under
`backend_report.gpu_worker`. The object includes `state`, `trust_state`,
`memory_ticket_id`, `fence_epoch`, `cpu_confirm_required`,
`can_source_exact_probability`, `fallback_reason`, `unavailable_reason`, and
`backpressure`.
The final worker `state` is `connected`, `unavailable`, or
`rejected-mismatch`. A connected result is exact only when its trust state is
`gpu-computed-cpu-confirmed` or `deterministic-reference-matched`. CPU fallback
is reported separately through `fallback_reason`; it is never represented as a
GPU result.
`backpressure` includes `gpu_queue_depth`, `cpu_worker_queue_depth`,
`readback_pending_batches`, `build_variant_buffer_pressure`,
`coverage_row_buffer_pressure`, `throttled_backend`, and `throttle_reason`.
The JSON contract is implemented by `backend_gpu_worker_contract` and pinned by
`json_backend_report_includes_gpu_worker_trust_state` and
`json_backend_report_includes_memory_ticket_and_fence_epoch`.
`json_gpu_worker_report_shows_connected_confirmed_state` and
`json_gpu_worker_report_shows_memory_ticket_and_fence` pin the Phase 10
visibility contract. GPU unavailable visibility is pinned by
`json_backend_report_includes_gpu_worker_unavailable_reason`.

An unavailable explicit `gpu` request reports a CPU fallback when fallback is
allowed. An unavailable `hybrid` capability instead reports normal CPU
selection with `fallback_used=false`, a null `backend_fallback_reason`,
`hybrid_status=cpu-selected`, and the capability detail in
`hybrid_disabled_reason`. A hybrid fallback is reserved for failures after GPU
execution has started.

`memory_report` includes `memory_leak_report_clean`, `live_scopes`,
`live_allocations`, `live_gpu_buffers`, `pending_release_queue`, and
`memory_pressure_level`.

Default text output keeps this short: backend selection, GPU unavailable/trust
summary, and memory clean/pressure summary. Verbose output may show lower-level
hybrid scheduler and C memory details.

Default text backend summary is intentionally small:

```text
backend: cpu
gpu: unavailable (gpu_kernel_unavailable)
memory: clean
```

Verbose text may include `gpu_worker_state`, `gpu_trust_state`,
`gpu_memory_ticket_id`, `gpu_fence_epoch`,
`gpu_backpressure_gpu_queue_depth`, `gpu_backpressure_cpu_worker_queue_depth`,
`gpu_backpressure_readback_pending_batches`,
`gpu_backpressure_build_variant_buffer_pressure`,
`gpu_backpressure_coverage_row_buffer_pressure`,
`gpu_backpressure_throttled_backend`, and
`gpu_backpressure_throttle_reason`. The default/verbose split is owned by
`BackendSummaryText` and pinned by
`text_default_summarizes_gpu_worker_without_internal_noise` and
`text_verbose_includes_gpu_worker_backpressure`.

`fumen-like` is a Clearra-owned adapter contract, not a search/build input type.
MVP1 writes and reads external `v115@` fumen-compatible comment pages using the
same base64 buffer, field repeat, action flag, and comment chunk layout used by
common fumen tooling. Raw fumen strings must be parsed in `clearra-fumen`
reader/adapter code before build coverage receives an already interpreted
`BuildTemplate`. Build coverage import/export contracts wrap typed
`BuildTemplate` values only; they do not parse raw external text.

MVP1 `convert` supports the narrow adapter direction `fumen-like -> text/json`. Encoding text/json back to fumen-like and importing arbitrary external fumen pages into build coverage are explicitly unsupported until those contracts have dedicated adapters.

## Spin And Score JSON Contract Rules

- Retained trace averages are not unconditional expected scores.
- SpinTarget probability is printed only from `PatternBitSet` union results.
- When `probability_complete=false`, output includes `truncation_reason` and
  `materialized_probability_mass`.
- Exact spin classification is not implied. Non-exact results include
  `spin_accuracy`.
- Score-aware objectives must disclose that probability did not change.
- FFI pointer identity must not be emitted as a product id. Output uses copied
  owned snapshots, stable candidate ids, operation set keys, coverage pattern
  ids, and trace keys.

Required JSON tests:

- `json_spin_probability_includes_universe_identity`
- `json_score_result_distinguishes_evaluation_scope`
- `json_special_spin_diagnostic_reports_disabled_reason`
- `json_probability_not_renormalized_after_observed_truncation`
- `observed_queue_truncation_is_not_renormalized`
- `output_distinguishes_total_solution_count_and_retained_trace_count`
- `json_retained_trace_average_not_labeled_expected_score`
- `json_score_matrix_capacity_exceeded_reports_budget_evidence`

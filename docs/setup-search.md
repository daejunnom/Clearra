# Setup Search

Setup search groups candidates by final occupied shape. Tiling variants and build variants are preserved as variants, while family probability is computed from PatternBitSet union coverage instead of summing variant probabilities.

The MVP2 service path emits `execution_scope=mvp2`, uses the `queue-pattern-shape-tiling-build-post-pc` enumeration strategy, and reports `post_pc_mode=scenario-clear-to-empty` with `post_pc_evaluation_attached=true`. It analyzes fixed, bag-aligned, and observed queue sources into materialized pattern ids, creates setup shape candidates, groups them into shape families, preserves tiling/build layers, attaches scenario post-PC evaluation by compiling a scenario preset into `SearchProblem`, and optionally evaluates retained traces with a score profile.

Enumeration is still bounded by `SetupLimits`: `shape_family_enumeration_complete`, `tiling_variant_enumeration_complete`, and `build_variant_enumeration_complete` disclose whether limits truncated any layer. The current tiling stage is placement-based deterministic shelf packing, not a full external setup solver. The probability invariant is strict: setup/build probability is always measured by OR-ing `PatternBitSet` coverage and applying `WeightedPatternSet` union probability. Score aggregation must never sum duplicate variant probabilities.

Setup output does not provide human-interpreted setup summary text. It exposes raw
exports only: `setup_raw_metrics`, `setup_raw_coverage_export`, `backend_report`,
`score_basis`, `diagnostic_evidence`, `coverage_overlap_report`,
`build_variant_metrics`, and `raw_condition_data`. X3 raw metrics include
`shape_family_id`, `tiling_variant_count`, `build_variant_count`,
`covered_pattern_count`, `coverage_probability`, `post_pc_solution_count`,
`score_basis`, `backend_report`, and `raw_coverage_export_path`; the setup
explorer schema consumes these fields for filtering without condition summary.

X5 raw metrics use `schema_version=2` and `metrics_kind=setup_raw_metrics`.
The v2 contract also exposes `shape_family_count`,
`score_aggregation_attached`, `setup_raw_metrics`,
`setup_raw_coverage_export`, `coverage_overlap_report`,
`build_variant_metrics`, and `diagnostic_evidence`. Raw coverage export is
machine-readable: it carries `pattern_universe_id`,
`pattern_weight_model_id`, `pattern_count`, `rows[]`, `family_unions[]`, and
`overlap_report`. These fields are analysis inputs only. The solver must not
state that a setup condition is good or bad, must not display raw counts as
probability, and must not hide coverage overlap.

## Spin And Score Coverage In Setup Search

Setup search may attach spin or score objectives to the same pattern universe
used for setup/build coverage. These objectives do not create a separate
probability rule:

- setup probability uses `CoverageRowKind::Setup` or `CoverageRowKind::Build`;
- spin target probability uses `CoverageRowKind::SpinTarget(spin_target_id)`;
- score matrix cells use `CoverageRowKind::ScoreCell(score_objective_cell_id)`.

Every row must carry `pattern_universe_id`, `pattern_count`, and
`pattern_weight_model_id`. The setup explorer must not merge rows from different
observed expansions, bag-aligned universes, or weight models even if their bit
lengths match.

Spin target filters such as TSD probability or all-spin double availability are
applied only after BuildUp enumeration and replay. A shape family or tiling
variant may be a promising setup, but it is not a spin target hit until an
accepted BuildVariant produces replay evidence satisfying `SpinTargetPredicate`.

Score aggregation must distinguish retained trace samples from pattern-universe
expectations. Setup result fields may expose retained-trace averages for
debugging, but raw metrics must label them separately from
`covered_pattern_conditional_average_score` and
`unconditional_expected_score`.

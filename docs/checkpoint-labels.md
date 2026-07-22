# PC Flow And Checkpoint Labels

Clearra PC requests are compiled problem presets, not separate solver families. An opening preset such as 2L, 4L, or 6L lowers to a scenario-shaped `SearchProblem` with an empty standard board, an exact piece window, a `ClearToEmpty` goal, and labels such as `2L`, `4L`, or `6L`. A scenario preset lowers an already interpreted board, queue, hold state, piece window, rule profile, and completion goal into the same `SearchProblem` type.

Execution continues through `PackingProblem` and `BuildUpProblem`. The packing phase enumerates candidates; the BuildUp phase decides whether those candidates can become valid queue/hold/reachability-respecting PC results. A packing candidate is not a solution before BuildUp accepts it.

Checkpoint labels, checkpoint schedules, continuation hints, chain classifiers, bag phase classifiers, and exact target policy are analysis metadata. They are not a search engine, do not choose a separate execution path, and must not replace the compiled goal, piece window, queue provenance, or backend report.

Observed queue input is not canonicalized into a single deterministic completion. Clearra expands the observed window into a materialized set of possible visible suffix patterns, keeps the matching boundary candidates, and exposes that universe through `PatternBitSet` plus `WeightedPatternSet`. If the pattern limit truncates expansion, `probability_complete` is false and the materialized probability mass remains below `1.0` instead of being renormalized.

Search count fields must not be inferred from retained traces. `total_solution_count` and `unique_solution_count` are count metrics; `retained_trace_count` describes how many replay traces are available for output. A run can have a complete count with only a small retained trace sample.

Setup-facing PC output should expose raw metrics and raw coverage export fields such as `setup_raw_metrics`, `setup_raw_coverage_export`, `backend_report`, `diagnostic_evidence`, `coverage_overlap_report`, and `build_variant_metrics`. Solver output should not turn these values into human-interpreted setup advice.

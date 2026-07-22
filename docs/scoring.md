# Scoring

Scoring is a post-processing layer over `SolutionTrace`; search must not depend on scoring profile internals.

External rule targets are documented separately in
[`tetrio-score-reference.md`](tetrio-score-reference.md) and
[`tetrio-tetra-league-season-2-attack.md`](tetrio-tetra-league-season-2-attack.md).
Those references define the external modes; they do not promote the current
Clearra evaluators to profile-specific exact support.

The current MVP2 scoring implementation is a basic approximation. Built-in names such as `jstris-ultra`, `tetrio`, and `ppt` identify preset score/attack parameter sets, not profile-specific exact implementations of those external games. The score model is split into profile-specific basic score/attack tables (`GuidelineScoreTable`, `JstrisUltraScoreTable`, and `TetrioScoreTable`) so named profiles no longer share one anonymous line-clear function. Output and exported JSON must disclose `accuracy_level=basic-approximation`, `profile_specific_exact=false`, and the reason `profile-specific basic score/attack tables with configurable spin detection`.

`tetrio` is the canonical TETR.IO score profile. Spin recognition is selected
independently through `t-spins`, `t-spins-plus`, `all-spin`, `all-spin-plus`,
`all-mini`, or `all-mini-plus`; the default is `t-spins`. All four all-piece
profiles require complete immobility and final-rotation evidence. A recognized
non-T spin maps to the T-spin Mini score class. PC score requests use the
separate `tetrio-pc-{spin-profile}` projection. That profile disables hard/soft-drop points,
level progression, and multiplayer attack. Its configurable initial B2B chain
defaults to exactly zero.

Spin classification is intentionally explicit. `disabled` never emits spin
events. `t-spin-simple` remains a compatibility/debug mode. Built-in named
profiles use `t-spins`, which requires a complete legal movement
trace, a final rotation, three blocked corners, the two-front-corner Mini rule,
and 90-degree first-success fifth-kick evidence for a full-spin override. A
180-degree SRS+ transition does not inherit that SRS 90-degree Mini upgrade. A
rotation that occurred earlier in the path is not a final-rotation proof. A
T lock that did not use a kick needs complete movement/rotation evidence but
does not invent a kick event; a kicked lock additionally requires step-matched,
first-success evidence whose result rotation and coordinates equal the actual
lock. Exact profile-specific support still requires source-pinned replay/golden
confirmation before `profile-specific-exact` may be accepted by validation.

Scenario and setup post-PC scoring must disclose the trace basis used for score/attack aggregation. `ScoreEvaluationSummary` owns `best_score`, `best_attack`, `evaluated_trace_count`, `evaluation_complete`, and `evaluation_basis`. The setup-facing aliases remain `score_evaluation_trace_count`, `score_evaluation_complete`, and `score_evaluation_basis`. `score_evaluation_trace_count` is the number of retained traces actually evaluated by the scoring model, `score_evaluation_complete` is true only when those evaluated traces cover the full counted solution set, and `score_evaluation_basis` is one of `all-traces`, `retained-traces`, or `sample`. Score expectation over retained traces must not be presented as a full-solution expectation when `total_solution_count` is larger than the evaluated trace count.

M27 scoring is post-processing. `core-c/src/scoring_events` provides placement,
clear, drop, and spin event basis data, while `clearra-replay` builds replay
events and `clearra-scoring` evaluates profiles from replay via
`ScoreModelEvaluator::evaluate_replay_trace`. Score output must state
`score_accuracy_level`, `score_evaluation_basis`,
`score_evaluation_complete`, and
`score_does_not_change_probability_union`; scoring must not alter PatternBitSet
OR union probability.

The PC projection also reports
`score_b2b_chain_rule=underlying-difficult-clear-only`,
`score_all_clear_b2b_extra_increment=0`, and both
`score_hard_drop_included=false` and `score_soft_drop_included=false`. The
Season 2 multiplayer All Clear B2B increment is not part of this score profile.

X4 scoring/objective output extends that contract.
`score_event_basis=build-variant-replay` means score events came from accepted
BuildUp evidence after replay conversion, not from search hot-path score logic.
`score_evaluation_scope` distinguishes `full`
evaluation from retained/sample evaluation, while `score_evaluation_basis`
keeps the exact basis value (`all-traces`, `retained-traces`, or `sample`).

The product score path reports one field average. Every legal execution is
evaluated, the highest legal score is selected independently for each concrete
supply pattern, and materialized pattern weights are applied afterward. A
pattern with no PC execution contributes score zero. Covered-pattern
conditional average remains diagnostic; the field average includes the entire
materialized universe. Per-solution averages and score-aware cover objectives
are not product request modes. Ordinary minimum solutions remain the exact
minimum PC coverage objective and do not depend on scoring.

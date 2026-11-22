# Search And PostProcess Boundary

Search proves PC/build feasibility. PostProcess explains or decorates accepted search evidence. These are separate layers.

## Search

Search enters through `SearchProblem` and the C hot path. C Packing creates
geometry candidates through one immutable `ConcreteRealizationCatalog`, its
`PlacementSkeleton` quotient, and Geometry Skeleton Exact Cover. Host reduction
canonicalizes candidates. C BuildUp BFS verifies operation order, piece source,
hold automaton, line clear, reachability, and goal constraints before a candidate
becomes a `BuildVariant`.

Search may not depend on fumen codecs, renderers, score evaluators, GUI schemas, or output writers.

## Supply Boundary

Geometry exact-cover does not own queue or hold state. It covers the compiled
required-cell universe with placement skeleton rows and tracks only the exact
remaining-cell domain and admissible used-piece counts. The forbidden product
shape is any geometry continuation storing a queue, `Vec<Piece>`, hold state, or
full trace.

BuildUp and Coverage consume the shared `PieceSource` and `HoldAutomaton`
contracts. `PieceSource` carries fixed queue, bag universe, observed window, or
materialized pattern universe identity, including provenance, completeness, and
truncation reason. `HoldAutomatonState` carries `piece_source_id`, `cursor`,
`hold_piece`, `bag_epoch`, `bag_remainder_key`, and provenance, and those fields
are part of the BuildUp memo key. This is the `BuildOrders(P) ∩ HoldReachableOrders(Q)`
boundary: order pruning is allowed only when the automaton/reachability domain
proves it.

The current product coverage path verifies each concrete pattern through
pattern-specific BuildUp and records the verified pattern bit directly. The
explicit-order language-intersection module is test-only and cannot authorize
product coverage or pruning until its BuildOrders and HoldReachableOrders inputs
are generated independently.

Historical native C GPU packing batches carry PieceSource/pattern identity and a piece multiset
window. They do not carry an ordered queue array. BuildUp reads the concrete
PieceSource pattern through its hold automaton state.

Plain marker: clr_packing_problem uses a piece_multiset_window and piece_source.
`clr_packing_problem` uses a `piece_multiset_window` and `piece_source`; it must
not own queue cursor or hold piece state. `clr_buildup_problem` owns the
`initial_hold_automaton` and verifies the operation-order path after a
PackingCandidate exists. This is the FFI boundary for
`search_problem_lowers_to_packing_problem`,
`packing_problem_uses_piece_multiset_not_fixed_order`, and
`build_up_problem_owns_piece_source_ref_and_hold_automaton`.
FFI completion also requires
`ffi_view_copies_native_buffers_to_owned_rust` and
`ffi_rejects_pointer_count_overflow_before_read`.

## PostProcess

Replay, fumen-like export, render, scoring, spin classification, and explanatory output consume accepted `BuildVariant`, `CoverageRow`, `CoverageMatrix`, or `ObjectiveResult` evidence. They may not retroactively prune PC search unless the pruning fact is already represented as a validated search-domain invariant.

The core executor exports a replay seed and never evaluates a `ScoreProfile` in its product path.
For PC requests, `clearra-app` passes that seed to `clearra-postprocess`, which attaches scoring
fields after search has completed.

PostProcess runs on CPU. GPU search and GPU postprocess have been retired; see [the current GPU contract](gpu-pipeline.md).

## PostProcess Pipeline And GPU Boundary

The connected score flow is:

`CandidateExecutionAggregate -> ReplayTrace -> ScoreMatrix -> ObjectiveReducer`.

CPU coverage union consumes the existing `PatternBitSet` rows. No GPU batch or CPU confirmation duplicate is produced.

Only buildable execution aggregates enter scoring. No count-only batch or
synthetic replay/evidence object is a product postprocess result. Postprocess
must not accept `PackingCandidate` as buildable evidence and must not create or
modify PC `CoverageRow` truth. `postprocess_does_not_change_pc_probability` is
the contract: PC coverage probability remains the search result.

The CPU/host remains the final owner of `SpecialSpinCaseRegistry` finalization,
unknown policy, exact minimum cover, Fumen/Fumen-like codecs, JSON/text
envelopes, diagnostics and
explain output, asset provenance validation, and custom score profile
validation.

Backend report fields for historical GPU requests remain decodable, but explicit
GPU or Hybrid product requests are unsupported without CPU fallback. Postprocess
does not rewrite the search backend report or create search coverage.

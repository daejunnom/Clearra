# Scoring Profiles

Scoring is optional post-processing in MVP1. Score and attack profile import/export, aggregation, and score-aware ranking belong to MVP2+ and must not make core perfect-clear search heavier by default.

## SpinTarget As Coverage Objective

Spin target probability is a first-class coverage objective, not a label on a
retained trace. Requests such as "TSD setup probability", "all-spin double
available", or `SpinThenPc` ask which queue patterns can produce a target spin
under a build/setup candidate.

The product flow is:

`BuildVariant -> ReplayTrace -> SpinClassifier -> SpinTargetPredicate -> CoverageRowKind::SpinTarget -> PatternBitSet OR -> SpinProbabilityResult`

The probability reducer is the same reducer used for PC/setup/build coverage.
Spin target probability must not be computed from retained trace samples or by
summing variant probabilities.

```rust
pub struct SpinTarget {
    pub id: SpinTargetId,
    pub spin_piece_selector: SpinPieceSelector,
    pub spin_kind: RequiredSpinKind,
    pub clear_lines: RequiredClearLines,
    pub mini_policy: SpinMiniPolicy,
    pub required_clear_kind: RequiredClearKind,
    pub required_score_profile: Option<ScoreProfileId>,
    pub target_probability_threshold: Option<ProbabilityValue>,
}

pub enum SpinPieceSelector {
    TOnly,
    AnyPiece,
    PieceSet(PieceSetId),
}

pub enum RequiredSpinKind {
    RegularSpin,
    MiniSpin,
    TSpin,
    TSpinMini,
    AllSpin,
    AllSpinMini,
    ProfileSpecific(ProfileSpecificSpinKindId),
}

pub enum SpinMiniPolicy {
    RegularOnly,
    MiniAllowed,
    MiniOnly,
    AllSpinAsMini,
}
```

## SpinClassifier

Spin classification is an object model over replay evidence:

```rust
pub trait SpinClassifier {
    fn classify(
        &self,
        input: SpinClassificationInput,
        profile: &ScoreProfile,
    ) -> SpinClassification;
}
```

`SpinClassificationInput` includes piece, rotation, placement, board before,
board after placement, board after clear, cleared lines, kick evidence,
movement info, and trace completeness. `SpinResult` includes piece, spin kind,
mini flag, cleared lines, rule id, kick-used flag, confidence, and accuracy
basis.

Exact kick-sensitive classifiers require `KickEvidence`. Without it, exact spin
classification is disabled or downgraded to an estimated/incomplete result.

## SpinTargetPredicate

```rust
pub struct SpinTargetPredicate {
    target: SpinTarget,
}

pub struct SpinTargetEvidence {
    pub spin_result: SpinResult,
    pub score_profile_id: Option<ScoreProfileId>,
    pub kick_evidence: Option<KickEvidence>,
    pub trace_completeness: TraceCompleteness,
    pub accuracy: SpinAccuracy,
}

impl SpinTargetPredicate {
    pub fn evaluate(
        &self,
        trace: &ReplayTrace,
        spin_result: &SpinResult,
        profile: &ScoreProfile,
    ) -> SpinTargetPredicateResult;
}
```

`SpinTargetPredicate` is applied after BuildUp and replay. It must not inspect a
raw `PackingCandidate` and declare target satisfaction.

## Kick-Sensitive Spin Classification

Fin, ISO, NEO, mini overrides, and profile-specific special spins are
kick-sensitive classification cases. They are not base kick tables. The
classifier needs kick index, offset, profile id, and first-success evidence from
replay. If a score profile requests exact special-spin semantics but the replay
does not carry kick evidence, validation or execution must report a diagnostic
instead of returning an exact result.

## SpecialSpinCaseRegistry

Special spin cases live in a registry separate from score table ids and kick
table ids. A case may be source-pinned, verified imported, descriptor-only, or
disabled. Descriptor-only cases may be displayed in UI but cannot enable exact
search/scoring output.

`KickSensitiveSpinRule` consults `SpecialSpinCaseRegistry` before it emits a
profile-specific exact spin. A case can classify only when exact classification
is enabled, the active score profile is allowed, the required kick signature
matches first-success kick evidence, and the board signature predicate matches
the replay-derived spin input. If no verified case matches, the rule may fall
back to ordinary corner or immobility classification, but kick evidence alone
must not become a Fin/ISO/NEO exact result.

## SpinAwardPolicy And All-Spin Policy

Named score profiles must separate T-spin-only and all-spin behavior:

```rust
pub enum SpinAwardPolicy {
    Disabled,
    TSpinsOnly,
    AllSpins,
    AllMini,
    AllSpinAsTSpinMini,
}

pub enum AllSpinScoreMapping {
    Disabled,
    NativeAllSpinTable,
    UseTSpinMiniTable,
}
```

`tetrio` is a TETR.IO source-pinned basic profile. Clearra treats
it as a public-score-table approximation, not as an official live rules mirror:
TETR.IO's official API warns that record/API structures may change, while the
public score references currently list Single 100, Double 300, Triple
500, Quad 800, Spin Quad 2600, Mini Spin Quad 1600, and All Clear 3500. Drop
score is modeled separately as `HardDrop2SoftDrop1`, because drop points are not
part of the level-multiplied line-clear table.

PC analysis uses a distinct `tetrio-pc-{spin-profile}` projection:

```text
ScoreProfile tetrio-pc-t-spins:
  spin_award_policy = TSpinsOnly
  all_spin_score_mapping = Disabled
  drop_score_policy = Disabled
  level_policy = Disabled
  attack_model = Disabled
  trace_requirement = PlacementTrace plus complete rotation evidence for T locks,
                      and exact first-success kick evidence when a kick was used
  initial_b2b_default = 0
  same_shape_trace_policy = HighestLegalTrace
```

All Clear selects the 3,500-point action row; it is not added to an ordinary
line-clear or T-spin action. Combo and B2B remain ordered execution state, so
two BuildUp orders of one tiling may produce different scores. The score
projection does not import the Season 2 multiplayer All-Clear `B2B +2` rule;
only the underlying difficult-clear action changes the B2B chain.

`tetrio` must not silently include all-spin behavior:

```text
ScoreProfile tetrio:
  spin_award_policy = TSpinsOnly
  all_spin_score_mapping = Disabled
  drop_score_policy = HardDrop2SoftDrop1
  source_pinned = true
```

Spin recognition is a separate selectable profile:

```text
SpinProfile t-spins / t-spins-plus:
  piece_scope = T only
  plus = immobile T fallback

SpinProfile all-spin / all-spin-plus:
  spin_award_policy = AllSpins
  all_spin_score_mapping = NativeAllSpinTable

SpinProfile all-mini / all-mini-plus:
  spin_award_policy = AllMini
  all_spin_score_mapping = UseTSpinMiniTable
```

All four all-piece policies require an all-piece spin classifier. A T-spin-only
classifier must not be accepted as the basis for all-spin or all-mini scoring.

## SpinAwardPolicy And Validator Boundaries

`SpinAwardPolicy::TSpinsOnly` is the default for the TETR.IO
profile. `SpinAwardPolicy::AllSpins`, `SpinAwardPolicy::AllMini`, and
`SpinAwardPolicy::AllSpinAsTSpinMini` are selectable policies, but they are
not silently enabled by named default profiles. The validator must reject a
TETR.IO-style default profile that requests all-spin scoring, and it must also
reject custom all-spin policies unless the selected `spin_classifier_id`
supports all-piece spin classification.

`DropScorePolicy::HardDrop2SoftDrop1` requires replay/drop event completeness.
A retained trace sample can display a sample score, but it cannot be promoted
to a full-universe expected score.

## Score Expectation Scopes

Score output must distinguish retained trace samples from pattern-universe
expectations:

```rust
pub enum ScoreEvaluationScope {
    RetainedTraceSample,
    CoveredPatternsConditional,
    FullPatternUniverseExpected,
}

pub enum ScoreAccuracy {
    Exact,
    PatternComplete,
    TraceSampleOnly,
    PlacementOnlyEstimate,
    KickSensitiveUnavailable,
    Incomplete,
}

pub struct FieldScoreSummary {
    pub materialized_pattern_count: usize,
    pub scored_pattern_count: usize,
    pub failed_pc_pattern_count: usize,
    pub field_average_score: ScoreValue,
    pub covered_pattern_conditional_average_score: Option<ScoreValue>,
    pub score_accuracy: ScoreAccuracy,
    pub trace_completeness: TraceCompleteness,
    pub evaluation_scope: ScoreEvaluationScope,
}
```

The field average selects the highest legal execution independently for each
materialized pattern. A pattern proven to have no PC contributes zero. The
covered-pattern conditional average excludes those failed patterns and is
diagnostic only. Per-solution score averages and score-aware cover selection are
not product contracts. Ordinary minimum PC cover remains in the coverage layer.

## ScoreProfileObjectValidator

The score profile object validator rejects exact claims unless the evaluator,
spin classifier, drop-score basis, and trace-completeness contract can support
them. A profile that asks for exact kick-sensitive spin classification without
kick evidence capability must fail validation or be explicitly disabled.

Validation evidence should include `score_profile_id`, `score_model_id`,
`attack_model_id`, `spin_classifier_id`, `spin_award_policy`,
`drop_score_policy`, `trace_completeness`, and whether all-piece classifier
capability is present. This keeps profile errors explainable without letting
CLI, GUI, or output writers duplicate scoring policy.

## Required Tests

- `field_average_includes_failed_pc_patterns_as_zero`
- `score_does_not_change_coverage_probability`
- `max_score_cover_uses_best_score_by_pattern`
- `max_score_cover_preserves_pattern_probability_once`
- `tetrio_profile_disables_all_spin_by_default`
- `score_profile_object_validator_rejects_exact_profile_with_basic_evaluator`
- `score_profile_object_validator_requires_trace_completeness_for_drop_score`
- `score_profile_object_validator`

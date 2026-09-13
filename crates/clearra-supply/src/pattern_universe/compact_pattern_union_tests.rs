use core::cell::Cell;
use std::collections::BTreeSet;

use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

use crate::{
    execution_automaton::SupplyProvenanceId, piece_source::PieceSourceId,
    queue::queue_pattern_expression::QueuePatternExpression,
};

use super::*;

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn limits() -> CompactPatternUnionLimits {
    CompactPatternUnionLimits::new(nz(32), nz(4096), nz(65_536))
}

fn initial(policy: HoldPolicy, held: Option<PieceKind>) -> SupplyExecutionState {
    SupplyExecutionState {
        hold_policy: policy,
        ..SupplyExecutionState::new(PieceSourceId::new(3), 0, held, 0, 0, SupplyProvenanceId(5))
    }
}

fn expression_source(expression: &str, prefix: usize) -> MaterializedPatternUniverse {
    let expression = QueuePatternExpression::parse(expression, 0)
        .unwrap()
        .prefix(prefix);
    let count = expression.pattern_count();
    assert!(expression.is_factorized());
    MaterializedPatternUniverse::from_factorized_queue_expression(
        PatternUniverseId::new(1),
        PatternWeightModelId::new(2),
        expression,
        ProbabilityValue::new(1.0 / count as f64).unwrap(),
        count as u128,
    )
    .unwrap()
}

fn prepare(
    source: &MaterializedPatternUniverse,
    initial: SupplyExecutionState,
) -> (CompactPatternUnionLanguage, CompactPatternUnionFrontier) {
    CompactPatternUnionLanguage::prepare(source, initial, limits(), &|| false)
        .unwrap()
        .unwrap()
}

fn symbolic_outputs(
    language: &CompactPatternUnionLanguage,
    frontier: &CompactPatternUnionFrontier,
    depth: usize,
) -> BTreeSet<Vec<PieceKind>> {
    fn visit(
        language: &CompactPatternUnionLanguage,
        frontier: &CompactPatternUnionFrontier,
        depth: usize,
        prefix: &mut Vec<PieceKind>,
        outputs: &mut BTreeSet<Vec<PieceKind>>,
    ) {
        if frontier.is_empty() {
            return;
        }
        if prefix.len() == depth {
            outputs.insert(prefix.clone());
            return;
        }
        for piece in PieceKind::STANDARD_TETROMINOES {
            let next = language.advance(frontier, piece, &|| false).unwrap();
            prefix.push(piece);
            visit(language, &next, depth, prefix, outputs);
            prefix.pop();
        }
    }
    let mut result = BTreeSet::new();
    visit(language, frontier, depth, &mut Vec::new(), &mut result);
    result
}

// Independent exhaustive oracle: explicit visible queues and the elementary
// queue/hold recurrence, not the new atom reader or Supply transition helper.
fn explicit_outputs(
    source: &MaterializedPatternUniverse,
    initial: SupplyExecutionState,
    depth: usize,
) -> BTreeSet<Vec<PieceKind>> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        queue: &[PieceKind],
        cursor: usize,
        held: Option<PieceKind>,
        policy: HoldPolicy,
        depth: usize,
        prefix: &mut Vec<PieceKind>,
        outputs: &mut BTreeSet<Vec<PieceKind>>,
    ) {
        if prefix.len() == depth {
            outputs.insert(prefix.clone());
            return;
        }
        let Some(&current) = queue.get(cursor) else {
            return;
        };
        if policy != HoldPolicy::Required {
            prefix.push(current);
            visit(queue, cursor + 1, held, policy, depth, prefix, outputs);
            prefix.pop();
        }
        if policy != HoldPolicy::Forbidden {
            match held {
                Some(piece) => {
                    prefix.push(piece);
                    visit(
                        queue,
                        cursor + 1,
                        Some(current),
                        policy,
                        depth,
                        prefix,
                        outputs,
                    );
                    prefix.pop();
                }
                None => {
                    if let Some(&next) = queue.get(cursor + 1) {
                        prefix.push(next);
                        visit(
                            queue,
                            cursor + 2,
                            Some(current),
                            policy,
                            depth,
                            prefix,
                            outputs,
                        );
                        prefix.pop();
                    }
                }
            }
        }
    }
    let queues = (0..source.pattern_count())
        .map(|index| source.sequence_at(index).into_owned())
        .collect::<BTreeSet<_>>();
    let mut outputs = BTreeSet::new();
    for queue in queues {
        visit(
            &queue,
            0,
            initial.hold_piece,
            initial.hold_policy,
            depth,
            &mut Vec::new(),
            &mut outputs,
        );
    }
    outputs
}

#[test]
fn compact_pattern_union_exhaustive_short_hold_languages_match_explicit_queues() {
    use PieceKind::{I, T};
    // The suffix keeps real compact storage while prefix projection gives a
    // small complete oracle. No sampling of the original ordinal universe.
    for (expression, visible) in [("P7", 3), ("[IO]2[OT]2P7", 4)] {
        let source = expression_source(expression, visible);
        for (policy, held) in [
            (HoldPolicy::Forbidden, None),
            (HoldPolicy::Allowed, None),
            (HoldPolicy::Allowed, Some(T)),
            (HoldPolicy::Required, None),
            (HoldPolicy::Required, Some(I)),
        ] {
            let initial = initial(policy, held);
            let (language, frontier) = prepare(&source, initial);
            for depth in 0..=visible + 1 {
                assert_eq!(
                    symbolic_outputs(&language, &frontier, depth),
                    explicit_outputs(&source, initial, depth),
                    "expression={expression} policy={policy:?} held={held:?} depth={depth}",
                );
            }
        }
    }
}

#[test]
fn compact_pattern_union_atom_boundaries_are_not_reconstructed_seven_bags() {
    use PieceKind::{I, O, S, T};
    let partial = expression_source("P4P4", 8);
    let standard = expression_source("P7P4", 11);
    let (partial, mut partial_frontier) = prepare(&partial, initial(HoldPolicy::Forbidden, None));
    let (standard, mut standard_frontier) =
        prepare(&standard, initial(HoldPolicy::Forbidden, None));
    for piece in [I, O, T, S, I] {
        partial_frontier = partial
            .advance(&partial_frontier, piece, &|| false)
            .unwrap();
        standard_frontier = standard
            .advance(&standard_frontier, piece, &|| false)
            .unwrap();
    }
    assert!(
        !partial_frontier.is_empty(),
        "the second four-draw atom has reset"
    );
    assert!(
        standard_frontier.is_empty(),
        "the first seven-bag is not exhausted"
    );
}

#[test]
fn compact_pattern_union_p7p4_prepares_without_cartesian_expansion() {
    let source = expression_source("P7P4", 11);
    let retained = source.checked_retained_capacity_bytes();
    let checks = Cell::new(0usize);
    let guard = || {
        checks.set(checks.get() + 1);
        false
    };
    let (language, mut frontier) = CompactPatternUnionLanguage::prepare(
        &source,
        initial(HoldPolicy::Allowed, None),
        limits(),
        &guard,
    )
    .unwrap()
    .unwrap();
    assert_eq!(language.source_pattern_count(), 4_233_600);
    assert_eq!(language.sequence_pieces(), 11);
    assert_eq!(language.atom_count(), 2);
    assert!(
        checks.get() < 20,
        "structural binding never visits millions of ordinals"
    );
    let mut peak = frontier.state_count();
    for piece in PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .chain(PieceKind::STANDARD_TETROMINOES.into_iter().take(3))
    {
        frontier = language.advance(&frontier, piece, &guard).unwrap();
        assert!(!frontier.is_empty());
        peak = peak.max(frontier.state_count());
    }
    assert_eq!(frontier.placed_pieces(), 10);
    assert!(
        peak < 128,
        "ambiguous histories merge into small supply-state sets: {peak}"
    );
    assert!(checks.get() < 10_000);
    assert_eq!(source.checked_retained_capacity_bytes(), retained);
    println!("compact_union_p7p4 source_patterns={} atoms={} placed={} peak_states={} guard_checks={} source_unranking=0",
        language.source_pattern_count(), language.atom_count(), frontier.placed_pieces(), peak, checks.get());
}

#[test]
fn compact_pattern_union_full_bag_storage_matches_factorized_storage() {
    let count = 210;
    let standard = MaterializedPatternUniverse::from_standard_7_bag_lexicographic(
        PatternUniverseId::new(1),
        PatternWeightModelId::new(2),
        3,
        count,
        ProbabilityValue::new(1.0 / count as f64).unwrap(),
        count as u128,
        true,
        None,
    )
    .unwrap();
    let factorized = expression_source("P7", 3);
    let (left, lstart) = prepare(&standard, initial(HoldPolicy::Allowed, None));
    let (right, rstart) = prepare(&factorized, initial(HoldPolicy::Allowed, None));
    assert_eq!(left.source_pattern_count(), 210);
    assert_eq!(
        right.source_pattern_count(),
        5040,
        "hidden multiplicity remains source-owned"
    );
    assert_eq!(
        symbolic_outputs(&left, &lstart, 3),
        symbolic_outputs(&right, &rstart, 3)
    );
}

#[test]
fn compact_pattern_union_never_draws_hidden_suffix_or_releases_hold_at_exhaustion() {
    let source = expression_source("P7", 1);
    let (language, start) = prepare(&source, initial(HoldPolicy::Allowed, Some(PieceKind::T)));
    let first = language.advance(&start, PieceKind::T, &|| false).unwrap();
    assert!(!first.is_empty());
    for piece in PieceKind::STANDARD_TETROMINOES {
        assert!(language
            .advance(&first, piece, &|| false)
            .unwrap()
            .is_empty());
    }
    let (language, start) = prepare(&source, initial(HoldPolicy::Required, None));
    for piece in PieceKind::STANDARD_TETROMINOES {
        assert!(language
            .advance(&start, piece, &|| false)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn compact_pattern_union_limits_and_cancellation_commit_no_partial_frontier() {
    let source = expression_source("P7", 3);
    let initial = initial(HoldPolicy::Allowed, None);
    for (states, attempts) in [(1, 65_536), (4096, 1)] {
        let (language, start) = CompactPatternUnionLanguage::prepare(
            &source,
            initial,
            CompactPatternUnionLimits::new(nz(32), nz(states), nz(attempts)),
            &|| false,
        )
        .unwrap()
        .unwrap();
        let before = start.states.clone();
        assert!(matches!(
            language.advance(&start, PieceKind::I, &|| false),
            Err(CompactPatternUnionError::FrontierStateLimit { .. }
                | CompactPatternUnionError::TransitionLimit { .. })
        ));
        assert_eq!(start.states, before);
        assert_eq!(start.placed_pieces(), 0);
    }
    let (language, start) = prepare(&source, initial);
    let checks = Cell::new(0);
    let normal = language
        .advance(&start, PieceKind::I, &|| {
            checks.set(checks.get() + 1);
            false
        })
        .unwrap();
    let last_check = checks.get();
    checks.set(0);
    assert_eq!(
        language
            .advance(&start, PieceKind::I, &|| {
                checks.set(checks.get() + 1);
                checks.get() == last_check
            })
            .unwrap_err(),
        CompactPatternUnionError::Cancelled
    );
    let resumed = language.advance(&start, PieceKind::I, &|| false).unwrap();
    assert_eq!(normal.states, resumed.states);
    assert_eq!(start.placed_pieces(), 0);
    assert_eq!(
        CompactPatternUnionLanguage::prepare(&source, initial, limits(), &|| true).unwrap_err(),
        CompactPatternUnionError::Cancelled
    );
}

#[test]
fn compact_pattern_union_foreign_frontiers_do_not_gain_authority_from_equal_ids() {
    let source = expression_source("P7", 3);
    let (first, start) = prepare(&source, initial(HoldPolicy::Allowed, None));
    let (other, _) = prepare(&source, initial(HoldPolicy::Allowed, None));
    assert_eq!(
        other.advance(&start, PieceKind::I, &|| false).unwrap_err(),
        CompactPatternUnionError::ForeignFrontier
    );
    assert!(first
        .clone()
        .advance(&start, PieceKind::I, &|| false)
        .is_ok());
    let first_result = first.advance(&start, PieceKind::I, &|| false).unwrap();
    let repeated_result = first
        .advance(&start.clone(), PieceKind::I, &|| false)
        .unwrap();
    let mut memo = std::collections::HashSet::new();
    assert!(memo.insert(first_result));
    assert!(
        !memo.insert(repeated_result),
        "equal supply sets within one owner share memo state"
    );
    let (_, foreign_start) = prepare(&source, initial(HoldPolicy::Allowed, None));
    assert_ne!(start, foreign_start);
}

#[test]
fn compact_pattern_union_rejects_inconsistent_compact_cardinality() {
    // Deliberately inconsistent internal storage: a 3-draw standard bag has
    // 210 queues, not 200. Uniform weights/complete=true are not sufficient.
    let source = MaterializedPatternUniverse::from_standard_7_bag_lexicographic(
        PatternUniverseId::new(1),
        PatternWeightModelId::new(2),
        3,
        200,
        ProbabilityValue::new(1.0 / 200.0).unwrap(),
        200,
        true,
        None,
    )
    .unwrap();
    assert_eq!(
        CompactPatternUnionLanguage::prepare(
            &source,
            initial(HoldPolicy::Allowed, None),
            limits(),
            &|| false
        )
        .unwrap_err(),
        CompactPatternUnionError::InconsistentSource
    );
}

#[test]
fn compact_pattern_union_rejects_incomplete_observed_and_nonfresh_inputs() {
    let source = expression_source("P7", 3);
    let mut state = initial(HoldPolicy::Allowed, None);
    state.observation.policy = QueueObservationPolicy::VisibleSeven;
    assert_eq!(
        CompactPatternUnionLanguage::prepare(&source, state, limits(), &|| false).unwrap_err(),
        CompactPatternUnionError::UnsupportedObservation
    );
    state.observation.policy = QueueObservationPolicy::FullQueueOracle;
    state.cursor = 1;
    assert_eq!(
        CompactPatternUnionLanguage::prepare(&source, state, limits(), &|| false).unwrap_err(),
        CompactPatternUnionError::UnsupportedInitialState
    );
    state.cursor = 0;
    state.bag_remainder_key = 16;
    assert_eq!(
        CompactPatternUnionLanguage::prepare(&source, state, limits(), &|| false).unwrap_err(),
        CompactPatternUnionError::UnsupportedInitialState
    );
    state.bag_remainder_key = 0;
    state.hold_empty = false;
    assert_eq!(
        CompactPatternUnionLanguage::prepare(&source, state, limits(), &|| false).unwrap_err(),
        CompactPatternUnionError::UnsupportedInitialState
    );
    assert!(matches!(
        CompactPatternUnionLanguage::prepare(
            &source,
            initial(HoldPolicy::Allowed, None),
            CompactPatternUnionLimits::new(nz(6), nz(4096), nz(65_536)),
            &|| false,
        ),
        Err(CompactPatternUnionError::SourcePieceLimit {
            limit: 6,
            attempted: 7
        })
    ));

    for complete in [true, false] {
        let source = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(2),
            vec![vec![PieceKind::I], vec![PieceKind::O]],
            vec![ProbabilityValue::new(0.5).unwrap(); 2],
            2,
            complete,
            None,
        )
        .unwrap();
        let result = CompactPatternUnionLanguage::prepare(
            &source,
            initial(HoldPolicy::Allowed, None),
            limits(),
            &|| false,
        );
        if complete {
            assert!(
                result.unwrap().is_none(),
                "explicit storage requires a separate owner"
            );
        } else {
            assert_eq!(
                result.unwrap_err(),
                CompactPatternUnionError::IncompleteSource
            );
        }
    }
}

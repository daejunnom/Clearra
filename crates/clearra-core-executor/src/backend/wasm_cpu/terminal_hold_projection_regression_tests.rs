//! Finite-source EOF is a property of each original input. These small exact
//! language products exercise coverage, labeled Hold paths, and projected
//! source suffixes without solving a physical placement family.

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{
        piece_kind::PieceKind::{self, I, J, L, O, S, T, Z},
        rotation::RotationState,
    },
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::{
    pattern::pattern_bitset::PatternBitSet,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_supply::{
    bag::BagState,
    hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
    mixed::supply_provenance::BagBoundaryEvidence,
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    piece_source::{PieceSource, PieceSourceId},
    queue::queue_pattern_expression::QueuePatternExpression,
    FiniteBuildPieceSourceRequest, FiniteBuildQueueRef, FiniteSupplyAllocationLedger,
    FiniteSupplyProvenanceRef,
};

use crate::backend::wasm_cpu::{
    buildup::{BuildOrderGraph, BuildOrderNodeSpec},
    coverage_product::{CoverageProductEvaluator, CoverageProductResult},
    piece_order_language::PieceOrderLanguageCache,
};

use super::{finite_terminal_release_allowed, terminal_release_pattern_word};

fn universe(sequences: Vec<Vec<PieceKind>>) -> MaterializedPatternUniverse {
    let count = sequences.len();
    MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(93),
        PatternWeightModelId::new(93),
        sequences,
        vec![ProbabilityValue::new(1.0 / count as f64).unwrap(); count],
        count as u128,
        true,
        None,
    )
    .unwrap()
}

fn hold(cursor: u16, held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(93),
        cursor,
        held,
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(93),
    )
}

fn chain(pieces: &[PieceKind]) -> BuildOrderGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for (depth, &piece) in pieces.iter().enumerate() {
        nodes.push(BuildOrderNodeSpec {
            edge_start: depth as u32,
            edge_count: 1,
            depth: depth as u8,
            accepting: false,
        });
        edges.push(BuildOrderGraph::edge(
            depth as u32 + 1,
            depth as u8,
            piece,
            RotationState::Zero,
            0,
            0,
            0,
        ));
    }
    nodes.push(BuildOrderNodeSpec {
        edge_start: edges.len() as u32,
        edge_count: 0,
        depth: pieces.len() as u8,
        accepting: true,
    });
    BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, true).unwrap()
}

fn count_paths(
    graph: &BuildOrderGraph,
    index: &PatternPiecePositionIndex,
    initial: HoldAutomatonState,
    flags: [bool; 3],
) -> CoverageProductResult {
    let [allow_hold, project_terminal, project_standard_bag] = flags;
    let result = CoverageProductEvaluator::default()
        .evaluate(
            graph,
            index,
            initial,
            allow_hold,
            project_terminal,
            project_standard_bag,
            true,
            false,
            &ExecutionControl::default(),
        )
        .unwrap();
    assert!(result.count_complete);
    result
}

#[test]
fn mixed_length_terminal_release_preserves_each_original_ordinal() {
    let source = universe(
        (0..130)
            .map(|ordinal| match ordinal % 3 {
                0 => Vec::new(),
                1 => vec![O],
                _ => vec![O, T],
            })
            .collect(),
    );
    let selected =
        PatternBitSet::from_pattern_indices(130, vec![0, 1, 2, 63, 64, 65, 127, 128, 129]).unwrap();
    for index in [
        PatternPiecePositionIndex::compile(&source).unwrap(),
        PatternPiecePositionIndex::compile_subset_before(&source, &selected, 130).unwrap(),
    ] {
        assert_eq!(index.sequence_len(), 2);
        for queue_position in 0..=3 {
            for alternate_active in [false, true] {
                let words: Vec<_> = (0..index.word_count())
                    .map(|word_index| {
                        let active = index.active_word(word_index)
                            & if alternate_active {
                                0x5555_5555_5555_5555
                            } else {
                                u64::MAX
                            };
                        terminal_release_pattern_word(
                            &index,
                            queue_position,
                            word_index,
                            active,
                            false,
                        )
                    })
                    .collect();
                // Independent finite-sequence lengths are the oracle; neither
                // bit-index maximum length nor projected masks decide it.
                let expected = (0..source.pattern_count())
                    .filter(|&ordinal| {
                        index.local_pattern_index(ordinal).is_some_and(|local| {
                            (!alternate_active || local % 2 == 0)
                                && queue_position != 0
                                && source.sequence_at(ordinal).len() == queue_position
                        })
                    })
                    .map(|ordinal| ordinal as u32)
                    .collect();
                assert_eq!(
                    index.expand_coverage_words(&words).unwrap(),
                    PatternBitSet::from_pattern_indices(130, expected).unwrap(),
                    "cursor={queue_position}, alternate_active={alternate_active}",
                );
            }
        }
    }
}

#[test]
fn mixed_length_count_all_keeps_three_labeled_hold_histories() {
    let source = universe(vec![vec![O], vec![O, T]]);
    let index = PatternPiecePositionIndex::compile(&source).unwrap();
    let graph = chain(&[O, T]);
    let initial = hold(0, Some(T));
    let result = count_paths(&graph, &index, initial, [true, true, false]);
    assert_eq!(result.coverage_bits.to_owned_words(), [0b11]);
    // [O]: Current(O), terminal-held(T). [O,T]: Current(O),
    // followed by either Current(T) or SwapHeld(T), two labeled histories.
    assert_eq!(result.path_count, 3);
    for (ordinal, expected_paths) in [(0, 1), (1, 2)] {
        let subset = PatternBitSet::from_pattern_indices(2, vec![ordinal]).unwrap();
        let selected =
            PatternPiecePositionIndex::compile_subset_before(&source, &subset, 2).unwrap();
        let per_original = count_paths(&graph, &selected, initial, [true, true, false]);
        assert_eq!(per_original.coverage_bits, subset);
        assert_eq!(per_original.path_count, expected_paths);
    }
    let mut languages = PieceOrderLanguageCache::default();
    let root = languages.canonicalize(&graph).unwrap();
    let language_result = CoverageProductEvaluator::default()
        .evaluate_language(
            &languages,
            root,
            &index,
            initial,
            true,
            true,
            false,
            &ExecutionControl::default(),
        )
        .unwrap();
    assert_eq!(language_result.coverage_bits, result.coverage_bits);
    assert_eq!(
        language_result
            .coverage_bits
            .first_pattern()
            .unwrap()
            .index(),
        0
    );
    assert_eq!(language_result.path_count, 0);
    assert!(language_result.count_complete);
}

#[test]
fn empty_and_past_eof_cannot_release_initial_hold() {
    let graph = chain(&[T]);
    for (sequence, cursor) in [(vec![], 0), (vec![O], 2), (vec![O, T], 3)] {
        assert!(!finite_terminal_release_allowed(
            sequence.len(),
            cursor as usize,
            true,
            true,
            false,
            true,
            true,
        ));
        let index = PatternPiecePositionIndex::compile(&universe(vec![sequence])).unwrap();
        let result = count_paths(&graph, &index, hold(cursor, Some(T)), [true, true, false]);
        assert!(result.coverage_bits.is_empty());
        assert_eq!(result.path_count, 0);
    }
    assert!(finite_terminal_release_allowed(
        1, 1, true, true, false, true, true
    ));
    let index = PatternPiecePositionIndex::compile(&universe(vec![vec![O]])).unwrap();
    let result = count_paths(&graph, &index, hold(1, Some(T)), [true, true, false]);
    assert_eq!(result.coverage_bits.to_owned_words(), [1]);
    assert_eq!(result.path_count, 1);
}

#[test]
fn inferred_seventh_draw_has_two_hold_actions_and_no_extra_terminal_release() {
    let index =
        PatternPiecePositionIndex::compile(&universe(vec![vec![O, T, S, Z, J, L]])).unwrap();
    let graph = chain(&[I]);
    let initial = hold(6, Some(I));
    let projected = count_paths(&graph, &index, initial, [true, true, true]);
    assert_eq!(projected.coverage_bits.to_owned_words(), [1]);
    assert_eq!(projected.path_count, 2, "Current(I) and SwapHeld(I) only");
    let finite = count_paths(&graph, &index, initial, [true, true, false]);
    assert_eq!(finite.coverage_bits, projected.coverage_bits);
    assert_eq!(
        finite.path_count, 1,
        "only the finite terminal-held release"
    );
}

#[test]
fn finite_p7_visible_three_keeps_twenty_four_original_suffixes() {
    let expression = QueuePatternExpression::parse("P7", 5_040).unwrap();
    let mut ledger = FiniteSupplyAllocationLedger::try_new(
        u128::MAX,
        expression.checked_retained_capacity_bytes().unwrap(),
    )
    .unwrap();
    let finite = PieceSource::materialize_finite_build(
        FiniteBuildPieceSourceRequest::new(
            FiniteBuildQueueRef::PatternExpression(&expression),
            3,
            5_040,
            FiniteSupplyProvenanceRef::new(
                "standard-7-bag",
                "standard-tetrominoes",
                None,
                BagBoundaryEvidence::FixedBoundary,
                false,
                false,
            ),
        ),
        &mut ledger,
    )
    .unwrap();
    let source = finite.piece_source().materialized_universe().unwrap();
    assert_eq!(source.pattern_count(), 5_040);
    let index = PatternPiecePositionIndex::compile(source).unwrap();
    assert_eq!(index.sequence_len(), 3);
    let original_ordinals: Vec<u32> = (0..expression.pattern_count())
        .filter(|&ordinal| expression.sequence_at(ordinal).starts_with(&[I, O, T]))
        .map(|ordinal| ordinal as u32)
        .collect();
    assert_eq!(original_ordinals.len(), 24);
    let expected_first = original_ordinals[0] as usize;
    let expected = PatternBitSet::from_pattern_indices(5_040, original_ordinals).unwrap();
    let placed = count_paths(
        &chain(&[I, O, T]),
        &index,
        hold(0, None),
        [false, false, false],
    );
    assert_eq!(placed.coverage_bits, expected);
    assert_eq!(placed.path_count, 24);
    assert_eq!(
        placed.coverage_bits.first_pattern().unwrap().index(),
        expected_first
    );
    // The retained full expression is an original-identity authority, not
    // permission to draw its invisible fourth piece from this finite source.
    let hidden = count_paths(
        &chain(&[I, O, T, J]),
        &index,
        hold(0, None),
        [false, false, false],
    );
    assert!(hidden.coverage_bits.is_empty());
    assert_eq!(hidden.path_count, 0);
    let terminal = count_paths(
        &chain(&[I, O, T, L]),
        &index,
        hold(0, Some(L)),
        [true, true, false],
    );
    assert_eq!(terminal.coverage_bits, expected);
    assert_eq!(terminal.path_count, 24);
}

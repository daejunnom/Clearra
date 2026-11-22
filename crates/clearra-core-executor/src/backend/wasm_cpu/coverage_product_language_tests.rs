//! Supply-product contracts for an already exact fixed-horizon kind language.
//! These fixtures do not make a physical reachability or see-7 policy claim.

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{
        piece_kind::PieceKind::{self, I, J, O, T},
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
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    piece_source::PieceSourceId,
};

use super::super::{buildup::BuildOrderNodeSpec, piece_order_language::PieceOrderLanguageCache};
use super::{BuildOrderGraph, CoverageProductEvaluator, WasmExactSearchError};

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

fn shared_suffix_graph() -> BuildOrderGraph {
    BuildOrderGraph::from_topological_parts(
        vec![
            BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 3,
                depth: 0,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 3,
                edge_count: 1,
                depth: 1,
                accepting: false,
            },
            BuildOrderNodeSpec {
                edge_start: 4,
                edge_count: 0,
                depth: 2,
                accepting: true,
            },
        ],
        vec![
            BuildOrderGraph::edge(1, 0, I, RotationState::Zero, 0, 0, 0),
            BuildOrderGraph::edge(1, 0, O, RotationState::Zero, 0, 0, 0),
            BuildOrderGraph::edge(1, 0, O, RotationState::Right, 0, 0, 0),
            BuildOrderGraph::edge(2, 1, T, RotationState::Zero, 0, 0, 0),
        ],
        0,
        0,
        false,
    )
    .unwrap()
}

fn initial_hold(cursor: u16, held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(92),
        cursor,
        held,
        3,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(92),
    )
}

fn single_pattern(sequence: Vec<PieceKind>) -> PatternPiecePositionIndex {
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(92),
        PatternWeightModelId::new(92),
        vec![sequence],
        vec![ProbabilityValue::ONE],
        1,
        true,
        None,
    )
    .unwrap();
    PatternPiecePositionIndex::compile(&universe).unwrap()
}

fn assert_coverage(
    graph: &BuildOrderGraph,
    index: &PatternPiecePositionIndex,
    hold: HoldAutomatonState,
    flags: [bool; 3],
    expected: PatternBitSet,
) {
    let [hold_enabled, projects_unplaced, projects_standard_bag] = flags;
    let mut cache = PieceOrderLanguageCache::default();
    let root = cache.canonicalize(graph).unwrap();
    let control = ExecutionControl::default();
    let graph_result = CoverageProductEvaluator::default()
        .evaluate(
            graph,
            index,
            hold,
            hold_enabled,
            projects_unplaced,
            projects_standard_bag,
            false,
            false,
            &control,
        )
        .unwrap();
    let language_result = CoverageProductEvaluator::default()
        .evaluate_language(
            &cache,
            root,
            index,
            hold,
            hold_enabled,
            projects_unplaced,
            projects_standard_bag,
            &control,
        )
        .unwrap();
    assert_eq!(graph_result.coverage_bits, expected);
    assert_eq!(language_result.coverage_bits, expected);
    assert_eq!(language_result.path_count, 0);
    assert!(language_result.count_complete);
}

#[test]
fn language_product_preserves_nonfresh_hold_and_sparse_original_ordinals() {
    let tails = [[I, T], [O, T], [T, I], [T, O], [J, T], [J, J]];
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(91),
        PatternWeightModelId::new(91),
        (0..130)
            .map(|ordinal| [J, tails[ordinal % 6][0], tails[ordinal % 6][1]].to_vec())
            .collect(),
        (0..130)
            .map(|ordinal| {
                ProbabilityValue::new(if ordinal == 0 { 0.0 } else { 1.0 / 129.0 }).unwrap()
            })
            .collect(),
        130,
        true,
        None,
    )
    .unwrap();
    let subset =
        PatternBitSet::from_pattern_indices(130, vec![0, 2, 63, 64, 65, 127, 129]).unwrap();
    let index = PatternPiecePositionIndex::compile_subset_before(&universe, &subset, 130).unwrap();
    let graph = shared_suffix_graph();

    // The prefix J has already been consumed. With O held, each selected tail
    // except J,J can produce either I,T or O,T. Ordinal zero has zero weight
    // but remains a covered original identity, across sparse word boundaries.
    assert_coverage(
        &graph,
        &index,
        initial_hold(1, Some(O)),
        [true, false, false],
        PatternBitSet::from_pattern_indices(130, vec![0, 2, 63, 64, 127, 129]).unwrap(),
    );
    assert_coverage(
        &graph,
        &index,
        initial_hold(1, None),
        [false, false, false],
        PatternBitSet::from_pattern_indices(130, vec![0, 127]).unwrap(),
    );
}

#[test]
fn language_product_preserves_empty_hold_and_finite_terminal_release() {
    let graph = chain(&[O, T]);
    for (enabled, expected) in [(false, Vec::new()), (true, vec![0])] {
        assert_coverage(
            &graph,
            &single_pattern(vec![I, O, T]),
            initial_hold(0, None),
            [enabled, false, false],
            PatternBitSet::from_pattern_indices(1, expected).unwrap(),
        );
    }
    for (projected, expected) in [(false, Vec::new()), (true, vec![0])] {
        assert_coverage(
            &graph,
            &single_pattern(vec![O]),
            initial_hold(0, Some(T)),
            [true, projected, false],
            PatternBitSet::from_pattern_indices(1, expected).unwrap(),
        );
    }
}

#[test]
fn language_product_uses_only_rooted_slots_and_resets_across_owners() {
    let mut cache = PieceOrderLanguageCache::default();
    cache.canonicalize(&chain(&[I; 80])).unwrap();
    let root = cache.canonicalize(&chain(&[O, T])).unwrap();
    assert!(cache.node_count() > 80);
    let mut evaluator = CoverageProductEvaluator::default();
    let control = ExecutionControl::default();
    let index = single_pattern(vec![O, T]);
    let result = evaluator
        .evaluate_language(
            &cache,
            root,
            &index,
            initial_hold(0, None),
            false,
            false,
            false,
            &control,
        )
        .unwrap();
    assert_eq!(result.coverage_bits.to_owned_words(), [1]);
    assert_eq!(evaluator.language_slots.ids.len(), 3);
    assert!(
        evaluator.product_masks.len()
            <= 4 * super::HOLD_STATE_COUNT * super::EXTRA_DRAW_STATE_COUNT
    );
    assert!(evaluator.retained_bytes() >= evaluator.language_slots.retained_bytes());

    let mut other_owner = PieceOrderLanguageCache::default();
    let other_root = other_owner.canonicalize(&chain(&[I])).unwrap();
    let result = evaluator
        .evaluate_language(
            &other_owner,
            other_root,
            &single_pattern(vec![I]),
            initial_hold(0, None),
            false,
            false,
            false,
            &control,
        )
        .unwrap();
    assert_eq!(result.coverage_bits.to_owned_words(), [1]);
    assert_eq!(evaluator.language_slots.ids.len(), 2);

    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    assert!(matches!(
        evaluator.evaluate_language(
            &cache,
            root,
            &index,
            initial_hold(0, None),
            false,
            false,
            false,
            &cancelled,
        ),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(evaluator.language_slots.ids.len(), 2);
}

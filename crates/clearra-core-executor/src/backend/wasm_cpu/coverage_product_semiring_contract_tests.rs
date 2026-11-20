//! Finite bounds for a *given* ranked placement graph. These tests neither
//! infer geometry from boards nor qualify the production reachability rules.

use std::collections::BTreeSet;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{
        piece_kind::PieceKind::{self, I, O, T},
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

use super::super::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
use super::{CoverageProductEvaluator, CoverageProductResult};

const ROOT: u32 = 0;
const AFTER_O: u32 = 1;
const COMPLETE: u32 = 2;
const DEAD: u32 = 3;
const RANKS: [u8; 4] = [0, 1, 2, 2];

#[derive(Clone, Copy)]
struct PhysicalEdge {
    from: u32,
    to: u32,
    kind: PieceKind,
    rotation: RotationState,
}

impl PhysicalEdge {
    const fn new(from: u32, to: u32, kind: PieceKind, rotation: RotationState) -> Self {
        Self {
            from,
            to,
            kind,
            rotation,
        }
    }
}

// Slots are distinct concrete realization identities, not extra copies of a
// kind edge. Slots 0, 1 and 2 have equal endpoints and kind but distinct poses.
const PHYSICAL: [PhysicalEdge; 5] = [
    PhysicalEdge::new(ROOT, AFTER_O, O, RotationState::Zero),
    PhysicalEdge::new(ROOT, AFTER_O, O, RotationState::Right),
    PhysicalEdge::new(ROOT, AFTER_O, O, RotationState::Left),
    PhysicalEdge::new(AFTER_O, COMPLETE, T, RotationState::Zero),
    PhysicalEdge::new(AFTER_O, DEAD, T, RotationState::Right),
];
const PLAIN_KIND: u8 = 0b0_1001;
const LOWER: u8 = 0b0_1011;
const UPPER: u8 = 0b1_1011;
const DIFFERENT_WITNESSES: u8 = 0b0_1101;

fn included_edges(mask: u8) -> impl Iterator<Item = (usize, PhysicalEdge)> {
    PHYSICAL
        .into_iter()
        .enumerate()
        .filter(move |&(slot, _)| mask & (1 << slot) != 0)
}

fn build_graph(mask: u8) -> BuildOrderGraph {
    let mut specs = Vec::new();
    let mut edges = Vec::new();
    for (node, depth) in RANKS.into_iter().enumerate() {
        let start = edges.len() as u32;
        let node = node as u32;
        for (_, edge) in included_edges(mask).filter(|(_, edge)| edge.from == node) {
            let operation = u8::from(node == AFTER_O);
            edges.push(BuildOrderGraph::edge(
                edge.to,
                operation,
                edge.kind,
                edge.rotation,
                0,
                0,
                0,
            ));
        }
        specs.push(BuildOrderNodeSpec {
            edge_start: start,
            edge_count: edges.len() as u32 - start,
            depth,
            accepting: node == COMPLETE,
        });
    }
    BuildOrderGraph::from_topological_parts(specs, edges, ROOT, 0, false).unwrap()
}

fn universe() -> MaterializedPatternUniverse {
    MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(63),
        PatternWeightModelId::new(21),
        vec![vec![O, T], vec![I, T], vec![T, O], vec![O, I]],
        [0.0, 0.25, 0.5, 0.25]
            .into_iter()
            .map(|weight| ProbabilityValue::new(weight).unwrap())
            .collect(),
        4,
        true,
        None,
    )
    .unwrap()
}

fn evaluate(
    graph: &BuildOrderGraph,
    index: &PatternPiecePositionIndex,
    count_paths: bool,
) -> CoverageProductResult {
    let initial_hold = HoldAutomatonState::new(
        PieceSourceId::new(63),
        0,
        Some(O),
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(63),
    );
    CoverageProductEvaluator::default()
        .evaluate(
            graph,
            index,
            initial_hold,
            true,
            false,
            false,
            count_paths,
            false,
            &ExecutionControl::default(),
        )
        .unwrap()
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum HoldAction {
    Current,
    SwapHeld,
}

type Trace = Vec<(usize, HoldAction)>;
type Witnesses = BTreeSet<Trace>;

/// Independent labeled product for this bounded fixture only: a Hold slot is
/// already occupied, neither lookahead nor terminal release is enabled, and
/// every step consumes precisely one current queue token. Preserve branches
/// even when Current and Swap have the same destination state.
fn labeled_witnesses(mask: u8, queue: &[PieceKind]) -> Witnesses {
    assert_eq!(queue.len(), 2);
    let mut witnesses = Witnesses::new();
    let mut work = vec![(ROOT, O, Trace::new())];
    while let Some((node, held, prefix)) = work.pop() {
        if node == COMPLETE {
            assert_eq!(prefix.len(), 2);
            assert!(witnesses.insert(prefix));
            continue;
        }
        if node == DEAD {
            continue;
        }
        let current = queue[prefix.len()];
        for (slot, edge) in included_edges(mask).filter(|(_, edge)| edge.from == node) {
            for (kind, action, next_held) in [
                (current, HoldAction::Current, held),
                (held, HoldAction::SwapHeld, current),
            ] {
                if kind == edge.kind {
                    let mut path = prefix.clone();
                    path.push((slot, action));
                    work.push((edge.to, next_held, path));
                }
            }
        }
    }
    witnesses
}

fn assert_product_reductions(
    mask: u8,
    universe: &MaterializedPatternUniverse,
    expected_counts: [u128; 4],
) -> [Witnesses; 4] {
    let graph = build_graph(mask);
    let full_index = PatternPiecePositionIndex::compile(universe).unwrap();
    let boolean = evaluate(&graph, &full_index, false);
    let count = evaluate(&graph, &full_index, true);
    assert!(count.count_complete);
    assert_eq!(count.path_count, expected_counts.into_iter().sum::<u128>());
    assert_eq!(count.coverage_bits, boolean.coverage_bits);
    assert_eq!(boolean.coverage_bits.to_owned_words(), [0b0111]);

    std::array::from_fn(|ordinal| {
        let pattern = PatternBitSet::from_pattern_indices(4, vec![ordinal as u32]).unwrap();
        let local =
            PatternPiecePositionIndex::compile_subset_before(universe, &pattern, 4).unwrap();
        let counted = evaluate(&graph, &local, true);
        let traces = labeled_witnesses(mask, universe.sequence_at(ordinal).as_ref());
        assert!(counted.count_complete, "original ordinal {ordinal}");
        assert_eq!(counted.path_count, expected_counts[ordinal]);
        assert_eq!(counted.path_count, traces.len() as u128);
        assert_eq!(
            counted.coverage_bits.to_owned_words(),
            [if traces.is_empty() {
                0
            } else {
                1_u64 << ordinal
            }]
        );
        traces
    })
}

#[test]
fn semiring_contract_matches_original_ordinals_but_not_erased_physical_or_hold_labels() {
    let universe = universe();
    let lower = assert_product_reductions(LOWER, &universe, [4, 2, 2, 0]);
    let kind = assert_product_reductions(PLAIN_KIND, &universe, [2, 1, 1, 0]);
    let different = assert_product_reductions(DIFFERENT_WITNESSES, &universe, [4, 2, 2, 0]);

    // Equal coverage is insufficient for CountAll; equal coverage and equal
    // counts without inclusion are still insufficient for replay identity.
    assert!(kind[0].is_subset(&lower[0]));
    assert_ne!(kind[0], lower[0]);
    assert_ne!(different[0], lower[0]);
    let current = [(0, HoldAction::Current), (3, HoldAction::Current)];
    let same_destination_swap = [(0, HoldAction::SwapHeld), (3, HoldAction::Current)];
    let other_pose = [(1, HoldAction::Current), (3, HoldAction::Current)];
    assert!(lower[0].contains(current.as_slice()));
    assert!(lower[0].contains(same_destination_swap.as_slice()));
    assert!(lower[0].contains(other_pose.as_slice()));
    assert!(!different[0].contains(other_pose.as_slice()));

    let reference = evaluate(
        &build_graph(LOWER),
        &PatternPiecePositionIndex::compile(&universe).unwrap(),
        false,
    );
    let accidentally_erased = PatternBitSet::from_pattern_indices(4, vec![1, 2]).unwrap();
    assert_ne!(reference.coverage_bits, accidentally_erased);
    assert_eq!(
        universe.weights().covered_weight(&reference.coverage_bits),
        Some(ProbabilityValue::new(0.75).unwrap())
    );
    assert_eq!(
        universe.weights().covered_weight(&reference.coverage_bits),
        universe.weights().covered_weight(&accidentally_erased)
    );
}

#[test]
fn semiring_contract_multiplicity_bounds_close_without_classifying_a_dead_edge() {
    let universe = universe();
    let lower = assert_product_reductions(LOWER, &universe, [4, 2, 2, 0]);
    let upper = assert_product_reductions(UPPER, &universe, [4, 2, 2, 0]);

    // An undecided physical edge is admitted only by the upper graph. Its
    // target is dead under either bound, so no accepting labeled path uses it.
    assert_ne!(LOWER, UPPER);
    assert_eq!(LOWER & UPPER, LOWER);
    assert!(!build_graph(UPPER).nodes[DEAD as usize].live);
    for (ordinal, (lo, hi)) in lower.iter().zip(&upper).enumerate() {
        assert!(lo.is_subset(hi));
        assert_eq!(lo.len(), hi.len(), "original ordinal {ordinal}");
        assert_eq!(lo, hi);
    }

    // The same proof cannot certify a graph that might contain another live
    // realization. The Boolean sets still agree, but the count bounds differ.
    let ambiguous = assert_product_reductions(LOWER | DIFFERENT_WITNESSES, &universe, [6, 3, 3, 0]);
    assert!(lower[0].is_subset(&ambiguous[0]));
    assert!(lower[0].len() < ambiguous[0].len());
}

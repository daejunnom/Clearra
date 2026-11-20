//! Supply oracles for a *given* shuffle of independent placement occurrences.
//! They do not infer that any physical placement pair is independent.

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::{
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_supply::{
    bag::BagState,
    hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
    pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex},
    piece_source::PieceSourceId,
    PatternUniverseMaterializer,
};

use crate::backend::wasm_cpu::{
    buildup::{BuildOrderGraph, BuildOrderNodeSpec},
    coverage_product::{CoverageProductEvaluator, CoverageProductResult},
    piece_order_language::PieceOrderLanguageCache,
    standard_bag_coverage::StandardBagCoverage,
};

fn initial(held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(77),
        0,
        held,
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(77),
    )
}

// Each fixture component consists of a single labeled operation. Explicit
// subsets keep both operation orders even when their emitted kinds coincide.
fn concrete_product(
    components: &[PieceKind],
    index: &PatternPiecePositionIndex,
    held: Option<PieceKind>,
    terminal_projection: bool,
    standard_bag_projection: bool,
) -> CoverageProductResult {
    assert!((1..=2).contains(&components.len()));
    let full = (1_u8 << components.len()) - 1;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for placed in 0..=full {
        let start = edges.len();
        for (op, &kind) in components.iter().enumerate() {
            let bit = 1_u8 << op;
            if placed & bit == 0 {
                edges.push(BuildOrderGraph::edge(
                    u32::from(placed | bit),
                    op as u8,
                    kind,
                    RotationState::Zero,
                    0,
                    0,
                    0,
                ));
            }
        }
        nodes.push(BuildOrderNodeSpec {
            edge_start: start as u32,
            edge_count: (edges.len() - start) as u32,
            depth: placed.count_ones() as u8,
            accepting: placed == full,
        });
    }
    let graph = BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, true).unwrap();
    let result = CoverageProductEvaluator::default()
        .evaluate(
            &graph,
            index,
            initial(held),
            true,
            terminal_projection,
            standard_bag_projection,
            true,
            false,
            &ExecutionControl::default(),
        )
        .unwrap();
    assert!(result.count_complete);
    result
}

fn symbolic_row(
    coverage: &mut StandardBagCoverage,
    languages: &mut PieceOrderLanguageCache,
    kinds: &[PieceKind],
    predecessors: &[u16],
) -> (u32, PatternBitSet) {
    let language = languages
        .compile_poset_language(kinds, predecessors, 4)
        .unwrap()
        .unwrap();
    let result = coverage
        .cover_language(languages, language.root, &ExecutionControl::default())
        .unwrap();
    let row = coverage.materialize_root(result.root).unwrap();
    assert_eq!(result.covered_pattern_count, row.count_ones() as usize);
    (result.root, row)
}

#[test]
fn component_shuffle_must_union_orders_after_shared_hold_and_terminal_release() {
    use PieceKind::{I, O};
    let source = PatternUniverseMaterializer::standard_7_bag(2, 0, 77).unwrap();
    assert_eq!(source.pattern_count(), 42);
    let index = PatternPiecePositionIndex::compile(&source).unwrap();
    let mut languages = PieceOrderLanguageCache::default();
    let mut projected = StandardBagCoverage::for_universe(&source, initial(None), true, true)
        .unwrap()
        .unwrap();
    let (io, io_row) = symbolic_row(&mut projected, &mut languages, &[I, O], &[0, 1]);
    let (oi, oi_row) = symbolic_row(&mut projected, &mut languages, &[I, O], &[2, 0]);
    let (_, shuffle) = symbolic_row(&mut projected, &mut languages, &[I, O], &[0, 0]);
    projected.merge_global(io).unwrap();
    projected.merge_global(oi).unwrap();
    assert_eq!(projected.materialize_global().unwrap(), shuffle);
    assert_eq!(io_row, shuffle);
    assert_eq!(oi_row, shuffle);

    let mut ordinary = StandardBagCoverage::for_universe(&source, initial(None), true, false)
        .unwrap()
        .unwrap();
    let (_, direct_io) = symbolic_row(&mut ordinary, &mut languages, &[I, O], &[0, 1]);
    let (_, can_place_i) = symbolic_row(&mut ordinary, &mut languages, &[I], &[0]);
    let (_, can_place_o) = symbolic_row(&mut ordinary, &mut languages, &[O], &[0]);
    for ordinal in 0..source.pattern_count() {
        let queue = source.sequence_at(ordinal);
        let id = PatternId::new(ordinal);
        assert_eq!(
            shuffle.contains(id),
            queue.as_ref() == [I, O] || queue.as_ref() == [O, I]
        );
        // OI -> IO requires storing O while placing I, then releasing O in
        // the other component after the final supplied draw. No private hold.
        assert_eq!(direct_io.contains(id), queue.as_ref() == [I, O]);
        assert_eq!(can_place_i.contains(id), queue.contains(&I));
        assert_eq!(can_place_o.contains(id), queue.contains(&O));
    }
    assert_eq!(
        (
            can_place_i.count_ones(),
            can_place_o.count_ones(),
            shuffle.count_ones()
        ),
        (12, 12, 2)
    );
    // 2/42 differs from (12/42)^2. Adding the two order events also counts
    // both successful queues twice, but action/occurrence counts must do so.
    let with_release = concrete_product(&[I, O], &index, None, true, true);
    let without_release = concrete_product(&[I, O], &index, None, false, false);
    assert_eq!(with_release.coverage_bits, shuffle);
    assert_eq!(without_release.coverage_bits, shuffle);
    assert_eq!(
        (with_release.path_count, without_release.path_count),
        (4, 2)
    );
}

#[test]
fn component_shuffle_cannot_duplicate_one_held_occurrence_or_erase_a_zero_weight_queue() {
    use PieceKind::{I, O, T};
    let source = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(77),
        PatternWeightModelId::new(19),
        vec![vec![O], vec![I], vec![T]],
        vec![
            ProbabilityValue::ZERO,
            ProbabilityValue::new(0.25).unwrap(),
            ProbabilityValue::new(0.75).unwrap(),
        ],
        3,
        true,
        None,
    )
    .unwrap();
    // An explicit weighted universe is outside the symbolic standard-bag
    // fast path. None is not evidence that the shuffle has zero coverage.
    assert!(
        StandardBagCoverage::for_universe(&source, initial(Some(O)), true, true)
            .unwrap()
            .is_none()
    );
    let index = PatternPiecePositionIndex::compile(&source).unwrap();
    let single = concrete_product(&[O], &index, Some(O), false, false);
    assert_eq!(single.coverage_bits.to_owned_words(), [0b111]);
    assert_eq!(
        source.weights().covered_weight(&single.coverage_bits),
        Some(ProbabilityValue::ONE)
    );

    let both = concrete_product(&[O, O], &index, Some(O), true, false);
    assert_eq!(both.coverage_bits.to_owned_words(), [0b001]);
    assert_eq!(
        source.weights().covered_weight(&both.coverage_bits),
        Some(ProbabilityValue::ZERO)
    );
    // Only queue [O] provides two occurrences: two concrete orders times
    // Current or SwapHeld on the first draw, then exactly one final release.
    assert_eq!(both.path_count, 4);
    let no_release = concrete_product(&[O, O], &index, Some(O), false, false);
    assert!(no_release.coverage_bits.is_empty());
    assert_eq!(no_release.path_count, 0);
}

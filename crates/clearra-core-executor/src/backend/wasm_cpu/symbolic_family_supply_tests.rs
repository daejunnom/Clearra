use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::universe::pattern_universe_id::PatternUniverseId;
use clearra_coverage::universe::pattern_weight_model_id::PatternWeightModelId;
use clearra_supply::bag::bag_state::BagState;
use clearra_supply::execution_automaton::{
    SupplyExecutionState, SupplyObservationIdentity, SupplyProvenanceId,
};
use clearra_supply::hold::hold_policy::HoldPolicy;
use clearra_supply::mixed::supply_provenance::{BagBoundaryEvidence, SupplyProvenance};
use clearra_supply::pattern_universe::materialized_pattern_universe::UniformCompactPatternSource;
use clearra_supply::pattern_universe::{MaterializedPatternUniverse, PieceMultisetKey};
use clearra_supply::piece_source::PieceSource;
use clearra_supply::queue::fixed_sequence::FixedSequence;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use super::super::geometry_partial_family::{ZddArena, ZddId, ZddLimits};
use super::{
    queue_variable, SupplyContract, SymbolicFamilySupply, SymbolicSupplyError, SymbolicSupplyState,
};

fn provenance() -> SupplyProvenance {
    SupplyProvenance::new(
        "standard-7-bag",
        "standard-tetrominoes",
        None,
        BagBoundaryEvidence::FixedBoundary,
        false,
        false,
    )
    .expect("finite source provenance")
}

fn pattern(expression: &str) -> PieceSource {
    PieceSource::queue_pattern_expression(
        QueuePatternExpression::parse(expression, 0).expect("source expression"),
        provenance(),
    )
    .expect("source universe")
}

fn contract(
    source: &PieceSource,
    hold_enabled: bool,
    held: Option<PieceKind>,
    projected: bool,
) -> SupplyContract {
    let universe = source.materialized_universe().expect("source universe");
    let standard_bag = matches!(
        universe.uniform_compact_source(),
        Some(UniformCompactPatternSource::Standard7Bag { .. })
    );
    SupplyContract {
        initial: SupplyExecutionState::with_contract(
            source.id(),
            source.kind(),
            0,
            held,
            if hold_enabled {
                HoldPolicy::Allowed
            } else {
                HoldPolicy::Forbidden
            },
            0,
            if standard_bag {
                BagState::fresh_standard_7_bag().packed_remainder_key()
            } else {
                0
            },
            SupplyObservationIdentity::full_queue_oracle(),
            SupplyProvenanceId(source.provenance().supply_provenance_id()),
        ),
        hold_enabled,
        projects_unplaced_lookahead: projected,
        projects_standard_bag_lookahead: projected && standard_bag,
        visible_len: universe.sequence_len_at(0),
    }
}

fn arena() -> ZddArena {
    ZddArena::new(ZddLimits::default())
}

#[test]
fn complete_segment_sources_compile_by_residual_masks_without_queue_expansion() {
    for (expression, count, draws, maximum_residuals) in [
        ("P1P7P7", 177_811_200, 15, 3 * 128),
        ("OP7P7", 25_401_600, 15, 3 * 128),
        ("P7P7", 25_401_600, 14, 2 * 128),
        ("[LOI]2P7", 30_240, 9, 2 * 128),
    ] {
        let source = pattern(expression);
        let mut arena = arena();
        let supply = SymbolicFamilySupply::compile_source(
            &source,
            contract(&source, true, None, true),
            31,
            &mut arena,
        )
        .expect("compact exact source");
        assert_eq!(supply.source_count(), count, "{expression}");
        assert_eq!(arena.count(supply.root()).unwrap(), count);
        assert_eq!(supply.visible_length(), draws);
        assert_eq!(supply.variable_count(), 31 + draws as u32 * 7);
        assert!(supply.compiled_residual_states() <= maximum_residuals);
    }
}

#[test]
fn each_position_is_one_hot_and_partial_segments_reset_at_their_real_boundary() {
    let source = pattern("[LOI]2P7");
    let mut arena = arena();
    let supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, false, None, false),
        5,
        &mut arena,
    )
    .unwrap();
    let first_l = arena
        .select(
            supply.root(),
            queue_variable(5, 0, PieceKind::L).unwrap(),
            true,
        )
        .unwrap();
    assert_eq!(arena.count(first_l).unwrap(), 10_080);
    let impossible_two_at_once = arena
        .select(first_l, queue_variable(5, 0, PieceKind::O).unwrap(), true)
        .unwrap();
    assert_eq!(impossible_two_at_once, ZddId::EMPTY);
    let forbidden_repeat = arena
        .select(first_l, queue_variable(5, 1, PieceKind::L).unwrap(), true)
        .unwrap();
    assert_eq!(forbidden_repeat, ZddId::EMPTY);
    let allowed_next_segment = arena
        .select(first_l, queue_variable(5, 2, PieceKind::L).unwrap(), true)
        .unwrap();
    assert_eq!(arena.count(allowed_next_segment).unwrap(), 1_440);
}

#[test]
fn hidden_factorized_draws_preserve_original_identity_but_are_not_consumable() {
    let expression = QueuePatternExpression::parse("P7", 0).unwrap().prefix(3);
    let source = PieceSource::queue_pattern_expression(expression, provenance()).unwrap();
    let mut arena = arena();
    let supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, false, None, false),
        2,
        &mut arena,
    )
    .unwrap();
    assert_eq!(supply.visible_length(), 3);
    assert_eq!(supply.variable_count(), 2 + 7 * 7);
    let mut root = supply.root();
    let mut state = supply.initial_state();
    for desired in [PieceKind::I, PieceKind::O, PieceKind::T] {
        let steps = supply
            .successors(&mut arena, root, state, desired, false)
            .unwrap();
        assert_eq!(steps.len(), 1);
        root = steps[0].root;
        state = steps[0].state;
    }
    assert_eq!(arena.count(root).unwrap(), 24);
    assert!(supply
        .successors(&mut arena, root, state, PieceKind::S, true)
        .unwrap()
        .is_empty());
}

#[test]
fn finite_hold_release_is_terminal_once_and_preserves_the_original_source() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
        provenance(),
    );
    let mut arena = arena();
    let supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, true, None, true),
        4,
        &mut arena,
    )
    .unwrap();
    let stored = supply
        .successors(
            &mut arena,
            supply.root(),
            supply.initial_state(),
            PieceKind::O,
            false,
        )
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].state.cursor(), 2);
    assert_eq!(stored[0].state.hold_piece(), Some(PieceKind::I));
    assert!(!supply.is_accepting(stored[0].state));
    assert!(supply
        .successors(
            &mut arena,
            stored[0].root,
            stored[0].state,
            PieceKind::I,
            false
        )
        .unwrap()
        .is_empty());
    let released = supply
        .successors(
            &mut arena,
            stored[0].root,
            stored[0].state,
            PieceKind::I,
            true,
        )
        .unwrap();
    assert_eq!(released.len(), 1);
    assert_eq!(released[0].root, supply.root());
    assert!(supply.is_accepting(released[0].state));
    assert!(supply
        .successors(
            &mut arena,
            released[0].root,
            released[0].state,
            PieceKind::I,
            true
        )
        .unwrap()
        .is_empty());
}

#[test]
fn inferred_standard_bag_current_cannot_be_bypassed_by_terminal_release() {
    let source = PieceSource::standard_7_bag(provenance(), 6, 0).unwrap();
    let mut arena = arena();
    let supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, true, Some(PieceKind::I), true),
        0,
        &mut arena,
    )
    .unwrap();
    let mut root = supply.root();
    for (position, piece) in [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
    ]
    .into_iter()
    .enumerate()
    {
        root = arena
            .select(root, queue_variable(0, position, piece).unwrap(), true)
            .unwrap();
    }
    assert_eq!(arena.count(root).unwrap(), 1);
    let state = SymbolicSupplyState {
        cursor: 6,
        hold: Some(PieceKind::I),
        terminal_projection_consumed: false,
    };
    let swap = supply
        .successors(&mut arena, root, state, PieceKind::I, true)
        .unwrap();
    assert_eq!(swap.len(), 1);
    assert_eq!(swap[0].state.cursor(), 7);
    assert_eq!(swap[0].state.hold_piece(), Some(PieceKind::L));
    assert!(!swap[0].state.terminal_projection_consumed);
    assert!(supply.is_accepting(swap[0].state));
    assert_eq!(swap[0].root, root);
    assert_eq!(supply.variable_count(), 6 * 7);
}

#[test]
fn source_guards_preserve_joint_placement_and_queue_correlations() {
    let source = pattern("P7");
    let mut arena = arena();
    let supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, false, None, false),
        2,
        &mut arena,
    )
    .unwrap();
    let first_i = arena
        .select(
            supply.root(),
            queue_variable(2, 0, PieceKind::I).unwrap(),
            true,
        )
        .unwrap();
    let first_o = arena
        .select(
            supply.root(),
            queue_variable(2, 0, PieceKind::O).unwrap(),
            true,
        )
        .unwrap();
    let row_i = arena.add(first_i, 0).unwrap();
    let row_o = arena.add(first_o, 1).unwrap();
    let joint = arena.union(row_i, row_o).unwrap();
    let steps = supply
        .successors(
            &mut arena,
            joint,
            supply.initial_state(),
            PieceKind::I,
            false,
        )
        .unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].root, row_i);
    assert_eq!(arena.count(steps[0].root).unwrap(), 720);
    let crossed = arena.select(steps[0].root, 1, true).unwrap();
    assert_eq!(crossed, ZddId::EMPTY);
    let variables = supply.source_variables().collect::<Vec<_>>();
    let placements = arena.project(joint, &variables).unwrap();
    assert_eq!(arena.count(placements).unwrap(), 2);
}

#[test]
fn arbitrary_explicit_multi_source_is_refused_without_ordinal_fallback() {
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(71),
        PatternWeightModelId::new(72),
        vec![vec![PieceKind::I], vec![PieceKind::O]],
        vec![ProbabilityValue::new(0.5).unwrap(); 2],
        2,
        true,
        None,
    )
    .unwrap();
    let source = PieceSource::materialized_pattern_universe(universe, provenance());
    let result = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, false, None, false),
        0,
        &mut arena(),
    );
    assert!(matches!(
        result,
        Err(SymbolicSupplyError::Unsupported(
            "source_has_no_compact_unique_queue_relation"
        ))
    ));
}

#[test]
fn finite_source_never_creates_a_new_draw_at_eof() {
    let source = PieceSource::fixed_queue(FixedSequence::new(vec![PieceKind::I]), provenance());
    let mut arena = arena();
    let supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, true, None, false),
        0,
        &mut arena,
    )
    .unwrap();
    let steps = supply
        .successors(
            &mut arena,
            supply.root(),
            supply.initial_state(),
            PieceKind::I,
            false,
        )
        .unwrap();
    assert_eq!(steps.len(), 1);
    assert!(supply.is_accepting(steps[0].state));
    for desired in PieceKind::STANDARD_TETROMINOES {
        assert!(supply
            .successors(&mut arena, steps[0].root, steps[0].state, desired, true)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn partial_fixed_horizons_have_exact_hold_inventories_without_source_enumeration() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O, PieceKind::T]),
        provenance(),
    );
    for (locks, expected) in [
        (1, vec![vec![PieceKind::I], vec![PieceKind::O]]),
        (
            2,
            vec![
                vec![PieceKind::I, PieceKind::O],
                vec![PieceKind::I, PieceKind::T],
                vec![PieceKind::O, PieceKind::T],
            ],
        ),
    ] {
        let mut arena = arena();
        let mut supply = SymbolicFamilySupply::compile_source(
            &source,
            contract(&source, true, None, false),
            10,
            &mut arena,
        )
        .unwrap();
        let actual = supply.reachable_multisets(&mut arena, locks).unwrap();
        let mut expected = expected
            .into_iter()
            .map(PieceMultisetKey::from_pieces)
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(actual, expected);
        assert_eq!(arena.count(supply.root()).unwrap(), 1);
    }
}

#[test]
fn standard_bag_inventory_dp_preserves_initial_hold_and_compacted_source_root() {
    let source = PieceSource::standard_7_bag(provenance(), 7, 0).unwrap();
    let mut arena = arena();
    let mut supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, true, Some(PieceKind::O), false),
        3,
        &mut arena,
    )
    .unwrap();
    let keys = supply.reachable_multisets(&mut arena, 2).unwrap();
    assert_eq!(keys.len(), 22);
    assert!(keys.contains(&PieceMultisetKey::from_pieces([PieceKind::O, PieceKind::O])));
    assert!(!keys.contains(&PieceMultisetKey::from_pieces([PieceKind::I, PieceKind::I])));
    assert!(keys.iter().all(|key| key.total_count() == 2));
    assert_eq!(arena.count(supply.root()).unwrap(), 5_040);
    let first_i = arena
        .select(
            supply.root(),
            queue_variable(3, 0, PieceKind::I).unwrap(),
            true,
        )
        .unwrap();
    assert_eq!(arena.count(first_i).unwrap(), 720);
}

#[test]
fn projected_full_draw_inventory_includes_only_original_pieces_at_finite_eof() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O, PieceKind::T]),
        provenance(),
    );
    let mut arena = arena();
    let mut supply = SymbolicFamilySupply::compile_source(
        &source,
        contract(&source, true, None, true),
        0,
        &mut arena,
    )
    .unwrap();
    assert_eq!(
        supply.reachable_multisets(&mut arena, 3).unwrap(),
        [PieceMultisetKey::from_pieces([
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ])]
    );
    assert_eq!(arena.count(supply.root()).unwrap(), 1);
    assert!(supply
        .reachable_multisets(&mut arena, 4)
        .unwrap()
        .is_empty());
    assert_eq!(arena.count(supply.root()).unwrap(), 1);
}

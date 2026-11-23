use clearra_core_domain::execution_cancellation::CancellationToken;

use super::super::{geometry_partial_family::ZddLimits, symbolic_boolean::BooleanLimits};
use super::*;

fn selected_member(
    sets: &mut ZddArena,
    selected: u32,
    row_count: u32,
    source: Option<u32>,
) -> ZddId {
    let mut root = ZddId::UNIT;
    if let Some(source) = source {
        root = sets.add(root, source).unwrap();
    }
    for row in 0..row_count {
        if selected & (1_u32 << row) != 0 {
            root = sets.add(root, row).unwrap();
        }
    }
    root
}

fn occupancy(cells: &[u64], initial: u64, selected: u32) -> u64 {
    cells
        .iter()
        .enumerate()
        .filter(|&(row, _)| selected & (1_u32 << row) != 0)
        .fold(initial, |board, (_, &cells)| board | cells)
}

fn predicate_from_truth(predicates: &mut BooleanArena, truth: u64, area: u32) -> BooleanId {
    fn compile(
        predicates: &mut BooleanArena,
        truth: u64,
        area: u32,
        variable: u32,
        assignment: u32,
    ) -> BooleanId {
        if variable == area {
            return if truth & (1_u64 << assignment) == 0 {
                BooleanId::FALSE
            } else {
                BooleanId::TRUE
            };
        }
        let low = compile(predicates, truth, area, variable + 1, assignment);
        let high = compile(
            predicates,
            truth,
            area,
            variable + 1,
            assignment | (1_u32 << variable),
        );
        predicates.mk(variable, low, high).unwrap()
    }
    compile(predicates, truth, area, 0, 0)
}

#[test]
fn symbolic_cell_domain_matches_every_three_row_family_under_or_occupancy() {
    let cells = vec![0b001, 0b011, 0b100];
    let control = ExecutionControl::default();
    for initial in [0, 0b010] {
        let mut sets = ZddArena::new(ZddLimits::default());
        let members: Vec<_> = (0..8)
            .map(|selected| selected_member(&mut sets, selected, 3, None))
            .collect();
        let mut predicates = BooleanArena::new(3, BooleanLimits::default()).unwrap();
        let mut bridge = CellFamilyBridge::new(cells.clone(), initial, 3, 0, 0).unwrap();
        for family in 0..256_u16 {
            let mut root = ZddId::EMPTY;
            for (selected, &member) in members.iter().enumerate() {
                if family & (1_u16 << selected) != 0 {
                    root = sets.union(root, member).unwrap();
                }
            }
            let image = bridge
                .domain(&sets, &mut predicates, root, &control)
                .unwrap();
            for board in 0..8_u64 {
                let expected = (0..8).any(|selected| {
                    family & (1_u16 << selected) != 0
                        && occupancy(&cells, initial, selected) == board
                });
                assert_eq!(
                    predicates.evaluate(image, |cell| board & (1_u64 << cell) != 0),
                    expected,
                    "initial={initial:b}, family={family:08b}, board={board:03b}",
                );
            }
        }
        assert!(bridge.work_steps() > 0);
    }
}

#[test]
fn symbolic_cell_filter_matches_every_three_cell_predicate_and_source_pair() {
    let cells = vec![0b001, 0b011, 0b100];
    let control = ExecutionControl::default();
    for initial in [0, 0b010] {
        let mut sets = ZddArena::new(ZddLimits::default());
        let mut root = ZddId::EMPTY;
        for selected in 0..8_u32 {
            let source = 3 + (selected.count_ones() & 1);
            let member = selected_member(&mut sets, selected, 3, Some(source));
            root = sets.union(root, member).unwrap();
        }
        let mut predicates = BooleanArena::new(3, BooleanLimits::default()).unwrap();
        let mut bridge = CellFamilyBridge::new(cells.clone(), initial, 3, 0, 0).unwrap();
        for truth in 0..256 {
            let condition = predicate_from_truth(&mut predicates, truth, 3);
            let filtered = bridge
                .filter(&mut sets, &mut predicates, root, condition, &control)
                .unwrap();
            for selected in 0..8_u32 {
                for source in 3..5 {
                    let variables = (0..3)
                        .filter(|&row| selected & (1_u32 << row) != 0)
                        .chain(std::iter::once(source));
                    let board = occupancy(&cells, initial, selected);
                    let expected =
                        source == 3 + (selected.count_ones() & 1) && truth & (1_u64 << board) != 0;
                    assert_eq!(
                        sets.contains(filtered, variables).unwrap(),
                        expected,
                        "initial={initial:b}, truth={truth:08b}, selected={selected:03b}, source={source}",
                    );
                }
            }
        }
        assert_eq!(sets.count(root).unwrap(), 8);
    }
}

#[test]
fn symbolic_cell_zero_suppression_keeps_another_rows_support_and_source_identity() {
    let mut sets = ZddArena::new(ZddLimits::default());
    let a = selected_member(&mut sets, 0b001, 3, Some(3));
    let b = selected_member(&mut sets, 0b100, 3, Some(4));
    let root = sets.union(a, b).unwrap();
    let mut predicates = BooleanArena::new(2, BooleanLimits::default()).unwrap();
    let mut bridge = CellFamilyBridge::new(vec![0b01, 0b10, 0b01], 0, 2, 0, 0).unwrap();
    let control = ExecutionControl::default();
    let present = predicates.variable(0).unwrap();
    let absent = predicates.variable(1).unwrap();
    let not_absent = predicates.not(absent).unwrap();
    for condition in [present, not_absent] {
        assert_eq!(
            bridge
                .filter(&mut sets, &mut predicates, root, condition, &control)
                .unwrap(),
            root,
        );
    }
    assert_eq!(
        bridge
            .filter(&mut sets, &mut predicates, root, absent, &control)
            .unwrap(),
        ZddId::EMPTY,
    );
    let image = bridge
        .domain(&sets, &mut predicates, root, &control)
        .unwrap();
    for board in 0..4_u64 {
        assert_eq!(
            predicates.evaluate(image, |cell| board & (1_u64 << cell) != 0),
            board == 1,
        );
    }
    assert_eq!(sets.count(root).unwrap(), 2);
    assert!(sets.contains(root, [0, 3]).unwrap());
    assert!(sets.contains(root, [2, 4]).unwrap());
    assert!(!sets.contains(root, [0, 4]).unwrap());
    assert!(!sets.contains(root, [2, 3]).unwrap());
}

#[test]
fn symbolic_cell_source_suffix_and_unit_are_exactly_the_initial_board() {
    let mut sets = ZddArena::new(ZddLimits::default());
    let queue_a = sets.add(ZddId::UNIT, 2).unwrap();
    let queue_b = sets.add(ZddId::UNIT, 9).unwrap();
    let queues = sets.union(queue_a, queue_b).unwrap();
    let mut predicates = BooleanArena::new(4, BooleanLimits::default()).unwrap();
    let mut bridge = CellFamilyBridge::new(vec![1, 2], 0b1010, 4, 0, 0).unwrap();
    let control = ExecutionControl::default();
    let initial_cell = predicates.variable(3).unwrap();
    let absent_cell = predicates.variable(2).unwrap();
    for root in [ZddId::UNIT, queues] {
        let image = bridge
            .domain(&sets, &mut predicates, root, &control)
            .unwrap();
        for board in 0..16_u64 {
            assert_eq!(
                predicates.evaluate(image, |cell| board & (1_u64 << cell) != 0),
                board == 0b1010,
            );
        }
        assert_eq!(
            bridge
                .filter(&mut sets, &mut predicates, root, initial_cell, &control)
                .unwrap(),
            root,
        );
        assert_eq!(
            bridge
                .filter(&mut sets, &mut predicates, root, absent_cell, &control)
                .unwrap(),
            ZddId::EMPTY,
        );
    }
    assert_eq!(
        bridge
            .domain(&sets, &mut predicates, ZddId::EMPTY, &control)
            .unwrap(),
        BooleanId::FALSE,
    );
}

#[test]
fn symbolic_cell_bit_sixty_three_is_a_cell_without_shift_overflow() {
    let initial = 1_u64 << 63;
    let mut sets = ZddArena::new(ZddLimits::default());
    let placed = sets.add(ZddId::UNIT, 0).unwrap();
    let root = sets.union(ZddId::UNIT, placed).unwrap();
    let mut predicates = BooleanArena::new(64, BooleanLimits::default()).unwrap();
    let mut bridge = CellFamilyBridge::new(vec![1], initial, 64, 0, 0).unwrap();
    let control = ExecutionControl::default();
    let image = bridge
        .domain(&sets, &mut predicates, root, &control)
        .unwrap();
    for board in [0, 1, initial, initial | 1, initial | 2] {
        assert_eq!(
            predicates.evaluate(image, |cell| board & (1_u64 << cell) != 0),
            board == initial || board == (initial | 1),
        );
    }
    let high_cell = predicates.variable(63).unwrap();
    assert_eq!(
        bridge
            .filter(&mut sets, &mut predicates, root, high_cell, &control)
            .unwrap(),
        root,
    );
}

#[test]
fn symbolic_cell_call_local_memos_do_not_alias_equal_ids_from_different_arenas() {
    let mut first = ZddArena::new(ZddLimits::default());
    let mut second = ZddArena::new(ZddLimits::default());
    let a = first.add(ZddId::UNIT, 0).unwrap();
    let b = second.add(ZddId::UNIT, 1).unwrap();
    assert_eq!(a, b);
    let mut predicates = BooleanArena::new(2, BooleanLimits::default()).unwrap();
    let mut bridge = CellFamilyBridge::new(vec![1, 2], 0, 2, 0, 0).unwrap();
    let control = ExecutionControl::default();
    let image_a = bridge.domain(&first, &mut predicates, a, &control).unwrap();
    let image_b = bridge
        .domain(&second, &mut predicates, b, &control)
        .unwrap();
    assert_ne!(image_a, image_b);
    let first_cell = predicates.variable(0).unwrap();
    let second_cell = predicates.variable(1).unwrap();
    assert_eq!(
        bridge
            .filter(&mut first, &mut predicates, a, first_cell, &control)
            .unwrap(),
        a,
    );
    assert_eq!(
        bridge
            .filter(&mut second, &mut predicates, b, second_cell, &control)
            .unwrap(),
        b,
    );
    assert_eq!(
        bridge
            .filter(&mut second, &mut predicates, b, first_cell, &control)
            .unwrap(),
        ZddId::EMPTY,
    );
    drop(second);
    assert!(bridge.support_owner.upgrade().is_none());
}

#[test]
fn symbolic_cell_support_reuses_immutable_nodes_and_resets_after_compaction() {
    let mut sets = ZddArena::new(ZddLimits::default());
    let discarded = sets.add(ZddId::UNIT, 0).unwrap();
    let mut predicates = BooleanArena::new(2, BooleanLimits::default()).unwrap();
    let first_cell = predicates.variable(0).unwrap();
    let second_cell = predicates.variable(1).unwrap();
    let mut bridge = CellFamilyBridge::new(vec![1, 2], 0, 2, 0, 0).unwrap();
    let control = ExecutionControl::default();
    assert_eq!(
        bridge
            .filter(&mut sets, &mut predicates, discarded, first_cell, &control)
            .unwrap(),
        discarded,
    );
    let support_work = bridge.support_work_steps();
    assert!(support_work > 0);
    assert_eq!(bridge.support_cache_entries(), 3);
    assert_eq!(
        bridge
            .filter(&mut sets, &mut predicates, discarded, first_cell, &control)
            .unwrap(),
        discarded,
    );
    assert_eq!(bridge.support_work_steps(), support_work);

    // Appending an unrelated node cannot change any cached old support.
    let retained = sets.add(ZddId::UNIT, 1).unwrap();
    assert_eq!(
        bridge
            .filter(&mut sets, &mut predicates, discarded, first_cell, &control)
            .unwrap(),
        discarded,
    );
    assert_eq!(bridge.support_work_steps(), support_work);
    let previous_epoch = sets.epoch();
    let mut roots = [retained];
    sets.compact(&mut roots).unwrap();
    assert_ne!(sets.epoch(), previous_epoch);
    assert_eq!(roots[0], discarded); // The numeric ID now names another row.
    assert_eq!(
        bridge
            .filter(&mut sets, &mut predicates, roots[0], second_cell, &control)
            .unwrap(),
        roots[0],
    );
    assert!(bridge.support_work_steps() > support_work);
    assert_eq!(bridge.support_cache_entries(), 3);

    let work_before_clear = bridge.support_work_steps();
    bridge.clear_support_cache();
    assert_eq!(bridge.support_cache_entries(), 0);
    assert!(bridge.support_owner.upgrade().is_none());
    assert_eq!(bridge.support_work_steps(), work_before_clear);
    assert_eq!(
        bridge
            .filter(&mut sets, &mut predicates, roots[0], second_cell, &control)
            .unwrap(),
        roots[0],
    );
    assert!(bridge.support_work_steps() > work_before_clear);
}

#[test]
fn symbolic_cell_invalid_masks_dimensions_and_ids_are_errors() {
    assert!(matches!(
        CellFamilyBridge::new(vec![], 0, 65, 0, 0),
        Err(SymbolicFamilyError::Unsupported("symbolic_cell_area")),
    ));
    for (rows, initial) in [(vec![4], 0), (vec![1], 4)] {
        assert!(matches!(
            CellFamilyBridge::new(rows, initial, 2, 0, 0),
            Err(SymbolicFamilyError::InvalidProblem("symbolic_cell_mask")),
        ));
    }
    let mut sets = ZddArena::new(ZddLimits::default());
    let mut predicates = BooleanArena::new(3, BooleanLimits::default()).unwrap();
    let mut bridge = CellFamilyBridge::new(vec![1], 0, 2, 0, 0).unwrap();
    let control = ExecutionControl::default();
    assert_eq!(
        bridge.domain(&sets, &mut predicates, ZddId::UNIT, &control),
        Err(SymbolicFamilyError::InvalidProblem(
            "symbolic_cell_arena_dimensions",
        )),
    );
    assert_eq!(
        bridge.filter(
            &mut sets,
            &mut predicates,
            ZddId::UNIT,
            BooleanId::TRUE,
            &control,
        ),
        Err(SymbolicFamilyError::InvalidProblem(
            "symbolic_cell_arena_dimensions",
        )),
    );
    let mut predicates = BooleanArena::new(2, BooleanLimits::default()).unwrap();
    let mut other_sets = ZddArena::new(ZddLimits::default());
    let outside = other_sets.add(ZddId::UNIT, 0).unwrap();
    assert_eq!(
        bridge.domain(&sets, &mut predicates, outside, &control),
        Err(SymbolicFamilyError::InvalidProblem("symbolic_set_family")),
    );
    assert_eq!(
        bridge.filter(
            &mut sets,
            &mut predicates,
            outside,
            BooleanId::TRUE,
            &control,
        ),
        Err(SymbolicFamilyError::InvalidProblem("symbolic_set_family")),
    );
}

#[test]
fn symbolic_cell_refusal_and_cancellation_never_return_an_empty_success() {
    let mut sets = ZddArena::new(ZddLimits::default());
    let root = sets.add(ZddId::UNIT, 0).unwrap();
    let mut predicates = BooleanArena::new(2, BooleanLimits::default()).unwrap();
    let condition = predicates.variable(0).unwrap();
    let control = ExecutionControl::default();
    for (max_work, max_memo, reason) in [
        (1, 0, "symbolic_cell_bridge_work_limit"),
        (0, 1, "symbolic_cell_bridge_memo_limit"),
    ] {
        let mut domain_bridge = CellFamilyBridge::new(vec![1], 0, 2, max_work, max_memo).unwrap();
        assert_eq!(
            domain_bridge.domain(&sets, &mut predicates, root, &control),
            Err(SymbolicFamilyError::Resource(reason)),
        );
        let mut filter_bridge = CellFamilyBridge::new(vec![1], 0, 2, max_work, max_memo).unwrap();
        assert_eq!(
            filter_bridge.filter(&mut sets, &mut predicates, root, condition, &control),
            Err(SymbolicFamilyError::Resource(reason)),
        );
        assert!(sets.contains(root, [0]).unwrap());
    }
    let stop = CancellationToken::new();
    let cancelled = ExecutionControl::new(stop.clone());
    stop.handle().cancel();
    let mut bridge = CellFamilyBridge::new(vec![1], 0, 2, 0, 0).unwrap();
    assert_eq!(
        bridge.domain(&sets, &mut predicates, root, &cancelled),
        Err(SymbolicFamilyError::Cancelled),
    );
    assert_eq!(
        bridge.filter(&mut sets, &mut predicates, root, condition, &cancelled),
        Err(SymbolicFamilyError::Cancelled),
    );
    let mut bounded = BooleanArena::new(
        2,
        BooleanLimits {
            max_nodes: 2,
            ..BooleanLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        bridge.domain(&sets, &mut bounded, ZddId::UNIT, &control),
        Err(SymbolicFamilyError::Resource(
            "symbolic_predicate_node_limit"
        )),
    );
}

use clearra_core_domain::board::board_size::BoardSize;
use clearra_core_executor::{
    area::{
        AreaComponent, AreaDecomposer, AreaScope, AreaTileabilityFailure, AreaTileabilityRules,
        RegionKind, ScenarioAreaPruner,
    },
    board::{board128_state::Board128State, wide_board_state::WideBoardState},
};
use clearra_geometry::layout::{
    board128_layout::Board128Layout, wide_board_layout::WideBoardLayout,
};

#[test]
fn area_decomposition_runs_through_non_board64_backend_families() {
    let board128 = {
        let layout = Board128Layout::new(BoardSize::new(8, 9).expect("size")).expect("layout");
        let wall = (0..9).fold(0_u128, |mask, y| mask | (1_u128 << (y * 8 + 3)));
        Board128State::new(layout, wall).expect("board")
    };
    assert_eq!(
        sorted(AreaDecomposer::empty_components(&board128).component_areas()),
        vec![27, 36]
    );

    let wide = {
        let layout = WideBoardLayout::new(BoardSize::new(12, 4).expect("size"));
        let wall = (0..4).map(|y| y * 12 + 5);
        WideBoardState::new(layout, wall).expect("board")
    };
    assert_eq!(
        sorted(AreaDecomposer::empty_components(&wide).component_areas()),
        vec![20, 24]
    );
}

#[test]
fn occupied_area_decomposition_reports_connected_components() {
    let layout = Board128Layout::new(BoardSize::new(9, 8).expect("size")).expect("layout");
    let occupied = (1_u128 << 0) | (1_u128 << 1) | (1_u128 << 9) | (1_u128 << 10) | (1_u128 << 8);
    let board = Board128State::new(layout, occupied).expect("board");

    assert_eq!(
        sorted(AreaDecomposer::occupied_components(&board).component_areas()),
        vec![1, 4]
    );
}

#[test]
fn tileability_uses_piece_area_rules_without_assuming_tetromino_only_runtime() {
    let standard = AreaTileabilityRules::standard_tetrominoes();
    let area_two = AreaComponent::new(RegionKind::Empty, vec![0, 1]);
    let area_four = AreaComponent::new(RegionKind::Empty, vec![0, 1, 2, 3]);

    let failed =
        clearra_core_executor::area::AreaTileabilityReport::check_component(&area_two, &standard);
    assert!(!failed.tileable());
    assert_eq!(
        failed.failure(),
        Some(AreaTileabilityFailure::ComponentAreaCannotBeComposed)
    );

    assert!(
        clearra_core_executor::area::AreaTileabilityReport::check_component(&area_four, &standard)
            .tileable()
    );

    let mixed_custom = AreaTileabilityRules::new([3, 4]).expect("custom area set");
    assert!(mixed_custom.can_compose_area(7));
    assert!(!mixed_custom.can_compose_area(2));
}

#[test]
fn mixed_piece_area_multiset_feasibility() {
    let mixed_custom = AreaTileabilityRules::new([4, 3, 3]).expect("custom area set");

    assert!(mixed_custom.can_compose_area(6));
    assert!(mixed_custom.can_compose_area(7));
    assert!(mixed_custom.can_compose_area(10));
    assert!(!mixed_custom.can_compose_area(5));
}

#[test]
fn missing_cells_mod_4_not_used_for_generic_feasibility() {
    let standard = AreaTileabilityRules::standard_tetrominoes();
    let generic = AreaTileabilityRules::new([3, 3]).expect("custom area set");

    assert!(!standard.can_compose_area(6));
    assert_ne!(6 % 4, 0);
    assert!(generic.can_compose_area(6));
}

#[test]
fn scenario_pruner_requires_an_explicit_area_scope() {
    let layout = Board128Layout::new(BoardSize::new(9, 8).expect("size")).expect("layout");
    let bottom_two_row_wall = (1_u128 << 1) | (1_u128 << 10);
    let board = Board128State::new(layout, bottom_two_row_wall).expect("board");

    assert_eq!(
        AreaDecomposer::empty_components(&board).component_areas(),
        vec![70]
    );

    let scoped =
        AreaDecomposer::decompose_in_scope(&board, RegionKind::Empty, AreaScope::rows_below(2));
    assert_eq!(sorted(scoped.component_areas()), vec![2, 14]);

    let decision = ScenarioAreaPruner::check_empty_components_below_rows(
        &board,
        2,
        &AreaTileabilityRules::standard_tetrominoes(),
    );
    assert!(decision.should_prune());
    assert_eq!(
        decision
            .failing_report()
            .map(|report| report.component_area()),
        Some(2)
    );
}

fn sorted(mut values: Vec<usize>) -> Vec<usize> {
    values.sort_unstable();
    values
}

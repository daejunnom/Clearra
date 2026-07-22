use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use super::*;

#[test]
fn generates_standard_placement_table_for_six_lines() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("10x6 fits in u64");
    let registry = standard_tetromino_registry();

    let table = PlacementTable::generate(layout, registry).expect("valid placement table");

    assert!(!table.is_empty());
    assert_eq!(table.layout(), layout);
    assert_eq!(table.backend_kind(), BoardBackendKind::Board64);
    assert!(table
        .placements()
        .iter()
        .all(|placement| placement.mask() & !layout.all_cells_mask() == 0));
}

#[test]
fn preserves_rotation_states_in_the_table() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("10x6 fits in u64");
    let registry = standard_tetromino_registry();

    let table = PlacementTable::generate(layout, registry).expect("valid placement table");

    assert_eq!(table.placements_for(PieceKind::O).count(), 45 * 4);
}

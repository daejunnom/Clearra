//! Physical witnesses for the selected-prefix negative-only ILC certificate.
//! Scalar drop/clear checks below do not call CandidateProjection or the
//! dependency closure when establishing that a fixture has a physical path.

use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{prefix_pruning::selected_prefix_stalls, GeometryCatalog};

fn fixture(
    width: u8,
    height: u8,
    initial: u64,
    ordered: &[(PieceKind, u64)],
) -> (GeometryCatalog, Vec<u32>) {
    assert!(u32::from(width) * u32::from(height) < 64);
    let full = (1_u64 << (u32::from(width) * u32::from(height))) - 1;
    let mut occupied = initial;
    for &(_, cells) in ordered {
        assert_eq!(cells.count_ones(), 4);
        assert_eq!(occupied & cells, 0);
        occupied |= cells;
    }
    assert_eq!(occupied, full);
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        width,
        height,
        initial,
        full & !initial,
    )
    .unwrap();
    let rows = ordered
        .iter()
        .map(|&(piece, cells)| catalog.skeleton_id(piece, cells).unwrap())
        .collect();
    (catalog, rows)
}

fn scalar_clear(width: u8, height: u8, board: u64) -> (u64, u16) {
    let row_mask = (1_u64 << width) - 1;
    let mut compacted = 0;
    let mut deleted = 0;
    let mut destination = 0;
    for row in 0..height {
        let cells = (board >> (u32::from(row) * u32::from(width))) & row_mask;
        if cells == row_mask {
            deleted |= 1 << row;
        } else {
            compacted |= cells << (destination * u32::from(width));
            destination += 1;
        }
    }
    (compacted, deleted)
}

fn scalar_drop_is_clear(width: u8, height: u8, board: u64, lock: u64) -> bool {
    let row_mask = (1_u64 << width) - 1;
    board & lock == 0
        && (lock & row_mask != 0 || board & (lock >> width) != 0)
        && (1..=height)
            .all(|distance| board & (lock << (u32::from(distance) * u32::from(width))) == 0)
}

/// Check a concrete vertical-drop path, preserving original row identity at
/// every clear. This is positive physics evidence only, never a negative
/// conclusion from failure to find one particular drop pose.
fn assert_harddrop_witness(
    catalog: &GeometryCatalog,
    ordered_rows: &[u32],
    expected_clear_counts: &[u32],
) {
    assert_eq!(ordered_rows.len(), expected_clear_counts.len());
    let width = catalog.width();
    let height = catalog.height();
    let mut board = catalog.initial_board();
    let mut logical = board;
    let mut deleted = 0_u16;
    for (&row_id, &expected_clears) in ordered_rows.iter().zip(expected_clear_counts) {
        let pose = catalog
            .instantiations(row_id, deleted)
            .find(|pose| scalar_drop_is_clear(width, height, board, pose.lock_mask))
            .expect("the recorded operation has an unobstructed grounded drop");
        let (next, cleared_current) = scalar_clear(width, height, board | pose.lock_mask);
        assert_eq!(cleared_current.count_ones(), expected_clears);
        let mut current = 0;
        let mut newly_deleted_original = 0;
        for original in 0..height {
            if deleted & (1 << original) == 0 {
                if cleared_current & (1 << current) != 0 {
                    newly_deleted_original |= 1 << original;
                }
                current += 1;
            }
        }
        deleted |= newly_deleted_original;
        logical |= catalog.skeleton(row_id).cells;
        // Independent reconstruction from all placed original cells checks
        // that a drop did not silently change its target-frame identity.
        assert_eq!(scalar_clear(width, height, logical), (next, deleted));
        board = next;
    }
    assert_eq!(board, 0);
    assert_eq!(deleted, (1_u16 << height) - 1);
}

fn assert_all_selected_subsets_survive(catalog: &GeometryCatalog, rows: &[u32]) {
    for subset in 0..(1_usize << rows.len()) {
        let mut selected = rows
            .iter()
            .enumerate()
            .filter_map(|(operation, &row)| (subset & (1 << operation) != 0).then_some(row))
            .collect::<Vec<_>>();
        assert!(
            !selected_prefix_stalls(catalog, &selected),
            "a physical completion exists: selected subset={subset:#x}"
        );
        // Selection order is not physical lock order. Reindexing the same
        // selected operations must preserve the contributor conjunctions.
        selected.reverse();
        assert!(
            !selected_prefix_stalls(catalog, &selected),
            "reversed selected subset={subset:#x}"
        );
    }
}

#[test]
fn physical_prefix_preserves_all_32768_subsets_of_full6l_mixed_witness() {
    use PieceKind::{I, J, L, O, S, T, Z};

    // Same actual inventory and witness as the executed buildup regression
    // geometric_edge_reuse_product_default_matches_full6l_mixed_finite_graph.
    // That test also checks the exact SRS+ graph and finite P1P7P7 product;
    // this test adds the selected-prefix certificate's full u15 domain.
    let ordered = [
        (I, 0x4010_0401_u64),
        (T, 0x100e),
        (T, 0x8070),
        (Z, 0x0003_0180),
        (S, 0x0300_6000),
        (S, 0x0006_00c0_0000),
        (J, 0x0001_8020_0800),
        (Z, 0x0018_0c00_0000),
        (L, 0x0060_1004_0000),
        (I, 0x0080_2008_0200),
        (I, 0x001e << 40),
        (O, 0x0003_00c0 << 40),
        (O, 0x000c_0300 << 40),
        (L, 0x1c01 << 40),
        (J, 0xe020 << 40),
    ];
    let (catalog, rows) = fixture(10, 6, 0, &ordered);
    assert_eq!(rows.len(), 15);
    for piece in PieceKind::STANDARD_TETROMINOES {
        assert_eq!(
            ordered.iter().filter(|(kind, _)| *kind == piece).count(),
            2 + usize::from(piece == I),
        );
    }
    let mut clears = [0; 15];
    clears[9] = 4;
    clears[14] = 2;
    assert_harddrop_witness(&catalog, &rows, &clears);
    assert_all_selected_subsets_survive(&catalog, &rows);
}

#[test]
fn physical_prefix_keeps_future_only_gap_rows_of_a_live_ilc_witness() {
    use PieceKind::O;

    // The last O occupies original rows 0 and 2, so original row 1 must
    // clear first. The right O pieces clear rows 1 then 3; the gapped left
    // O then lands in the two remaining rows and clears both together.
    let (catalog, rows) = fixture(4, 4, 0x3030, &[(O, 0x00cc), (O, 0xcc00), (O, 0x0303)]);
    let gapped = rows[2];
    assert_eq!(catalog.skeleton_occupied_rows(gapped), 0b0101);
    assert!(!catalog.realization_requirement_is_satisfied(gapped, 0));
    assert!(catalog.realization_requirement_is_satisfied(gapped, 0b0010));
    assert!(catalog.instantiations(gapped, 0).next().is_none());
    assert!(catalog.instantiations(gapped, 0b1010).next().is_some());
    assert_harddrop_witness(&catalog, &rows, &[1, 1, 2]);

    // In this selected prefix, row 1 has no selected contributor. It is a
    // full row of the final target and must be optimistically available.
    assert!(!selected_prefix_stalls(&catalog, &[gapped]));
    assert_all_selected_subsets_survive(&catalog, &rows);
}

#[test]
fn physical_prefix_rejects_a_selected_cycle_in_the_real_ilc_catalog() {
    use PieceKind::O;

    let (catalog, rows) = fixture(
        4,
        4,
        0,
        &[(O, 0x0303), (O, 0xc0c0), (O, 0x0c0c), (O, 0x3030)],
    );
    let a = rows[0];
    let b = rows[1];
    assert_eq!(catalog.skeleton_occupied_rows(a), 0b0101);
    assert_eq!(catalog.skeleton_occupied_rows(b), 0b1010);
    assert!(catalog
        .realizations(a)
        .iter()
        .all(|pose| pose.required_deleted_rows == 0b0010));
    assert!(catalog
        .realizations(b)
        .iter()
        .all(|pose| pose.required_deleted_rows == 0b0100));
    // Each singleton is still completable in the optimistic system. Taking
    // both selected operations keeps A->row1(B) and B->row2(A), so no future
    // operation or alternative concrete rotation can break this obstruction.
    assert!(!selected_prefix_stalls(&catalog, &[a]));
    assert!(!selected_prefix_stalls(&catalog, &[b]));
    assert!(selected_prefix_stalls(&catalog, &[a, b]));
    assert!(selected_prefix_stalls(&catalog, &[b, a]));
    assert!(selected_prefix_stalls(&catalog, &rows));
    // The explicit complete geometric tiling also has no first concrete
    // instantiation in frame zero, independently of movement/kick rules.
    assert!(rows
        .iter()
        .all(|&row| catalog.instantiations(row, 0).next().is_none()));
}

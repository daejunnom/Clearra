//! Exact one-edge materialization for a qualified four-row PC graph.
//!
//! Hydra-compatible graph states keep already cleared rows as full logical
//! rows at the bottom of the four-row target frame. This normalized frame can
//! change across an edge; it is not the request-wide inverse-lock-clear frame.
//! This module deliberately reuses Clearra's existing geometry catalog and
//! reachability engine instead of interpreting an edge as one preferred move.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::kicks::KickTableProfileId;

use super::{
    buildup::{compact_target_board, place_and_clear},
    catalog::GeometryCatalog,
    kick_profiles::builtin_kick_profile,
    reachability::ReachabilityWorkspace,
};

const WIDTH: u8 = 10;
const HEIGHT: u8 = 4;
const ROW_MASK: u64 = (1_u64 << WIDTH) - 1;
const FIELD_MASK: u64 = (1_u64 << (WIDTH * HEIGHT)) - 1;

/// One reachable physical lock that realizes a logical graph transition.
///
/// `occupied_cells` is in this edge's source-normalized frame. The path owner
/// must use the clear metadata to lift it into the request-wide ILC frame
/// before constructing a canonical tiling. `x`/`y` describe the physical lock.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4IlcPlacement {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    occupied_cells: u64,
    source_cleared_prefix: u8,
    physical_cleared_rows: u8,
}

impl Pc4IlcPlacement {
    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn rotation(self) -> RotationState {
        self.rotation
    }

    pub const fn x(self) -> i8 {
        self.x
    }

    pub const fn y(self) -> i8 {
        self.y
    }

    pub const fn occupied_cells(self) -> u64 {
        self.occupied_cells
    }

    pub const fn source_cleared_prefix(self) -> u8 {
        self.source_cleared_prefix
    }

    pub const fn physical_cleared_rows(self) -> u8 {
        self.physical_cleared_rows
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4IlcMaterializationError {
    SourceOutsideFourRows,
    TargetOutsideFourRows,
    SourceClearedRowsNotBottomPrefix,
    TargetClearedRowsNotBottomPrefix,
    TransitionRemovesLogicalCells,
    ClearedRowCountDecreased,
    TransitionAreaNotOneTetromino { cells: u32 },
    UnsupportedKickProfile,
    Geometry(&'static str),
}

impl Pc4IlcMaterializationError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::SourceOutsideFourRows => "pc4_ilc_source_outside_four_rows",
            Self::TargetOutsideFourRows => "pc4_ilc_target_outside_four_rows",
            Self::SourceClearedRowsNotBottomPrefix => {
                "pc4_ilc_source_cleared_rows_not_bottom_prefix"
            }
            Self::TargetClearedRowsNotBottomPrefix => {
                "pc4_ilc_target_cleared_rows_not_bottom_prefix"
            }
            Self::TransitionRemovesLogicalCells => "pc4_ilc_transition_removes_logical_cells",
            Self::ClearedRowCountDecreased => "pc4_ilc_cleared_row_count_decreased",
            Self::TransitionAreaNotOneTetromino { .. } => {
                "pc4_ilc_transition_area_not_one_tetromino"
            }
            Self::UnsupportedKickProfile => "pc4_ilc_unsupported_kick_profile",
            Self::Geometry(reason) => reason,
        }
    }
}

/// Recovers every Clearra-reachable placement represented by one graph edge.
///
/// The caller must still bind source/target hashes to a qualified immutable
/// graph generation. This function neither reads graph bytes nor grants
/// profile/target completeness. It only expands a single already-qualified
/// logical transition under the selected kick profile.
pub fn materialize_pc4_ilc_transition(
    source_cells: u64,
    target_cells: u64,
    piece: PieceKind,
    kick_profile: KickTableProfileId,
) -> Result<Vec<Pc4IlcPlacement>, Pc4IlcMaterializationError> {
    if source_cells & !FIELD_MASK != 0 {
        return Err(Pc4IlcMaterializationError::SourceOutsideFourRows);
    }
    if target_cells & !FIELD_MASK != 0 {
        return Err(Pc4IlcMaterializationError::TargetOutsideFourRows);
    }
    if source_cells.count_ones() > target_cells.count_ones() {
        return Err(Pc4IlcMaterializationError::TransitionRemovesLogicalCells);
    }
    if builtin_kick_profile(kick_profile).is_none() {
        return Err(Pc4IlcMaterializationError::UnsupportedKickProfile);
    }

    let source_deleted = bottom_full_row_prefix(source_cells)
        .ok_or(Pc4IlcMaterializationError::SourceClearedRowsNotBottomPrefix)?;
    let target_deleted = bottom_full_row_prefix(target_cells)
        .ok_or(Pc4IlcMaterializationError::TargetClearedRowsNotBottomPrefix)?;
    let added_area = target_cells.count_ones() - source_cells.count_ones();
    if added_area != 4 {
        return Err(Pc4IlcMaterializationError::TransitionAreaNotOneTetromino {
            cells: added_area,
        });
    }
    let source_prefix = source_deleted.count_ones() as u8;
    let target_prefix = target_deleted.count_ones() as u8;
    let Some(newly_cleared) = target_prefix.checked_sub(source_prefix) else {
        return Err(Pc4IlcMaterializationError::ClearedRowCountDecreased);
    };
    let physical_height = HEIGHT - source_prefix;
    let current_board = compact_target_board(WIDTH, HEIGHT, source_cells, source_deleted);
    let expected_board = compact_target_board(WIDTH, HEIGHT, target_cells, target_deleted);
    let mut reachability = ReachabilityWorkspace::default();
    reachability.configure(1);
    reachability.configure_kick_profile(kick_profile);

    let mut placements = Vec::new();
    // Reverse the target's normalization for every possible set of physical
    // rows removed by this lock. At most 16 masks exist in the four-row domain.
    for clear_rows in 0_u16..(1_u16 << physical_height) {
        if clear_rows.count_ones() != u32::from(newly_cleared) {
            continue;
        }
        let mut before_clear = 0_u64;
        let mut surviving_row = 0_u8;
        for row in 0..physical_height {
            let cells = if clear_rows & (1 << row) != 0 {
                ROW_MASK
            } else {
                let cells = (expected_board >> (surviving_row * WIDTH)) & ROW_MASK;
                surviving_row += 1;
                cells
            };
            before_clear |= cells << (row * WIDTH);
        }
        if current_board & !before_clear != 0 {
            continue;
        }
        let added_cells = before_clear & !current_board;
        if added_cells.count_ones() != 4 {
            continue;
        }
        let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
            WIDTH,
            physical_height,
            current_board,
            added_cells,
        )
        .map_err(|error| Pc4IlcMaterializationError::Geometry(error.reason()))?;
        let Some(row_id) = catalog.skeleton_id(piece, added_cells) else {
            continue;
        };
        for realization in catalog.instantiations(row_id, 0) {
            if current_board & realization.lock_mask != 0
                || !reachability.lock_reachable_instantiated(
                    &catalog,
                    current_board,
                    piece,
                    realization,
                )
            {
                continue;
            }
            let (next_board, cleared_current, _) = place_and_clear(
                WIDTH,
                physical_height,
                current_board | realization.lock_mask,
            );
            if cleared_current != clear_rows || next_board != expected_board {
                continue;
            }
            placements.push(Pc4IlcPlacement {
                piece,
                rotation: realization.rotation,
                x: realization.x,
                y: realization.lock_y,
                occupied_cells: added_cells << (source_prefix * WIDTH),
                source_cleared_prefix: source_prefix,
                physical_cleared_rows: clear_rows as u8,
            });
        }
    }
    placements.sort_unstable();
    placements.dedup();
    Ok(placements)
}

fn bottom_full_row_prefix(cells: u64) -> Option<u16> {
    let mut deleted = 0_u16;
    let mut saw_non_full = false;
    for row in 0..HEIGHT {
        let row_cells = (cells >> (usize::from(row) * usize::from(WIDTH))) & ROW_MASK;
        if row_cells == ROW_MASK {
            if saw_non_full {
                return None;
            }
            deleted |= 1_u16 << row;
        } else {
            saw_non_full = true;
        }
    }
    Some(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_hf_root_edges_equal_independent_empty_board_placements() {
        use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
        use std::collections::BTreeSet;

        // This immutable observation is compiled only into tests, never used
        // as a production dataset/profile identity or completeness receipt.
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/pc4-hf-root-20260913.json"
        ))
        .unwrap();
        assert_eq!(fixture["source_hash"], 0);
        let registry = standard_tetromino_registry();
        let mut total = 0;
        for (name, piece) in [
            ("I", PieceKind::I),
            ("J", PieceKind::J),
            ("L", PieceKind::L),
            ("O", PieceKind::O),
            ("S", PieceKind::S),
            ("T", PieceKind::T),
            ("Z", PieceKind::Z),
        ] {
            let mut observed = BTreeSet::new();
            let rows = fixture["pieces"][name].as_array().unwrap();
            for entry in rows {
                let hash = entry["hash"].as_u64().unwrap();
                let mut board = 0;
                for y in 0..4 {
                    for x in 0..10 {
                        if hash & (1 << (y * 10 + 9 - x)) != 0 {
                            board |= 1 << (y * 10 + x);
                        }
                    }
                }
                assert!(observed.insert(board), "duplicate root transition {name}");
                let placements =
                    materialize_pc4_ilc_transition(0, board, piece, KickTableProfileId::Jstris180)
                        .unwrap();
                assert!(!placements.is_empty(), "unrealized HF {name} hash={hash}");
                assert!(placements.iter().all(|p| p.occupied_cells() == board
                    && p.source_cleared_prefix() == 0
                    && p.physical_cleared_rows() == 0));
            }
            // On an empty board, each distinct rotation can be translated
            // horizontally and dropped to y=0. This does not call Geometry
            // or the graph materializer to generate the expected set.
            let mut expected = BTreeSet::new();
            let definition = registry.get(piece).unwrap();
            for rotation in RotationState::ALL {
                let shape = definition.shape(rotation);
                for x in 0..=(10 - shape.width()) {
                    let mut board = 0_u64;
                    for cell in shape.cells() {
                        board |= 1_u64
                            << (u32::from(cell.y() as u8) * 10
                                + u32::from(cell.x() as u8)
                                + u32::from(x));
                    }
                    expected.insert(board);
                }
            }
            assert_eq!(observed, expected, "HF vs local root set for {name}");
            total += rows.len();
        }
        assert_eq!(total, 162);
    }

    #[test]
    fn materializes_an_uncleared_root_edge_without_selecting_one_move() {
        let placements =
            materialize_pc4_ilc_transition(0, 0b1111, PieceKind::I, KickTableProfileId::SrsPlus)
                .expect("valid root edge");

        assert!(!placements.is_empty());
        assert!(placements.iter().all(|placement| {
            placement.piece() == PieceKind::I && placement.occupied_cells() == 0b1111
        }));
        assert!(placements
            .iter()
            .any(|placement| placement.x() == 0 && placement.y() == 0));
    }

    #[test]
    fn cleared_bottom_row_is_materialized_in_the_physical_frame() {
        let source = ROW_MASK | (0b11_1111_u64 << (WIDTH + 4));
        let target = ROW_MASK | (ROW_MASK << WIDTH);
        let projected_i = 0b1111_u64 << WIDTH;
        let placements =
            materialize_pc4_ilc_transition(source, target, PieceKind::I, KickTableProfileId::Srs90)
                .expect("valid post-clear edge");

        assert!(!placements.is_empty());
        assert!(placements
            .iter()
            .all(|placement| { placement.occupied_cells() == projected_i && placement.y() == 0 }));
    }

    #[test]
    fn upper_row_clear_recovers_the_physical_lock_despite_nonmonotone_graph_masks() {
        let source = 0b11000000 | (0b111111 << 10);
        let target = ROW_MASK | (0b11000000 << 10);
        assert_ne!(source & !target, 0);
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::Jstris180,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::NoKick,
        ] {
            let placements = materialize_pc4_ilc_transition(source, target, PieceKind::I, profile)
                .expect("valid graph normalization");
            assert!(!placements.is_empty(), "{profile:?}");
            assert!(placements
                .iter()
                .all(|p| p.occupied_cells() == 0b1111000000 << 10
                    && p.physical_cleared_rows() == 2
                    && p.source_cleared_prefix() == 0));
        }
    }

    #[test]
    fn rejects_non_monotone_and_noncanonical_graph_states() {
        assert_eq!(
            materialize_pc4_ilc_transition(0b1, 0, PieceKind::T, KickTableProfileId::SrsPlus,),
            Err(Pc4IlcMaterializationError::TransitionRemovesLogicalCells)
        );
        assert_eq!(
            materialize_pc4_ilc_transition(
                ROW_MASK << WIDTH,
                (ROW_MASK << WIDTH) | 0b1111,
                PieceKind::I,
                KickTableProfileId::SrsPlus,
            ),
            Err(Pc4IlcMaterializationError::SourceClearedRowsNotBottomPrefix)
        );
    }
}

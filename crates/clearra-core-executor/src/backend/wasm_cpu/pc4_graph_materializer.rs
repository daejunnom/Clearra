//! Exact one-edge materialization for a qualified four-row PC graph.
//!
//! Hydra-compatible graph states keep already cleared rows as full logical
//! rows at the bottom of the four-row target frame.  They therefore describe
//! inverse-lock-clear (ILC) cells, not a physical board after line clears.
//! This module deliberately reuses Clearra's existing geometry catalog and
//! reachability engine instead of interpreting an edge as one preferred move.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::kicks::KickTableProfileId;

use super::{
    buildup::{compact_target_board, merge_deleted_rows, place_and_clear},
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
/// `occupied_cells` remains in the four-row ILC target frame and is therefore
/// suitable for Clearra's canonical tiling identity. `x`/`y` describe the
/// actual lock before line clear and retain distinct replay realizations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4IlcPlacement {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    occupied_cells: u64,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4IlcMaterializationError {
    SourceOutsideFourRows,
    TargetOutsideFourRows,
    SourceClearedRowsNotBottomPrefix,
    TargetClearedRowsNotBottomPrefix,
    TransitionRemovesLogicalCells,
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
    if source_cells & !target_cells != 0 {
        return Err(Pc4IlcMaterializationError::TransitionRemovesLogicalCells);
    }
    if builtin_kick_profile(kick_profile).is_none() {
        return Err(Pc4IlcMaterializationError::UnsupportedKickProfile);
    }

    let source_deleted = bottom_full_row_prefix(source_cells)
        .ok_or(Pc4IlcMaterializationError::SourceClearedRowsNotBottomPrefix)?;
    let target_deleted = bottom_full_row_prefix(target_cells)
        .ok_or(Pc4IlcMaterializationError::TargetClearedRowsNotBottomPrefix)?;
    let added_cells = target_cells & !source_cells;
    if added_cells.count_ones() != 4 {
        return Err(Pc4IlcMaterializationError::TransitionAreaNotOneTetromino {
            cells: added_cells.count_ones(),
        });
    }

    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        WIDTH,
        HEIGHT,
        source_cells,
        added_cells,
    )
    .map_err(|error| Pc4IlcMaterializationError::Geometry(error.reason()))?;
    let Some(row_id) = catalog.skeleton_id(piece, added_cells) else {
        return Ok(Vec::new());
    };
    let current_board = compact_target_board(WIDTH, HEIGHT, source_cells, source_deleted);
    let expected_board = compact_target_board(WIDTH, HEIGHT, target_cells, target_deleted);
    let mut reachability = ReachabilityWorkspace::default();
    reachability.configure(1);
    reachability.configure_kick_profile(kick_profile);

    let mut placements = Vec::new();
    for realization in catalog.instantiations(row_id, source_deleted) {
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
        let (next_board, cleared_current, _) =
            place_and_clear(WIDTH, HEIGHT, current_board | realization.lock_mask);
        let Some(next_deleted) = merge_deleted_rows(HEIGHT, source_deleted, cleared_current) else {
            continue;
        };
        if next_deleted != target_deleted || next_board != expected_board {
            continue;
        }
        placements.push(Pc4IlcPlacement {
            piece,
            rotation: realization.rotation,
            x: realization.x,
            y: realization.lock_y,
            occupied_cells: added_cells,
        });
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

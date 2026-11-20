//! Bounded lower-bound evidence for a single admitted empty PC2 family.
//!
//! This verifier only certifies a kick-free lock sequence under the product's
//! canonical sky-entry model. It does not certify family completeness, any
//! other queue, or runs requiring a hold, kick, or non-harddrop approach.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::kicks::{KickTableProfileId, VerifiedKickTableProfile};

use crate::core_execution_result::CorePathStep;

use super::{
    buildup::{compact_target_board, merge_deleted_rows, place_and_clear},
    catalog::GeometryCatalog,
    kick_profiles::builtin_kick_profile,
    reachability::ReachabilityWorkspace,
};

const WIDTH: u8 = 10;
const HEIGHT: u8 = 2;
const LOCK_COUNT: usize = 5;
const FULL_ROW: u64 = (1_u64 << WIDTH) - 1;
const FULL_BOARD: u64 = (1_u64 << (WIDTH * HEIGHT)) - 1;
const ALL_ORIGINAL_ROWS: u16 = (1_u16 << HEIGHT) - 1;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Pc2ReplayProof {
    catalog_identity: u64,
    profile: KickTableProfileId,
    rows_in_lock_order: [u32; LOCK_COUNT],
    newly_cleared_original_rows: [u16; LOCK_COUNT],
}

/// The caller must admit the family identity and bind the expected prefix to
/// its fixed queue before supplying the exact catalog rows and representative.
/// `None` is not an absence proof; it is an unqualified representative.
pub(super) fn verify_compact_five_lock_replay(
    catalog: &GeometryCatalog,
    exact_row_ids: &[u32; LOCK_COUNT],
    verified_profile: &VerifiedKickTableProfile,
    expected_prefix: &[PieceKind; LOCK_COUNT],
    representative: &[CorePathStep],
) -> Option<Pc2ReplayProof> {
    let profile = verified_profile.profile();
    if catalog.width() != WIDTH
        || catalog.height() != HEIGHT
        || catalog.initial_board() != 0
        || catalog.required_cells() != FULL_BOARD
        || representative.len() != LOCK_COUNT
        || builtin_kick_profile(profile.profile_id())? != profile
    {
        return None;
    }

    let mut required = 0_u64;
    let mut rows = [None; LOCK_COUNT];
    for (&id, entry) in exact_row_ids.iter().zip(&mut rows) {
        let row = catalog.try_skeleton(id)?;
        if row.cells.count_ones() != 4 || row.cells & (!FULL_BOARD | required) != 0 {
            return None;
        }
        required |= row.cells;
        *entry = Some((id, row));
    }
    if required != FULL_BOARD {
        return None;
    }

    let mut workspace = ReachabilityWorkspace::default();
    let mut physical_board = 0_u64;
    let mut logical_placed = 0_u64;
    let mut deleted_rows = 0_u16;
    let mut rows_in_lock_order = [0_u32; LOCK_COUNT];
    let mut newly_cleared_original_rows = [0_u16; LOCK_COUNT];
    for (lock, (step, &expected_piece)) in representative.iter().zip(expected_prefix).enumerate() {
        if step.hold() != "use-current" || step.piece() != expected_piece {
            return None;
        }
        let rotation = RotationState::from_quarter_turns(step.rotation()).ok()?;
        let x = i8::try_from(step.x()).ok()?;
        let y = i8::try_from(step.y()).ok()?;
        let (index, id, row, realization) =
            rows.iter().enumerate().find_map(|(index, entry)| {
                let &(id, row) = entry.as_ref()?;
                if row.piece != expected_piece {
                    return None;
                }
                let mask = compact_target_board(WIDTH, HEIGHT, row.cells, deleted_rows);
                if mask.count_ones() != 4 || physical_board & mask != 0 {
                    return None;
                }
                catalog
                    .instantiations(id, deleted_rows)
                    .find(|realization| {
                        realization.rotation == rotation
                            && realization.x == x
                            && realization.lock_y == y
                            && realization.lock_mask == mask
                    })
                    .map(|realization| (index, id, row, realization))
            })?;
        if !workspace.lock_harddrop_reachable_instantiated(WIDTH, physical_board, realization) {
            return None;
        }

        let (next_board, physical_clears, cleared_lines) =
            place_and_clear(WIDTH, HEIGHT, physical_board | realization.lock_mask);
        let next_deleted_rows = merge_deleted_rows(HEIGHT, deleted_rows, physical_clears)?;
        let newly_cleared = next_deleted_rows & !deleted_rows;
        let next_placed = logical_placed | row.cells;
        if cleared_lines != step.cleared_lines()
            || cleared_lines != newly_cleared.count_ones() as u8
            || next_deleted_rows != full_original_rows(next_placed)
            || next_board != compact_target_board(WIDTH, HEIGHT, next_placed, next_deleted_rows)
        {
            return None;
        }

        rows[index] = None;
        rows_in_lock_order[lock] = id;
        newly_cleared_original_rows[lock] = newly_cleared;
        logical_placed = next_placed;
        physical_board = next_board;
        deleted_rows = next_deleted_rows;
    }

    (physical_board == 0 && deleted_rows == ALL_ORIGINAL_ROWS && logical_placed == FULL_BOARD)
        .then_some(Pc2ReplayProof {
            catalog_identity: catalog.identity_digest(),
            profile: profile.profile_id(),
            rows_in_lock_order,
            newly_cleared_original_rows,
        })
}

fn full_original_rows(board: u64) -> u16 {
    u16::from(board & FULL_ROW == FULL_ROW) | (u16::from(board >> WIDTH == FULL_ROW) << 1)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind::{I, O};
    use clearra_rules::kicks::SrsKicks;

    use super::*;

    const PREFIX: [PieceKind; LOCK_COUNT] = [I, I, O, O, O];

    fn fixture() -> (
        GeometryCatalog,
        VerifiedKickTableProfile,
        [u32; LOCK_COUNT],
        [CorePathStep; LOCK_COUNT],
    ) {
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(WIDTH, HEIGHT, 0, FULL_BOARD)
                .expect("empty PC2 catalog");
        let profile = VerifiedKickTableProfile::try_new(SrsKicks::profile()).expect("SRS 90 table");
        let rows = [
            (I, 0x0000f),
            (I, 0x03c00),
            (O, 0x0c030),
            (O, 0x300c0),
            (O, 0xc0300),
        ]
        .map(|(piece, mask)| catalog.skeleton_id(piece, mask).expect("literal PC2 row"));
        let path = [
            CorePathStep::new(I, 0, 0, 0, "use-current", 0),
            CorePathStep::new(I, 0, 0, 1, "use-current", 0),
            CorePathStep::new(O, 0, 4, 0, "use-current", 0),
            CorePathStep::new(O, 0, 6, 0, "use-current", 0),
            CorePathStep::new(O, 0, 8, 0, "use-current", 2),
        ];
        (catalog, profile, rows, path)
    }

    #[test]
    fn distributed_pc2_replay_verifies_real_iiooo_harddrops_and_atomic_two_clear() {
        let (catalog, profile, rows, path) = fixture();
        let mut canonical_rows = rows;
        canonical_rows.reverse();
        let proof =
            verify_compact_five_lock_replay(&catalog, &canonical_rows, &profile, &PREFIX, &path)
                .expect("five real harddrops clear exactly two original rows");
        assert_eq!(proof.catalog_identity, catalog.identity_digest());
        assert_eq!(proof.profile, KickTableProfileId::Srs90);
        assert_eq!(proof.rows_in_lock_order, rows);
        assert_eq!(proof.newly_cleared_original_rows, [0, 0, 0, 0, 0b11]);
    }

    #[test]
    fn distributed_pc2_replay_resolves_compacted_lock_y_into_original_rows() {
        let (catalog, profile, _, _) = fixture();
        let rows = [
            (I, 0x0000f),
            (I, 0x000f0),
            (O, 0xc0300),
            (I, 0x03c00),
            (I, 0x3c000),
        ]
        .map(|(piece, mask)| catalog.skeleton_id(piece, mask).expect("literal PC2 row"));
        let expected = [I, I, O, I, I];
        let mut path = [
            CorePathStep::new(I, 0, 0, 0, "use-current", 0),
            CorePathStep::new(I, 0, 4, 0, "use-current", 0),
            CorePathStep::new(O, 0, 8, 0, "use-current", 1),
            CorePathStep::new(I, 0, 0, 0, "use-current", 0),
            CorePathStep::new(I, 0, 4, 0, "use-current", 1),
        ];
        let proof = verify_compact_five_lock_replay(&catalog, &rows, &profile, &expected, &path)
            .expect("third and fifth locks clear distinct original rows");
        assert_eq!(proof.rows_in_lock_order, rows);
        assert_eq!(proof.newly_cleared_original_rows, [0, 0, 0b01, 0, 0b10]);

        path[3] = CorePathStep::new(I, 0, 0, 1, "use-current", 0);
        assert!(
            verify_compact_five_lock_replay(&catalog, &rows, &profile, &expected, &path).is_none()
        );
    }

    #[test]
    fn distributed_pc2_replay_rejects_unsupported_order_hold_and_non_catalog_pose() {
        let (catalog, profile, rows, path) = fixture();
        let verify = |candidate: &[CorePathStep]| {
            verify_compact_five_lock_replay(&catalog, &rows, &profile, &PREFIX, candidate)
        };
        let mut swapped = path.clone();
        swapped.swap(0, 2);
        assert!(verify(&swapped).is_none());
        let mut floating = path.clone();
        floating.swap(0, 1);
        assert!(verify(&floating).is_none());
        let mut altered = path.clone();
        for step in [
            CorePathStep::new(I, 0, 0, 0, "hold-swap", 0),
            CorePathStep::new(I, 1, 0, 0, "use-current", 0),
            CorePathStep::new(I, 4, 0, 0, "use-current", 0),
            CorePathStep::new(I, 0, 1, 0, "use-current", 0),
            CorePathStep::new(I, 0, 256, 0, "use-current", 0),
        ] {
            altered[0] = step;
            assert!(verify(&altered).is_none());
        }
    }

    #[test]
    fn distributed_pc2_replay_rejects_overlap_bad_rows_and_forged_clear_claims() {
        let (catalog, profile, rows, path) = fixture();
        let verify = |candidate_rows: &[u32; LOCK_COUNT], candidate: &[CorePathStep]| {
            verify_compact_five_lock_replay(&catalog, candidate_rows, &profile, &PREFIX, candidate)
        };
        let mut overlap = path.clone();
        overlap[3] = path[2].clone();
        assert!(verify(&rows, &overlap).is_none());
        assert!(verify(&rows, &path[..LOCK_COUNT - 1]).is_none());

        let mut malformed_rows = rows;
        malformed_rows[0] = rows[1];
        assert!(verify(&malformed_rows, &path).is_none());
        malformed_rows[0] = u32::MAX;
        assert!(verify(&malformed_rows, &path).is_none());

        let mut false_clear = path.clone();
        false_clear[2] = CorePathStep::new(O, 0, 4, 0, "use-current", 1);
        assert!(verify(&rows, &false_clear).is_none());
        false_clear[2] = path[2].clone();
        false_clear[4] = CorePathStep::new(O, 0, 8, 0, "use-current", 1);
        assert!(verify(&rows, &false_clear).is_none());
    }
}

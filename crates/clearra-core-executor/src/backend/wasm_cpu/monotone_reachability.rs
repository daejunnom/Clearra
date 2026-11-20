//! Exact Boolean O reachability without a pose graph or visited-state storage.
//!
//! Admission proves from the actual registry and kick entries that all O
//! rotations occupy the same normalized 2x2 cells and cannot translate the
//! piece. The connected solver's sky seeds include every collision-free x and
//! rotation above the board. A path is therefore a sequence of horizontal
//! components at monotonically decreasing anchor heights. Reversing that
//! sequence gives the row-bitset recurrence below.
//!
//! This is a spawn-to-lock Boolean primitive under the existing solver's sky
//! contract. It is not a finite-lock-delay, finesse, spin or arbitrary-entry
//! relation. A caller must also preserve the original line-clear frame: cells
//! below the target cannot affect this movement, but clearing their row can
//! change the coordinate system.

use std::sync::OnceLock;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{KickTableProfile, KickTableProfileId};

use crate::conditioned_local_relation::LocalRelationRowFrame;

use super::{builtin_kick_profile, normalized_kick_delta};

/// Constructible only after the registry/profile monotonicity proof succeeds.
/// The proof is independent of width, height, board occupancy and target pose.
#[derive(Debug)]
pub(super) struct MonotoneOCapability {
    _private: (),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MonotoneOResult {
    pub reachable: bool,
    /// Number of physical anchor rows evaluated, not primitive pose states.
    pub row_steps: u8,
    /// Holding this occupancy fixed preserves the Boolean result. Only this
    /// query's fixed physical board/frame and admitted profile are covered.
    pub influence_mask: u64,
}

/// Positive-only interval admission. `proven == false` is Unknown and must
/// return to another proof or the ordinary exact producer; it is not a claim
/// that any board in the interval is unreachable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MonotoneOIntervalAttempt {
    pub proven: bool,
    /// Includes rows spent on an unsuccessful maximum-board proof.
    pub row_steps: u8,
}

impl MonotoneOCapability {
    /// Cache the structural proof once per immutable connected profile.
    /// A matching name alone is never the proof: `compile` checks the shapes
    /// and all enabled O kick offsets before returning the capability.
    pub fn for_profile(profile_id: KickTableProfileId) -> Option<&'static Self> {
        macro_rules! admitted {
            () => {{
                static CAPABILITY: OnceLock<Option<MonotoneOCapability>> = OnceLock::new();
                CAPABILITY
                    .get_or_init(|| Self::compile(builtin_kick_profile(profile_id)?))
                    .as_ref()
            }};
        }
        match profile_id {
            KickTableProfileId::Srs90 => admitted!(),
            KickTableProfileId::SrsPlus => admitted!(),
            KickTableProfileId::SrsX => admitted!(),
            KickTableProfileId::Jstris180 => admitted!(),
            KickTableProfileId::NoKick => admitted!(),
            KickTableProfileId::Asc
            | KickTableProfileId::Ars
            | KickTableProfileId::Imported
            | KickTableProfileId::Custom => None,
        }
    }

    fn compile(profile: &KickTableProfile) -> Option<Self> {
        let definition = standard_tetromino_registry().get(PieceKind::O)?;
        for rotation in RotationState::ALL {
            let mut cells = 0_u8;
            for cell in definition.shape(rotation).cells() {
                if !(0..=1).contains(&cell.x()) || !(0..=1).contains(&cell.y()) {
                    return None;
                }
                cells |= 1 << (cell.y() * 2 + cell.x());
            }
            if cells != 0b1111 {
                return None;
            }
        }
        for entry in profile.entries() {
            let transition = entry.transition();
            if transition.piece() != PieceKind::O
                || (!profile.supports_180() && transition.is_180())
            {
                continue;
            }
            // Even a later nonzero offset is conservatively unsupported.
            // This proof does not assume an earlier origin attempt dominates
            // that offset under every future representation of the profile.
            for offset in entry.sequence().offsets() {
                if normalized_kick_delta(
                    PieceKind::O,
                    transition.from(),
                    transition.to(),
                    offset.dx(),
                    offset.dy(),
                ) != (0, 0)
                {
                    return None;
                }
            }
        }
        Some(Self { _private: () })
    }

    /// Return an exact lock answer and an occupancy-invariance certificate.
    /// Invalid dimensions/poses are Unknown (`None`), not negative pruning.
    /// All four normalized rotations share this physical result by admission.
    pub fn lock_reachable(
        &self,
        width: u8,
        height: u8,
        board: u64,
        _rotation: RotationState,
        x: i8,
        y: i8,
    ) -> Option<MonotoneOResult> {
        if !(2..=10).contains(&width)
            || !(1..=6).contains(&height)
            || x < 0
            || x >= width as i8 - 1
            || y < 0
            || y >= height as i8
        {
            return None;
        }
        let board_bits = u32::from(width) * u32::from(height);
        if board_bits >= 64 || board >> board_bits != 0 {
            return None;
        }
        let row_cells = (1_u16 << width) - 1;
        let anchor_mask = (1_u16 << (width - 1)) - 1;
        let target_anchor = 1_u16 << x;
        let support = if y == 0 {
            0
        } else {
            u64::from(target_anchor | (target_anchor << 1)) << (u32::from(width) * (y as u32 - 1))
        };
        if y > 0 && board & support == 0 {
            return Some(MonotoneOResult {
                reachable: false,
                row_steps: 0,
                influence_mask: support,
            });
        }

        let mut influence_mask = support;
        let mut frontier = target_anchor;
        let mut row_steps = 0;
        for row in y as u8..height {
            let occupied = board_row(board, width, height, row, row_cells)
                | board_row(board, width, height, row + 1, row_cells);
            let free_anchors = !(occupied | (occupied >> 1)) & anchor_mask;
            let incoming = frontier;
            frontier = horizontal_closure(incoming, free_anchors);
            row_steps += 1;

            // The closure is the union of free intervals touched by incoming
            // anchors. Preserve every such interval, its adjacent blocked
            // anchors, and every blocked incoming anchor. Changes outside
            // these cells cannot change the selected horizontal components.
            let inspected = (incoming | frontier | (frontier << 1) | (frontier >> 1)) & anchor_mask;
            let inspected_cells = (inspected | (inspected << 1)) & row_cells;
            influence_mask |= cells_in_row(inspected_cells, width, height, row);
            influence_mask |= cells_in_row(inspected_cells, width, height, row + 1);

            if frontier == 0 {
                return Some(MonotoneOResult {
                    reachable: false,
                    row_steps,
                    influence_mask,
                });
            }
            // Reversing a downward step keeps the anchor x. At the next row
            // only its free-anchor test remains before horizontal closure.
        }
        // Above the last physical row all admissible x/rotations are sky
        // seeds; a nonempty frontier therefore reaches a real source.
        Some(MonotoneOResult {
            reachable: true,
            row_steps,
            influence_mask,
        })
    }

    /// Certify every physical board `minimum <= board <= maximum` without a
    /// cached observation or intermediate-board enumeration.
    ///
    /// With O's admitted zero-translation rotations, removing obstacles cannot
    /// destroy a movement path. A reachable lock on the maximum board supplies
    /// a path on every smaller board, while grounding on the minimum board
    /// supplies support on every larger board. These are different obligations:
    /// maximum-board grounding alone would be unsound.
    ///
    /// The caller supplies one ILC realization's exact four-cell lock mask and
    /// must separately establish the original-row contributor conditions. This
    /// method checks physical frame capacity and pose/mask equality, not those
    /// candidate-specific contributor conditions. The current shared
    /// `LocalRelationRowFrame` contract supports ten columns only; unlike the
    /// single-board primitive, this interval API preserves that restriction.
    #[allow(clippy::too_many_arguments)]
    pub fn proves_lock_interval(
        &self,
        width: u8,
        height: u8,
        minimum: u64,
        maximum: u64,
        rotation: RotationState,
        x: i8,
        y: i8,
        expected_lock_mask: u64,
        frame: LocalRelationRowFrame,
    ) -> Option<MonotoneOIntervalAttempt> {
        if width != 10
            || !(1..=6).contains(&height)
            || x < 0
            || x >= width as i8 - 1
            || y < 0
            || y >= height as i8 - 1
            || minimum & !maximum != 0
            || frame.target_height() != height
            || !frame.accepts_physical_board(width, minimum)
            || !frame.accepts_physical_board(width, maximum)
        {
            return None;
        }
        let bottom = 3_u64 << (x as u32 + y as u32 * u32::from(width));
        let lock_mask = bottom | (bottom << width);
        if expected_lock_mask != lock_mask || !frame.accepts_physical_board(width, lock_mask) {
            return None;
        }
        if y > 0 && minimum & (bottom >> width) == 0 {
            return Some(MonotoneOIntervalAttempt {
                proven: false,
                row_steps: 0,
            });
        }
        let maximum_result = self.lock_reachable(width, height, maximum, rotation, x, y)?;
        Some(MonotoneOIntervalAttempt {
            proven: maximum_result.reachable,
            row_steps: maximum_result.row_steps,
        })
    }
}

#[inline]
fn board_row(board: u64, width: u8, height: u8, row: u8, row_cells: u16) -> u16 {
    if row >= height {
        0
    } else {
        ((board >> (u32::from(width) * u32::from(row))) as u16) & row_cells
    }
}

#[inline]
fn cells_in_row(cells: u16, width: u8, height: u8, row: u8) -> u64 {
    if row >= height {
        0
    } else {
        u64::from(cells) << (u32::from(width) * u32::from(row))
    }
}

/// Bit-parallel horizontal component closure in four fixed doubling steps.
/// `left`/`right` certify every intervening free anchor for a jump, so this is
/// algebraic propagation through intervals, not a jump over blocked anchors.
#[inline]
fn horizontal_closure(seeds: u16, free: u16) -> u16 {
    let mut reached = seeds & free;
    let mut left = free;
    let mut right = free;
    for shift in [1, 2, 4, 8] {
        reached |= ((reached << shift) & left) | ((reached >> shift) & right);
        left &= left << shift;
        right &= right >> shift;
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_rules::{
        kicks::{KickOffset, KickOffsetSequence, KickTableEntry, KickTransition},
        profile::rule_profile::RuleProfileId,
    };

    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];

    #[test]
    fn capability_checks_actual_profile_offsets_not_the_profile_name() {
        for profile in PROFILES {
            assert!(MonotoneOCapability::for_profile(profile).is_some());
        }
        for profile in [
            KickTableProfileId::Asc,
            KickTableProfileId::Ars,
            KickTableProfileId::Imported,
            KickTableProfileId::Custom,
        ] {
            assert!(MonotoneOCapability::for_profile(profile).is_none());
        }
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let counterfeit = KickTableProfile::new(
                KickTableProfileId::Srs90,
                RuleProfileId::Srs,
                vec![KickTableEntry::new(
                    KickTransition::new(PieceKind::O, RotationState::Zero, RotationState::Right),
                    KickOffsetSequence::new(vec![KickOffset::new(dx, dy)]),
                )],
            );
            assert!(MonotoneOCapability::compile(&counterfeit).is_none());
        }
    }

    #[test]
    fn horizontal_interval_closure_matches_every_nine_bit_graph() {
        for free in 0..1_u16 << 9 {
            for seeds in 0..1_u16 << 9 {
                let mut expected = seeds & free;
                loop {
                    let next = (expected | (expected << 1) | (expected >> 1)) & free;
                    if next == expected {
                        break;
                    }
                    expected = next;
                }
                assert_eq!(horizontal_closure(seeds, free), expected);
            }
        }
    }

    #[test]
    fn all_small_boards_match_independent_profile_bfs_in_every_rotation() {
        for profile in PROFILES {
            let capability = MonotoneOCapability::for_profile(profile).unwrap();
            for (width, height) in [(2, 3), (3, 4), (4, 3)] {
                for board in 0..1_u64 << (width * height) {
                    let expected = crate::reachability_reference::reference_spawn_lock_anchors(
                        width,
                        height,
                        board,
                        PieceKind::O,
                        profile,
                    )
                    .unwrap();
                    for rotation in RotationState::ALL {
                        for y in 0..height {
                            for x in 0..width - 1 {
                                let result = capability
                                    .lock_reachable(
                                        width, height, board, rotation, x as i8, y as i8,
                                    )
                                    .unwrap();
                                let anchor = 1_u64 << (y * width + x);
                                assert_eq!(
                                    result.reachable,
                                    expected[rotation.quarter_turns() as usize] & anchor != 0,
                                    "profile={profile:?}, {width}x{height}, board={board:#x}, ({x},{y}) {rotation:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn footprint_excludes_only_cells_proven_irrelevant_for_all_changes() {
        let profile = KickTableProfileId::Jstris180;
        let capability = MonotoneOCapability::for_profile(profile).unwrap();
        let (width, height) = (3, 3);
        let board_mask = (1_u64 << (width * height)) - 1;
        let expected = (0..=board_mask)
            .map(|board| {
                crate::reachability_reference::reference_spawn_lock_anchors(
                    width,
                    height,
                    board,
                    PieceKind::O,
                    profile,
                )
                .unwrap()[0]
            })
            .collect::<Vec<_>>();
        for board in 0..=board_mask {
            for y in 0..height {
                for x in 0..width - 1 {
                    let result = capability
                        .lock_reachable(width, height, board, RotationState::Zero, x as i8, y as i8)
                        .unwrap();
                    assert_eq!(result.influence_mask & !board_mask, 0);
                    let variable = board_mask & !result.influence_mask;
                    let fixed = board & result.influence_mask;
                    let mut assignment = variable;
                    loop {
                        let other = fixed | assignment;
                        let anchor = 1_u64 << (y * width + x);
                        assert_eq!(
                            result.reachable,
                            expected[other as usize] & anchor != 0,
                            "board={board:#x}, other={other:#x}, mask={:#x}, target=({x},{y})",
                            result.influence_mask
                        );
                        if assignment == 0 {
                            break;
                        }
                        assignment = (assignment - 1) & variable;
                    }
                }
            }
        }
    }

    #[test]
    fn distant_roof_entry_is_relevant_but_lower_disconnected_cells_are_not() {
        let profile = KickTableProfileId::Jstris180;
        let capability = MonotoneOCapability::for_profile(profile).unwrap();
        let roof = 0xff_u64 << 20;
        let remote_i = (2..6).fold(0_u64, |mask, y| mask | (1_u64 << (y * 10 + 8)));
        let open = capability
            .lock_reachable(10, 6, roof, RotationState::Zero, 0, 0)
            .unwrap();
        let closed = capability
            .lock_reachable(10, 6, roof | remote_i, RotationState::Zero, 0, 0)
            .unwrap();
        assert!(open.reachable);
        assert!(!closed.reachable);
        assert_ne!(open.influence_mask & remote_i, 0);
        // The board inside x=0..3 is unchanged. A fixed two-cell neighborhood
        // of the target cannot see the I that closes the distant entry.
        let local = (0..6).fold(0_u64, |mask, y| mask | (0xf_u64 << (y * 10)));
        assert_eq!(roof & local, (roof | remote_i) & local);
        for (board, actual) in [(roof, open), (roof | remote_i, closed)] {
            let expected = crate::reachability_reference::reference_spawn_lock_anchors(
                10,
                6,
                board,
                PieceKind::O,
                profile,
            )
            .unwrap();
            assert_eq!(actual.reachable, expected[0] & 1 != 0);
        }

        let target = capability
            .lock_reachable(10, 6, 1_u64 << 20, RotationState::Zero, 0, 3)
            .unwrap();
        assert!(target.reachable);
        assert_eq!(target.influence_mask & ((1_u64 << 20) - 1), 0);
        assert_eq!(target.influence_mask & (0xff_u64 << 22), 0);
    }

    #[test]
    fn invalid_dimensions_and_poses_are_unknown() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Srs90).unwrap();
        for (width, height, board, x, y) in [
            (1, 4, 0, 0, 0),
            (11, 4, 0, 0, 0),
            (10, 0, 0, 0, 0),
            (10, 7, 0, 0, 0),
            (10, 4, 1_u64 << 40, 0, 0),
            (10, 4, 0, -1, 0),
            (10, 4, 0, 9, 0),
            (10, 4, 0, 0, -1),
            (10, 4, 0, 0, 4),
        ] {
            assert!(capability
                .lock_reachable(width, height, board, RotationState::Zero, x, y)
                .is_none());
        }
    }

    #[test]
    fn interval_without_cache_covers_all_sixteen_intermediate_boards() {
        // A non-floor target with guaranteed minimum support, and a second
        // fixture with a nonzero original-row deletion frame. Each has four
        // independently optional cells, all covered by one maximum-board proof.
        let cases = [
            (
                10_u8,
                4_u8,
                1_u64 | (3_u64 << 30),
                (1_u64 << 2) | (1_u64 << 3) | (1_u64 << 32) | (1_u64 << 33),
                1_i8,
                LocalRelationRowFrame::new(4, 0).unwrap(),
            ),
            (
                10,
                4,
                3_u64 << 20,
                0xf_u64 << 22,
                0,
                LocalRelationRowFrame::new(4, 1).unwrap(),
            ),
        ];
        for profile in PROFILES {
            let capability = MonotoneOCapability::for_profile(profile).unwrap();
            for &(width, height, minimum, variable, y, frame) in &cases {
                let bottom = 3_u64 << (y as u32 * u32::from(width));
                let expected_lock_mask = bottom | (bottom << width);
                let maximum = minimum | variable;
                let attempt = capability
                    .proves_lock_interval(
                        width,
                        height,
                        minimum,
                        maximum,
                        RotationState::Zero,
                        0,
                        y,
                        expected_lock_mask,
                        frame,
                    )
                    .unwrap();
                assert!(attempt.proven);
                assert!(attempt.row_steps > 0);
                let mut assignment = variable;
                let mut count = 0;
                loop {
                    let board = minimum | assignment;
                    assert!(frame.accepts_physical_board(width, board));
                    let expected = crate::reachability_reference::reference_spawn_lock_anchors(
                        width,
                        height,
                        board,
                        PieceKind::O,
                        profile,
                    )
                    .unwrap();
                    let anchor = 1_u64 << (y as u32 * u32::from(width));
                    for rotation in RotationState::ALL {
                        assert_ne!(expected[rotation.quarter_turns() as usize] & anchor, 0);
                    }
                    count += 1;
                    if assignment == 0 {
                        break;
                    }
                    assignment = (assignment - 1) & variable;
                }
                assert_eq!(count, 16);
            }
        }
    }

    #[test]
    fn interval_cannot_borrow_grounding_from_the_maximum_board() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Jstris180).unwrap();
        let (width, height) = (10, 4);
        let expected_lock_mask = (3_u64 << 10) | (3_u64 << 20);
        assert!(
            capability
                .lock_reachable(width, height, 1, RotationState::Zero, 0, 1)
                .unwrap()
                .reachable
        );
        let attempt = capability
            .proves_lock_interval(
                width,
                height,
                0,
                1,
                RotationState::Zero,
                0,
                1,
                expected_lock_mask,
                LocalRelationRowFrame::new(height, 0).unwrap(),
            )
            .unwrap();
        assert!(!attempt.proven);
        assert_eq!(attempt.row_steps, 0);
    }

    #[test]
    fn interval_blocked_maximum_is_unknown_even_when_minimum_is_reachable() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Jstris180).unwrap();
        let minimum = 0xff_u64 << 20;
        let maximum = minimum | (2..6).fold(0_u64, |mask, y| mask | (1_u64 << (y * 10 + 8)));
        assert!(
            capability
                .lock_reachable(10, 6, minimum, RotationState::Zero, 0, 0)
                .unwrap()
                .reachable
        );
        let attempt = capability
            .proves_lock_interval(
                10,
                6,
                minimum,
                maximum,
                RotationState::Zero,
                0,
                0,
                3 | (3 << 10),
                LocalRelationRowFrame::new(6, 0).unwrap(),
            )
            .unwrap();
        assert!(!attempt.proven);
        assert!(attempt.row_steps > 0);
    }

    #[test]
    fn interval_rejects_wrong_frame_inclusion_and_ilc_mask() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Srs90).unwrap();
        let lock_mask = 3 | (3 << 10);
        let valid_frame = LocalRelationRowFrame::new(4, 0).unwrap();
        let attempt = |minimum, maximum, mask, frame| {
            capability.proves_lock_interval(
                10,
                4,
                minimum,
                maximum,
                RotationState::Zero,
                0,
                0,
                mask,
                frame,
            )
        };
        assert!(attempt(0, 0, lock_mask, valid_frame).unwrap().proven);
        assert!(attempt(1, 0, lock_mask, valid_frame).is_none());
        assert!(attempt(0, 0, lock_mask ^ 1, valid_frame).is_none());
        assert!(attempt(0, 0, lock_mask, LocalRelationRowFrame::new(3, 0).unwrap()).is_none());
        assert!(attempt(
            0,
            1_u64 << 30,
            lock_mask,
            LocalRelationRowFrame::new(4, 1).unwrap(),
        )
        .is_none());
        // Even empty bounds cannot admit an ILC lock above this frame's
        // surviving physical rows.
        assert!(capability
            .proves_lock_interval(
                10,
                4,
                0,
                0,
                RotationState::Zero,
                0,
                1,
                (3 << 10) | (3 << 20),
                LocalRelationRowFrame::new(4, 0b0011).unwrap(),
            )
            .is_none());
        // The small-width single-board primitive is valid, but the current
        // shared compacted-row-frame API explicitly admits only width 10.
        assert!(
            capability
                .lock_reachable(6, 4, 0, RotationState::Zero, 0, 0)
                .unwrap()
                .reachable
        );
        assert!(capability
            .proves_lock_interval(
                6,
                4,
                0,
                0,
                RotationState::Zero,
                0,
                0,
                3 | (3 << 6),
                valid_frame,
            )
            .is_none());
    }
}

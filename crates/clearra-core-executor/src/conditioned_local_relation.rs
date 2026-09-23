//! Exact, query-local entry-to-exit relation on a complete physical board.
//!
//! This is a qualification primitive, not an installed accelerator pack. It
//! evaluates every move and first-success ordered kick against the *entire*
//! board. Only the traversal is restricted to the pose window. Exits preserve
//! paths that may leave the window and later re-enter it, so absence of a lock
//! with nonempty exits is never a global negative certificate.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::kicks::KickTableProfileId;

use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
use crate::row_frame::CompactedRowFrame;

/// Inclusive anchor-pose rectangle; the piece footprint may cross its edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConditionedPoseWindow {
    pub min_x: i8,
    pub max_x: i8,
    pub min_y: i8,
    pub max_y: i8,
}

/// The exact lookup order used by BuildUp for a qualified relation pack.
/// The fixed array avoids allocating in the solver; only its prefix is valid.
/// Candidate generation must use this same function instead of inventing a
/// window that the product never queries.
pub fn solver_local_relation_windows(
    width: u8,
    height: u8,
    ceiling: i8,
) -> Option<([ConditionedPoseWindow; 3], usize)> {
    if width != 10 || !(1..=6).contains(&height) || ceiling < height as i8 {
        return None;
    }
    let base = ConditionedPoseWindow {
        min_x: 0,
        max_x: width as i8 - 1,
        min_y: 0,
        max_y: ceiling,
    };
    let mut windows = [base; 3];
    let mut count = 0;
    for min_y in [0, height as i8 / 2, height as i8] {
        if count > 0 && windows[count - 1].min_y == min_y {
            continue;
        }
        windows[count] = ConditionedPoseWindow { min_y, ..base };
        count += 1;
    }
    Some((windows, count))
}

/// Qualification-only view of the product's canonical sky-entry poses. The
/// value is generated from the same compiled reachability template as the
/// solver and does not grant any pack or profile authority.
#[cfg(any(test, feature = "qualification-reference"))]
pub fn solver_local_relation_spawn_entries(
    height: u8,
    piece: PieceKind,
    profile: KickTableProfileId,
) -> Option<(i8, Vec<ConditionedReachabilityEntryPose>)> {
    if !(1..=6).contains(&height) {
        return None;
    }
    crate::backend::exact_solver_local_relation_spawn_entries(height, piece, profile)
}

/// Original target-row correspondence for a compacted physical board. The
/// target height stays fixed during BuildUp; clearing an original row removes
/// its physical occupancy and shifts surviving rows downward. This mask is
/// deliberately separate from the 4L-only legal-board membership codec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalRelationRowFrame {
    frame: CompactedRowFrame,
}

impl LocalRelationRowFrame {
    pub const fn new(target_height: u8, deleted_original_rows: u16) -> Option<Self> {
        let frame = match CompactedRowFrame::new(target_height, deleted_original_rows) {
            Some(frame) if frame.surviving_rows() != 0 => frame,
            _ => return None,
        };
        Some(Self { frame })
    }

    pub const fn target_height(self) -> u8 {
        self.frame.target_height()
    }

    pub const fn deleted_original_rows(self) -> u8 {
        self.frame.deleted_original_rows()
    }

    pub const fn surviving_rows(self) -> u8 {
        self.frame.surviving_rows()
    }

    /// Map a compact physical row back to the original target-row frame.
    /// Cleared rows have no physical row and must not be fabricated as poses.
    pub const fn original_row_for_physical(self, physical_row: u8) -> Option<u8> {
        self.frame.original_row_for_physical(physical_row)
    }

    pub const fn physical_row_for_original(self, original_row: u8) -> Option<u8> {
        self.frame.physical_row_for_original(original_row)
    }

    pub fn accepts_physical_board(self, width: u8, board: u64) -> bool {
        self.frame.accepts_physical_board(width, board)
    }
}

#[cfg(test)]
mod solver_context_tests {
    use super::{solver_local_relation_spawn_entries, solver_local_relation_windows};
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_rules::kicks::KickTableProfileId;

    #[test]
    fn solver_window_order_is_shared_with_candidate_generation() {
        for height in 1..=6 {
            let (ceiling, entries) =
                solver_local_relation_spawn_entries(height, PieceKind::J, KickTableProfileId::SrsX)
                    .unwrap();
            assert!(!entries.is_empty());
            assert!(entries.windows(2).all(|pair| {
                let key = |pose: &crate::ConditionedReachabilityEntryPose| {
                    (pose.rotation.quarter_turns(), pose.x, pose.y)
                };
                key(&pair[0]) < key(&pair[1])
            }));
            let (windows, count) = solver_local_relation_windows(10, height, ceiling).unwrap();
            let observed = windows[..count]
                .iter()
                .map(|window| window.min_y)
                .collect::<Vec<_>>();
            let mut expected = vec![0, (height / 2) as i8, height as i8];
            expected.dedup();
            assert_eq!(observed, expected);
            assert!(windows[..count].iter().all(|window| {
                window.min_x == 0 && window.max_x == 9 && window.max_y == ceiling
            }));
        }
        assert!(solver_local_relation_windows(9, 4, 8).is_none());
        assert!(solver_local_relation_windows(10, 0, 8).is_none());
        assert!(
            solver_local_relation_spawn_entries(7, PieceKind::J, KickTableProfileId::SrsX)
                .is_none()
        );
    }
}

#[cfg(test)]
mod row_frame_tests {
    use super::LocalRelationRowFrame;
    use crate::legal_board::OriginalRowFrame;

    #[test]
    fn all_supported_row_masks_preserve_target_height_and_compacted_capacity() {
        for height in 1..=6_u8 {
            for mask in 0..(1_u16 << height) {
                let frame = LocalRelationRowFrame::new(height, mask);
                if mask.count_ones() == u32::from(height) {
                    assert!(
                        frame.is_none(),
                        "terminal all-cleared state is not a local query"
                    );
                    continue;
                }
                let frame = frame.expect("valid original-row mask");
                assert_eq!(frame.target_height(), height);
                assert_eq!(u16::from(frame.deleted_original_rows()), mask);
                assert_eq!(frame.surviving_rows(), height - mask.count_ones() as u8);
                let top_physical_bit = 10_u32 * u32::from(frame.surviving_rows());
                assert!(frame.accepts_physical_board(10, 0));
                assert!(!frame.accepts_physical_board(10, 1_u64 << top_physical_bit));
                for physical in 0..frame.surviving_rows() {
                    let original = frame.original_row_for_physical(physical).unwrap();
                    assert_eq!(frame.physical_row_for_original(original), Some(physical));
                }
                assert_eq!(
                    frame.original_row_for_physical(frame.surviving_rows()),
                    None
                );
                for original in 0..height {
                    assert_eq!(
                        frame.physical_row_for_original(original).is_none(),
                        mask & (1_u16 << original) != 0
                    );
                }
                assert_eq!(frame.physical_row_for_original(height), None);
                if height == 4 {
                    let mut physical_board = 0_u64;
                    let mut expected_replay = 0_u64;
                    for physical in 0..frame.surviving_rows() {
                        physical_board |= 1_u64 << (10 * physical);
                        let original = frame.original_row_for_physical(physical).unwrap();
                        expected_replay |= 1_u64 << (10 * original);
                    }
                    for original in 0..height {
                        if mask & (1_u16 << original) != 0 {
                            expected_replay |= 0x3ff_u64 << (10 * original);
                        }
                    }
                    let product_frame = OriginalRowFrame::from_deleted_rows(mask).unwrap();
                    assert_eq!(
                        product_frame.replay_frame_board(physical_board).unwrap(),
                        expected_replay
                    );
                }
            }
        }
        assert!(LocalRelationRowFrame::new(0, 0).is_none());
        assert!(LocalRelationRowFrame::new(7, 0).is_none());
        assert!(LocalRelationRowFrame::new(4, 1 << 4).is_none());
    }
}

/// Ephemeral relation bound to one rule/window/entry identity. Its dependency
/// mask is a conservative *in-process* reuse condition, not a signed product
/// pack, persistent key, or proof that a cropped window is globally closed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactConditionedLocalRelation {
    pub(crate) width: u8,
    pub(crate) height: u8,
    pub(crate) board: u64,
    pub(crate) row_frame: LocalRelationRowFrame,
    pub(crate) piece: PieceKind,
    pub(crate) kick_profile: KickTableProfileId,
    pub(crate) window: ConditionedPoseWindow,
    pub(crate) entries: Vec<ConditionedReachabilityEntryPose>,
    pub(crate) dependency_mask: u64,
    pub(crate) dependency_occupancy: u64,
    pub(crate) grounded_lock_anchors: [u64; 4],
    pub(crate) exits: Vec<ConditionedReachabilityEntryPose>,
}

impl ExactConditionedLocalRelation {
    pub const fn source_board(&self) -> u64 {
        self.board
    }

    pub const fn row_frame(&self) -> LocalRelationRowFrame {
        self.row_frame
    }

    pub const fn grounded_lock_anchors(&self) -> [u64; 4] {
        self.grounded_lock_anchors
    }

    pub fn exits(&self) -> &[ConditionedReachabilityEntryPose] {
        &self.exits
    }

    /// Compose a matching query board with the complete-board search from
    /// every first exit. A stored relation can match boards other than its
    /// source board when their dependency cells agree. The continuation must
    /// therefore search the *query* board: cells outside the local dependency
    /// mask can change global paths after an exit. Any path that leaves the
    /// window must cross one of those exits first and may later re-enter it.
    /// This proves locks reachable from the recorded entry poses, not from
    /// spawn or from entry poses that the caller has not globally reached.
    ///
    /// A qualified product may replace the continuation with a shared exact
    /// macro/cache, but must preserve this result. This reference composition
    /// is not installed in the solver hot path.
    pub fn compose_exact_global_lock_anchors_for_board(&self, board: u64) -> Option<[u64; 4]> {
        if !self.matches_query_with_frame(
            self.width,
            self.height,
            board,
            self.row_frame,
            self.piece,
            self.kick_profile,
            self.window,
            &self.entries,
        ) {
            return None;
        }
        let mut anchors = self.grounded_lock_anchors;
        if !self.exits.is_empty() {
            let continuation = crate::backend::exact_entry_lock_anchors(
                self.width,
                self.height,
                board,
                self.piece,
                self.kick_profile,
                &self.exits,
            )?;
            for rotation in 0..4 {
                anchors[rotation] |= continuation[rotation];
            }
        }
        Some(anchors)
    }

    pub const fn dependency_mask(&self) -> u64 {
        self.dependency_mask
    }

    /// The identity and every collision condition that can affect this local
    /// relation must match. An exit must still be composed with global search;
    /// this predicate does not authorize a global negative conclusion.
    #[allow(clippy::too_many_arguments)]
    pub fn matches_query(
        &self,
        width: u8,
        height: u8,
        board: u64,
        piece: PieceKind,
        kick_profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> bool {
        let Some(frame) = LocalRelationRowFrame::new(height, 0) else {
            return false;
        };
        self.matches_query_with_frame(
            width,
            height,
            board,
            frame,
            piece,
            kick_profile,
            window,
            entries,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn matches_query_with_frame(
        &self,
        width: u8,
        height: u8,
        board: u64,
        frame: LocalRelationRowFrame,
        piece: PieceKind,
        kick_profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> bool {
        if self.width != width
            || self.height != height
            || self.row_frame != frame
            || self.piece != piece
            || self.kick_profile != kick_profile
            || self.window != window
            || entries.is_empty()
            || entries.len() > 640
            || !frame.accepts_physical_board(width, board)
            || board & self.dependency_mask != self.dependency_occupancy
        {
            return false;
        }
        let mut canonical = entries.to_vec();
        canonical.sort_unstable_by_key(entry_pose_key);
        canonical.dedup();
        canonical == self.entries
    }
}

fn entry_pose_key(pose: &ConditionedReachabilityEntryPose) -> (u8, i8, i8) {
    (pose.rotation.quarter_turns(), pose.x, pose.y)
}

/// Return the exact locks reachable *without leaving* `window` and every
/// collision-free first exit. The caller must prove the entries are globally
/// reachable; this function checks only their placeability on the full board.
/// An invalid board, window or entry returns `None`, not an empty relation.
pub fn derive_exact_conditioned_local_relation(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    kick_profile: KickTableProfileId,
    window: ConditionedPoseWindow,
    entries: &[ConditionedReachabilityEntryPose],
) -> Option<ExactConditionedLocalRelation> {
    crate::backend::exact_local_relation(width, height, board, piece, kick_profile, window, entries)
}

/// Preserve the original-row frame as part of a candidate's identity. The
/// primitive collision traversal still operates on the compact physical
/// board, while later replay/witness composition must retain this frame.
#[allow(clippy::too_many_arguments)]
pub fn derive_exact_conditioned_local_relation_with_frame(
    width: u8,
    height: u8,
    board: u64,
    frame: LocalRelationRowFrame,
    piece: PieceKind,
    kick_profile: KickTableProfileId,
    window: ConditionedPoseWindow,
    entries: &[ConditionedReachabilityEntryPose],
) -> Option<ExactConditionedLocalRelation> {
    if frame.target_height() != height || !frame.accepts_physical_board(width, board) {
        return None;
    }
    let mut relation = derive_exact_conditioned_local_relation(
        width,
        height,
        board,
        piece,
        kick_profile,
        window,
        entries,
    )?;
    relation.row_frame = frame;
    Some(relation)
}

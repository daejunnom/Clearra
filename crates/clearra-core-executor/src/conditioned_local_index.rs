//! Unqualified in-process index for exact local-relation candidates.
//!
//! This neither installs an accelerator nor grants negative-answer authority.
//! A future immutable pack must additionally prove its source generation,
//! compressed size, independent differential result, and signed identity.

use core::cmp::Ordering;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::kicks::KickTableProfileId;

use crate::conditioned_local_relation::{
    ConditionedPoseWindow, ExactConditionedLocalRelation, LocalRelationRowFrame,
};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;

pub(crate) type PoseKey = (u8, i8, i8);
pub(crate) type ContextKey = (u8, u8, u8, u8, i8, i8, i8, i8, Vec<PoseKey>);
const MAX_CONTEXT_CANDIDATES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationIndexError {
    WrongProfile,
    InvalidRecord,
    ConflictingConditions,
    TooManyContextCandidates,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationCandidateLookup<'a> {
    Hit(&'a ExactConditionedLocalRelation),
    Miss,
    OutOfScope,
}

/// Exact-context groups prevent a relation for one piece, entry set or pose
/// frame from being reused for another. Overlapping occupancy conditions with
/// different results are rejected before any lookup can succeed.
pub struct LocalRelationCandidateIndex {
    profile: KickTableProfileId,
    records: Vec<ExactConditionedLocalRelation>,
}

impl LocalRelationCandidateIndex {
    pub fn new(
        profile: KickTableProfileId,
        mut records: Vec<ExactConditionedLocalRelation>,
    ) -> Result<Self, LocalRelationIndexError> {
        if crate::legal_board::accelerator_profile_name(profile).is_err() {
            return Err(LocalRelationIndexError::WrongProfile);
        }
        if records.is_empty() {
            return Err(LocalRelationIndexError::InvalidRecord);
        }
        for record in &records {
            if record.kick_profile != profile {
                return Err(LocalRelationIndexError::WrongProfile);
            }
            if !record_is_canonical(record) {
                return Err(LocalRelationIndexError::InvalidRecord);
            }
        }
        // The serialized pack already uses this canonical order. Sort here
        // too because the constructor also serves independently derived local
        // candidates. A flat range avoids duplicating every entry-pose vector
        // in a BTreeMap; product ownership and worker sharing remain separate.
        if records
            .windows(2)
            .any(|pair| compare_record_key(&pair[0], &pair[1]) == Ordering::Greater)
        {
            records.sort_unstable_by(compare_record_key);
        }
        let mut group_start = 0;
        for index in 0..records.len() {
            if index > 0 && compare_context(&records[index - 1], &records[index]) != Ordering::Equal
            {
                group_start = index;
            }
            if index - group_start >= MAX_CONTEXT_CANDIDATES {
                return Err(LocalRelationIndexError::TooManyContextCandidates);
            }
            for other in &records[group_start..index] {
                let record = &records[index];
                if conditions_overlap(record, other)
                    && (record.grounded_lock_anchors != other.grounded_lock_anchors
                        || record.exits != other.exits)
                {
                    return Err(LocalRelationIndexError::ConflictingConditions);
                }
            }
        }
        Ok(Self { profile, records })
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn retained_bytes(&self) -> usize {
        let pose_bytes = core::mem::size_of::<ConditionedReachabilityEntryPose>();
        self.records.iter().fold(
            self.records.capacity() * core::mem::size_of::<ExactConditionedLocalRelation>(),
            |total, record| {
                total
                    .saturating_add(record.entries.capacity().saturating_mul(pose_bytes))
                    .saturating_add(record.exits.capacity().saturating_mul(pose_bytes))
            },
        )
    }

    #[cfg(any(test, feature = "qualification-reference"))]
    pub(crate) fn records(&self) -> &[ExactConditionedLocalRelation] {
        &self.records
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lookup(
        &self,
        width: u8,
        height: u8,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> LocalRelationCandidateLookup<'_> {
        let Some(frame) = LocalRelationRowFrame::new(height, 0) else {
            return LocalRelationCandidateLookup::OutOfScope;
        };
        self.lookup_with_frame(width, height, board, frame, piece, profile, window, entries)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lookup_with_frame(
        &self,
        width: u8,
        height: u8,
        board: u64,
        frame: LocalRelationRowFrame,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> LocalRelationCandidateLookup<'_> {
        if profile != self.profile
            || width != 10
            || !(1..=6).contains(&height)
            || frame.target_height() != height
            || !frame.accepts_physical_board(width, board)
            || entries.is_empty()
            || entries.len() > 640
            || window.min_x < 0
            || window.max_x >= width as i8
            || window.min_x > window.max_x
            || window.min_y < 0
            || window.min_y > window.max_y
            || window.max_y > 32
            || entries.iter().any(|entry| {
                entry.x < window.min_x
                    || entry.x > window.max_x
                    || entry.y < window.min_y
                    || entry.y > window.max_y
            })
        {
            return LocalRelationCandidateLookup::OutOfScope;
        }
        let key = query_key(width, height, frame, piece, window, entries);
        let mut lower = 0;
        let mut upper = self.records.len();
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            if compare_record_query(&self.records[middle], &key) == Ordering::Less {
                lower = middle + 1;
            } else {
                upper = middle;
            }
        }
        for record in &self.records[lower..] {
            if compare_record_query(record, &key) != Ordering::Equal {
                break;
            }
            if board & record.dependency_mask == record.dependency_occupancy {
                return LocalRelationCandidateLookup::Hit(record);
            }
        }
        LocalRelationCandidateLookup::Miss
    }
}

fn record_is_canonical(record: &ExactConditionedLocalRelation) -> bool {
    let bits = u32::from(record.width) * u32::from(record.height);
    record.width == 10
        && (1..=6).contains(&record.height)
        && record.row_frame.target_height() == record.height
        && record
            .row_frame
            .accepts_physical_board(record.width, record.board)
        && record.dependency_mask >> bits == 0
        && record.dependency_occupancy == record.board & record.dependency_mask
        && record
            .grounded_lock_anchors
            .iter()
            .all(|anchors| anchors >> bits == 0)
        && !record.entries.is_empty()
        && strictly_sorted(&record.entries)
        && strictly_sorted(&record.exits)
        && record.window.min_x >= 0
        && record.window.max_x < record.width as i8
        && record.window.min_x <= record.window.max_x
        && record.window.min_y >= 0
        && record.window.min_y <= record.window.max_y
        && record.window.max_y <= 32
        && record.entries.iter().all(|entry| {
            entry.x >= record.window.min_x
                && entry.x <= record.window.max_x
                && entry.y >= record.window.min_y
                && entry.y <= record.window.max_y
        })
        && record.exits.iter().all(|exit| {
            exit.x < record.window.min_x
                || exit.x > record.window.max_x
                || exit.y < record.window.min_y
                || exit.y > record.window.max_y
        })
}

fn strictly_sorted(poses: &[ConditionedReachabilityEntryPose]) -> bool {
    poses
        .windows(2)
        .all(|pair| pose_key(&pair[0]) < pose_key(&pair[1]))
}

fn conditions_overlap(
    a: &ExactConditionedLocalRelation,
    b: &ExactConditionedLocalRelation,
) -> bool {
    (a.dependency_occupancy ^ b.dependency_occupancy) & (a.dependency_mask & b.dependency_mask) == 0
}

pub(crate) fn compare_record_key(
    left: &ExactConditionedLocalRelation,
    right: &ExactConditionedLocalRelation,
) -> Ordering {
    compare_context(left, right)
        .then_with(|| left.dependency_mask.cmp(&right.dependency_mask))
        .then_with(|| left.dependency_occupancy.cmp(&right.dependency_occupancy))
}

fn compare_context(
    left: &ExactConditionedLocalRelation,
    right: &ExactConditionedLocalRelation,
) -> Ordering {
    context_prefix(left)
        .cmp(&context_prefix(right))
        .then_with(|| {
            left.entries
                .iter()
                .map(pose_key)
                .cmp(right.entries.iter().map(pose_key))
        })
}

fn compare_record_query(record: &ExactConditionedLocalRelation, query: &ContextKey) -> Ordering {
    context_prefix(record)
        .cmp(&(
            query.0, query.1, query.2, query.3, query.4, query.5, query.6, query.7,
        ))
        .then_with(|| {
            record
                .entries
                .iter()
                .map(pose_key)
                .cmp(query.8.iter().copied())
        })
}

fn context_prefix(record: &ExactConditionedLocalRelation) -> (u8, u8, u8, u8, i8, i8, i8, i8) {
    (
        record.width,
        record.height,
        record.row_frame.deleted_original_rows(),
        piece_code(record.piece),
        record.window.min_x,
        record.window.max_x,
        record.window.min_y,
        record.window.max_y,
    )
}

fn query_key(
    width: u8,
    height: u8,
    frame: LocalRelationRowFrame,
    piece: PieceKind,
    window: ConditionedPoseWindow,
    entries: &[ConditionedReachabilityEntryPose],
) -> ContextKey {
    let mut entries: Vec<_> = entries.iter().map(pose_key).collect();
    entries.sort_unstable();
    entries.dedup();
    (
        width,
        height,
        frame.deleted_original_rows(),
        piece_code(piece),
        window.min_x,
        window.max_x,
        window.min_y,
        window.max_y,
        entries,
    )
}

fn pose_key(pose: &ConditionedReachabilityEntryPose) -> PoseKey {
    (pose.rotation.quarter_turns(), pose.x, pose.y)
}

pub(crate) fn piece_code(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::rotation::RotationState;

    use super::{
        LocalRelationCandidateIndex, LocalRelationCandidateLookup, LocalRelationIndexError,
    };
    use crate::conditioned_local_relation::{
        derive_exact_conditioned_local_relation, ConditionedPoseWindow,
    };
    use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_rules::kicks::KickTableProfileId;

    fn candidate() -> (
        crate::conditioned_local_relation::ExactConditionedLocalRelation,
        ConditionedPoseWindow,
        ConditionedReachabilityEntryPose,
    ) {
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 4,
            max_y: 4,
        };
        let entry = ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 4,
        };
        let relation = derive_exact_conditioned_local_relation(
            10,
            4,
            0,
            PieceKind::T,
            KickTableProfileId::SrsPlus,
            window,
            &[entry],
        )
        .expect("valid candidate relation");
        (relation, window, entry)
    }

    #[test]
    fn compatible_conditions_are_indexed_without_mixing_profiles_or_entries() {
        let (relation, window, entry) = candidate();
        let outside = (0..40)
            .map(|cell| 1_u64 << cell)
            .find(|bit| relation.dependency_mask() & *bit == 0)
            .expect("narrow relation has an unaffected cell");
        let second = derive_exact_conditioned_local_relation(
            10,
            4,
            outside,
            PieceKind::T,
            KickTableProfileId::SrsPlus,
            window,
            &[entry],
        )
        .expect("same dependency condition");
        let index =
            LocalRelationCandidateIndex::new(KickTableProfileId::SrsPlus, vec![relation, second])
                .expect("overlapping records agree");
        assert_eq!(index.record_count(), 2);
        assert!(matches!(
            index.lookup(
                10,
                4,
                outside,
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                window,
                &[entry]
            ),
            LocalRelationCandidateLookup::Hit(_)
        ));
        assert_eq!(
            index.lookup(
                10,
                4,
                0,
                PieceKind::T,
                KickTableProfileId::NoKick,
                window,
                &[entry]
            ),
            LocalRelationCandidateLookup::OutOfScope
        );
        assert_eq!(
            index.lookup(
                10,
                4,
                0,
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                window,
                &[ConditionedReachabilityEntryPose {
                    rotation: RotationState::Right,
                    ..entry
                }],
            ),
            LocalRelationCandidateLookup::Miss
        );
    }

    #[test]
    fn conflicting_overlapping_conditions_fail_before_lookup() {
        let (relation, _, _) = candidate();
        let mut corrupt = relation.clone();
        corrupt.grounded_lock_anchors[0] ^= 1;
        assert!(matches!(
            LocalRelationCandidateIndex::new(KickTableProfileId::SrsPlus, vec![relation, corrupt],),
            Err(LocalRelationIndexError::ConflictingConditions)
        ));
    }

    #[test]
    fn flat_sorted_ranges_preserve_context_and_profile_lookup() {
        let (t_relation, window, entry) = candidate();
        let l_relation = derive_exact_conditioned_local_relation(
            10,
            4,
            0,
            PieceKind::L,
            KickTableProfileId::SrsPlus,
            window,
            &[entry],
        )
        .expect("other piece has a valid entry");
        // The input order is deliberately opposite the canonical piece order.
        let index = LocalRelationCandidateIndex::new(
            KickTableProfileId::SrsPlus,
            vec![l_relation, t_relation],
        )
        .expect("sorted flat candidate index");
        assert_eq!(index.record_count(), 2);
        assert!(
            index.retained_bytes()
                >= 2 * core::mem::size_of::<
                    crate::conditioned_local_relation::ExactConditionedLocalRelation,
                >()
        );
        for piece in [PieceKind::T, PieceKind::L] {
            assert!(matches!(
                index.lookup(
                    10,
                    4,
                    0,
                    piece,
                    KickTableProfileId::SrsPlus,
                    window,
                    &[entry]
                ),
                LocalRelationCandidateLookup::Hit(_)
            ));
        }
        assert_eq!(
            index.lookup(
                10,
                4,
                0,
                PieceKind::S,
                KickTableProfileId::SrsPlus,
                window,
                &[entry]
            ),
            LocalRelationCandidateLookup::Miss
        );
    }
}

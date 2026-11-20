//! Local qualification of an imported ordered compact kick table. Numeric
//! profile IDs describe the source, not the identity of the ordered table.

use std::sync::Arc;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::{
    kicks::{normalized_kick_delta, KickTableProfileId, VerifiedKickTableProfile},
    profile::rule_profile::RuleProfileId,
};

use super::piece_index;

const SLOTS_PER_PIECE: usize = 12;
const MAX_TRANSITIONS: usize = 7 * SLOTS_PER_PIECE;
const MAX_OFFSETS_PER_TRANSITION: usize = 12;
type OrderedDeltas = [Option<Box<[(i8, i8)]>>; SLOTS_PER_PIECE];

/// Limits on logical compiler allocation counts, not allocator RSS. Pose
/// and target counts bound each corresponding linear table; the offset cap
/// covers both index tables and the temporary reverse cursor together.
#[derive(Clone, Copy)]
struct CompileBudget {
    poses: usize,
    rotation_targets: usize,
    index_entries: usize,
}

#[derive(Debug, Eq, PartialEq)]
enum CompactKickAdmission {
    Unsupported,
    IndexOrCoordinateOverflow,
    BudgetExceeded,
}

#[derive(Debug, Eq, PartialEq)]
struct PieceKicks {
    ceiling: i8,
    // Missing and present-but-empty are different ordered table identities.
    ordered: OrderedDeltas,
}

#[derive(Debug, Eq, PartialEq)]
struct CompactKickTable {
    width: u8,
    height: u8,
    profile: KickTableProfileId,
    rule: RuleProfileId,
    allow_180: bool,
    pieces: [PieceKicks; 7],
}

/// One request owner. Structural equality checks exact content. A future
/// shared-template route must carry this same owner, not reconstruct it from
/// its profile tag or reuse a memo from another request with the same tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CompactKickAuthority(Arc<CompactKickTable>);

pub(super) struct CompactPieceKicks<'a> {
    pub(super) width: u8,
    pub(super) height: u8,
    pub(super) ceiling: i8,
    pub(super) allow_180: bool,
    pub(super) ordered: [&'a [(i8, i8)]; SLOTS_PER_PIECE],
}

impl CompactKickAuthority {
    fn admit(
        width: u8,
        height: u8,
        selected_rule: RuleProfileId,
        verified: &VerifiedKickTableProfile,
        budget: CompileBudget,
    ) -> Result<Self, CompactKickAdmission> {
        use CompactKickAdmission::{BudgetExceeded, IndexOrCoordinateOverflow, Unsupported};

        let profile = verified.profile();
        if !(1..=10).contains(&width)
            || !(1..=6).contains(&height)
            || profile.source_rule() != selected_rule
            || !supported_profile(profile.id(), selected_rule)
            || profile.entries().len() > MAX_TRANSITIONS
        {
            return Err(Unsupported);
        }
        let allow_180 = profile.supports_180();
        let mut pieces: [PieceKicks; 7] = std::array::from_fn(|_| PieceKicks {
            ceiling: height as i8,
            ordered: std::array::from_fn(|_| None),
        });
        for entry in profile.entries() {
            let transition = entry.transition();
            let from = transition.from();
            let to = transition.to();
            let slot = usize::from(from.quarter_turns()) * 3
                + rotation_slot(from, to).ok_or(Unsupported)?;
            let piece = &mut pieces[piece_index(transition.piece())];
            let offsets = entry.sequence().offsets();
            if piece.ordered[slot].is_some()
                || entry.unsupported_reason().is_some()
                || offsets.len() > MAX_OFFSETS_PER_TRANSITION
            {
                return Err(Unsupported);
            }
            let mut normalized = Vec::with_capacity(offsets.len());
            for offset in offsets {
                // The existing standard origin components are in [-2, 2],
                // with a maximum difference of 3 on either axis. This
                // conservative guard also protects the intermediate i8 sum.
                if offset.dx().unsigned_abs() > 124 || offset.dy().unsigned_abs() > 124 {
                    return Err(IndexOrCoordinateOverflow);
                }
                normalized.push(normalized_kick_delta(
                    transition.piece(),
                    from,
                    to,
                    offset.dx(),
                    offset.dy(),
                ));
            }
            piece.ordered[slot] = Some(normalized.into_boxed_slice());
        }

        let mut poses = 0_usize;
        let mut rotation_targets = 0_usize;
        let mut index_entries = 0_usize;
        for piece in &mut pieces {
            let mut max_up = 0_i16;
            let mut max_right = 0_i16;
            let mut max_down = 0_i16;
            let mut max_candidates = 0_usize;
            for orientation in piece.ordered.chunks_exact(3) {
                let mut candidates = 0_usize;
                for kicks in orientation.iter().flatten() {
                    candidates = candidates
                        .checked_add(kicks.len())
                        .ok_or(IndexOrCoordinateOverflow)?;
                    for &(dx, dy) in kicks.iter() {
                        max_right = max_right.max(i16::from(dx));
                        max_down = max_down.max(-i16::from(dy));
                        max_up = max_up.max(i16::from(dy));
                    }
                }
                max_candidates = max_candidates.max(candidates);
            }
            let ceiling = i16::from(height) + max_down;
            // Check compiled i8 coordinates for every candidate, including
            // boundary-rejected moves. Detect overflow before the compiler
            // could mistake a wrapped coordinate for a valid destination.
            // Lower bounds follow from a nonnegative source and the checked
            // normalized displacement >= -127.
            if (i16::from(width) - 1 + max_right) > 127 || ceiling + max_up.max(1) > 127 {
                return Err(IndexOrCoordinateOverflow);
            }
            piece.ceiling = i8::try_from(ceiling).map_err(|_| IndexOrCoordinateOverflow)?;
            let height_with_sky =
                usize::try_from(ceiling + 1).map_err(|_| IndexOrCoordinateOverflow)?;
            let piece_poses = 4_usize
                .checked_mul(usize::from(width))
                .and_then(|count| count.checked_mul(height_with_sky))
                .ok_or(IndexOrCoordinateOverflow)?;
            // u16::MAX is the invalid state sentinel, not a real index.
            if piece_poses > usize::from(u16::MAX) {
                return Err(IndexOrCoordinateOverflow);
            }
            let piece_targets = piece_poses
                .checked_mul(max_candidates)
                .ok_or(IndexOrCoordinateOverflow)?;
            // Forward offsets and reverse bucket cursors store u32.
            u32::try_from(piece_targets).map_err(|_| IndexOrCoordinateOverflow)?;
            // Forward offsets have 3*n+1 entries; reverse offsets and the
            // concurrently retained reverse cursor each have n+1.
            let piece_indices = piece_poses
                .checked_mul(5)
                .and_then(|count| count.checked_add(3))
                .ok_or(IndexOrCoordinateOverflow)?;
            poses = poses
                .checked_add(piece_poses)
                .ok_or(IndexOrCoordinateOverflow)?;
            rotation_targets = rotation_targets
                .checked_add(piece_targets)
                .ok_or(IndexOrCoordinateOverflow)?;
            index_entries = index_entries
                .checked_add(piece_indices)
                .ok_or(IndexOrCoordinateOverflow)?;
        }
        if poses > budget.poses
            || rotation_targets > budget.rotation_targets
            || index_entries > budget.index_entries
        {
            return Err(BudgetExceeded);
        }
        Ok(Self(Arc::new(CompactKickTable {
            width,
            height,
            profile: profile.id(),
            rule: selected_rule,
            allow_180,
            pieces,
        })))
    }

    pub(super) fn piece(&self, piece: PieceKind) -> CompactPieceKicks<'_> {
        let piece = &self.0.pieces[piece_index(piece)];
        CompactPieceKicks {
            width: self.0.width,
            height: self.0.height,
            ceiling: piece.ceiling,
            allow_180: self.0.allow_180,
            ordered: std::array::from_fn(|slot| piece.ordered[slot].as_deref().unwrap_or(&[])),
        }
    }
}

fn supported_profile(profile: KickTableProfileId, rule: RuleProfileId) -> bool {
    use KickTableProfileId as Kick;
    use RuleProfileId as Rule;
    matches!(
        (profile, rule),
        (Kick::Srs90, Rule::Srs)
            | (Kick::SrsPlus, Rule::SrsPlus)
            | (Kick::SrsX, Rule::SrsX)
            | (Kick::Jstris180, Rule::Jstris180)
            | (Kick::NoKick, Rule::NoKick)
            | (
                Kick::Imported,
                Rule::Srs | Rule::SrsPlus | Rule::SrsX | Rule::Jstris180 | Rule::NoKick
            )
    )
}

fn rotation_slot(from: RotationState, to: RotationState) -> Option<usize> {
    if to == from.clockwise() {
        Some(0)
    } else if to == from.counter_clockwise() {
        Some(1)
    } else if to == from.rotated_180() {
        Some(2)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use clearra_rules::kicks::{
        KickOffset, KickOffsetSequence, KickTableEntry, KickTableProfile, KickTransition, SrsKicks,
    };

    use crate::reachability_reference::reference_spawn_locks_for_ordered_profile;

    use super::super::{
        kick_profiles::builtin_kick_profile,
        reachability::{
            search_reachable_locks, ReachabilityScratch, ReachabilityTemplate, ReachableLocks,
        },
    };
    use super::*;

    const BUDGET: CompileBudget = CompileBudget {
        poses: 10_000,
        rotation_targets: 100_000,
        index_entries: 50_000,
    };

    fn verify(profile: KickTableProfile) -> VerifiedKickTableProfile {
        VerifiedKickTableProfile::try_new(profile).expect("verified ordered table")
    }

    fn replace_t_kicks(offsets: Vec<KickOffset>) -> VerifiedKickTableProfile {
        let base = SrsKicks::profile();
        let transition =
            KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
        let mut entries = base.entries().to_vec();
        let selected = entries
            .iter_mut()
            .find(|entry| entry.transition() == transition)
            .unwrap();
        *selected = KickTableEntry::new(transition, KickOffsetSequence::new(offsets));
        verify(KickTableProfile::new(
            base.id(),
            base.source_rule(),
            entries,
        ))
    }

    fn exact_locks(
        profile: &VerifiedKickTableProfile,
        authority: &CompactKickAuthority,
        board: u64,
        piece: PieceKind,
    ) -> [u64; 4] {
        let normalized = authority.piece(piece);
        let template = ReachabilityTemplate::compile_authority(authority, piece);
        let actual =
            search_reachable_locks(&template, board, &mut ReachabilityScratch::default(), None);
        assert!(actual.exhaustive);
        let expected = reference_spawn_locks_for_ordered_profile(
            normalized.width,
            normalized.height,
            board,
            piece,
            profile.profile(),
        )
        .unwrap();
        assert_locks(actual.locks, expected, normalized.width, normalized.height);
        expected
    }

    fn assert_locks(actual: ReachableLocks, expected: [u64; 4], width: u8, height: u8) {
        for rotation in RotationState::ALL {
            let expected_anchors = expected[usize::from(rotation.quarter_turns())];
            for y in 0..height {
                for x in 0..width {
                    assert_eq!(
                        actual.contains(width, rotation, x as i8, y as i8),
                        expected_anchors & (1 << (u16::from(y) * u16::from(width) + u16::from(x)))
                            != 0,
                        "anchor {rotation:?} ({x},{y})"
                    );
                }
            }
        }
    }

    #[test]
    fn all_builtin_ordered_tables_compile_under_the_same_bound() {
        for id in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let profile = verify(builtin_kick_profile(id).unwrap().clone());
            let bound = CompactKickAuthority::admit(
                10,
                6,
                profile.profile().source_rule(),
                &profile,
                BUDGET,
            )
            .unwrap();
            for piece in PieceKind::STANDARD_TETROMINOES {
                for board in [0, 0x10_4800] {
                    let expected = exact_locks(&profile, &bound, board, piece);
                    let builtin = ReachabilityTemplate::compile(10, 6, piece, id);
                    let actual = search_reachable_locks(
                        &builtin,
                        board,
                        &mut ReachabilityScratch::default(),
                        None,
                    );
                    assert!(actual.exhaustive);
                    assert_locks(actual.locks, expected, 10, 6);
                }
            }
        }
    }

    #[test]
    fn same_id_different_first_kick_changes_two_line_t_i_witness() {
        const INITIAL: u64 = 0x000f_83c7;
        const T_LOCK: u64 = 0x4038;
        const I_ORIGINAL: u64 = 0x3c00;

        let canonical = verify(SrsKicks::profile());
        let base = canonical.profile();
        let mut records = base.entries().to_vec();
        records.reverse();
        let reversed_records = verify(KickTableProfile::new(
            base.id(),
            base.source_rule(),
            records,
        ));
        let transition =
            KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
        let mut offsets = base.sequence_for(transition).unwrap().offsets().to_vec();
        assert_eq!(offsets[2], KickOffset::new(1, -1));
        offsets[..=2].rotate_right(1);
        let changed = replace_t_kicks(offsets);
        let admit = |profile| {
            CompactKickAuthority::admit(10, 2, RuleProfileId::Srs, profile, BUDGET).unwrap()
        };
        let a = admit(&canonical);
        let b = admit(&changed);
        assert_eq!(a, admit(&reversed_records));
        assert_ne!(a, b);
        assert_eq!(canonical.profile().id(), changed.profile().id());
        assert_eq!(
            canonical.profile().source_rule(),
            changed.profile().source_rule()
        );
        let reaches_t =
            |profile, bound| exact_locks(profile, bound, INITIAL, PieceKind::T)[0] & (1 << 3) != 0;
        assert!(!reaches_t(&canonical, &a));
        assert!(reaches_t(&changed, &b));
        assert!(!reaches_t(&canonical, &a));
        assert_eq!(INITIAL & T_LOCK, 0);
        let after_t = INITIAL | T_LOCK;
        assert_eq!(after_t & 0x3ff, 0x3ff);
        let second_field = after_t >> 10;
        assert_eq!(second_field, 0x3f0);
        assert_eq!(I_ORIGINAL >> 10, 0xf);
        assert_ne!(
            exact_locks(&changed, &b, second_field, PieceKind::I)[0] & 1,
            0
        );
        assert_eq!(second_field | (I_ORIGINAL >> 10), 0x3ff);
    }

    #[test]
    fn profiles_are_not_compiled_when_shape_coordinates_or_budget_are_unqualified() {
        use CompactKickAdmission::{BudgetExceeded, IndexOrCoordinateOverflow, Unsupported};

        let canonical = verify(SrsKicks::profile());
        let far = replace_t_kicks(vec![KickOffset::new(127, 0)]);
        // Normalization succeeds, but adding the resulting x displacement
        // to some compact source anchors still exceeds i8.
        let far_from_some_anchors = replace_t_kicks(vec![KickOffset::new(124, 0)]);
        let long = replace_t_kicks(vec![KickOffset::new(0, 0); MAX_OFFSETS_PER_TRANSITION + 1]);
        assert_eq!(
            CompactKickAuthority::admit(10, 7, RuleProfileId::Srs, &canonical, BUDGET),
            Err(Unsupported)
        );
        assert_eq!(
            CompactKickAuthority::admit(10, 2, RuleProfileId::SrsPlus, &canonical, BUDGET),
            Err(Unsupported)
        );
        assert_eq!(
            CompactKickAuthority::admit(10, 2, RuleProfileId::Srs, &far, BUDGET),
            Err(IndexOrCoordinateOverflow)
        );
        assert_eq!(
            CompactKickAuthority::admit(10, 2, RuleProfileId::Srs, &far_from_some_anchors, BUDGET),
            Err(IndexOrCoordinateOverflow)
        );
        assert_eq!(
            CompactKickAuthority::admit(10, 2, RuleProfileId::Srs, &long, BUDGET),
            Err(Unsupported)
        );
        for budget in [
            CompileBudget { poses: 1, ..BUDGET },
            CompileBudget {
                rotation_targets: 1,
                ..BUDGET
            },
            CompileBudget {
                index_entries: 1,
                ..BUDGET
            },
        ] {
            assert_eq!(
                CompactKickAuthority::admit(10, 2, RuleProfileId::Srs, &canonical, budget),
                Err(BudgetExceeded)
            );
        }
    }
}

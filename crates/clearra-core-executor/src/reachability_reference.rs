//! Bounded differential oracle for the optimized spawn-to-lock traversal.
//!
//! This deliberately does not use compiled state masks, transition tables,
//! seed tables, or the production first-successful-kick helper. It interprets
//! the profile's ordered kick sequence against shape cells for every move.

use std::collections::{HashSet, VecDeque};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{
    KickTableProfile, KickTableProfileId, KickTransition, NoKick, SrsKicks,
};
use std::sync::OnceLock;

#[path = "reachability_reference_local.rs"]
mod local_relation;
pub use local_relation::{
    reference_local_dependency_mask, reference_local_relation, ReferenceLocalRelation,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Pose {
    rotation: RotationState,
    x: i16,
    y: i16,
}

/// A concrete entry into the full physical board. A local relation may use
/// this oracle only after proving that its caller actually reached the entry.
/// Invalid or obstructed entries make the query invalid, not unreachable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReferenceReachabilityEntryPose {
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
}

fn reference_center(piece: PieceKind, rotation: RotationState) -> (i8, i8) {
    let index = rotation.quarter_turns() as usize;
    match piece {
        PieceKind::I => [(0, 0), (-2, 2), (0, 1), (-1, 2)][index],
        PieceKind::O => (0, 0),
        _ => [(1, 0), (0, 1), (1, 1), (1, 1)][index],
    }
}

// Explicit-entry and window queries retain their declared finite builtin
// contract. They do not claim the all-sky last-entry reduction for a fixed
// spawn. The all-sky oracle below deliberately uses the larger universal cap.
fn reference_ceiling(height: u8, piece: PieceKind, profile: &KickTableProfile) -> i16 {
    i16::from(height) + reference_vertical_bounds(piece, profile).0
}

fn reference_vertical_bounds(piece: PieceKind, profile: &KickTableProfile) -> (i16, i16) {
    let mut downward_reach = 0_i16;
    let mut upward_reach = 0_i16;
    for entry in profile.entries() {
        let transition = entry.transition();
        if transition.piece() != piece || (!profile.supports_180() && transition.is_180()) {
            continue;
        }
        let (_, from_y) = reference_center(piece, transition.from());
        let (_, to_y) = reference_center(piece, transition.to());
        for offset in entry.sequence().offsets() {
            let delta_y = i16::from(offset.dy()) + i16::from(from_y) - i16::from(to_y);
            downward_reach = downward_reach.max(-delta_y);
            upward_reach = upward_reach.max(delta_y);
        }
    }
    (downward_reach, upward_reach)
}

type ShapeCells = [[(i8, i8); 4]; 4];

fn placeable(width: u8, height: u8, board: u64, shapes: &ShapeCells, pose: Pose) -> bool {
    shapes[pose.rotation.quarter_turns() as usize]
        .iter()
        .all(|&(dx, dy)| {
            let x = i16::from(pose.x) + i16::from(dx);
            let y = i16::from(pose.y) + i16::from(dy);
            x >= 0
                && x < i16::from(width)
                && y >= 0
                && (y >= i16::from(height)
                    || board & (1_u64 << (y as usize * width as usize + x as usize)) == 0)
        })
}

fn reference_locks(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile: &KickTableProfile,
    entries: Option<&[ReferenceReachabilityEntryPose]>,
) -> Option<[u64; 4]> {
    let ceiling = if entries.is_some() {
        reference_ceiling(height, piece, profile)
    } else {
        // Independent raw-data derivation, not the production capability
        // checker. H+D+U retains every earlier candidate at a low source or
        // the last sky-to-low source, for every accepted ordered kick list.
        let (downward, upward) = reference_vertical_bounds(piece, profile);
        i16::from(height) + downward + upward
    };
    let definition = standard_tetromino_registry()
        .get(piece)
        .expect("standard piece");
    let shapes = RotationState::ALL.map(|rotation| {
        definition
            .shape(rotation)
            .cells()
            .map(|cell| (cell.x(), cell.y()))
    });
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut enqueue = |pose: Pose, queue: &mut VecDeque<Pose>| {
        if pose.x >= 0
            && pose.x < i16::from(width)
            && pose.y >= 0
            && pose.y <= ceiling
            && placeable(width, height, board, &shapes, pose)
            && visited.insert(pose)
        {
            queue.push_back(pose);
        }
    };

    if let Some(entries) = entries {
        // Silently dropping an invalid seed would turn a malformed relation
        // query into an apparently complete negative proof.
        for entry in entries {
            let pose = Pose {
                rotation: entry.rotation,
                x: i16::from(entry.x),
                y: i16::from(entry.y),
            };
            if pose.x < 0
                || pose.x >= i16::from(width)
                || pose.y < 0
                || pose.y > ceiling
                || !placeable(width, height, board, &shapes, pose)
            {
                return None;
            }
            enqueue(pose, &mut queue);
        }
    } else {
        for rotation in RotationState::ALL {
            for y in i16::from(height)..=ceiling {
                for x in 0..i16::from(width) {
                    enqueue(Pose { rotation, x, y }, &mut queue);
                }
            }
        }
    }

    let mut locks = [0_u64; 4];
    while let Some(pose) = queue.pop_front() {
        let down = Pose {
            y: pose.y - 1,
            ..pose
        };
        if pose.y < i16::from(height)
            && (pose.y == 0 || !placeable(width, height, board, &shapes, down))
        {
            let anchor = pose.y as usize * width as usize + pose.x as usize;
            if anchor < 64 {
                locks[pose.rotation.quarter_turns() as usize] |= 1_u64 << anchor;
            }
        }
        enqueue(down, &mut queue);
        enqueue(
            Pose {
                x: pose.x - 1,
                ..pose
            },
            &mut queue,
        );
        enqueue(
            Pose {
                x: pose.x + 1,
                ..pose
            },
            &mut queue,
        );

        let rotations = [
            pose.rotation.clockwise(),
            pose.rotation.counter_clockwise(),
            pose.rotation.rotated_180(),
        ];
        for (slot, to) in rotations.into_iter().enumerate() {
            if slot == 2 && !profile.supports_180() {
                continue;
            }
            let Some(sequence) =
                profile.sequence_for(KickTransition::new(piece, pose.rotation, to))
            else {
                continue;
            };
            let (from_x, from_y) = reference_center(piece, pose.rotation);
            let (to_x, to_y) = reference_center(piece, to);
            for offset in sequence.offsets() {
                let target = Pose {
                    rotation: to,
                    x: pose.x + i16::from(offset.dx()) + i16::from(from_x) - i16::from(to_x),
                    y: pose.y + i16::from(offset.dy()) + i16::from(from_y) - i16::from(to_y),
                };
                if target.x >= 0
                    && target.x < i16::from(width)
                    && target.y >= 0
                    && target.y <= ceiling
                    && placeable(width, height, board, &shapes, target)
                {
                    // Ordered kicks: a later target is not an alternative once
                    // the first collision-free target succeeds.
                    enqueue(target, &mut queue);
                    break;
                }
            }
        }
    }
    Some(locks)
}

fn reference_profile(id: KickTableProfileId) -> Option<&'static KickTableProfile> {
    match id {
        KickTableProfileId::Srs90 => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::profile))
        }
        KickTableProfileId::SrsPlus => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::srs_plus_profile))
        }
        KickTableProfileId::SrsX => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::srs_x_profile))
        }
        KickTableProfileId::Jstris180 => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::jstris_180_profile))
        }
        KickTableProfileId::NoKick => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(NoKick::profile))
        }
        _ => None,
    }
}

/// Independent, deliberately unoptimized reference for local qualification.
/// It shares canonical piece/kick data but none of the compiled transition
/// tables, collision masks, or traversal helpers of the product solver.
pub fn reference_spawn_lock_anchors(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile_id: KickTableProfileId,
) -> Option<[u64; 4]> {
    let bits = u32::from(width) * u32::from(height);
    if width == 0 || width > 10 || !(1..=6).contains(&height) || bits > 64 || board >> bits != 0 {
        return None;
    }
    let profile = reference_profile(profile_id)?;
    reference_locks(width, height, board, piece, profile, None)
}

#[cfg(test)]
pub(crate) fn reference_spawn_locks_for_ordered_profile(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile: &KickTableProfile,
) -> Option<[u64; 4]> {
    reference_locks(width, height, board, piece, profile, None)
}

/// Independent bounded oracle for a caller-supplied set of *actual* entry
/// poses. This does not claim that a cropped local window is closed: the BFS
/// still sees the complete physical board and can leave and re-enter a region.
pub fn reference_entry_lock_anchors(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile_id: KickTableProfileId,
    entries: &[ReferenceReachabilityEntryPose],
) -> Option<[u64; 4]> {
    let bits = u32::from(width) * u32::from(height);
    if width == 0 || width > 10 || !(1..=6).contains(&height) || bits > 64 || board >> bits != 0 {
        return None;
    }
    let profile = reference_profile(profile_id)?;
    reference_locks(width, height, board, piece, profile, Some(entries))
}

#[cfg(test)]
mod entry_tests {
    use super::{
        reference_entry_lock_anchors, reference_spawn_lock_anchors, ReferenceReachabilityEntryPose,
    };
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_rules::kicks::KickTableProfileId;

    #[test]
    fn explicit_entry_locks_are_a_subset_of_sky_seed_locks() {
        let entry = ReferenceReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 4,
        };
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let from_entry =
                reference_entry_lock_anchors(10, 4, 0, PieceKind::T, profile, &[entry])
                    .expect("the sky entry is valid");
            let from_sky = reference_spawn_lock_anchors(10, 4, 0, PieceKind::T, profile)
                .expect("bounded reference domain");
            assert!(from_entry.iter().any(|anchors| *anchors != 0));
            assert!(from_entry
                .iter()
                .zip(from_sky)
                .all(|(entry_anchors, sky_anchors)| entry_anchors & !sky_anchors == 0));
        }
    }

    #[test]
    fn invalid_entry_is_not_a_verified_negative() {
        let invalid = ReferenceReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: -1,
            y: 4,
        };
        assert_eq!(
            reference_entry_lock_anchors(
                10,
                4,
                0,
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                &[invalid]
            ),
            None
        );
        assert_eq!(
            reference_entry_lock_anchors(10, 4, 0, PieceKind::T, KickTableProfileId::SrsPlus, &[]),
            Some([0; 4])
        );
    }
}

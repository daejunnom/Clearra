//! Independent primitive local-relation oracle for bounded qualification.
//!
//! This interprets raw shape cells and ordered kicks. It does not use the
//! product's compiled state masks, transition arrays or local traversal.

use std::collections::{HashSet, VecDeque};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{KickTableProfile, KickTableProfileId, KickTransition};

use super::{
    placeable, reference_ceiling, reference_center, reference_profile, Pose,
    ReferenceReachabilityEntryPose, ShapeCells,
};
use crate::conditioned_local_relation::ConditionedPoseWindow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceLocalRelation {
    pub grounded_lock_anchors: [u64; 4],
    pub exits: Vec<ReferenceReachabilityEntryPose>,
    pub reached_source_dependency_mask: u64,
}

/// Exact window-internal locks and first exits under the complete board.
/// Invalid or obstructed entries are rejected rather than treated as UNSAT.
pub fn reference_local_relation(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile_id: KickTableProfileId,
    window: ConditionedPoseWindow,
    entries: &[ReferenceReachabilityEntryPose],
) -> Option<ReferenceLocalRelation> {
    let bits = u32::from(width) * u32::from(height);
    if width == 0
        || width > 10
        || !(1..=6).contains(&height)
        || bits > 64
        || entries.is_empty()
        || board >> bits != 0
    {
        return None;
    }
    let profile = reference_profile(profile_id)?;
    let ceiling = reference_ceiling(height, piece, profile);
    if window.min_x < 0
        || window.max_x >= width as i8
        || window.min_x > window.max_x
        || window.min_y < 0
        || window.max_y > ceiling
        || window.min_y > window.max_y
    {
        return None;
    }
    let definition = standard_tetromino_registry()
        .get(piece)
        .expect("standard piece");
    let shapes: ShapeCells = RotationState::ALL.map(|rotation| {
        definition
            .shape(rotation)
            .cells()
            .map(|cell| (cell.x(), cell.y()))
    });
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut exits = HashSet::new();
    for entry in entries {
        let pose = Pose {
            rotation: entry.rotation,
            x: entry.x,
            y: entry.y,
        };
        if !inside(pose, window) || !valid_pose(width, height, board, ceiling, &shapes, pose) {
            return None;
        }
        if visited.insert(pose) {
            queue.push_back(pose);
        }
    }

    let mut locks = [0_u64; 4];
    while let Some(pose) = queue.pop_front() {
        let down = Pose {
            y: pose.y - 1,
            ..pose
        };
        if pose.y < height as i8
            && (pose.y == 0 || !valid_pose(width, height, board, ceiling, &shapes, down))
        {
            let anchor = pose.y as usize * width as usize + pose.x as usize;
            locks[pose.rotation.quarter_turns() as usize] |= 1_u64 << anchor;
        }
        for target in [
            down,
            Pose {
                x: pose.x - 1,
                ..pose
            },
            Pose {
                x: pose.x + 1,
                ..pose
            },
        ] {
            accept_successor(
                width,
                height,
                board,
                ceiling,
                &shapes,
                window,
                target,
                &mut visited,
                &mut queue,
                &mut exits,
            );
        }
        for (slot, to) in [
            pose.rotation.clockwise(),
            pose.rotation.counter_clockwise(),
            pose.rotation.rotated_180(),
        ]
        .into_iter()
        .enumerate()
        {
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
                    x: pose.x + offset.dx() + from_x - to_x,
                    y: pose.y + offset.dy() + from_y - to_y,
                };
                if valid_pose(width, height, board, ceiling, &shapes, target) {
                    // First collision-free target wins even when it is outside
                    // the local window; later kicks are not alternative exits.
                    accept_successor(
                        width,
                        height,
                        board,
                        ceiling,
                        &shapes,
                        window,
                        target,
                        &mut visited,
                        &mut queue,
                        &mut exits,
                    );
                    break;
                }
            }
        }
    }
    let reached_source_dependency_mask = visited.iter().fold(0_u64, |mask, &source| {
        mask | source_collision_dependencies(
            width, height, ceiling, piece, profile, &shapes, source,
        )
        .expect("a visited source is statically valid")
    });
    let mut exits: Vec<_> = exits
        .into_iter()
        .map(|pose| ReferenceReachabilityEntryPose {
            rotation: pose.rotation,
            x: pose.x,
            y: pose.y,
        })
        .collect();
    exits.sort_unstable_by_key(|pose| (pose.rotation.quarter_turns(), pose.x, pose.y));
    Some(ReferenceLocalRelation {
        grounded_lock_anchors: locks,
        exits,
        reached_source_dependency_mask,
    })
}

/// Collision cells that can influence a traversal confined to `window`.
/// This deliberately rebuilds the dependency closure from raw shape cells
/// and the profile's ordered kick offsets, without reading compiled masks or
/// transition arrays. This *static* upper bound includes every valid source
/// in the window; the record audit instead checks the narrower closure of
/// sources actually reached on the record's board.
pub fn reference_local_dependency_mask(
    width: u8,
    height: u8,
    piece: PieceKind,
    profile_id: KickTableProfileId,
    window: ConditionedPoseWindow,
) -> Option<u64> {
    if width != 10 || !(1..=6).contains(&height) {
        return None;
    }
    let profile = reference_profile(profile_id)?;
    let ceiling = reference_ceiling(height, piece, profile);
    if window.min_x < 0
        || window.max_x >= width as i8
        || window.min_x > window.max_x
        || window.min_y < 0
        || window.max_y > ceiling
        || window.min_y > window.max_y
    {
        return None;
    }
    let definition = standard_tetromino_registry().get(piece)?;
    let shapes: ShapeCells = RotationState::ALL.map(|rotation| {
        definition
            .shape(rotation)
            .cells()
            .map(|cell| (cell.x(), cell.y()))
    });
    let mut mask = 0_u64;
    for rotation in RotationState::ALL {
        for y in window.min_y..=window.max_y {
            for x in window.min_x..=window.max_x {
                let source = Pose { rotation, x, y };
                if let Some(source_mask) = source_collision_dependencies(
                    width, height, ceiling, piece, profile, &shapes, source,
                ) {
                    mask |= source_mask;
                }
            }
        }
    }
    Some(mask)
}

fn source_collision_dependencies(
    width: u8,
    height: u8,
    ceiling: i8,
    piece: PieceKind,
    profile: &KickTableProfile,
    shapes: &ShapeCells,
    source: Pose,
) -> Option<u64> {
    let mut mask = physical_collision_mask(width, height, ceiling, shapes, source)?;
    for candidate in [
        Pose {
            y: source.y - 1,
            ..source
        },
        Pose {
            x: source.x - 1,
            ..source
        },
        Pose {
            x: source.x + 1,
            ..source
        },
    ] {
        mask |= physical_collision_mask(width, height, ceiling, shapes, candidate).unwrap_or(0);
    }
    for (slot, to) in [
        source.rotation.clockwise(),
        source.rotation.counter_clockwise(),
        source.rotation.rotated_180(),
    ]
    .into_iter()
    .enumerate()
    {
        if slot == 2 && !profile.supports_180() {
            continue;
        }
        let Some(sequence) = profile.sequence_for(KickTransition::new(piece, source.rotation, to))
        else {
            continue;
        };
        let (from_x, from_y) = reference_center(piece, source.rotation);
        let (to_x, to_y) = reference_center(piece, to);
        for offset in sequence.offsets() {
            let candidate = Pose {
                rotation: to,
                x: source.x + offset.dx() + from_x - to_x,
                y: source.y + offset.dy() + from_y - to_y,
            };
            mask |= physical_collision_mask(width, height, ceiling, shapes, candidate).unwrap_or(0);
        }
    }
    Some(mask)
}

fn physical_collision_mask(
    width: u8,
    height: u8,
    ceiling: i8,
    shapes: &ShapeCells,
    pose: Pose,
) -> Option<u64> {
    if pose.x < 0 || pose.x >= width as i8 || pose.y < 0 || pose.y > ceiling {
        return None;
    }
    let mut mask = 0_u64;
    for &(dx, dy) in &shapes[pose.rotation.quarter_turns() as usize] {
        let x = i16::from(pose.x) + i16::from(dx);
        let y = i16::from(pose.y) + i16::from(dy);
        if x < 0 || x >= i16::from(width) || y < 0 {
            return None;
        }
        if y < i16::from(height) {
            mask |= 1_u64 << (y as usize * width as usize + x as usize);
        }
    }
    Some(mask)
}

fn inside(pose: Pose, window: ConditionedPoseWindow) -> bool {
    pose.x >= window.min_x
        && pose.x <= window.max_x
        && pose.y >= window.min_y
        && pose.y <= window.max_y
}

fn valid_pose(
    width: u8,
    height: u8,
    board: u64,
    ceiling: i8,
    shapes: &ShapeCells,
    pose: Pose,
) -> bool {
    pose.x >= 0
        && pose.x < width as i8
        && pose.y >= 0
        && pose.y <= ceiling
        && placeable(width, height, board, shapes, pose)
}

#[allow(clippy::too_many_arguments)]
fn accept_successor(
    width: u8,
    height: u8,
    board: u64,
    ceiling: i8,
    shapes: &ShapeCells,
    window: ConditionedPoseWindow,
    target: Pose,
    visited: &mut HashSet<Pose>,
    queue: &mut VecDeque<Pose>,
    exits: &mut HashSet<Pose>,
) {
    if !valid_pose(width, height, board, ceiling, shapes, target) {
        return;
    }
    if inside(target, window) {
        if visited.insert(target) {
            queue.push_back(target);
        }
    } else {
        exits.insert(target);
    }
}

//! Query-local pose-window traversal with complete-board collision semantics.

use super::*;
use crate::conditioned_local_relation::{
    ConditionedPoseWindow, ExactConditionedLocalRelation, LocalRelationRowFrame,
};

pub(crate) fn exact_local_relation(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile_id: KickTableProfileId,
    window: ConditionedPoseWindow,
    entries: &[ConditionedReachabilityEntryPose],
) -> Option<ExactConditionedLocalRelation> {
    if width != 10
        || !(1..=6).contains(&height)
        || board >> (u32::from(width) * u32::from(height)) != 0
        || entries.is_empty()
        || builtin_kick_profile(profile_id).is_none()
    {
        return None;
    }
    let template = ReachabilityTemplate::compile(width, height, piece, profile_id);
    if window.min_x < 0
        || window.max_x >= width as i8
        || window.min_x > window.max_x
        || window.min_y < 0
        || window.max_y > template.ceiling
        || window.min_y > window.max_y
        || entries.len() > template.state_masks.len()
    {
        return None;
    }
    let dependency_mask = local_dependency_mask(&template, window);
    let mut scratch = ReachabilityScratch::default();
    let generation = scratch.begin_search(template.state_masks.len());
    for &entry in entries {
        let state = State {
            rotation: entry.rotation,
            x: entry.x,
            y: entry.y,
        };
        let index = state_index(width, template.ceiling, state)?;
        if !pose_inside_window(state, window)
            || template.state_masks[index] == INVALID_STATE_MASK
            || board & template.state_masks[index] != 0
        {
            return None;
        }
        push_index_if_placeable(
            &template,
            board,
            u16::try_from(index).ok()?,
            &mut scratch.visited_generations,
            generation,
            &mut scratch.queue,
        );
    }

    let mut locks = [0_u64; 4];
    let mut exits = Vec::new();
    let mut cursor = 0;
    while cursor < scratch.queue.len() {
        let source = usize::from(scratch.queue[cursor]);
        cursor += 1;
        let pose = state_from_index(width, template.ceiling, source);
        if pose.y < height as i8 && grounded_index(&template, board, source) {
            let anchor = pose.y as usize * width as usize + pose.x as usize;
            locks[pose.rotation.quarter_turns() as usize] |= 1_u64 << anchor;
        }
        for &target in &template.translation_targets[source] {
            push_local_successor(
                &template,
                board,
                window,
                target,
                &mut scratch,
                generation,
                &mut exits,
            );
        }
        for slot in 0..if template.allow_180 { 3 } else { 2 } {
            if let Some(target) = first_successful_kick(&template, board, source, slot) {
                push_local_successor(
                    &template,
                    board,
                    window,
                    target,
                    &mut scratch,
                    generation,
                    &mut exits,
                );
            }
        }
    }
    exits.sort_unstable_by_key(pose_key);
    exits.dedup();
    let mut canonical_entries = entries.to_vec();
    canonical_entries.sort_unstable_by_key(pose_key);
    canonical_entries.dedup();
    Some(ExactConditionedLocalRelation {
        width,
        height,
        board,
        row_frame: LocalRelationRowFrame::new(height, 0)?,
        piece,
        kick_profile: profile_id,
        window,
        entries: canonical_entries,
        dependency_mask,
        dependency_occupancy: board & dependency_mask,
        grounded_lock_anchors: locks,
        exits,
    })
}

/// Conservative closure of every collision read originating at a pose in
/// this window: the source, translations (including grounding), and *all*
/// ordered kick candidates. Including blocked and not-yet-reachable sources
/// prevents an obstacle change from opening a new path under the same key.
fn local_dependency_mask(template: &ReachabilityTemplate, window: ConditionedPoseWindow) -> u64 {
    let mut mask = 0_u64;
    for source in 0..template.state_masks.len() {
        let pose = state_from_index(template.width, template.ceiling, source);
        if !pose_inside_window(pose, window) || template.state_masks[source] == INVALID_STATE_MASK {
            continue;
        }
        mask |= template.state_masks[source];
        for &target in &template.translation_targets[source] {
            if target != INVALID_STATE_INDEX {
                mask |= template.state_masks[usize::from(target)];
            }
        }
        for slot in 0..if template.allow_180 { 3 } else { 2 } {
            let transition = source * 3 + slot;
            let begin = template.rotation_target_offsets[transition] as usize;
            let end = template.rotation_target_offsets[transition + 1] as usize;
            for &target in &template.rotation_targets[begin..end] {
                mask |= template.state_masks[usize::from(target)];
            }
        }
    }
    mask
}

fn pose_key(pose: &ConditionedReachabilityEntryPose) -> (u8, i8, i8) {
    (pose.rotation.quarter_turns(), pose.x, pose.y)
}

fn pose_inside_window(pose: State, window: ConditionedPoseWindow) -> bool {
    pose.x >= window.min_x
        && pose.x <= window.max_x
        && pose.y >= window.min_y
        && pose.y <= window.max_y
}

#[allow(clippy::too_many_arguments)]
fn push_local_successor(
    template: &ReachabilityTemplate,
    board: u64,
    window: ConditionedPoseWindow,
    target: u16,
    scratch: &mut ReachabilityScratch,
    generation: u16,
    exits: &mut Vec<ConditionedReachabilityEntryPose>,
) {
    if target == INVALID_STATE_INDEX {
        return;
    }
    let index = usize::from(target);
    if template.state_masks[index] == INVALID_STATE_MASK || board & template.state_masks[index] != 0
    {
        return;
    }
    let pose = state_from_index(template.width, template.ceiling, index);
    if pose_inside_window(pose, window) {
        push_index_if_placeable(
            template,
            board,
            target,
            &mut scratch.visited_generations,
            generation,
            &mut scratch.queue,
        );
    } else {
        exits.push(ConditionedReachabilityEntryPose {
            rotation: pose.rotation,
            x: pose.x,
            y: pose.y,
        });
    }
}

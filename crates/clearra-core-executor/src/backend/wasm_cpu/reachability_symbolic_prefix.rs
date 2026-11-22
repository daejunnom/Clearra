//! Bounded Boolean lock relation for a batch of executed-prefix occupancies.
//!
//! A lane is an immutable compact board in one frame for one piece/profile.
//! The word at each pose denotes exactly the lanes that have reached it.
//! This does not produce witness paths, spin evidence or multiplicities.

use std::collections::VecDeque;

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::super::WasmExactSearchError;
use super::{
    state_from_index, InstantiatedRealization, ReachabilityTemplate, INVALID_STATE_INDEX,
    INVALID_STATE_MASK,
};

const MAX_POSES: usize = 1024;
const MAX_WORKLIST_POPS: usize = 8192;
const MAX_ACTION_TERMS: usize = 131_072;
const CANCELLATION_CHECK_INTERVAL: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in super::super) struct PrefixPoseWork {
    pub attempted_relations: u64,
    pub completed_relations: u64,
    pub lane_boards: u64,
    #[cfg(test)]
    pub occupied_cell_marks: u64,
    pub plane_validation_terms: u64,
    pub pose_domains: u64,
    pub pose_cell_terms: u64,
    pub worklist_pops: u64,
    pub reached_pose_lanes: u64,
    pub translation_terms: u64,
    pub ordered_kick_terms: u64,
    /// Largest once-compiled count of globally impossible reverse candidates.
    /// This is a template footprint, not a per-query saved-work estimate.
    pub global_kick_pruned_edges_peak: u64,
    pub ground_terms: u64,
    pub peak_logical_bytes: usize,
}

#[derive(Clone, Copy, Default)]
struct LockDomain {
    footprint: u64,
    grounded_lanes: u64,
}

/// Positive and negative answers are authoritative only within these lanes
/// and for a lock pose whose physical footprint matches the template.
pub(in super::super) struct PrefixLockRelation {
    width: u8,
    height: u8,
    #[cfg(test)]
    lanes: u64,
    domains: Vec<LockDomain>,
}

impl PrefixLockRelation {
    /// The caller supplies an already validated ILC realization for this
    /// piece, frame and lane. Use `lock_permission` when validating that seam
    /// rather than interpreting an unbound request as a proven negative.
    #[cfg(test)]
    pub(in super::super) fn permits(
        &self,
        realization: InstantiatedRealization,
        lane: usize,
    ) -> bool {
        self.lock_permission(realization, lane) == Some(true)
    }

    #[cfg(test)]
    pub(in super::super) fn lock_permission(
        &self,
        realization: InstantiatedRealization,
        lane: usize,
    ) -> Option<bool> {
        if lane >= u64::BITS as usize || self.lanes & (1_u64 << lane) == 0 {
            return None;
        }
        self.grounded_lanes(realization)
            .map(|lanes| lanes & (1_u64 << lane) != 0)
    }

    pub(in super::super) fn grounded_lanes(
        &self,
        realization: InstantiatedRealization,
    ) -> Option<u64> {
        if !(0..self.width as i8).contains(&realization.x)
            || !(0..self.height as i8).contains(&realization.lock_y)
            || realization.lock_mask.count_ones() != 4
        {
            return None;
        }
        let stride = usize::from(self.width);
        let index = (realization.rotation.quarter_turns() as usize * usize::from(self.height)
            + realization.lock_y as usize)
            * stride
            + realization.x as usize;
        let domain = &self.domains[index];
        (realization.lock_mask == domain.footprint).then_some(domain.grounded_lanes)
    }
}

#[derive(Clone, Copy, Default)]
struct PoseState {
    free_lanes: u64,
    arrived_lanes: u64,
    pending_lanes: u64,
}

// Each new (pose, lane) is enqueued exactly once, possibly batched with
// pending lanes. A pose is present in the worklist iff its pending word is
// nonzero, so maximum queue length is the number of template poses.
fn add_lanes(states: &mut [PoseState], queue: &mut VecDeque<usize>, target: usize, lanes: u64) {
    let state = &mut states[target];
    let added = lanes & !state.arrived_lanes;
    if added != 0 {
        if state.pending_lanes == 0 {
            queue.push_back(target);
        }
        state.arrived_lanes |= added;
        state.pending_lanes |= added;
    }
}

pub(super) fn compile_prefix_lock_relation_from_planes(
    template: &ReachabilityTemplate,
    occupied: &[u64; u64::BITS as usize],
    lanes: u64,
    control: &ExecutionControl,
    work: &mut PrefixPoseWork,
) -> Result<Option<PrefixLockRelation>, WasmExactSearchError> {
    compile_with_cancel_check(template, occupied, lanes, work, || control.is_cancelled())
}

fn check_cancellation(
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<(), WasmExactSearchError> {
    if should_cancel() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}

fn compile_with_cancel_check(
    template: &ReachabilityTemplate,
    occupied: &[u64; u64::BITS as usize],
    lanes: u64,
    work: &mut PrefixPoseWork,
    mut should_cancel: impl FnMut() -> bool,
) -> Result<Option<PrefixLockRelation>, WasmExactSearchError> {
    check_cancellation(&mut should_cancel)?;
    work.attempted_relations += 1;
    let state_count = template.state_masks.len();
    if lanes == 0
        || !(1..=10).contains(&template.width)
        || !(1..=6).contains(&template.height)
        || !(1..=MAX_POSES).contains(&state_count)
    {
        return Ok(None);
    }
    let area = usize::from(template.width) * usize::from(template.height);
    for (cell, &owners) in occupied.iter().enumerate() {
        work.plane_validation_terms += 1;
        let allowed = if cell < area { lanes } else { 0 };
        if owners & !allowed != 0 {
            return Ok(None);
        }
    }
    work.lane_boards += u64::from(lanes.count_ones());

    let mut states = Vec::new();
    let mut queue = VecDeque::new();
    let mut domains = Vec::new();
    if states.try_reserve_exact(state_count).is_err()
        || queue.try_reserve_exact(state_count).is_err()
        || domains.try_reserve_exact(4 * area).is_err()
    {
        check_cancellation(&mut should_cancel)?;
        return Ok(None);
    }
    domains.resize(4 * area, LockDomain::default());
    work.peak_logical_bytes = work.peak_logical_bytes.max(
        core::mem::size_of_val(occupied)
            + states.capacity() * core::mem::size_of::<PoseState>()
            + queue.capacity() * core::mem::size_of::<usize>()
            + domains.capacity() * core::mem::size_of::<LockDomain>(),
    );

    for (index, &footprint) in template.state_masks.iter().enumerate() {
        if index.is_multiple_of(CANCELLATION_CHECK_INTERVAL) {
            check_cancellation(&mut should_cancel)?;
        }
        work.pose_domains += 1;
        let mut cells = footprint;
        let mut free_lanes = 0;
        if footprint != INVALID_STATE_MASK {
            free_lanes = lanes;
            while cells != 0 && free_lanes != 0 {
                free_lanes &= !occupied[cells.trailing_zeros() as usize];
                cells &= cells - 1;
                work.pose_cell_terms += 1;
            }
        }
        states.push(PoseState {
            free_lanes,
            ..PoseState::default()
        });
    }

    for &seed in &template.sky_seeds {
        let seed = usize::from(seed);
        let free_lanes = states[seed].free_lanes;
        add_lanes(&mut states, &mut queue, seed, free_lanes);
    }

    let mut worklist_pops = 0_usize;
    let mut action_terms = 0;
    let kick_slots = if template.allow_180 { 3 } else { 2 };
    while let Some(source) = queue.pop_front() {
        if worklist_pops.is_multiple_of(CANCELLATION_CHECK_INTERVAL) {
            check_cancellation(&mut should_cancel)?;
        }
        if worklist_pops == MAX_WORKLIST_POPS {
            return Ok(None);
        }
        worklist_pops += 1;
        work.worklist_pops += 1;
        let delta = core::mem::take(&mut states[source].pending_lanes);
        work.reached_pose_lanes += u64::from(delta.count_ones());
        for &target in &template.translation_targets[source] {
            if target == INVALID_STATE_INDEX {
                continue;
            }
            action_terms += 1;
            work.translation_terms += 1;
            let target = usize::from(target);
            let destinations = delta & states[target].free_lanes;
            add_lanes(&mut states, &mut queue, target, destinations);
        }
        for slot in 0..kick_slots {
            let transition = source * 3 + slot;
            let begin = template.rotation_target_offsets[transition] as usize;
            let end = template.rotation_target_offsets[transition + 1] as usize;
            let mut pending_choice = delta;
            for &target in &template.rotation_targets[begin..end] {
                action_terms += 1;
                work.ordered_kick_terms += 1;
                let target = usize::from(target);
                let destinations = pending_choice & states[target].free_lanes;
                // First success is consumed even when it yields an already
                // discovered pose. A later kick cannot use these lanes.
                pending_choice &= !destinations;
                add_lanes(&mut states, &mut queue, target, destinations);
                if pending_choice == 0 {
                    break;
                }
            }
        }
        if action_terms > MAX_ACTION_TERMS {
            check_cancellation(&mut should_cancel)?;
            return Ok(None);
        }
    }

    let width = usize::from(template.width);
    for (index, state) in states.iter().enumerate() {
        if index.is_multiple_of(CANCELLATION_CHECK_INTERVAL) {
            check_cancellation(&mut should_cancel)?;
        }
        let pose = state_from_index(template.width, template.ceiling, index);
        if pose.y >= template.height as i8 || template.state_masks[index] == INVALID_STATE_MASK {
            continue;
        }
        let down = template.translation_targets[index][0];
        let grounded = if pose.y == 0 || down == INVALID_STATE_INDEX {
            state.arrived_lanes
        } else {
            work.ground_terms += 1;
            state.arrived_lanes & !states[usize::from(down)].free_lanes
        };
        let target = pose.rotation.quarter_turns() as usize * area
            + pose.y as usize * width
            + pose.x as usize;
        domains[target] = LockDomain {
            footprint: template.state_masks[index],
            grounded_lanes: grounded,
        };
    }
    check_cancellation(&mut should_cancel)?;
    work.completed_relations += 1;
    Ok(Some(PrefixLockRelation {
        width: template.width,
        height: template.height,
        #[cfg(test)]
        lanes,
        domains,
    }))
}

#[cfg(test)]
fn compile_prefix_lock_relation(
    template: &ReachabilityTemplate,
    boards: &[u64],
    work: &mut PrefixPoseWork,
) -> Option<PrefixLockRelation> {
    let area = usize::from(template.width) * usize::from(template.height);
    if !(1..=u64::BITS as usize).contains(&boards.len())
        || area >= u64::BITS as usize
        || boards.iter().any(|&board| board >> area != 0)
    {
        return None;
    }
    let mut occupied = [0_u64; u64::BITS as usize];
    for (lane, &board) in boards.iter().enumerate() {
        let mut cells = board;
        while cells != 0 {
            occupied[cells.trailing_zeros() as usize] |= 1_u64 << lane;
            cells &= cells - 1;
            work.occupied_cell_marks += 1;
        }
    }
    let lanes = u64::MAX >> (u64::BITS as usize - boards.len());
    compile_with_cancel_check(template, &occupied, lanes, work, || false)
        .expect("uncancelled primitive test relation")
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_rules::kicks::KickTableProfileId;

    use super::*;
    use crate::reachability_reference::reference_spawn_lock_anchors;

    #[test]
    fn prefix_lock_words_match_independent_scalar_on_all_profiles() {
        const WIDTH: u8 = 10;
        const HEIGHT: u8 = 4;
        // Exercise every lane (including bit 63), repeated boards and
        // collision terms in all four rows. Dense lanes force different
        // first-success kicks from sparse lanes in the same word.
        let mut boards: [u64; 64] = std::array::from_fn(|i| {
            if i < 16 {
                i as u64 * 0x4010_0401 & 0xff_ffff_ffff
            } else {
                (i as u64 * 0x3fb7_245a_7975)
                    & (u64::MAX >> (64 - u32::from(WIDTH) * u32::from(HEIGHT)))
            }
        });
        boards[1] = 0;
        boards[62] = 0x4002;
        boards[63] = 0x5000;
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
                let mut work = PrefixPoseWork::default();
                let relation = compile_prefix_lock_relation(&template, &boards, &mut work).unwrap();
                assert_eq!(work.completed_relations, 1);
                assert_eq!(work.lane_boards, 64);
                assert!(work.worklist_pops < work.reached_pose_lanes);
                let area = usize::from(WIDTH) * usize::from(HEIGHT);
                for (lane, board) in boards.into_iter().enumerate() {
                    let expected =
                        reference_spawn_lock_anchors(WIDTH, HEIGHT, board, piece, profile)
                            .expect("finite scalar reference");
                    let mut actual = [0; 4];
                    for (index, domain) in relation.domains.iter().enumerate() {
                        let rotation = index / area;
                        let cell = index % area;
                        let allowed = domain.grounded_lanes & (1_u64 << lane) != 0;
                        if allowed {
                            actual[rotation] |= 1_u64 << cell;
                        }
                        if domain.footprint.count_ones() == 4 {
                            let realization = InstantiatedRealization {
                                lock_mask: domain.footprint,
                                rotation: RotationState::ALL[rotation],
                                x: (cell % usize::from(WIDTH)) as i8,
                                lock_y: (cell / usize::from(WIDTH)) as i8,
                            };
                            assert_eq!(relation.permits(realization, lane), allowed);
                        }
                    }
                    assert_eq!(
                        actual, expected,
                        "{profile:?} {piece:?} lane {lane} {board:#x}"
                    );
                }
            }
        }
    }

    #[test]
    fn later_kick_can_become_available_in_a_lane_with_an_extra_blocker() {
        let profile = KickTableProfileId::Srs90;
        let template = ReachabilityTemplate::compile(10, 3, PieceKind::T, profile);
        let without_blocker = 0x4002;
        let boards = [without_blocker, without_blocker | (1 << 11)];
        let mut work = PrefixPoseWork::default();
        let relation = compile_prefix_lock_relation(&template, &boards, &mut work).unwrap();
        let target = InstantiatedRealization {
            lock_mask: 0x201c,
            rotation: RotationState::Zero,
            x: 2,
            lock_y: 0,
        };
        // Right-to-Zero tries (1,1), then (2,1), then this lock at
        // (2,0). The added blocker changes the first valid kick.
        for (lane, (board, expected)) in boards.into_iter().zip([false, true]).enumerate() {
            let scalar = reference_spawn_lock_anchors(10, 3, board, PieceKind::T, profile)
                .expect("finite scalar reference");
            assert_eq!(scalar[0] & (1 << 2) != 0, expected);
            assert_eq!(relation.lock_permission(target, lane), Some(expected));
        }
    }

    #[test]
    fn malformed_prefix_domains_never_produce_a_negative_relation() {
        let template =
            ReachabilityTemplate::compile(10, 4, PieceKind::I, KickTableProfileId::Srs90);
        let mut work = PrefixPoseWork::default();
        assert!(compile_prefix_lock_relation(&template, &[], &mut work).is_none());
        assert!(compile_prefix_lock_relation(&template, &[0; 65], &mut work).is_none());
        assert!(compile_prefix_lock_relation(&template, &[1_u64 << 40], &mut work).is_none());
        assert_eq!(work.completed_relations, 0);
        let relation = compile_prefix_lock_relation(&template, &[0], &mut work).unwrap();
        let bound = InstantiatedRealization {
            lock_mask: 0b1111,
            rotation: RotationState::Zero,
            x: 0,
            lock_y: 0,
        };
        assert_eq!(relation.lock_permission(bound, 1), None);
        assert_eq!(
            relation.lock_permission(
                InstantiatedRealization {
                    lock_mask: 0b11110,
                    ..bound
                },
                0,
            ),
            None
        );
    }

    #[test]
    fn symbolic_sparse_occupancy_planes_cannot_answer_unowned_lanes() {
        let template =
            ReachabilityTemplate::compile(10, 3, PieceKind::T, KickTableProfileId::Srs90);
        let target = InstantiatedRealization {
            lock_mask: 0x201c,
            rotation: RotationState::Zero,
            x: 2,
            lock_y: 0,
        };
        let lanes = (1_u64 << 5) | (1_u64 << 63);
        let mut occupied = [0; u64::BITS as usize];
        occupied[1] = lanes;
        occupied[14] = lanes;
        occupied[11] = 1_u64 << 63;
        let control = ExecutionControl::default();
        let mut work = PrefixPoseWork::default();
        let compile = |occupied, work: &mut PrefixPoseWork| {
            compile_prefix_lock_relation_from_planes(&template, occupied, lanes, &control, work)
        };
        let relation = compile(&occupied, &mut work).unwrap().unwrap();
        assert_eq!(work.lane_boards, 2);
        assert_eq!(relation.grounded_lanes(target), Some(1_u64 << 63));
        assert_eq!(relation.lock_permission(target, 0), None);
        assert_eq!(relation.lock_permission(target, 64), None);
        assert_eq!(relation.lock_permission(target, 5), Some(false));
        assert_eq!(relation.lock_permission(target, 63), Some(true));
        let mut inactive_owner = occupied;
        inactive_owner[11] |= 1_u64 << 6;
        assert!(compile(&inactive_owner, &mut work).unwrap().is_none());
        let mut outside_board = occupied;
        outside_board[30] = lanes;
        assert!(compile(&outside_board, &mut work).unwrap().is_none());
        assert_eq!(work.completed_relations, 1);
    }

    #[test]
    fn symbolic_pose_cancellation_does_not_certify_partial_or_terminal_relation() {
        let template =
            ReachabilityTemplate::compile(10, 4, PieceKind::T, KickTableProfileId::Srs90);
        let occupied = [0; u64::BITS as usize];
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();
        let mut work = PrefixPoseWork::default();
        assert!(matches!(
            compile_prefix_lock_relation_from_planes(&template, &occupied, 1, &control, &mut work),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(work, PrefixPoseWork::default());

        let mut polls = 0;
        let complete = compile_with_cancel_check(&template, &occupied, 1, &mut work, || {
            polls += 1;
            false
        });
        assert!(matches!(complete, Ok(Some(_))));
        let pose_polls = template
            .state_masks
            .len()
            .div_ceil(CANCELLATION_CHECK_INTERVAL);
        let mid_work_poll = 1 + pose_polls + 2;
        assert!(mid_work_poll < polls);
        assert!(work.worklist_pops > CANCELLATION_CHECK_INTERVAL as u64);
        for (cancel_on, expected_pops) in [
            (mid_work_poll, CANCELLATION_CHECK_INTERVAL as u64),
            (polls, work.worklist_pops),
        ] {
            let mut incomplete = PrefixPoseWork::default();
            let mut checkpoints = 0;
            assert!(matches!(
                compile_with_cancel_check(&template, &occupied, 1, &mut incomplete, || {
                    checkpoints += 1;
                    checkpoints == cancel_on
                }),
                Err(WasmExactSearchError::Cancelled)
            ));
            assert_eq!(incomplete.worklist_pops, expected_pops);
            assert_eq!(incomplete.attempted_relations, 1);
            assert_eq!(incomplete.completed_relations, 0);
        }
    }
}

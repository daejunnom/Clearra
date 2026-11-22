//! Exact reverse Boolean reachability for the realizations of one operation.
//!
//! Each lane is one immutable board and uses the template's real forward
//! transition semantics. Reaching a sky seed proves that lane. Only a fully
//! exhausted lane is an exact miss. Allocation failure refuses the relation;
//! cancellation and resource refusal are never negative movement answers.

use std::collections::VecDeque;

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::super::WasmExactSearchError;
use super::{
    InstantiatedRealization, PrefixPoseWork, ReachabilityTemplate, State, INVALID_STATE_INDEX,
    INVALID_STATE_MASK,
};

const WORD_BITS: usize = u64::BITS as usize;
const MAX_POSES: usize = u16::MAX as usize;
const CANCEL_INTERVAL: usize = 64;

#[derive(Clone, Copy, Default)]
struct Pose {
    free: u64,
    visited: u64,
    pending: u64,
}

/// Worker-owned storage. No board, rule, or result is reused between queries.
#[derive(Default)]
pub(super) struct ReverseScratch {
    computed_free: Vec<u64>,
    poses: Vec<Pose>,
    queue: VecDeque<u16>,
}

impl ReverseScratch {
    pub(super) fn checked_retained_upper_bound(states: u128) -> Option<u128> {
        states
            .checked_mul((core::mem::size_of::<Pose>() + 2 * core::mem::size_of::<u16>()) as u128)?
            .checked_add(
                states
                    .checked_add(63)?
                    .checked_div(64)?
                    .checked_mul(core::mem::size_of::<u64>() as u128)?,
            )
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.poses.capacity() * core::mem::size_of::<Pose>()
            + self.queue.capacity() * core::mem::size_of::<u16>()
            + self.computed_free.capacity() * core::mem::size_of::<u64>()
    }

    fn start<'a>(
        &'a mut self,
        occupied: &'a [u64; WORD_BITS],
        lanes: u64,
        state_count: usize,
        work: &mut PrefixPoseWork,
    ) -> Option<ReverseRelation<'a>> {
        // A witnessed, refused, or cancelled query may have left queued
        // states. Reset its entire frontier and visibility before reuse.
        self.queue.clear();
        let word_count = state_count.div_ceil(WORD_BITS);
        if word_count > self.computed_free.len() {
            self.computed_free
                .try_reserve_exact(word_count - self.computed_free.len())
                .ok()?;
            self.computed_free.resize(word_count, 0);
        }
        self.computed_free.fill(0);
        if state_count > self.poses.len() {
            self.poses
                .try_reserve_exact(state_count - self.poses.len())
                .ok()?;
            self.poses.resize(state_count, Pose::default());
        }
        self.queue.try_reserve_exact(state_count).ok()?;
        work.peak_logical_bytes = work.peak_logical_bytes.max(
            core::mem::size_of::<Self>()
                + core::mem::size_of::<ReverseRelation<'_>>()
                + core::mem::size_of_val(occupied)
                + self.retained_bytes(),
        );
        Some(ReverseRelation {
            scratch: self,
            occupied,
            lanes,
        })
    }
}

struct ReverseRelation<'a> {
    scratch: &'a mut ReverseScratch,
    occupied: &'a [u64; WORD_BITS],
    lanes: u64,
}

impl ReverseRelation<'_> {
    fn free(
        &mut self,
        template: &ReachabilityTemplate,
        index: usize,
        work: &mut PrefixPoseWork,
    ) -> u64 {
        let known = &mut self.scratch.computed_free[index / WORD_BITS];
        let bit = 1_u64 << (index % WORD_BITS);
        if *known & bit == 0 {
            let mut cells = template.state_masks[index];
            let mut free = 0;
            if cells != INVALID_STATE_MASK {
                free = self.lanes;
                while cells != 0 && free != 0 {
                    free &= !self.occupied[cells.trailing_zeros() as usize];
                    cells &= cells - 1;
                    work.pose_cell_terms += 1;
                }
            }
            // The first query of this pose initializes all of its state.
            self.scratch.poses[index] = Pose {
                free,
                ..Pose::default()
            };
            *known |= bit;
            work.pose_domains += 1;
        }
        self.scratch.poses[index].free
    }

    fn enqueue(&mut self, index: usize, lanes: u64) {
        // Every seed, translation and rotation computes the source's free
        // lanes first, so it cannot enqueue a pose with stale visitation.
        debug_assert_ne!(
            self.scratch.computed_free[index / WORD_BITS] & (1_u64 << (index % WORD_BITS)),
            0
        );
        let pose = &mut self.scratch.poses[index];
        let unseen = lanes & !pose.visited;
        if unseen != 0 {
            if pose.pending == 0 {
                self.scratch.queue.push_back(index as u16);
            }
            pose.pending |= unseen;
            pose.visited |= unseen;
        }
    }

    fn pop(&mut self) -> Option<(usize, u64)> {
        let current = usize::from(self.scratch.queue.pop_front()?);
        let pending = core::mem::take(&mut self.scratch.poses[current].pending);
        Some((current, pending))
    }
}

pub(super) fn compile_prefix_target_lanes_from_planes(
    template: &ReachabilityTemplate,
    occupied: &[u64; WORD_BITS],
    lanes: u64,
    locks: &[InstantiatedRealization],
    control: &ExecutionControl,
    work: &mut PrefixPoseWork,
    scratch: &mut ReverseScratch,
) -> Result<Option<u64>, WasmExactSearchError> {
    let check = || {
        if control.is_cancelled() {
            Err(WasmExactSearchError::Cancelled)
        } else {
            Ok(())
        }
    };
    check()?;
    work.attempted_relations += 1;
    let state_count = template.state_masks.len();
    if lanes == 0
        || !(1..=10).contains(&template.width)
        || !(1..=6).contains(&template.height)
        || !(1..=MAX_POSES).contains(&state_count)
    {
        return Ok(None);
    }
    let Some(first) = locks.first() else {
        return Ok(None);
    };
    if first.lock_mask.count_ones() != 4 {
        return Ok(None);
    }
    let mut seeds = [None; 4];
    for (i, lock) in locks.iter().enumerate() {
        if i.is_multiple_of(CANCEL_INTERVAL) {
            check()?;
        }
        // Realizations may share a result only for one concrete operation.
        // Validate each pose against that exact footprint, not just its anchor.
        if lock.lock_mask != first.lock_mask || lock.lock_y >= template.height as i8 {
            return Ok(None);
        }
        let Some(index) = super::state_index(
            template.width,
            template.ceiling,
            State {
                rotation: lock.rotation,
                x: lock.x,
                y: i16::from(lock.lock_y),
            },
        ) else {
            return Ok(None);
        };
        if template.state_masks[index] != lock.lock_mask {
            return Ok(None);
        }
        let seed = &mut seeds[lock.rotation.quarter_turns() as usize];
        let pose = (index, lock.lock_y == 0);
        if seed.is_some_and(|previous| previous != pose) {
            return Ok(None);
        }
        *seed = Some(pose);
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
    work.global_kick_pruned_edges_peak = work
        .global_kick_pruned_edges_peak
        .max(template.globally_pruned_rotation_sources as u64);
    let Some(mut relation) = scratch.start(occupied, lanes, state_count, work) else {
        check()?;
        return Ok(None);
    };
    let mut unresolved = 0_u64;
    for (seed, is_floor) in seeds.into_iter().flatten() {
        let mut grounded = relation.free(template, seed, work);
        let down = template.translation_targets[seed][0];
        if grounded != 0 && !is_floor && down != INVALID_STATE_INDEX {
            work.ground_terms += 1;
            grounded &= !relation.free(template, usize::from(down), work);
        }
        unresolved |= grounded;
        relation.enqueue(seed, grounded);
    }

    let mut reached = 0_u64;
    let mut worklist_pops = 0_usize;
    let maximum_pops = state_count * lanes.count_ones() as usize;
    let mut action_terms = 0_u64;
    while unresolved != 0 {
        let Some((current, pending)) = relation.pop() else {
            break;
        };
        if worklist_pops.is_multiple_of(CANCEL_INTERVAL) {
            check()?;
        }
        worklist_pops += 1;
        // Each enqueue contributes at least one previously unseen pose/lane.
        // This is a termination invariant, not an empirical work cutoff.
        debug_assert!(worklist_pops <= maximum_pops);
        work.worklist_pops += 1;
        let delta = pending & unresolved;
        if delta == 0 {
            continue;
        }
        work.reached_pose_lanes += u64::from(delta.count_ones());
        if template.is_sky_seed(current) {
            reached |= delta;
            unresolved &= !delta;
            continue;
        }
        for &source in &template.reverse_translation_sources[current] {
            if source != INVALID_STATE_INDEX {
                action_terms += 1;
                work.translation_terms += 1;
                let source = usize::from(source);
                let added = delta & relation.free(template, source, work);
                relation.enqueue(source, added);
            }
        }
        let start = template.reverse_rotation_offsets[current] as usize;
        let end = template.reverse_rotation_offsets[current + 1] as usize;
        for &rotation in &template.reverse_rotation_sources[start..end] {
            action_terms += 1;
            if action_terms.is_multiple_of(CANCEL_INTERVAL as u64) {
                check()?;
            }
            work.ordered_kick_terms += 1;
            #[cfg(any(test, feature = "local-search-ab"))]
            {
                work.reverse_rotation_candidate_checks += 1;
            }
            let source = usize::from(rotation.source);
            let mut added = delta & relation.free(template, source, work);
            for &prior in super::reverse_rotation_prior_targets(template, rotation) {
                if added == 0 {
                    break;
                }
                action_terms += 1;
                if action_terms.is_multiple_of(CANCEL_INTERVAL as u64) {
                    check()?;
                }
                work.ordered_kick_terms += 1;
                #[cfg(any(test, feature = "local-search-ab"))]
                {
                    work.reverse_rotation_prior_checks += 1;
                }
                // Current is already known free. Any earlier free target
                // wins, even if that earlier pose was already visited.
                added &= !relation.free(template, usize::from(prior), work);
            }
            relation.enqueue(source, added);
        }
    }

    check()?;
    work.completed_relations += 1;
    Ok(Some(reached))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_core_domain::piece::rotation::RotationState;
    use clearra_rules::kicks::KickTableProfileId;

    use super::*;
    use crate::reachability_reference;

    fn planes(boards: &[(usize, u64)]) -> ([u64; WORD_BITS], u64) {
        let mut occupied = [0_u64; WORD_BITS];
        let mut lanes = 0_u64;
        for &(lane, mut board) in boards {
            let bit = 1_u64 << lane;
            lanes |= bit;
            while board != 0 {
                occupied[board.trailing_zeros() as usize] |= bit;
                board &= board - 1;
            }
        }
        (occupied, lanes)
    }

    fn set_global_filter(template: ReachabilityTemplate, enabled: bool) -> ReachabilityTemplate {
        set_global_filter_mode(
            template,
            if enabled {
                super::super::GlobalKickPruneMode::Current
            } else {
                super::super::GlobalKickPruneMode::Off
            },
        )
    }

    fn set_global_filter_mode(
        mut template: ReachabilityTemplate,
        mode: super::super::GlobalKickPruneMode,
    ) -> ReachabilityTemplate {
        let (offsets, sources, work) = super::super::compile_reverse_rotation_sources(
            &template.state_masks,
            &template.rotation_target_offsets,
            &template.rotation_targets,
            mode,
        );
        template.globally_pruned_rotation_sources = template.rotation_targets.len() - sources.len();
        template.reverse_rotation_offsets = offsets;
        template.reverse_rotation_sources = sources;
        template.global_kick_prune_compile_work = work;
        template
    }

    #[test]
    fn custom_upward_kicks_preserve_earlier_guards_beyond_narrow_sky_coordinates() {
        use crate::backend::wasm_cpu::compact_kick_authority::CompactKickAuthority;
        use clearra_rules::kicks::{
            KickOffset, KickOffsetSequence, KickTableEntry, KickTableProfile, SrsKicks,
            VerifiedKickTableProfile,
        };
        use clearra_rules::profile::rule_profile::RuleProfileId;

        const WIDTH: u8 = 6;
        const HEIGHT: u8 = 5;
        let wall = (0..HEIGHT).fold(0_u64, |board, y| board | (0b001100 << (WIDTH * y)));
        let sealed = wall | (0b110000 << 12);
        let boards = [(0, sealed), (63, 0)];
        let (occupied, lanes) = planes(&boards);
        let locks = RotationState::ALL.map(|rotation| InstantiatedRealization {
            lock_mask: 0b110000 | (0b110000 << WIDTH),
            rotation,
            x: 4,
            lock_y: 0,
        });

        for upward in [7, i8::MAX] {
            let base = SrsKicks::srs_plus_profile();
            let entries = base
                .entries()
                .iter()
                .map(|entry| {
                    if entry.transition().piece() == PieceKind::O {
                        KickTableEntry::new(
                            entry.transition(),
                            KickOffsetSequence::new(vec![
                                KickOffset::new(0, upward),
                                KickOffset::new(4, 0),
                            ]),
                        )
                    } else {
                        entry.clone()
                    }
                })
                .collect();
            let profile = VerifiedKickTableProfile::try_new(KickTableProfile::new(
                KickTableProfileId::Custom,
                RuleProfileId::SrsPlus,
                entries,
            ))
            .unwrap();
            let authority = CompactKickAuthority::admit_profile(
                WIDTH,
                HEIGHT,
                RuleProfileId::SrsPlus,
                &profile,
            )
            .unwrap();
            assert_eq!(
                authority.piece(PieceKind::O).ceiling,
                i16::from(HEIGHT) + i16::from(upward)
            );
            for mode in [
                super::super::GlobalKickPruneMode::Off,
                super::super::GlobalKickPruneMode::Current,
                super::super::GlobalKickPruneMode::Improved,
            ] {
                let template = set_global_filter_mode(
                    ReachabilityTemplate::compile_authority(&authority, PieceKind::O),
                    mode,
                );
                let result = compile_prefix_target_lanes_from_planes(
                    &template,
                    &occupied,
                    lanes,
                    &locks,
                    &ExecutionControl::default(),
                    &mut PrefixPoseWork::default(),
                    &mut ReverseScratch::default(),
                )
                .unwrap()
                .expect("full admitted sky domain is evaluated");
                assert_eq!(result, 1_u64 << 63, "upward={upward}, filter={mode:?}");
                for (lane, board) in boards {
                    let oracle = reachability_reference::reference_spawn_locks_for_ordered_profile(
                        WIDTH,
                        HEIGHT,
                        board,
                        PieceKind::O,
                        profile.profile(),
                    )
                    .unwrap();
                    assert_eq!(
                        result & (1_u64 << lane) != 0,
                        oracle.iter().any(|a| a & (1 << 4) != 0)
                    );
                }
            }
        }
    }

    #[test]
    fn complete_ordered_prefix_beyond_255_candidates_is_not_truncated_or_renumbered() {
        use crate::backend::wasm_cpu::compact_kick_authority::CompactKickAuthority;
        use clearra_rules::kicks::{
            KickOffset, KickOffsetSequence, KickTableEntry, KickTableProfile, KickTransition,
            SrsKicks, VerifiedKickTableProfile,
        };
        use clearra_rules::profile::rule_profile::RuleProfileId;

        let base = SrsKicks::profile();
        let selected = KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
        let mut offsets = vec![KickOffset::new(0, 0); 300];
        offsets.push(KickOffset::new(1, -1));
        let entries = base
            .entries()
            .iter()
            .map(|entry| {
                if entry.transition() == selected {
                    KickTableEntry::new(selected, KickOffsetSequence::new(offsets.clone()))
                } else {
                    entry.clone()
                }
            })
            .collect();
        let profile = VerifiedKickTableProfile::try_new(KickTableProfile::new(
            KickTableProfileId::Imported,
            RuleProfileId::Srs,
            entries,
        ))
        .unwrap();
        let authority =
            CompactKickAuthority::admit_profile(10, 3, RuleProfileId::Srs, &profile).unwrap();
        let boards = [(0, 0x4002), (63, 0x4802)];
        let (occupied, lanes) = planes(&boards);
        let lock = InstantiatedRealization {
            lock_mask: 0x201c,
            rotation: RotationState::Zero,
            x: 2,
            lock_y: 0,
        };
        for mode in [
            super::super::GlobalKickPruneMode::Off,
            super::super::GlobalKickPruneMode::Current,
            super::super::GlobalKickPruneMode::Improved,
        ] {
            let template = set_global_filter_mode(
                ReachabilityTemplate::compile_authority(&authority, PieceKind::T),
                mode,
            );
            let source = super::super::state_index(
                10,
                template.ceiling,
                State {
                    rotation: RotationState::Right,
                    x: 2,
                    y: 0,
                },
            )
            .unwrap();
            let first = template.rotation_target_offsets[source * 3 + 1] as usize;
            assert_eq!(template.rotation_kick_indices[first + 300], 300);
            assert!(template.reverse_rotation_sources.iter().any(|edge| {
                usize::from(edge.source) == source
                    && edge.slot == 1
                    && edge.prior_target_count == 300
            }));
            let result = compile_prefix_target_lanes_from_planes(
                &template,
                &occupied,
                lanes,
                &[lock],
                &ExecutionControl::default(),
                &mut PrefixPoseWork::default(),
                &mut ReverseScratch::default(),
            )
            .unwrap()
            .expect("all 300 earlier free-candidate reads remain representable");
            assert_eq!(result, 1_u64 << 63, "filter={mode:?}");
            for (lane, board) in boards {
                let oracle = reachability_reference::reference_spawn_locks_for_ordered_profile(
                    10,
                    3,
                    board,
                    PieceKind::T,
                    profile.profile(),
                )
                .unwrap();
                assert_eq!(result & (1_u64 << lane) != 0, oracle[0] & (1 << 2) != 0);
            }
        }
    }

    #[test]
    fn global_kick_pruning_preserves_reverse_lanes_above_four_lines_and_reduces_work() {
        const WIDTH: u8 = 10;
        let control = ExecutionControl::default();
        let mut before_terms = 0;
        let mut after_terms = 0;
        let mut compared_locks = 0;
        let mut positive = 0;
        let mut negative = 0;
        let mut removed_edges = 0;
        for height in [5_u8, 6] {
            let boards = [
                (0, 0),
                (1, 0x4002),
                (7, 0x4802),
                (19, (0x3ff_u64 & !(3 << 4)) << ((height - 2) * WIDTH)),
                (47, 0x3f_5022_07e7),
                (
                    63,
                    (0x1ff_u64 << ((height - 1) * WIDTH)) | (0x155_u64 << ((height - 3) * WIDTH)),
                ),
            ];
            let (occupied, lanes) = planes(&boards);
            for profile in [
                KickTableProfileId::Srs90,
                KickTableProfileId::SrsPlus,
                KickTableProfileId::SrsX,
                KickTableProfileId::Jstris180,
                KickTableProfileId::NoKick,
            ] {
                for piece in PieceKind::STANDARD_TETROMINOES {
                    let baseline = set_global_filter(
                        ReachabilityTemplate::compile(WIDTH, height, piece, profile),
                        false,
                    );
                    let filtered = set_global_filter(
                        ReachabilityTemplate::compile(WIDTH, height, piece, profile),
                        true,
                    );
                    let reused = set_global_filter_mode(
                        ReachabilityTemplate::compile(WIDTH, height, piece, profile),
                        super::super::GlobalKickPruneMode::Improved,
                    );
                    assert_eq!(
                        filtered.reverse_rotation_offsets,
                        reused.reverse_rotation_offsets
                    );
                    assert_eq!(
                        filtered.reverse_rotation_sources,
                        reused.reverse_rotation_sources
                    );
                    // Both runs retain precisely the same ordered guard input.
                    assert_eq!(baseline.state_masks, filtered.state_masks);
                    assert_eq!(
                        baseline.rotation_target_offsets,
                        filtered.rotation_target_offsets
                    );
                    assert_eq!(baseline.rotation_targets, filtered.rotation_targets);
                    assert_eq!(
                        baseline.rotation_kick_indices,
                        filtered.rotation_kick_indices
                    );
                    removed_edges += filtered.globally_pruned_rotation_sources;
                    let reference = boards.map(|(lane, board)| {
                        (
                            lane,
                            reachability_reference::reference_spawn_lock_anchors(
                                WIDTH, height, board, piece, profile,
                            )
                            .unwrap(),
                        )
                    });
                    let mut targets = BTreeMap::<u64, Vec<InstantiatedRealization>>::new();
                    for (index, &mask) in filtered.state_masks.iter().enumerate() {
                        if mask.count_ones() != 4 {
                            continue;
                        }
                        let state = super::super::state_from_index(WIDTH, filtered.ceiling, index);
                        targets
                            .entry(mask)
                            .or_default()
                            .push(InstantiatedRealization {
                                lock_mask: mask,
                                rotation: state.rotation,
                                x: state.x,
                                lock_y: i8::try_from(state.y).expect("low target"),
                            });
                    }
                    let mut old_scratch = ReverseScratch::default();
                    let mut new_scratch = ReverseScratch::default();
                    let mut reused_scratch = ReverseScratch::default();
                    for locks in targets.values().step_by(11) {
                        let mut old_work = PrefixPoseWork::default();
                        let mut new_work = PrefixPoseWork::default();
                        let mut reused_work = PrefixPoseWork::default();
                        let old = compile_prefix_target_lanes_from_planes(
                            &baseline,
                            &occupied,
                            lanes,
                            locks,
                            &control,
                            &mut old_work,
                            &mut old_scratch,
                        )
                        .unwrap()
                        .expect("unfiltered finite target relation");
                        let new = compile_prefix_target_lanes_from_planes(
                            &filtered,
                            &occupied,
                            lanes,
                            locks,
                            &control,
                            &mut new_work,
                            &mut new_scratch,
                        )
                        .unwrap()
                        .expect("filtered finite target relation");
                        let reused_result = compile_prefix_target_lanes_from_planes(
                            &reused,
                            &occupied,
                            lanes,
                            locks,
                            &control,
                            &mut reused_work,
                            &mut reused_scratch,
                        )
                        .unwrap()
                        .expect("reused-predicate finite target relation");
                        assert_eq!(new, old, "{height}L {piece:?} {profile:?} {locks:?}");
                        assert_eq!(reused_result, new);
                        assert_eq!(reused_work, new_work);
                        assert_eq!(new_work.worklist_pops, old_work.worklist_pops);
                        assert_eq!(new_work.reached_pose_lanes, old_work.reached_pose_lanes);
                        assert!(new_work.pose_domains <= old_work.pose_domains);
                        assert!(new_work.ordered_kick_terms <= old_work.ordered_kick_terms);
                        assert_eq!(
                            new_work.ordered_kick_terms,
                            new_work.reverse_rotation_candidate_checks
                                + new_work.reverse_rotation_prior_checks
                        );
                        assert_eq!(old_work.global_kick_pruned_edges_peak, 0);
                        assert_eq!(
                            new_work.global_kick_pruned_edges_peak,
                            filtered.globally_pruned_rotation_sources as u64
                        );
                        before_terms += old_work.ordered_kick_terms;
                        after_terms += new_work.ordered_kick_terms;
                        for (lane, anchors) in reference {
                            let expected = locks.iter().any(|lock| {
                                let bit =
                                    lock.lock_y as usize * usize::from(WIDTH) + lock.x as usize;
                                anchors[lock.rotation.quarter_turns() as usize] & (1_u64 << bit)
                                    != 0
                            });
                            assert_eq!(
                                new & (1_u64 << lane) != 0,
                                expected,
                                "{height}L {piece:?} {profile:?} lane={lane} {locks:?}"
                            );
                            compared_locks += 1;
                            positive += usize::from(expected);
                            negative += usize::from(!expected);
                        }
                    }
                }
            }
        }
        assert!(compared_locks > 1_000);
        assert!(positive > 0 && negative > 0);
        assert!(removed_edges > 0);
        assert!(after_terms < before_terms);
        eprintln!("global E2 reverse parity: heights=5,6 profiles=5 pieces=7 locks={compared_locks} positive={positive} negative={negative} template_edges_removed={removed_edges} ordered_terms_before={before_terms} ordered_terms_after={after_terms}");
    }

    #[test]
    fn reverse_target_lanes_match_independent_primitive_across_profiles() {
        let width = 10;
        let height = 4;
        let boards = [
            (0, 0),
            (3, 0x4002),
            (8, 0x4802),
            (23, 0x03_9ff4_a4a4),
            (63, 0x3f_5022_07e7),
        ];
        let (occupied, lanes) = planes(&boards);
        let control = ExecutionControl::default();
        let mut observed_witness = false;
        let mut observed_miss = false;
        let mut observed_realization_union = false;
        let mut scratch = ReverseScratch::default();
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(width, height, piece, profile);
                let reference = boards.map(|(lane, board)| {
                    (
                        lane,
                        reachability_reference::reference_spawn_lock_anchors(
                            width, height, board, piece, profile,
                        )
                        .unwrap(),
                    )
                });
                let mut targets = BTreeMap::<u64, Vec<InstantiatedRealization>>::new();
                for (index, &mask) in template.state_masks.iter().enumerate() {
                    if mask.count_ones() != 4 {
                        continue;
                    }
                    let state = super::super::state_from_index(width, template.ceiling, index);
                    targets
                        .entry(mask)
                        .or_default()
                        .push(InstantiatedRealization {
                            lock_mask: mask,
                            rotation: state.rotation,
                            x: state.x,
                            lock_y: i8::try_from(state.y).expect("low target"),
                        });
                }
                for locks in targets.values().step_by(7) {
                    let mut work = PrefixPoseWork::default();
                    let actual = compile_prefix_target_lanes_from_planes(
                        &template,
                        &occupied,
                        lanes,
                        locks,
                        &control,
                        &mut work,
                        &mut scratch,
                    )
                    .unwrap()
                    .expect("bounded concrete operation");
                    assert_eq!(work.completed_relations, 1);
                    observed_realization_union |= locks.len() > 1;
                    for (lane, anchors) in reference {
                        let expected = locks.iter().any(|lock| {
                            let cell = lock.lock_y as usize * usize::from(width) + lock.x as usize;
                            anchors[lock.rotation.quarter_turns() as usize] & (1_u64 << cell) != 0
                        });
                        assert_eq!(
                            actual & (1_u64 << lane) != 0,
                            expected,
                            "{profile:?} {piece:?} lane {lane} locks {locks:?}"
                        );
                        observed_witness |= expected;
                        observed_miss |= !expected;
                    }
                }
            }
        }
        assert!(observed_witness && observed_miss && observed_realization_union);
    }

    #[test]
    fn reverse_target_uses_first_legal_kick_per_lane_and_refuses_unbound_targets() {
        let profile = KickTableProfileId::Srs90;
        let template = ReachabilityTemplate::compile(10, 3, PieceKind::T, profile);
        let boards = [(0, 0x4002), (63, 0x4802)];
        let (occupied, lanes) = planes(&boards);
        let target = InstantiatedRealization {
            lock_mask: 0x201c,
            rotation: RotationState::Zero,
            x: 2,
            lock_y: 0,
        };
        let control = ExecutionControl::default();
        let mut work = PrefixPoseWork::default();
        let mut scratch = ReverseScratch::default();
        assert_eq!(
            compile_prefix_target_lanes_from_planes(
                &template,
                &occupied,
                lanes,
                &[target],
                &control,
                &mut work,
                &mut scratch,
            )
            .unwrap(),
            Some(1_u64 << 63),
        );
        for (lane, board) in boards {
            let scalar = reachability_reference::reference_spawn_lock_anchors(
                10,
                3,
                board,
                PieceKind::T,
                profile,
            )
            .unwrap();
            assert_eq!(scalar[0] & (1 << 2) != 0, lane == 63);
        }
        let wrong = InstantiatedRealization {
            x: target.x + 1,
            ..target
        };
        assert_eq!(
            compile_prefix_target_lanes_from_planes(
                &template,
                &occupied,
                lanes,
                &[wrong],
                &control,
                &mut work,
                &mut scratch,
            )
            .unwrap(),
            None,
        );
        assert_eq!(work.completed_relations, 1);
        control.cancellation.handle().cancel();
        assert!(matches!(
            compile_prefix_target_lanes_from_planes(
                &template,
                &occupied,
                lanes,
                &[target],
                &control,
                &mut work,
                &mut scratch,
            ),
            Err(WasmExactSearchError::Cancelled),
        ));
        assert_eq!(work.completed_relations, 1);
    }

    #[test]
    fn reverse_target_reused_storage_matches_cold_queries_with_changing_lanes() {
        let template =
            ReachabilityTemplate::compile(10, 3, PieceKind::T, KickTableProfileId::Srs90);
        let target = InstantiatedRealization {
            lock_mask: 0x201c,
            rotation: RotationState::Zero,
            x: 2,
            lock_y: 0,
        };
        let control = ExecutionControl::default();
        let mut reused = ReverseScratch::default();
        assert_eq!(reused.retained_bytes(), 0);
        for (boards, expected) in [
            (&[(0, 0x4002), (63, 0x4802)][..], 1_u64 << 63),
            (&[(0, 0x4802), (11, 0x4002)][..], 1),
            (&[(0, 0x4002), (11, 0x4802)][..], 1 << 11),
            (&[(63, 0x4002)][..], 0),
        ] {
            let (occupied, lanes) = planes(boards);
            let solve = |scratch: &mut ReverseScratch| {
                let mut work = PrefixPoseWork::default();
                let result = compile_prefix_target_lanes_from_planes(
                    &template,
                    &occupied,
                    lanes,
                    &[target],
                    &control,
                    &mut work,
                    scratch,
                )
                .unwrap();
                (result, work)
            };
            let mut cold = ReverseScratch::default();
            let cold_result = solve(&mut cold);
            assert_eq!(cold_result.0, Some(expected));
            assert_eq!(solve(&mut reused), cold_result);
            assert_ne!(reused.retained_bytes(), 0);
            assert_eq!(reused.retained_bytes(), cold.retained_bytes());
        }
    }
}

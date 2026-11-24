//! Target-scoped collision conditions observed by the exact reverse search.
//!
//! This observes an already completed standard `reverse_lock_reachable` call;
//! it performs no second traversal and allocates no storage. The condition
//! preserves that ONE Boolean target answer, including an exhaustive negative.
//! It never certifies the complete lock family or an arbitrary proven exit.

use super::*;

/// Recover a sufficient read footprint from the immediately preceding exact
/// reverse query on this template and target.
///
/// The caller must retain the source board, template/profile, canonical owner
/// and original-row frame along with this mask. If another board agrees on the
/// mask, every board-dependent branch in the processed trace agrees: target
/// collision/grounding, predecessor admission, and all earlier ordered kick
/// competitors. Induction on the queue cursor preserves the answer. A positive
/// trace ends at a real sky seed; a negative trace exhausts the whole queue.
///
/// Only the processed prefix is observed. A successful search may have queued
/// additional poses which it never processes. Conversely, an early target
/// rejection never starts a search, so a zero-count result must not inspect the
/// scratch queue left by an unrelated prior call. Missing or inconsistent
/// metadata returns `None`, allowing the existing exact path to remain the
/// authority. Do not pass a scoring predecessor query or an unqualified
/// `reverse_lock_reachable_via_proven_exits` result here.
pub(super) fn reverse_lock_dependency_mask(
    template: &ReachabilityTemplate,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &ReachabilityScratch,
    result: &ReverseReachabilityResult,
) -> Option<u64> {
    let cells = usize::from(template.width).checked_mul(usize::from(template.height))?;
    if template.width == 0
        || template.width > i8::MAX as u8
        || template.height == 0
        || cells > u64::BITS as usize
        || template.ceiling < template.height as i8
    {
        return None;
    }
    let state_count = 4_usize
        .checked_mul(usize::try_from(template.ceiling).ok()?.checked_add(1)?)?
        .checked_mul(usize::from(template.width))?;
    if state_count > usize::from(u16::MAX)
        || template.state_masks.len() != state_count
        || template.translation_targets.len() != state_count
        || template.reverse_translation_sources.len() != state_count
        || template.rotation_target_offsets.len() != state_count.checked_mul(3)?.checked_add(1)?
        || template.reverse_rotation_offsets.len() != state_count.checked_add(1)?
        || template.sky_seed_words.len() != state_count.div_ceil(64)
    {
        return None;
    }
    let board_mask = if cells == u64::BITS as usize {
        u64::MAX
    } else {
        (1_u64 << cells) - 1
    };
    let Some(target) = state_index(template.width, template.ceiling, State { rotation, x, y })
    else {
        // The standard query rejects this target before reading any board bit.
        return (!result.reachable && result.visited_state_count == 0).then_some(0);
    };
    if *template.state_masks.get(target)? == INVALID_STATE_MASK {
        return (!result.reachable && result.visited_state_count == 0).then_some(0);
    }

    let mut mask = valid_pose_mask(template, target, board_mask)?;
    if y != 0 {
        let below = template.translation_targets.get(target)?[0];
        if below != INVALID_STATE_INDEX {
            mask |= valid_pose_mask(template, usize::from(below), board_mask)?;
        }
    }
    if result.visited_state_count == 0 {
        // Target collision and grounding are the only possible board reads.
        // The queue may contain stale states, even from another template.
        return (!result.reachable).then_some(mask);
    }

    let processed = scratch.queue.get(..result.visited_state_count)?;
    if usize::from(*processed.first()?) != target
        || (!result.reachable && result.visited_state_count != scratch.queue.len())
    {
        return None;
    }
    let last = usize::from(*processed.last()?);
    if last >= state_count || result.reachable != template.is_sky_seed(last) {
        // In particular, a non-sky proven exit cannot become a spawn proof.
        return None;
    }

    for (cursor, &current) in processed.iter().enumerate() {
        let current = usize::from(current);
        mask |= valid_pose_mask(template, current, board_mask)?;
        if template.is_sky_seed(current) {
            if !result.reachable || cursor + 1 != processed.len() {
                return None;
            }
            // The primitive returns before examining this seed's predecessors.
            continue;
        }

        for &source in template.reverse_translation_sources.get(current)? {
            if source != INVALID_STATE_INDEX {
                mask |= admission_mask(template, usize::from(source), board_mask)?;
            }
        }
        let start = usize::try_from(*template.reverse_rotation_offsets.get(current)?).ok()?;
        let end = usize::try_from(*template.reverse_rotation_offsets.get(current + 1)?).ok()?;
        for source in template.reverse_rotation_sources.get(start..end)? {
            let source_index = usize::from(source.source);
            let slot = usize::from(source.slot);
            if slot >= 3 || (slot == 2 && !template.allow_180) {
                return None;
            }
            let transition = source_index.checked_mul(3)?.checked_add(slot)?;
            let prior_start =
                usize::try_from(*template.rotation_target_offsets.get(transition)?).ok()?;
            let transition_end =
                usize::try_from(*template.rotation_target_offsets.get(transition + 1)?).ok()?;
            let prior_end = prior_start.checked_add(usize::from(source.prior_target_count))?;
            if prior_end >= transition_end
                || usize::from(*template.rotation_targets.get(prior_end)?) != current
            {
                return None;
            }
            // Retain these guards conservatively even when an already visited
            // source lets the primitive skip this edge without reading them.
            for &earlier in template.rotation_targets.get(prior_start..prior_end)? {
                mask |= valid_pose_mask(template, usize::from(earlier), board_mask)?;
            }
            // Including this mask when the kick guard failed is conservative.
            // Geometrically invalid sources short-circuit without a board read.
            mask |= admission_mask(template, source_index, board_mask)?;
        }
    }
    Some(mask)
}

fn valid_pose_mask(template: &ReachabilityTemplate, index: usize, board_mask: u64) -> Option<u64> {
    let mask = *template.state_masks.get(index)?;
    (mask != INVALID_STATE_MASK && mask & !board_mask == 0).then_some(mask)
}

fn admission_mask(template: &ReachabilityTemplate, index: usize, board_mask: u64) -> Option<u64> {
    let mask = *template.state_masks.get(index)?;
    if mask == INVALID_STATE_MASK {
        Some(0)
    } else {
        (mask & !board_mask == 0).then_some(mask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reachability_reference::reference_spawn_lock_anchors;

    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];

    fn contains(anchors: [u64; 4], width: u8, target: State) -> bool {
        anchors[target.rotation.quarter_turns() as usize]
            & (1_u64 << (target.y as usize * usize::from(width) + target.x as usize))
            != 0
    }

    #[test]
    fn every_profile_and_piece_preserves_reverse_answers_under_each_outside_bit_change() {
        const WIDTH: u8 = 6;
        const HEIGHT: u8 = 4;
        const CELLS: usize = (WIDTH * HEIGHT) as usize;
        let full = (1_u64 << CELLS) - 1;
        // A roof with a one-cell edge opening gives both accessible top locks
        // and genuinely sealed, collision-free O targets below it.
        let board = (0x1f_u64 << 12) | (1_u64 << 5);
        let mut total_perturbations = 0;
        for profile in PROFILES {
            let mut positives = 0;
            let mut exhausted_negatives = 0;
            let mut positive_perturbations = 0;
            let mut negative_perturbations = 0;
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
                let oracle =
                    reference_spawn_lock_anchors(WIDTH, HEIGHT, board, piece, profile).unwrap();
                let mut candidates = [None; 3];
                for rotation in RotationState::ALL {
                    for y in 0..HEIGHT as i8 {
                        for x in 0..WIDTH as i8 {
                            let target = State { rotation, x, y };
                            let index = state_index(WIDTH, template.ceiling, target).unwrap();
                            if template.state_masks[index] == INVALID_STATE_MASK {
                                continue;
                            }
                            let expected = contains(oracle, WIDTH, target);
                            let valid_lock = board & template.state_masks[index] == 0
                                && grounded_index(&template, board, index);
                            let class = if expected {
                                0
                            } else if valid_lock {
                                1
                            } else {
                                2
                            };
                            candidates[class].get_or_insert(target);
                        }
                    }
                }
                assert!(
                    candidates[0].is_some(),
                    "{profile:?}/{piece:?}: positive fixture"
                );
                let mut changed_oracles: [Option<[u64; 4]>; CELLS] = [None; CELLS];
                let mut scratch = ReachabilityScratch::default();
                for (class, target) in candidates.into_iter().enumerate() {
                    let Some(target) = target else { continue };
                    let result = reverse_lock_reachable(
                        &template,
                        board,
                        target.rotation,
                        target.x,
                        target.y,
                        &mut scratch,
                    );
                    let expected = contains(oracle, WIDTH, target);
                    assert_eq!(
                        result.reachable, expected,
                        "{profile:?}/{piece:?}/{target:?}"
                    );
                    if class == 0 {
                        positives += 1;
                        assert!(result.visited_state_count > 0);
                    } else if class == 1 {
                        exhausted_negatives += 1;
                        assert!(result.visited_state_count > 0);
                        assert_eq!(result.visited_state_count, scratch.queue.len());
                    } else {
                        assert_eq!(result.visited_state_count, 0);
                    }
                    let mask = reverse_lock_dependency_mask(
                        &template,
                        target.rotation,
                        target.x,
                        target.y,
                        &scratch,
                        &result,
                    )
                    .expect("exact reverse trace certificate");
                    assert_eq!(mask & !full, 0);
                    let mut outside = full & !mask;
                    while outside != 0 {
                        let cell = outside.trailing_zeros() as usize;
                        outside &= outside - 1;
                        let changed_board = board ^ (1_u64 << cell);
                        let changed_oracle = *changed_oracles[cell].get_or_insert_with(|| {
                            reference_spawn_lock_anchors(
                                WIDTH,
                                HEIGHT,
                                changed_board,
                                piece,
                                profile,
                            )
                            .unwrap()
                        });
                        assert_eq!(
                            contains(changed_oracle, WIDTH, target),
                            expected,
                            "{profile:?}/{piece:?}/{target:?}, cell={cell}, mask={mask:#x}"
                        );
                        total_perturbations += 1;
                        if class == 0 {
                            positive_perturbations += 1;
                        } else if class == 1 {
                            negative_perturbations += 1;
                        }
                    }
                }
            }
            assert_eq!(positives, 7, "every piece gets a positive trace");
            assert!(
                exhausted_negatives > 0,
                "{profile:?}: real reverse negatives"
            );
            assert!(
                positive_perturbations > 0,
                "{profile:?}: nonvacuous positive reuse"
            );
            assert!(
                negative_perturbations > 0,
                "{profile:?}: nonvacuous negative reuse"
            );
        }
        eprintln!("reverse dependency: {total_perturbations} outside-bit oracle comparisons");
    }

    #[test]
    fn zero_count_rejections_ignore_stale_scratch_and_preserve_target_preconditions() {
        let template =
            ReachabilityTemplate::compile(4, 4, PieceKind::O, KickTableProfileId::SrsPlus);
        let mut scratch = ReachabilityScratch::default();
        // An early return is allowed to leave entirely unrelated queue contents.
        scratch.queue = vec![INVALID_STATE_INDEX];
        for (board, x, y, expected_mask) in [
            (1_u64, 0, 0, 3_u64 | (3_u64 << 4)),
            (0, 0, 1, 3_u64 | (3_u64 << 4) | (3_u64 << 8)),
            (0, 3, 0, 0),
        ] {
            let result =
                reverse_lock_reachable(&template, board, RotationState::Zero, x, y, &mut scratch);
            assert!(!result.reachable);
            assert_eq!(result.visited_state_count, 0);
            assert_eq!(scratch.queue, [INVALID_STATE_INDEX]);
            let mask = reverse_lock_dependency_mask(
                &template,
                RotationState::Zero,
                x,
                y,
                &scratch,
                &result,
            )
            .unwrap();
            assert_eq!(mask, expected_mask);
            for cell in 0..16 {
                if mask & (1_u64 << cell) == 0 {
                    let anchors = reference_spawn_lock_anchors(
                        4,
                        4,
                        board ^ (1_u64 << cell),
                        PieceKind::O,
                        KickTableProfileId::SrsPlus,
                    )
                    .unwrap();
                    assert!(!contains(
                        anchors,
                        4,
                        State {
                            rotation: RotationState::Zero,
                            x,
                            y
                        }
                    ));
                }
            }
        }
    }

    #[test]
    fn reverse_triple_kick_trace_retains_the_enabling_j_dependency() {
        const WIDTH: u8 = 5;
        const HEIGHT: u8 = 6;
        let cells = |coordinates: &[(u8, u8)]| {
            coordinates.iter().fold(0_u64, |mask, &(x, y)| {
                mask | (1_u64 << (usize::from(y) * usize::from(WIDTH) + usize::from(x)))
            })
        };
        let without_j = cells(&[
            (0, 0),
            (1, 0),
            (0, 1),
            (0, 2),
            (3, 0),
            (4, 0),
            (2, 1),
            (3, 1),
            (3, 2),
            (4, 2),
            (2, 3),
            (3, 3),
            (4, 1),
            (4, 3),
        ]);
        let j = cells(&[(0, 3), (0, 4), (0, 5), (1, 5)]);
        let profile = KickTableProfileId::SrsPlus;
        let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, PieceKind::T, profile);
        let target = State {
            rotation: RotationState::Right,
            x: 1,
            y: 1,
        };
        let mut scratch = ReachabilityScratch::default();
        for (board, expected) in [(without_j, false), (without_j | j, true)] {
            let oracle =
                reference_spawn_lock_anchors(WIDTH, HEIGHT, board, PieceKind::T, profile).unwrap();
            assert_eq!(contains(oracle, WIDTH, target), expected);
            let result = reverse_lock_reachable(
                &template,
                board,
                target.rotation,
                target.x,
                target.y,
                &mut scratch,
            );
            assert_eq!(result.reachable, expected);
            let mask = reverse_lock_dependency_mask(
                &template,
                target.rotation,
                target.x,
                target.y,
                &scratch,
                &result,
            )
            .unwrap();
            assert_ne!(mask & j, 0, "adding J changes ordered-kick reachability");
        }
    }

    #[test]
    fn incomplete_negative_or_non_sky_positive_trace_produces_no_certificate() {
        let template =
            ReachabilityTemplate::compile(4, 4, PieceKind::O, KickTableProfileId::SrsPlus);
        let target = state_index(
            4,
            template.ceiling,
            State {
                rotation: RotationState::Zero,
                x: 0,
                y: 0,
            },
        )
        .unwrap() as u16;
        let mut scratch = ReachabilityScratch::default();
        scratch.queue = vec![target, target];
        for reachable in [false, true] {
            let result = ReverseReachabilityResult {
                reachable,
                visited_state_count: 1,
            };
            assert!(reverse_lock_dependency_mask(
                &template,
                RotationState::Zero,
                0,
                0,
                &scratch,
                &result,
            )
            .is_none());
        }
    }
}

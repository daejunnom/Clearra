//! Same-board Boolean proof reuse must preserve the complete first-discovery
//! trace, not only the final answer. Final-rotation evidence has a different
//! contract and is compared separately under both scoring rankings.

use std::cell::Cell;

use super::*;
use crate::reachability_reference::reference_spawn_lock_anchors;

const PROFILES: [KickTableProfileId; 5] = [
    KickTableProfileId::Srs90,
    KickTableProfileId::SrsPlus,
    KickTableProfileId::SrsX,
    KickTableProfileId::Jstris180,
    KickTableProfileId::NoKick,
];
const WIDTH: u8 = 6;
const HEIGHT: u8 = 4;
const BOARDS: [u64; 6] = [
    0,
    1 << 5,
    (0x1f << 12) | (1 << 5),
    (0x0f << 12) | (1 << 5),
    0b101010 | (0b010101 << 6) | (0b100001 << 12),
    0b001111 | (0b001001 << 6) | (0b001111 << 12),
];

#[derive(Clone, Copy, Debug, Default)]
struct GuardWork {
    checks: usize,
    visited_skips: usize,
    nonempty_checks: usize,
    nonempty_skips: usize,
}

thread_local! {
    static MODE_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
    static GUARD_WORK: Cell<GuardWork> = const {
        Cell::new(GuardWork {
            checks: 0, visited_skips: 0, nonempty_checks: 0, nonempty_skips: 0,
        })
    };
}

pub(super) fn mode_override() -> Option<bool> {
    MODE_OVERRIDE.with(Cell::get)
}

pub(super) fn record_guard_check(nonempty: bool) {
    GUARD_WORK.with(|work| {
        let mut next = work.get();
        next.checks += 1;
        next.nonempty_checks += usize::from(nonempty);
        work.set(next);
    });
}

pub(super) fn record_visited_skip(nonempty: bool) {
    GUARD_WORK.with(|work| {
        let mut next = work.get();
        next.visited_skips += 1;
        next.nonempty_skips += usize::from(nonempty);
        work.set(next);
    });
}

fn observe<T>(enabled: bool, action: impl FnOnce() -> T) -> (T, GuardWork) {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            MODE_OVERRIDE.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(MODE_OVERRIDE.with(|mode| mode.replace(Some(enabled))));
    GUARD_WORK.with(|work| work.set(GuardWork::default()));
    let result = action();
    (result, GUARD_WORK.with(Cell::get))
}

fn targets(width: u8, height: u8) -> impl Iterator<Item = State> {
    RotationState::ALL.into_iter().flat_map(move |rotation| {
        (0..height as i8).flat_map(move |y| (0..width as i8).map(move |x| State { rotation, x, y }))
    })
}

fn reference_contains(anchors: [u64; 4], width: u8, target: State) -> bool {
    anchors[target.rotation.quarter_turns() as usize]
        & (1_u64 << (target.y as usize * usize::from(width) + target.x as usize))
        != 0
}

fn assert_same_scratch(legacy: &ReachabilityScratch, optimized: &ReachabilityScratch) {
    assert_eq!(legacy.generation, optimized.generation);
    assert_eq!(legacy.visited_generations, optimized.visited_generations);
    // Include queued-but-unprocessed states after an early positive return.
    assert_eq!(legacy.queue, optimized.queue);
    assert_eq!(legacy.retained_bytes(), optimized.retained_bytes());
}

fn assert_same_work(legacy: GuardWork, optimized: GuardWork) {
    assert_eq!(legacy.visited_skips, 0);
    assert_eq!(legacy.nonempty_skips, 0);
    assert_eq!(legacy.checks, optimized.checks + optimized.visited_skips);
    assert_eq!(
        legacy.nonempty_checks,
        optimized.nonempty_checks + optimized.nonempty_skips
    );
}

fn boolean_pair(
    template: &ReachabilityTemplate,
    board: u64,
    target: State,
    legacy_scratch: &mut ReachabilityScratch,
    optimized_scratch: &mut ReachabilityScratch,
) -> (ReverseReachabilityResult, GuardWork) {
    let (legacy, legacy_work) = observe(false, || {
        reverse_lock_reachable(
            template,
            board,
            target.rotation,
            target.x,
            target.y,
            legacy_scratch,
        )
    });
    let (optimized, optimized_work) = observe(true, || {
        reverse_lock_reachable(
            template,
            board,
            target.rotation,
            target.x,
            target.y,
            optimized_scratch,
        )
    });
    assert_eq!(
        (legacy.reachable, legacy.visited_state_count),
        (optimized.reachable, optimized.visited_state_count),
        "board={board:#x} target={target:?}"
    );
    assert_same_scratch(legacy_scratch, optimized_scratch);
    assert_same_work(legacy_work, optimized_work);
    let dependency = |scratch: &ReachabilityScratch, result: &ReverseReachabilityResult| {
        reverse_dependency::reverse_lock_dependency_mask(
            template,
            target.rotation,
            target.x,
            target.y,
            scratch,
            result,
        )
    };
    let legacy_mask = dependency(legacy_scratch, &legacy);
    assert!(
        legacy_mask.is_some(),
        "ordinary reverse query has a complete trace"
    );
    assert_eq!(legacy_mask, dependency(optimized_scratch, &optimized));
    if optimized.visited_state_count != 0 {
        assert!(optimized_scratch.queue.iter().all(|&index| {
            let mask = template.state_masks[usize::from(index)];
            mask != INVALID_STATE_MASK && board & mask == 0
        }));
    }
    (optimized, optimized_work)
}

#[test]
fn visited_first_mode_is_strict_and_defaults_to_enabled() {
    use std::env::VarError;

    assert_eq!(
        parse_reachability_visited_first(Err(VarError::NotPresent)),
        Ok(true)
    );
    assert_eq!(parse_reachability_visited_first(Ok("0".into())), Ok(false));
    assert_eq!(parse_reachability_visited_first(Ok("1".into())), Ok(true));
    for value in ["", "true", "false", "2", "01", " 1", "1 ", "1\n"] {
        assert_eq!(
            parse_reachability_visited_first(Ok(value.into())),
            Err("wasm_reachability_visited_first_mode_invalid")
        );
    }
    assert_eq!(
        parse_reachability_visited_first(Err(VarError::NotUnicode(std::ffi::OsString::new()))),
        Err("wasm_reachability_visited_first_mode_invalid")
    );
}

#[test]
fn visited_first_reverse_queries_preserve_answers_trace_and_guard_work_for_all_profiles() {
    let mut queries = 0;
    let mut checks = 0;
    let mut skips = 0;
    let mut nonempty_skips = 0;
    for profile in PROFILES {
        let mut positives = 0;
        let mut exhausted_negatives = 0;
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
            let mut legacy = ReachabilityScratch::default();
            let mut optimized = ReachabilityScratch::default();
            for board in BOARDS {
                let reference =
                    reference_spawn_lock_anchors(WIDTH, HEIGHT, board, piece, profile).unwrap();
                for target in targets(WIDTH, HEIGHT) {
                    let (result, work) =
                        boolean_pair(&template, board, target, &mut legacy, &mut optimized);
                    assert_eq!(
                        result.reachable,
                        reference_contains(reference, WIDTH, target),
                        "{profile:?} {piece:?} board={board:#x} target={target:?}"
                    );
                    positives += usize::from(result.reachable);
                    exhausted_negatives +=
                        usize::from(!result.reachable && result.visited_state_count != 0);
                    queries += 1;
                    checks += work.checks;
                    skips += work.visited_skips;
                    nonempty_skips += work.nonempty_skips;
                }
            }
        }
        assert!(positives > 0, "{profile:?} positive coverage");
        assert!(
            exhausted_negatives > 0,
            "{profile:?} exhaustive-negative coverage"
        );
    }
    assert!(checks > 0);
    assert!(
        skips > 0,
        "the optimized branch must actually omit kick guards"
    );
    assert!(
        nonempty_skips > 0,
        "skip guards with real earlier kick competitors, not only empty guards"
    );
    eprintln!(
        "visited-first differential: {queries} targets, {} legacy guards, {checks} optimized guards, {skips} guards skipped ({nonempty_skips} with earlier competitors)",
        checks + skips
    );
}

#[test]
fn visited_first_proven_exit_queries_preserve_first_discovery_and_answers() {
    let mut non_sky_exits_used = 0;
    for profile in PROFILES {
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
            let mut legacy = ReachabilityScratch::default();
            let mut optimized = ReachabilityScratch::default();
            for board in [BOARDS[0], BOARDS[2]] {
                let reference =
                    reference_spawn_lock_anchors(WIDTH, HEIGHT, board, piece, profile).unwrap();
                // These non-sky exits have an independent global spawn proof,
                // so composing them cannot create a new reachable target.
                let exits = targets(WIDTH, HEIGHT)
                    .filter(|&target| reference_contains(reference, WIDTH, target))
                    .take(3)
                    .map(|target| ConditionedReachabilityEntryPose {
                        rotation: target.rotation,
                        x: target.x,
                        y: target.y,
                    })
                    .collect::<Vec<_>>();
                assert!(!exits.is_empty());
                for target in targets(WIDTH, HEIGHT) {
                    let (before, before_work) = observe(false, || {
                        reverse_lock_reachable_via_proven_exits(
                            &template,
                            board,
                            target.rotation,
                            target.x,
                            target.y,
                            &mut legacy,
                            &exits,
                        )
                        .unwrap()
                    });
                    let (after, after_work) = observe(true, || {
                        reverse_lock_reachable_via_proven_exits(
                            &template,
                            board,
                            target.rotation,
                            target.x,
                            target.y,
                            &mut optimized,
                            &exits,
                        )
                        .unwrap()
                    });
                    assert_eq!(
                        (before.reachable, before.visited_state_count),
                        (after.reachable, after.visited_state_count)
                    );
                    assert_eq!(
                        after.reachable,
                        reference_contains(reference, WIDTH, target)
                    );
                    assert_same_scratch(&legacy, &optimized);
                    assert_same_work(before_work, after_work);
                    non_sky_exits_used +=
                        usize::from(after.reachable && after.visited_state_count == 1);
                }
            }
        }
    }
    assert!(non_sky_exits_used > 0);
}

fn scoring_pair<const LEGACY_T_SPIN: bool>(
    template: &ReachabilityTemplate,
    board: u64,
    target: State,
    legacy: &mut ReachabilityScratch,
    optimized: &mut ReachabilityScratch,
) -> Option<ScoringLockEvidence> {
    let evidence = |scratch: &mut ReachabilityScratch| {
        preferred_scoring_lock_evidence::<LEGACY_T_SPIN>(
            template,
            board,
            target.rotation,
            target.x,
            target.y,
            scratch,
        )
        .map(|evidence| {
            evidence.with_immobile_before_clear(scoring_lock_is_immobile(
                template,
                board,
                target.rotation,
                target.x,
                target.y,
            ))
        })
    };
    let (before, before_work) = observe(false, || evidence(legacy));
    let (after, after_work) = observe(true, || evidence(optimized));
    assert_eq!(
        before, after,
        "board={board:#x} target={target:?} legacy-rank={LEGACY_T_SPIN}"
    );
    assert_same_scratch(legacy, optimized);
    assert_same_work(before_work, after_work);
    after
}

#[test]
fn visited_first_preserves_final_rotation_evidence_for_both_scoring_rankings() {
    let mut rotations = 0;
    let mut absent = 0;
    for profile in PROFILES {
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
            let mut legacy = ReachabilityScratch::default();
            let mut optimized = ReachabilityScratch::default();
            for board in [BOARDS[0], BOARDS[2], BOARDS[5]] {
                for target in targets(WIDTH, HEIGHT) {
                    for evidence in [
                        scoring_pair::<false>(
                            &template,
                            board,
                            target,
                            &mut legacy,
                            &mut optimized,
                        ),
                        scoring_pair::<true>(&template, board, target, &mut legacy, &mut optimized),
                    ] {
                        rotations += usize::from(evidence.is_some());
                        absent += usize::from(evidence.is_none());
                    }
                }
            }
        }
    }
    assert!(rotations > 0);
    assert!(absent > 0);

    // The TST cavity requires a later first-success kick. Preserve its actual
    // predecessor, request and kick index, not merely an evidence-present bit.
    let template = ReachabilityTemplate::compile(5, 6, PieceKind::T, KickTableProfileId::SrsPlus);
    let board = [
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
        (0, 3),
        (0, 4),
        (0, 5),
        (1, 5),
    ]
    .into_iter()
    .fold(0_u64, |mask, (x, y)| mask | (1_u64 << (y * 5 + x)));
    let target = State {
        rotation: RotationState::Right,
        x: 1,
        y: 1,
    };
    let mut legacy = ReachabilityScratch::default();
    let mut optimized = ReachabilityScratch::default();
    for evidence in [
        scoring_pair::<false>(&template, board, target, &mut legacy, &mut optimized),
        scoring_pair::<true>(&template, board, target, &mut legacy, &mut optimized),
    ] {
        let evidence = evidence.expect("the upper-left J enables the TST rotation");
        assert!(evidence.last_action_was_rotation());
        assert!(evidence.used_kick());
        assert!(evidence.first_success_confirmed());
    }
}

#[test]
fn visited_first_ignores_stale_marks_across_wrap_boards_templates_and_rejected_targets() {
    let template =
        ReachabilityTemplate::compile(WIDTH, HEIGHT, PieceKind::T, KickTableProfileId::SrsPlus);
    let stale = || ReachabilityScratch {
        // Include invalid poses and a larger preceding domain. Wrapping must
        // clear all stale marks before visited can serve as a proof again.
        visited_generations: vec![u16::MAX; template.state_masks.len() + 8],
        generation: u16::MAX,
        queue: vec![INVALID_STATE_INDEX],
    };
    let mut legacy = stale();
    let mut optimized = stale();
    let target = State {
        rotation: RotationState::Zero,
        x: 1,
        y: 0,
    };
    let (result, _) = boolean_pair(&template, 0, target, &mut legacy, &mut optimized);
    assert!(result.reachable);
    assert_eq!(optimized.generation, 1);
    for board in BOARDS {
        for target in targets(WIDTH, HEIGHT) {
            boolean_pair(&template, board, target, &mut legacy, &mut optimized);
        }
    }
    for target in [
        State { x: -1, ..target },
        State {
            x: WIDTH as i8,
            ..target
        },
        State { y: -1, ..target },
        State {
            y: template.ceiling + 1,
            ..target
        },
        State {
            x: WIDTH as i8 - 1,
            ..target
        },
    ] {
        let generation = optimized.generation;
        let (result, _) = boolean_pair(&template, 0, target, &mut legacy, &mut optimized);
        assert!(!result.reachable);
        assert_eq!(result.visited_state_count, 0);
        assert_eq!(optimized.generation, generation);
    }
    let smaller = ReachabilityTemplate::compile(4, 3, PieceKind::O, KickTableProfileId::NoKick);
    for target in targets(4, 3) {
        boolean_pair(&smaller, 0, target, &mut legacy, &mut optimized);
    }
}

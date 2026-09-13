//! Real Range admission and Core ILC materialization for the observation
//! producer; synthetic graph qualification is not upstream qualification.
use std::collections::BTreeSet;

use crate::{
    AppOnlinePc4ObservationCandidateRequest, Pc4BagDisclosure, Pc4HiddenQueueDisclosure,
    Pc4HiddenQueueSource, Pc4ObservationCandidateBudgets, Pc4PartialBagRemainder,
};
use clearra_pc4_tablebase::{
    FixedQueueHoldBudgets, Pc4BagProfile, Pc4BagRevealBudgets, Pc4ExactProbability,
    Pc4ObservationFrontierBudgets, Pc4ObservationGraphBudgets,
};

use super::*;

fn start(
    fixture: &ClearPath,
    lines: u8,
    profile: Pc4RuleProfile,
    disclosure: Pc4QueueDisclosure,
    hold: FixedQueueHoldState,
    cache_records: usize,
) -> (AppOnlinePc4CandidateSession, Guard) {
    start_with_field(
        fixture,
        lines,
        profile,
        disclosure,
        hold,
        cache_records,
        None,
    )
}

fn start_with_field(
    fixture: &ClearPath,
    lines: u8,
    profile: Pc4RuleProfile,
    disclosure: Pc4QueueDisclosure,
    hold: FixedQueueHoldState,
    cache_records: usize,
    start_field_id: Option<u32>,
) -> (AppOnlinePc4CandidateSession, Guard) {
    start_prepared(
        fixture,
        lines,
        profile,
        hold,
        cache_records,
        start_field_id,
        |target| match prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
            target,
            Pc4InputSurface::NonInteractiveCli,
            disclosure,
        ))
        .unwrap()
        {
            Pc4InputDisclosureDecision::Ready(prepared) => prepared,
            other => panic!("fixture is fully disclosed: {other:?}"),
        },
    )
}

fn start_prepared(
    fixture: &ClearPath,
    lines: u8,
    profile: Pc4RuleProfile,
    hold: FixedQueueHoldState,
    cache_records: usize,
    start_field_id: Option<u32>,
    prepare: impl FnOnce(
        clearra_pc4_tablebase::QualifiedPc4TargetIdentity,
    ) -> crate::Pc4PreparedOnlineInput,
) -> (AppOnlinePc4CandidateSession, Guard) {
    let target_lines = Pc4TargetLines::new(lines).unwrap();
    let generation = pin(activated_snapshot_for_dataset(
        "observation-range-fixture",
        Some(profile),
        target_lines,
        u32::from(lines) + 1,
        *fixture.ids_by_step.last().unwrap(),
        &fixture.dataset,
    ));
    let target = generation
        .activated_snapshot()
        .qualified_target(profile, Pc4TerminalUseCase::PcSearch, target_lines)
        .unwrap();
    let prepared = prepare(target);
    let board = StandardPcBoard::from_words(lines, [fixture.initial_board, 0, 0, 0]).unwrap();
    let source = canonical_source(&prepared, board, hold);
    let guard = Guard::new(source.clone());
    let frontier_budgets = Pc4ObservationFrontierBudgets::new(
        Pc4BagRevealBudgets::new(
            nonzero(8),
            nonzero(512),
            nonzero(512),
            nonzero(8),
            nonzero(512),
            nonzero(512),
        ),
        FixedQueueHoldBudgets::new(nonzero(512), nonzero(512), nonzero(512), nonzero(512)),
        nonzero(8),
        nonzero(4),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(512),
    );
    let request = AppOnlinePc4ObservationCandidateRequest::for_prepared_input(
        &source,
        &prepared,
        board,
        hold,
        LookupSessionId::new(1).unwrap(),
        start_field_id.unwrap_or(fixture.ids_by_step[0]),
        frontier_budgets,
        TerminalDepthContract::QueueExhaustedOnly,
        FixedQueueTraversalBudgets::new(nonzero(128), nonzero(128), nonzero(4), nonzero(128)),
        FixedQueueTraversalPageBudgets::new(nonzero(8), nonzero(8)),
        Pc4ObservationGraphBudgets::new(
            nonzero(256),
            nonzero(1024),
            nonzero(1024),
            nonzero(512),
            nonzero(8),
            nonzero(8),
            nonzero(8),
        ),
        ConcretePathMaterializationBudgets::new(
            nonzero(4),
            nonzero(32),
            nonzero(256),
            nonzero(256),
        ),
        Pc4ObservationCandidateBudgets::new(
            nonzero(8),
            nonzero(8),
            nonzero(8),
            nonzero(8),
            nonzero(32),
            nonzero(512),
            nonzero(8192),
        ),
        Pc4LookupGraphCacheLimits::new(nonzero(cache_records), nonzero(4096), nonzero(4096)),
        nonzero(1),
        range_limits(),
        &guard,
    )
    .unwrap();
    let session =
        AppOnlinePc4CandidateSession::start_observation(generation, request, &guard).unwrap();
    (session, guard)
}

fn drive(
    session: &mut AppOnlinePc4CandidateSession,
    guard: &Guard,
    fixture: &ClearPath,
) -> (AppOnlinePc4CandidateStep, BTreeSet<u32>) {
    let mut lookups = BTreeSet::new();
    let mut previous_lookup: Option<LookupSessionId> = None;
    let mut ordinal = 0;
    for _ in 0..4096 {
        match session.step(guard) {
            AppOnlinePc4CandidateStep::NeedRange(request) => {
                assert!(session.completed_reducer_input().is_none());
                assert!(session.completed_observation_family().is_none());
                if previous_lookup != Some(request.lookup_session()) {
                    if let Some(previous) = previous_lookup {
                        assert!(request.lookup_session().get() > previous.get());
                    }
                    previous_lookup = Some(request.lookup_session());
                    ordinal = 0;
                    assert!(
                        lookups.insert(session.active_lookup_field_id().unwrap()),
                        "branches must share the graph cache"
                    );
                }
                ordinal += 1;
                session
                    .admit_range(
                        attempt(ordinal),
                        partial_input(&request, &fixture.dataset),
                        guard,
                    )
                    .unwrap();
            }
            AppOnlinePc4CandidateStep::ObservationProgress { .. } => {
                assert!(session.completed_reducer_input().is_none());
            }
            terminal => return (terminal, lookups),
        }
    }
    panic!("bounded observation fixture stalled")
}

#[test]
fn pc4_range_hold_union_matches_seven_existing_products_for_all_profiles_and_target_lines() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    for lines in 1..=4 {
        let fixture = clear_path(lines);
        for profile in Pc4RuleProfile::ALL {
            let all_i = vec![Pc4GraphPiece::I; usize::from(lines)];
            let mut store_queue = vec![Pc4GraphPiece::O];
            store_queue.extend_from_slice(&all_i);
            let mut swap_queue = all_i.clone();
            swap_queue[0] = Pc4GraphPiece::O;
            for (queue, hold) in [
                (all_i.clone(), FixedQueueHoldState::Disabled),
                (store_queue, FixedQueueHoldState::Empty),
                (swap_queue, FixedQueueHoldState::Occupied(Pc4GraphPiece::I)),
                (
                    all_i.clone(),
                    FixedQueueHoldState::Occupied(Pc4GraphPiece::O),
                ),
                (all_i, FixedQueueHoldState::Occupied(Pc4GraphPiece::I)),
            ] {
                let (mut session, guard) = start(
                    &fixture,
                    lines,
                    profile,
                    Pc4QueueDisclosure::FixedExplicit(queue.clone()),
                    hold,
                    5,
                );
                let (terminal, lookups) = drive(&mut session, &guard, &fixture);
                assert!(
                    matches!(
                        terminal,
                        AppOnlinePc4CandidateStep::Complete {
                            canonical_candidates: 1,
                            ..
                        }
                    ),
                    "{lines}L {profile:?} {hold:?}: {terminal:?}"
                );
                assert_eq!(lookups.len(), usize::from(lines) + 1);
                let family = session.completed_observation_family().unwrap();
                assert_eq!(family.reveal_outcomes().len(), 1);
                assert_eq!(
                    family.total_reveal_probability(),
                    Pc4ExactProbability::one()
                );
                assert!(family.reveal_outcomes()[0]
                    .reveal()
                    .terminal_bag_state()
                    .is_none());
                let input = session.completed_reducer_input().unwrap();
                let mut masks = input.candidates()[0].placement_masks().to_vec();
                masks.sort_unstable();
                let mut expected = fixture.original_placements.clone();
                expected.sort_unstable();
                assert_eq!(masks, expected);
                product_contracts::assert_product_parity_with_supply(
                    lines,
                    profile,
                    fixture.initial_board,
                    &queue,
                    hold,
                    input,
                    &guard,
                );
                let address = input.candidates().as_ptr();
                let owned = session.into_completed_reducer_input(&guard).unwrap();
                assert_eq!(owned.candidates().as_ptr(), address);
                product_contracts::assert_owned_score_product_parity_with_supply(
                    lines,
                    profile,
                    fixture.initial_board,
                    &queue,
                    hold,
                    owned,
                    &guard,
                );
            }
        }
    }
}

fn start_pattern(
    fixture: &ClearPath,
    lines: u8,
    profile: Pc4RuleProfile,
    pattern: &str,
    hold: FixedQueueHoldState,
) -> (AppOnlinePc4CandidateSession, Guard) {
    let problem = product_contracts::compile_pattern_problem(
        lines,
        profile,
        fixture.initial_board,
        pattern,
        hold,
    );
    let mut preparation = crate::Pc4CompiledPatternPreparation::begin(
        problem,
        crate::Pc4CompiledPatternLimits::new(nonzero(5040), nonzero(8), nonzero(1)),
    )
    .unwrap();
    while !preparation.advance(nonzero(1), &|| false).unwrap() {}
    let pattern_source = preparation.finish().unwrap();
    start_prepared(fixture, lines, profile, hold, 5, None, |target| {
        crate::Pc4PreparedOnlineInput::for_compiled_pattern(
            target,
            Pc4InputSurface::NonInteractiveCli,
            pattern_source,
        )
        .unwrap()
    })
}

#[test]
fn pc4_compiled_pattern_range_union_matches_six_products_and_preserves_zero_hit_mass() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    for lines in 1..=4 {
        let fixture = clear_path(lines);
        for profile in Pc4RuleProfile::ALL {
            for (pattern, hold, zero_hits) in [
                (
                    format!("[IO]{}", "I".repeat(usize::from(lines) - 1)),
                    FixedQueueHoldState::Disabled,
                    1,
                ),
                (
                    format!("[OT]{}", "I".repeat(usize::from(lines))),
                    FixedQueueHoldState::Empty,
                    0,
                ),
                (
                    format!("[IO]{}", "I".repeat(usize::from(lines) - 1)),
                    FixedQueueHoldState::Occupied(Pc4GraphPiece::I),
                    0,
                ),
            ] {
                let (mut session, guard) = start_pattern(&fixture, lines, profile, &pattern, hold);
                let (terminal, lookups) = drive(&mut session, &guard, &fixture);
                assert!(
                    matches!(
                        terminal,
                        AppOnlinePc4CandidateStep::Complete {
                            canonical_candidates: 1,
                            ..
                        }
                    ),
                    "{lines}L {profile:?} {pattern} {hold:?}: {terminal:?}"
                );
                assert_eq!(
                    lookups.len(),
                    usize::from(lines) + 1,
                    "all pattern and hold branches share one cache"
                );
                let family = session.completed_observation_family().unwrap();
                assert_eq!(family.reveal_outcomes().len(), 2);
                assert_eq!(
                    family.total_reveal_probability(),
                    Pc4ExactProbability::one()
                );
                assert_eq!(
                    family
                        .reveal_outcomes()
                        .iter()
                        .filter(|outcome| outcome.candidates().is_empty())
                        .count(),
                    zero_hits
                );
                for (ordinal, outcome) in family.reveal_outcomes().iter().enumerate() {
                    assert_eq!(outcome.reveal().reveal_rank(), ordinal as u128);
                    assert_eq!(outcome.reveal().probability().numerator(), 1);
                    assert_eq!(outcome.reveal().probability().denominator(), 2);
                    assert_eq!(outcome.reveal().terminal_bag_state(), None);
                }
                let input = session.into_completed_reducer_input(&guard).unwrap();
                product_contracts::assert_pattern_product_parity(
                    lines,
                    profile,
                    fixture.initial_board,
                    &pattern,
                    hold,
                    input,
                    &guard,
                );
            }
        }
    }
}

#[test]
fn pc4_compiled_pattern_range_keeps_projected_duplicate_ordinals_in_probability_denominator() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = clear_path(1);
    for profile in Pc4RuleProfile::ALL {
        let (mut session, guard) = start_pattern(
            &fixture,
            1,
            profile,
            "[IO][TZ]",
            FixedQueueHoldState::Disabled,
        );
        let (terminal, lookups) = drive(&mut session, &guard, &fixture);
        assert!(matches!(
            terminal,
            AppOnlinePc4CandidateStep::Complete {
                canonical_candidates: 1,
                ..
            }
        ));
        assert_eq!(lookups.len(), 2);
        let family = session.completed_observation_family().unwrap();
        assert_eq!(family.reveal_outcomes().len(), 4);
        assert_eq!(
            family
                .reveal_outcomes()
                .iter()
                .filter(|outcome| outcome.candidates().is_empty())
                .count(),
            2
        );
        for outcome in family.reveal_outcomes() {
            assert_eq!(outcome.reveal().probability().denominator(), 4);
            assert_eq!(outcome.reveal().revealed_pieces().len(), 1);
        }
        let input = session.into_completed_reducer_input(&guard).unwrap();
        product_contracts::assert_pattern_product_parity(
            1,
            profile,
            fixture.initial_board,
            "[IO][TZ]",
            FixedQueueHoldState::Disabled,
            input,
            &guard,
        );
    }
}

fn hidden(visible: Pc4GraphPiece, placements: usize) -> Pc4QueueDisclosure {
    Pc4QueueDisclosure::PatternOrHidden(
        Pc4HiddenQueueDisclosure::new(
            Pc4HiddenQueueSource::Pattern,
            vec![visible],
            0,
            1,
            placements,
            Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).unwrap(),
            2,
            Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete([1, 1, 0, 0, 0, 0, 0])),
        )
        .unwrap(),
    )
}

#[test]
fn pc4_range_hidden_reveals_keep_zero_hit_mass_and_controllable_hold_choices_separate() {
    for (lines, current, hold, successful_outcomes) in [
        (1, Pc4GraphPiece::I, FixedQueueHoldState::Disabled, 2),
        (1, Pc4GraphPiece::O, FixedQueueHoldState::Empty, 1),
        (2, Pc4GraphPiece::I, FixedQueueHoldState::Disabled, 1),
        (
            2,
            Pc4GraphPiece::O,
            FixedQueueHoldState::Occupied(Pc4GraphPiece::I),
            1,
        ),
    ] {
        let fixture = clear_path(lines);
        let (mut session, guard) = start(
            &fixture,
            lines,
            Pc4RuleProfile::Srs,
            hidden(current, usize::from(lines)),
            hold,
            5,
        );
        let (terminal, _) = drive(&mut session, &guard, &fixture);
        assert!(
            matches!(
                terminal,
                AppOnlinePc4CandidateStep::Complete {
                    canonical_candidates: 1,
                    ..
                }
            ),
            "{terminal:?}"
        );
        let family = session.completed_observation_family().unwrap();
        assert_eq!(family.reveal_outcomes().len(), 2);
        assert_eq!(
            family.total_reveal_probability(),
            Pc4ExactProbability::one()
        );
        assert_eq!(
            family
                .reveal_outcomes()
                .iter()
                .filter(|outcome| !outcome.candidates().is_empty())
                .count(),
            successful_outcomes
        );
        assert!(family.reveal_outcomes().iter().all(|outcome| outcome
            .reveal()
            .probability()
            .numerator()
            == 1
            && outcome.reveal().probability().denominator() == 2
            && outcome.reveal().terminal_bag_state().is_some()));
    }
}

#[test]
fn pc4_waiting_range_cancellation_and_revocation_are_immediate_and_terminal() {
    let fixture = clear_path(1);
    for condition in 0..3 {
        let (mut session, guard) = start(
            &fixture,
            1,
            Pc4RuleProfile::Srs,
            Pc4QueueDisclosure::FixedExplicit(vec![Pc4GraphPiece::O, Pc4GraphPiece::I]),
            FixedQueueHoldState::Empty,
            5,
        );
        let AppOnlinePc4CandidateStep::NeedRange(pending) = session.step(&guard) else {
            panic!("first range required")
        };
        match condition {
            0 => guard.cancelled.set(true),
            1 => guard.source_current.set(false),
            _ => guard.snapshot_current.set(false),
        }
        let stopped = session.step(&guard);
        assert!(match condition {
            0 => matches!(stopped, AppOnlinePc4CandidateStep::Cancelled),
            1 => matches!(
                stopped,
                AppOnlinePc4CandidateStep::Failed(
                    AppOnlinePc4FixedQueueCandidateFailure::StaleSource
                )
            ),
            _ => matches!(
                stopped,
                AppOnlinePc4CandidateStep::Failed(
                    AppOnlinePc4FixedQueueCandidateFailure::StaleSnapshot
                )
            ),
        });
        assert!(session.active_lookup_field_id().is_none());
        assert!(session.completed_reducer_input().is_none());
        guard.cancelled.set(false);
        guard.source_current.set(true);
        guard.snapshot_current.set(true);
        assert_eq!(session.step(&guard), stopped);
        assert!(matches!(
            session.admit_range(
                attempt(1),
                partial_input(&pending, &fixture.dataset),
                &guard
            ),
            Err(AppOnlinePc4RangeError::LookupNotAwaitingRange)
        ));
    }
}

#[test]
fn pc4_hold_branches_share_cache_budget_and_cannot_seal_a_partial_union_on_failure() {
    let fixture = clear_path(4);
    let (mut session, guard) = start(
        &fixture,
        4,
        Pc4RuleProfile::Srs,
        Pc4QueueDisclosure::FixedExplicit(vec![
            Pc4GraphPiece::O,
            Pc4GraphPiece::I,
            Pc4GraphPiece::I,
            Pc4GraphPiece::I,
            Pc4GraphPiece::I,
        ]),
        FixedQueueHoldState::Empty,
        2,
    );
    let (failed, _) = drive(&mut session, &guard, &fixture);
    assert!(matches!(
        failed,
        AppOnlinePc4CandidateStep::Failed(
            AppOnlinePc4FixedQueueCandidateFailure::LookupAdmission {
                reason: "pc4_lookup_graph_cache_budget_exceeded",
                ..
            }
        )
    ));
    assert!(session.completed_reducer_input().is_none());
    assert!(session.completed_observation_family().is_none());
    assert_eq!(session.step(&guard), failed);
}

#[test]
fn pc4_range_admission_revocation_cannot_revive_a_candidate_session() {
    for cancelled in [false, true] {
        let fixture = clear_path(1);
        let (mut session, guard) = start(
            &fixture,
            1,
            Pc4RuleProfile::Srs,
            Pc4QueueDisclosure::FixedExplicit(vec![Pc4GraphPiece::I]),
            FixedQueueHoldState::Disabled,
            5,
        );
        let AppOnlinePc4CandidateStep::NeedRange(pending) = session.step(&guard) else {
            panic!("initial Range required")
        };
        if cancelled {
            guard.cancelled.set(true);
        } else {
            guard.snapshot_current.set(false);
        }
        let expected_error = if cancelled {
            clearra_pc4_tablebase::RangeAdmissionError::Cancelled
        } else {
            clearra_pc4_tablebase::RangeAdmissionError::SnapshotStale
        };
        assert_eq!(
            session.admit_range(
                attempt(1),
                partial_input(&pending, &fixture.dataset),
                &guard
            ),
            Err(AppOnlinePc4RangeError::Admission(expected_error))
        );
        // No intermediate step polls the rejected guard. Admission itself must
        // close the composed owner before a corrected/late response arrives.
        guard.cancelled.set(false);
        guard.snapshot_current.set(true);
        let expected = if cancelled {
            AppOnlinePc4CandidateStep::Cancelled
        } else {
            AppOnlinePc4CandidateStep::Failed(AppOnlinePc4FixedQueueCandidateFailure::StaleSnapshot)
        };
        assert_eq!(session.step(&guard), expected);
        assert_eq!(session.active_lookup_field_id(), None);
        assert!(session.completed_reducer_input().is_none());
        assert_eq!(
            session.admit_range(
                attempt(1),
                partial_input(&pending, &fixture.dataset),
                &guard
            ),
            Err(AppOnlinePc4RangeError::LookupNotAwaitingRange)
        );
        assert_eq!(session.step(&guard), expected);
    }
}

#[test]
fn pc4_initial_graph_field_must_match_even_when_the_queue_has_no_solution() {
    let fixture = clear_path(1);
    for start_field_id in [None, Some(*fixture.ids_by_step.last().unwrap())] {
        let (mut session, guard) = start_with_field(
            &fixture,
            1,
            Pc4RuleProfile::Srs,
            Pc4QueueDisclosure::FixedExplicit(vec![Pc4GraphPiece::O]),
            FixedQueueHoldState::Disabled,
            5,
            start_field_id,
        );
        let (terminal, lookups) = drive(&mut session, &guard, &fixture);
        assert_eq!(
            lookups.len(),
            1,
            "empty unions also verify the initial node"
        );
        if start_field_id.is_none() {
            assert!(matches!(
                terminal,
                AppOnlinePc4CandidateStep::Complete {
                    canonical_candidates: 0,
                    ..
                }
            ));
            assert_eq!(
                session
                    .completed_observation_family()
                    .unwrap()
                    .total_reveal_probability(),
                Pc4ExactProbability::one()
            );
        } else {
            assert!(matches!(
                terminal,
                AppOnlinePc4CandidateStep::Failed(
                    AppOnlinePc4FixedQueueCandidateFailure::LookupAdmission {
                        reason: "pc4_online_candidate_initial_field_mismatch",
                        ..
                    }
                )
            ));
            assert!(session.completed_reducer_input().is_none());
        }
    }
}

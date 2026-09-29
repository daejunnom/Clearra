use super::*;
use super::{
    fields::{OrientationAdvance, Orientations},
    solver::Accounting,
    source::Sources,
};
use crate::recovery_build::{
    staged::diagram::{Diagram, NONE},
    RecoveryBuildFields, RecoveryBuildFixedQuery,
};
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
fn mask(v: u64) -> Mask {
    Mask::from_words([v, 0, 0, 0])
}
fn query(targets: &[u64], supplies: &[&str]) -> RecoveryChainQuery {
    RecoveryChainQuery {
        height: 8,
        initial: Mask::EMPTY,
        targets: targets.iter().copied().map(mask).collect(),
        supplies: supplies.iter().map(|s| s.to_string()).collect(),
        early_limit: CrossStageEarlyLimit::Auto,
        allow_piece_exchange: false,
        hold_enabled: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}
fn assert_counts(report: &RecoveryChainCoverage, expected: [u128; 3]) {
    assert_eq!(
        [
            report.normal_count,
            report.recovery_count,
            report.no_path_count
        ],
        expected
    );
    assert_eq!(report.possible, expected.iter().sum());
    let total =
        report.normal_probability + report.recovery_probability + report.no_path_probability;
    assert!((total - 1.0).abs() < 1e-12);
}

#[test]
fn three_targets_are_executed_as_one_continuous_supply_and_board() {
    let q = query(&[0xf, 0xc030, 0x300c0], &["I", "O", "O"]);
    let r = q.coverage(&ExecutionControl::default()).unwrap();
    assert_counts(&r, [1, 0, 0]);
    let w = r.normal_example.unwrap();
    assert_eq!(w.steps.len(), 3);
    assert_eq!(w.early_by_boundary, vec![0, 0]);
    assert_eq!(w.exchange_by_stage, vec![[0; 7]; 3]);
    assert_eq!(w.steps.last().unwrap().completed_stages, 3);
}
#[test]
fn one_held_token_survives_two_boundaries_until_terminal_release() {
    let mut q = query(&[0xf, 0xc030, 0x300c0], &["O", "I", "O"]);
    q.allow_piece_exchange = true;
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = q.coverage(&ExecutionControl::default()).unwrap();
    assert_counts(&r, [1, 0, 0]);
    let w = r.normal_example.unwrap();
    assert_eq!(w.steps[0].source_index, 1);
    assert_eq!(w.steps.last().unwrap().source_index, 0);
    assert_eq!(
        w.steps.last().unwrap().hold_decision,
        "release-held-at-terminal"
    );
    assert_eq!(w.steps.last().unwrap().source_stage, 0);
    assert_eq!(w.early_by_boundary, vec![0, 0]);
    q.allow_piece_exchange = false;
    assert_counts(
        &q.coverage(&ExecutionControl::default()).unwrap(),
        [0, 0, 1],
    );
    q.allow_piece_exchange = true;
    q.hold_enabled = false;
    assert_counts(
        &q.coverage(&ExecutionControl::default()).unwrap(),
        [0, 0, 1],
    );
}
#[test]
fn later_target_lock_counts_against_every_crossed_open_boundary() {
    let mut q = query(&[0xf << 20, 0xf << 10, 0xf], &["I", "I", "I"]);
    q.hold_enabled = false;
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    assert_counts(
        &q.coverage(&ExecutionControl::default()).unwrap(),
        [0, 0, 1],
    );
    q.early_limit = CrossStageEarlyLimit::AtMost(2);
    let r = q.coverage(&ExecutionControl::default()).unwrap();
    assert_counts(&r, [0, 1, 0]);
    assert_eq!(r.recovery_example.unwrap().early_by_boundary, vec![2, 1]);
}
#[test]
fn physical_line_clear_does_not_delete_logical_target_ownership() {
    let mut q = query(&[0xf, 0xc03 << 10, 0x300c << 10], &["I", "O", "O"]);
    q.initial = mask(0x3f0);
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = q.coverage(&ExecutionControl::default()).unwrap();
    assert_counts(&r, [1, 0, 0]);
    let w = r.normal_example.unwrap();
    assert_eq!(w.steps[0].cleared_lines, 1);
    assert_eq!(w.steps[0].board_after, [0; 4]);
    assert_eq!(
        Mask::from_words(w.steps[1].logical_placement).count_ones(),
        4
    );
    assert_eq!(w.terminal_board, mask(0x3c0f).words());
}
#[test]
fn starting_mirror_reflects_all_remaining_targets_without_relabeling_supplies() {
    let mut q = query(&[0x1007, 0x300c00, 0xf0], &["J", "O", "I"]);
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = q.coverage(&ExecutionControl::default()).unwrap();
    assert_counts(&r, [1, 0, 0]);
    let w = r.normal_example.unwrap();
    for (target, original) in w.targets.iter().zip(&q.targets) {
        assert_eq!(
            *target,
            original.mirrored_horizontally(10, 8).unwrap().words()
        );
    }
    assert_eq!(w.queues[0], vec![PieceKind::J]);
    q.initial = mask(8);
    assert_counts(
        &q.coverage(&ExecutionControl::default()).unwrap(),
        [0, 0, 1],
    );
}
#[test]
fn orientation_enumeration_is_cancellable_and_duplicate_free() {
    let q = query(&[0xf, 0xc030, 0x300c0], &["I", "O", "O"]);
    let mut cursor = Orientations::new(&q);
    let mut values = Vec::new();
    loop {
        match cursor.advance(&q, &ExecutionControl::default()).unwrap() {
            OrientationAdvance::Pending => {}
            OrientationAdvance::Found(v) => {
                assert!(!values.contains(&v));
                values.push(v);
            }
            OrientationAdvance::Done => break,
        }
    }
    assert!(!values.is_empty());
    assert_eq!(values[0], q.targets);
}
#[test]
fn source_automaton_counts_four_P7_spaces_without_expanding_the_product() {
    let q = query(
        &[0xf, 0xf << 10, 0xf << 20, 0xf << 30],
        &["P7", "P7", "P7", "P7"],
    );
    let mut d = Diagram::default();
    let s = Sources::compile(&q, &mut d, &ExecutionControl::default()).unwrap();
    assert_eq!(s.possible, 5040_u128.pow(4));
    assert_eq!(s.boundaries, vec![0, 7, 14, 21, 28]);
    assert!(
        d.node_count() < 2000,
        "source DAG must not allocate one node per Cartesian input"
    );
    assert_eq!(
        s.measure(&mut d, s.universe, &ExecutionControl::default())
            .unwrap(),
        (5040_u128.pow(4), 1.0)
    );
    assert_eq!(
        s.measure(&mut d, NONE, &ExecutionControl::default())
            .unwrap(),
        (0, 0.0)
    );
    let first = s.first(&d, s.universe).unwrap();
    assert_eq!(first.len(), 4);
    assert!(first.iter().all(|q| q.len() == 7));
    for i in 0..28 {
        assert_eq!(s.stage_of(i), Some(usize::from(i / 7)));
    }
}
#[test]
fn symbolic_three_source_union_matches_all_small_fixed_input_tuples() {
    let q = query(&[0xf, 0xc030, 0x300c0], &["[IO]", "[IO]", "[IO]"]);
    let r = q.coverage(&ExecutionControl::default()).unwrap();
    let mut counts = [0_u128; 3];
    for a in ["I", "O"] {
        for b in ["I", "O"] {
            for c in ["I", "O"] {
                let mut fixed = q.clone();
                fixed.supplies = vec![a.into(), b.into(), c.into()];
                let f = fixed.coverage(&ExecutionControl::default()).unwrap();
                counts[0] += f.normal_count;
                counts[1] += f.recovery_count;
                counts[2] += f.no_path_count;
            }
        }
    }
    assert_counts(&r, counts);
    assert_eq!(r.possible, 8);
    assert_eq!(r.normal_probability, counts[0] as f64 / 8.0);
}
#[test]
fn two_target_compatibility_uses_independent_existing_fixed_queue_search() {
    for exchange in [false, true] {
        for hold in [false, true] {
            for early in [0, 1, 2] {
                let mut q = query(&[0xf, 0xc030], &["[IO]", "[IO]"]);
                q.allow_piece_exchange = exchange;
                q.hold_enabled = hold;
                q.early_limit = CrossStageEarlyLimit::AtMost(early);
                let actual = q.coverage(&ExecutionControl::default()).unwrap();
                let mut counts = [0_u128; 3];
                // A full-left target and its whole-suffix reflection are the two
                // independently specified directions; the intermediate board is not
                // symmetric in this fixture. No new orientation code is used here.
                for a in [PieceKind::I, PieceKind::O] {
                    for b in [PieceKind::I, PieceKind::O] {
                        let mut status = RecoveryBuildStatus::NoPath;
                        for mirror in [false, true] {
                            let reflect = |v: Mask| {
                                if mirror {
                                    v.mirrored_horizontally(10, 8).unwrap()
                                } else {
                                    v
                                }
                            };
                            let old = RecoveryBuildFixedQuery {
                                fields: RecoveryBuildFields {
                                    height: 8,
                                    initial: Mask::EMPTY,
                                    middle: reflect(mask(0xf)),
                                    result: reflect(mask(0xc030)),
                                },
                                first_supply: vec![a],
                                second_supply: vec![b],
                                early_limit: q.early_limit,
                                allow_piece_exchange: exchange,
                                hold_enabled: hold,
                                preserve_b2b: false,
                                initial_b2b: true,
                                rule_profile: q.rule_profile,
                                spin_profile: q.spin_profile,
                            };
                            let found = old.search(&ExecutionControl::default()).unwrap().status;
                            if found == RecoveryBuildStatus::Normal {
                                status = found;
                                break;
                            }
                            if found == RecoveryBuildStatus::Recovery {
                                status = found;
                            }
                        }
                        counts[match status {
                            RecoveryBuildStatus::Normal => 0,
                            RecoveryBuildStatus::Recovery => 1,
                            RecoveryBuildStatus::NoPath => 2,
                        }] += 1;
                    }
                }
                assert_counts(&actual, counts);
            }
        }
    }
}
#[test]
fn accounting_rejects_type_substitution_and_no_borrowed_source_bypasses_early() {
    let d = [1, 1, 1];
    let root = Accounting::new(3);
    let next = root.lock(2, 2, 1, &d, &[1, 1]).unwrap();
    assert_eq!(next.early, vec![1, 1]);
    assert!(next.lock(1, 1, 0, &d, &[1, 1]).is_none());
    let a = root
        .lock(1, 0, 0, &d, &[0, 0])
        .unwrap()
        .lock(2, 1, 3, &d, &[0, 0])
        .unwrap()
        .lock(0, 2, 3, &d, &[0, 0])
        .unwrap();
    assert!(a.terminal(&d, true));
    assert!(!a.terminal(&d, false));
    assert_eq!(a.source_used, vec![1, 1, 1]);
    assert_eq!(a.early, vec![0, 0]);
}
#[test]
fn a_partial_session_cannot_return_a_finished_probability() {
    let q = query(&[0xf, 0xc030, 0x300c0], &["I", "O", "O"]);
    let s = RecoveryChainSession::new(q, &ExecutionControl::default()).unwrap();
    assert_eq!(
        s.finish(&ExecutionControl::default()),
        Err(RecoveryChainError::Incomplete)
    );
}
#[test]
fn shared_frame_validation_rejects_hidden_overlap_and_preserves_empty_rows() {
    let mut q = query(&[0xf, 0xf << 20, 0xf << 40], &["I", "I", "I"]);
    q.validate().unwrap();
    q.targets[2] = q.targets[0];
    assert_eq!(q.validate(), Err(RecoveryChainError::OverlappingStage(2)));
    q.targets[2] = mask(1);
    assert_eq!(q.validate(), Err(RecoveryChainError::InvalidStageArea(2)));
    let b = mask(0xf << 40);
    let actual = fields::compact(b, (1 << 0) | (1 << 2), 8);
    assert_eq!(actual, mask(0xf << 20));
    assert_eq!(
        fields::lift(
            crate::board::ForwardBoard::from_mask(actual),
            (1 << 0) | (1 << 2),
            8
        ),
        Some(b)
    );
}
#[test]
fn oversized_source_window_is_an_error_not_zero_coverage_or_truncation() {
    let mut q = query(&[0xf, 0xc030], &["I", "O"]);
    q.supplies[1] = "O".repeat(source::MAX_SOURCE_WINDOW);
    assert_eq!(
        q.validate(),
        Err(RecoveryChainError::SupplyWindowTooLong {
            maximum: source::MAX_SOURCE_WINDOW
        })
    );
}
#[test]
fn cancellation_never_becomes_an_empty_exact_coverage() {
    use clearra_core_domain::execution_cancellation::CancellationToken;
    let q = query(&[0xf, 0xc030, 0x300c0], &["I", "O", "O"]);
    let token = CancellationToken::new();
    let handle = token.handle();
    let control = ExecutionControl::new(token);
    let mut session = RecoveryChainSession::new(q.clone(), &control).unwrap();
    handle.cancel();
    assert_eq!(
        session.advance(1, &control),
        Err(RecoveryChainError::Core(RecoveryBuildError::Cancelled))
    );
    assert_eq!(
        session.finish(&control),
        Err(RecoveryChainError::Core(RecoveryBuildError::Cancelled))
    );
    assert_eq!(
        q.coverage(&control),
        Err(RecoveryChainError::Core(RecoveryBuildError::Cancelled))
    );
}
#[test]
fn cooperative_slice_size_does_not_change_exact_output() {
    let q = query(&[0xf, 0xc030, 0x300c0], &["I", "O", "O"]);
    let expected = q.coverage(&ExecutionControl::default()).unwrap();
    for fuel in [1, 7, 256] {
        let mut session =
            RecoveryChainSession::new(q.clone(), &ExecutionControl::default()).unwrap();
        while !session.advance(fuel, &ExecutionControl::default()).unwrap() {}
        assert_eq!(
            session.finish(&ExecutionControl::default()).unwrap(),
            expected
        );
    }
}

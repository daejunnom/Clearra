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
fn source_automaton_counts_four_p7_spaces_without_expanding_the_product() {
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

#[test]
fn complete_chain_catalog_keeps_both_mirrors_without_summing_probabilities() {
    let q = query(&[0xc03, 0x300c, 0xc030], &["[IO]", "[IO]", "[IO]"]);
    let c = q.catalog(&ExecutionControl::default()).unwrap();
    assert!(c.complete);
    assert_eq!(c.input, q);
    assert_eq!(c.solutions.len(), 2);
    assert_counts(&c.coverage, [1, 0, 7]);
    assert_eq!(c.coverage_classes, vec![vec![0, 1]]);
    for row in c.solutions {
        assert_eq!(row.covered_count, 1);
        assert!((row.probability - 0.125).abs() < 1e-12);
        assert_eq!(row.example.steps.len(), 3);
    }
}
#[test]
fn complete_chain_catalog_preserves_alternative_tilings_not_just_first_witness() {
    let q = query(&[0x3c0f, 0x3c0, 0xc03 << 20], &["[IO][IO]", "I", "O"]);
    let control = ExecutionControl::default();
    let c = q.catalog(&control).unwrap();
    let reference = q.coverage(&control).unwrap();
    assert_eq!(
        [
            c.coverage.normal_count,
            c.coverage.recovery_count,
            c.coverage.no_path_count
        ],
        [
            reference.normal_count,
            reference.recovery_count,
            reference.no_path_count
        ]
    );
    assert_eq!(
        c.solutions.len(),
        4,
        "II and OO tilings, each in two directions"
    );
    assert_eq!(c.coverage_classes.len(), 2);
    for support in &c.coverage_classes {
        assert_eq!(support.len(), 2);
    }
    let signatures = c
        .solutions
        .iter()
        .map(|r| {
            let mut counts = [0; 7];
            for s in r.example.steps.iter().filter(|s| s.target_stage == 0) {
                counts[crate::recovery_build::search::piece_index(s.piece)] += 1;
            }
            counts
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(signatures.len(), 2);
}
#[test]
fn chain_catalog_union_matches_continuous_reference_with_hold_and_early_limits() {
    let control = ExecutionControl::default();
    for hold in [false, true] {
        for exchange in [false, true] {
            for early in [0, 1, 2] {
                let mut q = query(&[0xf, 0xc030, 0x300c0], &["[IO]", "[IO]", "[IO]"]);
                q.hold_enabled = hold;
                q.allow_piece_exchange = exchange;
                q.early_limit = CrossStageEarlyLimit::AtMost(early);
                let expected = q.coverage(&control).unwrap();
                let actual = q.catalog(&control).unwrap();
                assert_counts(
                    &actual.coverage,
                    [
                        expected.normal_count,
                        expected.recovery_count,
                        expected.no_path_count,
                    ],
                );
                assert!(
                    (actual.coverage.normal_probability - expected.normal_probability).abs()
                        < 1e-12
                );
                assert!(
                    (actual.coverage.recovery_probability - expected.recovery_probability).abs()
                        < 1e-12
                );
                for row in actual.solutions {
                    assert!(row.covered_count > 0);
                    assert!(row.example.early_by_boundary.iter().all(|n| *n <= early));
                }
            }
        }
    }
}
#[test]
fn chain_catalog_recovers_non_adjacent_early_placements_and_held_source() {
    let control = ExecutionControl::default();
    let mut q = query(&[0xf << 20, 0xf << 10, 0xf], &["I", "I", "I"]);
    q.early_limit = CrossStageEarlyLimit::AtMost(2);
    let report = q.catalog(&control).unwrap();
    assert_counts(&report.coverage, [0, 1, 0]);
    assert_eq!(
        report.coverage.recovery_example.unwrap().early_by_boundary,
        vec![2, 1]
    );
    let mut q = query(&[0xf, 0xc030, 0x300c0], &["O", "I", "O"]);
    q.allow_piece_exchange = true;
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let report = q.catalog(&control).unwrap();
    assert_counts(&report.coverage, [1, 0, 0]);
    for row in report.solutions {
        let last = row.example.steps.last().unwrap();
        assert_eq!(last.source_index, 0);
        assert_eq!(last.hold_decision, "release-held-at-terminal");
    }
}
#[test]
fn chain_plan_inventory_is_complemented_across_all_remaining_stages() {
    let control = ExecutionControl::default();
    let mut q = query(&[0x3c0f, 0x3c0f << 6, 0xc030], &["II", "OO", "O"]);
    q.allow_piece_exchange = true;
    let mut diagram = Diagram::default();
    let sources = Sources::compile(&q, &mut diagram, &control).unwrap();
    let mut producer = super::plan::Producer::new(&q, &sources).unwrap();
    let mut count = 0;
    while !producer.done {
        if let Some(plan) = producer.advance(7, &control).unwrap() {
            let mut inventory = [0; 7];
            for tile in plan.stages.iter().flatten() {
                inventory[usize::from(tile.piece)] += 1;
            }
            assert_eq!(inventory, [2, 0, 0, 3, 0, 0, 0]);
            count += 1;
        }
    }
    assert_eq!(count, 4);
}
#[test]
fn chain_catalog_is_cooperative_and_refuses_incomplete_or_cancelled_proofs() {
    let q = query(&[0xc03, 0x300c, 0xc030], &["O", "O", "O"]);
    let control = ExecutionControl::default();
    let expected = q.catalog(&control).unwrap();
    for fuel in [1, 7, 256] {
        let mut session = RecoveryChainCatalogSession::new(q.clone(), &control).unwrap();
        while !session.advance(fuel, &control).unwrap() {}
        assert_eq!(session.finish(&control).unwrap(), expected);
    }
    assert!(matches!(
        RecoveryChainCatalogSession::new(q.clone(), &control)
            .unwrap()
            .finish(&control),
        Err(RecoveryChainError::Incomplete)
    ));
    let token = clearra_core_domain::execution_cancellation::CancellationToken::new();
    let handle = token.handle();
    let control = ExecutionControl::new(token);
    let mut session = RecoveryChainCatalogSession::new(q, &control).unwrap();
    handle.cancel();
    assert_eq!(
        session.advance(1, &control),
        Err(RecoveryBuildError::Cancelled.into())
    );
    assert_eq!(
        session.finish(&control),
        Err(RecoveryBuildError::Cancelled.into())
    );
}

fn parallel_catalog(q: RecoveryChainQuery, workers: usize) -> RecoveryChainCatalog {
    let control = ExecutionControl::default();
    let mut c = RecoveryChainCoordinator::new(q, workers, &control).unwrap();
    let init = c.worker_initialization();
    let mut pool = (0..workers.saturating_sub(1).max(1))
        .map(|_| RecoveryChainWorker::new(&init).unwrap())
        .collect::<Vec<_>>();
    let mut pending = Vec::new();
    let mut index = 0;
    for _ in 0..100_000 {
        match c.produce(17, &control).unwrap() {
            RecoveryChainProduce::Batch(bytes) => {
                let w = &mut pool[index % workers.saturating_sub(1).max(1)];
                index += 1;
                let mut result = w.consume(&bytes, &control).unwrap();
                while w.has_pending_work() {
                    assert!(result.is_none());
                    result = w.advance(7, &control).unwrap();
                }
                pending.push(result.unwrap());
                // Retain the oldest task while later tasks complete first.
                if pending.len() > 2 {
                    c.absorb(&pending.pop().unwrap(), &control).unwrap();
                }
            }
            RecoveryChainProduce::Pending => {
                if let Some(result) = pending.pop() {
                    c.absorb(&result, &control).unwrap();
                }
            }
            RecoveryChainProduce::Completed => {
                assert!(pending.is_empty());
                let p = c.progress();
                assert_eq!(p.issued_plans, p.completed_plans);
                return c.finish(&control).unwrap();
            }
        }
    }
    panic!("bounded small chain did not finish")
}
#[test]
fn three_intermediate_fields_and_four_supplies_preserve_exact_catalog_across_workers() {
    let mut q = query(
        &[0xc03, 0x300c, 0xc030, 0x300c0],
        &["[IO]", "[IO]", "[IO]", "[IO]"],
    );
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    let control = ExecutionControl::default();
    let expected = q.catalog(&control).unwrap();
    assert_eq!(expected.coverage.possible, 16);
    assert_counts(&expected.coverage, [1, 0, 15]);
    assert_eq!(expected.solutions.len(), 2);
    for workers in [1, 2, 4, 11] {
        let actual = parallel_catalog(q.clone(), workers);
        assert_eq!(actual, expected, "worker budget {workers}");
        assert!(actual
            .solutions
            .iter()
            .all(|s| s.example.steps.len() == 4 && s.example.early_by_boundary.len() == 3));
    }
}
#[test]
fn four_stage_line_clears_and_three_boundary_hold_are_preserved_over_value_packets() {
    let mut clears = query(
        &[0xf, 0xf << 10, 0xf << 20, 0xf << 30],
        &["I", "I", "I", "I"],
    );
    clears.initial = mask(0x3f0 | (0x3f0 << 10) | (0x3f0 << 20) | (0x3f0 << 30));
    clears.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = parallel_catalog(clears.clone(), 4);
    assert_counts(&r.coverage, [1, 0, 0]);
    let w = r.coverage.normal_example.as_ref().unwrap();
    assert_eq!(w.terminal_board, [0; 4]);
    assert_eq!(
        w.steps.iter().map(|s| s.cleared_lines).collect::<Vec<_>>(),
        vec![1; 4]
    );
    assert_eq!(r, clears.catalog(&ExecutionControl::default()).unwrap());
    let mut held = query(&[0xf, 0xc030, 0x300c0, 0xc0300], &["O", "I", "O", "O"]);
    held.allow_piece_exchange = true;
    held.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = parallel_catalog(held.clone(), 11);
    assert_counts(&r.coverage, [1, 0, 0]);
    let last = r
        .coverage
        .normal_example
        .as_ref()
        .unwrap()
        .steps
        .last()
        .unwrap();
    assert_eq!(last.source_index, 0);
    assert_eq!(last.source_stage, 0);
    assert_eq!(last.hold_decision, "release-held-at-terminal");
    assert_eq!(r, held.catalog(&ExecutionControl::default()).unwrap());
}
#[test]
fn chain_worker_repair_and_alternative_tilings_match_serial_catalog_exactly() {
    let mut repair = query(&[0xf << 20, 0xf << 10, 0xf], &["I", "I", "I"]);
    repair.hold_enabled = false;
    repair.early_limit = CrossStageEarlyLimit::AtMost(2);
    let mut alternative = query(&[0x3c0f, 0x3c0, 0xc03 << 20], &["[IO][IO]", "I", "O"]);
    alternative.early_limit = CrossStageEarlyLimit::AtMost(0);
    for q in [repair, alternative] {
        let expected = q.catalog(&ExecutionControl::default()).unwrap();
        for workers in [2, 4, 11] {
            assert_eq!(parallel_catalog(q.clone(), workers), expected);
        }
    }
}
#[test]
fn chain_packets_reject_stale_query_duplicate_partial_and_incomplete_results() {
    let q = query(&[0xc03, 0x300c, 0xc030], &["O", "O", "O"]);
    let control = ExecutionControl::default();
    let mut c = RecoveryChainCoordinator::new(q.clone(), 2, &control).unwrap();
    let init = c.worker_initialization();
    let mut worker = RecoveryChainWorker::new(&init).unwrap();
    assert!(matches!(
        worker.advance(1, &control),
        Err(RecoveryChainError::InvalidState(_))
    ));
    let bytes = loop {
        if let RecoveryChainProduce::Batch(b) = c.produce(17, &control).unwrap() {
            break b;
        }
    };
    for n in [0, 5, bytes.len() / 2, bytes.len() - 1] {
        assert!(worker.consume(&bytes[..n], &control).is_err());
    }
    let mut different = q.clone();
    different.hold_enabled = false;
    let stale = RecoveryChainCoordinator::new(different, 2, &control)
        .unwrap()
        .worker_initialization();
    assert!(RecoveryChainWorker::new(&stale)
        .unwrap()
        .consume(&bytes, &control)
        .is_err());
    let mut answer = worker.consume(&bytes, &control).unwrap();
    while worker.has_pending_work() {
        answer = worker.advance(1, &control).unwrap();
    }
    let answer = answer.unwrap();
    let before = c.progress();
    for n in [0, 5, answer.len() / 2, answer.len() - 1] {
        assert!(c.absorb(&answer[..n], &control).is_err());
        assert_eq!(c.progress(), before);
    }
    c.absorb(&answer, &control).unwrap();
    let after = c.progress();
    assert!(c.absorb(&answer, &control).is_err());
    assert_eq!(c.progress(), after);
    assert!(matches!(
        c.finish(&control),
        Err(RecoveryChainError::Incomplete)
    ));
    assert!(RecoveryChainCoordinator::new(q.clone(), 0, &control).is_err());
    let mut c = RecoveryChainCoordinator::new(q, 2, &control).unwrap();
    control.cancellation.handle().cancel();
    assert!(matches!(
        c.produce(1, &control),
        Err(RecoveryChainError::Core(RecoveryBuildError::Cancelled))
    ));
}

use super::*;
use crate::recovery_build::{
    RecoveryBuildFields, RecoveryBuildParallelCoordinator, RecoveryBuildParallelProduce,
    RecoveryBuildParallelWorker, RecoveryBuildStatus,
};
use crate::CrossStageEarlyLimit;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
fn m(n: u64) -> Mask {
    Mask::from_words([n, 0, 0, 0])
}
fn query() -> RecoveryBuildQuery {
    RecoveryBuildQuery {
        stages: vec![
            RecoveryBuildStage {
                target: m(0xc03),
                supply: "[IO]".into(),
            },
            RecoveryBuildStage {
                target: m(0x300c),
                supply: "[IO]".into(),
            },
            RecoveryBuildStage {
                target: m(0xf0),
                supply: "[IO]".into(),
            },
        ],
        fields: RecoveryBuildFields {
            height: 8,
            initial: Mask::EMPTY,
            middle: m(0xc03),
            result: m(0xf0),
        },
        first_supply: "[IO]".into(),
        second_supply: "[IO]".into(),
        early_limit: CrossStageEarlyLimit::AtMost(0),
        allow_piece_exchange: true,
        hold_enabled: false,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
        all_solutions: true,
        minimum_solutions: false,
        required_solution_keys: Vec::new(),
        minimum_source_identity: None,
    }
}
#[test]
fn recovery_chain_three_independent_supplies_have_exact_not_multiplied_coverage() {
    let mut q = query();
    let control = ExecutionControl::default();
    let p = q.search(&control).unwrap();
    assert_eq!(
        (
            p.possible,
            p.normal_count,
            p.recovery_count,
            p.no_path_count
        ),
        (8, 1, 0, 7)
    );
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    let p = q.search(&control).unwrap();
    assert_eq!(
        (
            p.possible,
            p.normal_count,
            p.recovery_count,
            p.no_path_count
        ),
        (8, 1, 2, 5)
    );
    q.hold_enabled = true;
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let p = q.search(&control).unwrap();
    assert_eq!(
        (p.normal_count, p.recovery_count, p.no_path_count),
        (3, 0, 5)
    );
    q.allow_piece_exchange = false;
    let p = q.search(&control).unwrap();
    assert_eq!(
        (p.normal_count, p.recovery_count, p.no_path_count),
        (1, 0, 7)
    );
}
#[test]
fn recovery_chain_one_early_placement_charges_every_crossed_boundary() {
    let mut q = query();
    for (s, word) in q.stages.iter_mut().zip(["I", "O", "O"]) {
        s.supply = word.into();
    }
    q.first_supply = "I".into();
    q.second_supply = "O".into();
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    let p = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!((p.normal_count, p.recovery_count), (0, 1));
    for s in &p.solutions {
        let c = s.example.path.chain.as_ref().unwrap();
        assert_eq!(c.early_by_boundary, vec![1, 1]);
        assert_eq!(c.placement_stages, vec![2, 0, 1]);
    }
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    assert_eq!(
        q.search(&ExecutionControl::default())
            .unwrap()
            .no_path_count,
        1
    );
}
#[test]
fn recovery_chain_hold_is_not_reset_between_three_boundaries() {
    let mut q = query();
    q.stages.insert(
        2,
        RecoveryBuildStage {
            target: m(0xc030),
            supply: "O".into(),
        },
    );
    q.stages[0].supply = "I".into();
    q.stages[1].supply = "O".into();
    q.stages[3].supply = "O".into();
    // Move the final horizontal I out of the third O's cells.
    q.stages[3].target = m(0x3c0);
    q.fields.result = m(0x3c0);
    q.first_supply = "I".into();
    q.second_supply = "O".into();
    q.hold_enabled = true;
    let p = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(p.normal_count, 1);
    let path = &p.normal_example.unwrap().path;
    assert_eq!(
        path.steps
            .iter()
            .map(|s| s.source_index)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 0]
    );
    assert_eq!(
        path.steps.last().unwrap().hold_decision,
        "release-held-at-terminal"
    );
}
#[test]
fn recovery_chain_clears_keep_shared_cell_ownership() {
    let mut q = query();
    q.fields.initial = m(0x3f0);
    q.fields.middle = m(0xf);
    q.stages[0].target = m(0xf);
    q.stages[1].target = m(0xc03 << 10);
    q.stages[2].target = m(0x300c << 10);
    q.fields.result = q.stages[2].target;
    for (s, w) in q.stages.iter_mut().zip(["I", "O", "O"]) {
        s.supply = w.into();
    }
    q.first_supply = "I".into();
    q.second_supply = "O".into();
    q.preserve_b2b = true;
    let p = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(p.normal_count, 1);
    let e = p.normal_example.unwrap();
    assert_eq!(e.path.steps[0].cleared_lines, 1);
    assert_eq!(
        e.path.chain.as_ref().unwrap().placement_stages,
        vec![0, 1, 2]
    );
    assert_eq!(e.path.terminal_board, m(0x3c0f).words());
}
#[test]
fn recovery_chain_pool_reordering_preserves_counts_and_catalog() {
    let mut q = query();
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    q.hold_enabled = true;
    let control = ExecutionControl::default();
    let expected = q.search(&control).unwrap();
    for slots in [1, 2, 4, 11] {
        let mut c = RecoveryBuildParallelCoordinator::new(q.clone(), slots).unwrap();
        let init = c.worker_initialization();
        let mut w = RecoveryBuildParallelWorker::new(&init).unwrap();
        let mut replies = Vec::new();
        let mut done = false;
        for _ in 0..100000 {
            let (status, bytes) = c.produce(1, &control).unwrap();
            match status {
                RecoveryBuildParallelProduce::Batch => {
                    assert!(w.consume(&bytes, &control).unwrap().is_none());
                    loop {
                        if let Some(reply) = w.advance(&control).unwrap() {
                            replies.push(reply);
                            break;
                        }
                    }
                }
                RecoveryBuildParallelProduce::Pending => {
                    for reply in replies.drain(..).rev() {
                        c.absorb(&reply, &control).unwrap();
                    }
                }
                RecoveryBuildParallelProduce::Completed => {
                    done = true;
                    break;
                }
                _ => panic!("unexpected cancellation"),
            }
        }
        assert!(done);
        let actual = c.finish(&control).unwrap();
        assert_eq!(actual, expected);
    }
}
#[test]
fn recovery_chain_bad_tail_cannot_be_silently_ignored() {
    let mut q = query();
    q.stages.last_mut().unwrap().supply = "J".into();
    assert!(q.validate().is_err());
    q.second_supply = "J".into();
    let p = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(p.no_path_count, p.possible);
    q.stages[1].target = q.stages[0].target;
    assert!(q.validate().is_err());
}
#[test]
fn recovery_chain_symbolic_source_does_not_expand_four_p7_bags() {
    let mut q = query();
    q.stages.push(RecoveryBuildStage {
        target: m(0x3c00000),
        supply: "P7".into(),
    });
    for s in &mut q.stages {
        s.supply = "P7".into();
    }
    q.fields.result = q.stages.last().unwrap().target;
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    let mut d = super::super::staged::diagram::Diagram::default();
    let c = ExecutionControl::default();
    let source = source::Source::new(&q, &mut d, &c).unwrap();
    assert_eq!(source.possible, 5040_u128.pow(4));
    let measured = source.measure(&mut d, source.universe, &c).unwrap();
    assert_eq!(measured.0, source.possible);
    assert!((measured.1 - 1.0).abs() < 1e-12);
}
#[test]
fn recovery_chain_cancellation_never_becomes_no_path() {
    use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert_eq!(
        query().search(&ExecutionControl::new(token)),
        Err(Error::Cancelled)
    );
}

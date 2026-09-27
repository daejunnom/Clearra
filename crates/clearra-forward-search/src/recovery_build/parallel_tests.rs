use super::*;
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

fn query() -> RecoveryBuildQuery {
    RecoveryBuildQuery {
        fields: RecoveryBuildFields {
            height: 8,
            initial: Board256Mask::EMPTY,
            middle: Board256Mask::from_words([15, 0, 0, 0]),
            result: Board256Mask::from_words([0xc030, 0, 0, 0]),
        },
        first_supply: "[IO]".into(),
        second_supply: "[IO]".into(),
        early_limit: CrossStageEarlyLimit::Auto,
        allow_piece_exchange: true,
        hold_enabled: false,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}
fn complete(
    worker: &mut RecoveryBuildParallelWorker,
    task: &[u8],
    control: &ExecutionControl,
) -> Vec<u8> {
    let mut output = worker.consume(task, control).unwrap();
    while worker.has_pending_work() {
        output = worker.advance(control).unwrap();
    }
    output.unwrap()
}
#[test]
fn recovery_build_parallel_reordered_pairs_preserve_exact_report() {
    for hold in [false, true] {
        for exchange in [false, true] {
            for preserve in [false, true] {
                let mut q = query();
                q.hold_enabled = hold;
                q.allow_piece_exchange = exchange;
                q.preserve_b2b = preserve;
                let control = ExecutionControl::default();
                let serial = q.search(&control).unwrap();
                assert_eq!(serial.possible, 4);
                for workers in [2, 3, 11] {
                    let mut coordinator =
                        RecoveryBuildParallelCoordinator::new(q.clone(), workers).unwrap();
                    let mut worker =
                        RecoveryBuildParallelWorker::new(&coordinator.worker_initialization())
                            .unwrap();
                    let mut results = Vec::new();
                    loop {
                        let (status, task) = coordinator.produce(32, &control).unwrap();
                        if status != RecoveryBuildParallelProduce::Batch {
                            break;
                        }
                        results.push(complete(&mut worker, &task, &control));
                    }
                    assert!(results.len() > 1);
                    for result in results.into_iter().rev() {
                        coordinator.absorb(&result, &control).unwrap();
                    }
                    assert_eq!(
                        coordinator.produce(32, &control).unwrap().0,
                        RecoveryBuildParallelProduce::Completed
                    );
                    assert_eq!(coordinator.finish(&control).unwrap(), serial);
                }
            }
        }
    }
}
#[test]
fn recovery_build_parallel_rejects_duplicate_foreign_truncated_and_missing_results() {
    let control = ExecutionControl::default();
    let mut c = RecoveryBuildParallelCoordinator::new(query(), 2).unwrap();
    let mut w = RecoveryBuildParallelWorker::new(&c.worker_initialization()).unwrap();
    let (_, task) = c.produce(1, &control).unwrap();
    let result = complete(&mut w, &task, &control);
    for len in [0, 4, result.len() - 1] {
        assert!(c.absorb(&result[..len], &control).is_err());
    }
    let mut trailing = result.clone();
    trailing.push(0);
    assert!(c.absorb(&trailing, &control).is_err());
    let mut q = query();
    q.hold_enabled = true;
    let mut wrong = RecoveryBuildParallelWorker::new(
        &RecoveryBuildParallelCoordinator::new(q, 2)
            .unwrap()
            .worker_initialization(),
    )
    .unwrap();
    assert!(wrong.consume(&task, &control).is_err());
    c.absorb(&result, &control).unwrap();
    assert!(c.absorb(&result, &control).is_err());
    assert!(c.finish(&control).is_err());
}
#[test]
fn recovery_build_parallel_cancellation_does_not_certify_no_path() {
    let control = ExecutionControl::default();
    let mut c = RecoveryBuildParallelCoordinator::new(query(), 2).unwrap();
    let mut w = RecoveryBuildParallelWorker::new(&c.worker_initialization()).unwrap();
    let (_, task) = c.produce(32, &control).unwrap();
    control.cancellation.handle().cancel();
    assert_eq!(
        c.produce(32, &control).unwrap().0,
        RecoveryBuildParallelProduce::Cancelled
    );
    assert!(w.consume(&task, &control).is_err());
    assert!(c.finish(&control).is_err());
}
#[test]
fn recovery_build_parallel_p7_product_is_lazy_and_reorder_window_is_bounded() {
    let mut q = query();
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    let mut c = RecoveryBuildParallelCoordinator::new(q, 2).unwrap();
    assert_eq!(c.progress().possible, 25_401_600);
    let control = ExecutionControl::default();
    let mut issued = 0;
    loop {
        let (status, bytes) = c.produce(32, &control).unwrap();
        if status == RecoveryBuildParallelProduce::Pending {
            break;
        }
        assert_eq!(status, RecoveryBuildParallelProduce::Batch);
        assert!(bytes.len() < 2048);
        issued += 1;
        assert!(issued <= 8);
    }
    assert_eq!(issued, 8);
    assert_eq!(c.progress().issued, 256);
    assert_eq!(c.progress().completed, 0);
    assert!(c.finish(&control).is_err());
}
#[test]
fn recovery_build_parallel_worker_yields_between_pairs_without_repeating_one() {
    let mut q = query();
    q.first_supply = "*".into();
    q.second_supply = "*".into();
    let control = ExecutionControl::default();
    let expected = q.search(&control).unwrap();
    let mut c = RecoveryBuildParallelCoordinator::new(q, 2).unwrap();
    let mut w = RecoveryBuildParallelWorker::new(&c.worker_initialization()).unwrap();
    let mut yields = 0;
    loop {
        let (status, task) = c.produce(32, &control).unwrap();
        match status {
            RecoveryBuildParallelProduce::Completed => break,
            RecoveryBuildParallelProduce::Batch => {
                let mut result = w.consume(&task, &control).unwrap();
                while w.has_pending_work() {
                    assert!(w.consume(&task, &control).is_err());
                    yields += 1;
                    result = w.advance(&control).unwrap();
                }
                c.absorb(&result.unwrap(), &control).unwrap();
            }
            _ => panic!("single outstanding task must make progress"),
        }
    }
    assert!(yields > 0);
    assert_eq!(w.progress().completed, 49);
    assert_eq!(c.finish(&control).unwrap(), expected);
}

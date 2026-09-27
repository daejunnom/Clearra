use super::*;
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
fn small() -> RecoveryBuildQuery {
    RecoveryBuildQuery {
        fields: RecoveryBuildFields {
            height: 8,
            initial: Board256Mask::EMPTY,
            middle: Board256Mask::from_words([15, 0, 0, 0]),
            result: Board256Mask::from_words([0xc030, 0, 0, 0]),
        },
        first_supply: "[IJLOSTZ]".into(),
        second_supply: "[IJLOSTZ]".into(),
        early_limit: CrossStageEarlyLimit::Auto,
        allow_piece_exchange: true,
        hold_enabled: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}
#[test]
fn recovery_parallel_reordered_batches_equal_serial_including_canonical_examples_and_float_bits() {
    let control = ExecutionControl::default();
    for exchange in [false, true] {
        for b2b in [false, true] {
            let mut q = small();
            q.allow_piece_exchange = exchange;
            q.preserve_b2b = b2b;
            let serial = q.search(&control).unwrap();
            let mut c = RecoveryBuildCoordinator::new(q, 4).unwrap();
            let init = c.worker_initialization();
            let mut a = RecoveryBuildWorker::new(&init).unwrap();
            let mut b = RecoveryBuildWorker::new(&init).unwrap();
            let (_, first) = c.produce(&control).unwrap();
            let (_, last) = c.produce(&control).unwrap();
            let (_, r1) = a.consume(&first, &control).unwrap();
            let (_, r2) = b.consume(&last, &control).unwrap();
            c.absorb(&r2, &control).unwrap();
            assert_eq!(c.progress().evaluated, 0);
            c.absorb(&r1, &control).unwrap();
            assert_eq!(c.progress().evaluated, 49);
            assert_eq!(
                c.produce(&control).unwrap().0,
                RecoveryBuildProduce::Completed
            );
            assert_eq!(c.finish(&control).unwrap(), serial);
        }
    }
}
#[test]
fn recovery_parallel_receipts_are_owned_complete_and_exactly_once() {
    let control = ExecutionControl::default();
    let q = small();
    let mut c = RecoveryBuildCoordinator::new(q.clone(), 2).unwrap();
    let init = c.worker_initialization();
    let (_, task) = c.produce(&control).unwrap();
    let mut w = RecoveryBuildWorker::new(&init).unwrap();
    let (_, receipt) = w.consume(&task, &control).unwrap();
    for cut in 0..receipt.len() {
        assert!(super::parallel_wire::read_result(&receipt[..cut], &init).is_err());
    }
    let mut other = q;
    other.preserve_b2b = true;
    let other = RecoveryBuildCoordinator::new(other, 2).unwrap();
    assert!(RecoveryBuildWorker::new(&other.worker_initialization())
        .unwrap()
        .consume(&task, &control)
        .is_err());
    c.absorb(&receipt, &control).unwrap();
    let accepted = c.progress();
    assert!(c.absorb(&receipt, &control).is_err());
    assert_eq!(c.progress(), accepted);
    assert!(
        c.finish(&control).is_err(),
        "an unprocessed last batch must not be reported complete"
    );
}
#[test]
fn recovery_parallel_user_p7_fixture_keeps_full_domain_and_cancellation() {
    // Screenshot logical result is 50 bits higher on the shared editor canvas;
    // here the API receives the exact five-row-compacted result, not a new field.
    let q = RecoveryBuildQuery {
        fields: RecoveryBuildFields {
            height: 10,
            initial: Board256Mask::from_words([0xc0383f3fc7, 0, 0, 0]),
            middle: Board256Mask::from_words([0x3ff3fc7c0c038, 0, 0, 0]),
            result: Board256Mask::from_words([0x30483f07f3f8f, 0, 0, 0]),
        },
        first_supply: "P7".into(),
        second_supply: "P7".into(),
        early_limit: CrossStageEarlyLimit::Auto,
        allow_piece_exchange: true,
        hold_enabled: true,
        preserve_b2b: true,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    };
    let mut c = RecoveryBuildCoordinator::new(q, 4).unwrap();
    assert_eq!(c.progress().possible, 25_401_600);
    let control = ExecutionControl::default();
    for _ in 0..8 {
        assert_eq!(c.produce(&control).unwrap().0, RecoveryBuildProduce::Batch);
    }
    assert_eq!(
        c.produce(&control).unwrap().0,
        RecoveryBuildProduce::Pending,
        "bounded inflight window, not a capped search domain"
    );
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    let cancelled = ExecutionControl::new(token);
    assert_eq!(
        c.produce(&cancelled).unwrap().0,
        RecoveryBuildProduce::Cancelled
    );
    assert_eq!(
        c.finish(&cancelled).unwrap_err(),
        RecoveryBuildError::Cancelled
    );
}
#[test]
fn recovery_parallel_user_geometry_first_actual_batch_agrees_with_individual_fixed_searches() {
    let mut q = small();
    q.fields = RecoveryBuildFields {
        height: 10,
        initial: Board256Mask::from_words([0xc0383f3fc7, 0, 0, 0]),
        middle: Board256Mask::from_words([0x3ff3fc7c0c038, 0, 0, 0]),
        result: Board256Mask::from_words([0x30483f07f3f8f, 0, 0, 0]),
    };
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    q.preserve_b2b = true;
    let domain = super::parallel::Domain::new(q.clone()).unwrap();
    let control = ExecutionControl::default();
    let mut c = RecoveryBuildCoordinator::new(q, 4).unwrap();
    let init = c.worker_initialization();
    let (_, task) = c.produce(&control).unwrap();
    let (_, receipt) = RecoveryBuildWorker::new(&init)
        .unwrap()
        .consume(&task, &control)
        .unwrap();
    let (start, rows) = super::parallel_wire::read_result(&receipt, &init).unwrap();
    assert_eq!(start, 0);
    for (i, row) in rows.iter().enumerate() {
        let expected = domain.evaluate(i as u128, &control).unwrap();
        assert_eq!(row.status, expected.path.status);
        assert_eq!(row.states, expected.path.states as u64);
    }
    c.absorb(&receipt, &control).unwrap();
    assert_eq!(c.progress().possible, 25_401_600);
    assert_eq!(c.progress().evaluated, RECOVERY_PAIRS_PER_TASK as u128);
    assert!(
        c.finish(&control).is_err(),
        "a prefix test is not full P7/P7 success"
    );
}

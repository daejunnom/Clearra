//! End-to-end source/hold checks with independently countable tiny universes.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl,
};
use clearra_forward_search::{
    CrossStageEarlyLimit, RecoveryBuildFields, RecoveryBuildQuery, RecoveryChainStage,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

fn cells(indices: &[u16]) -> Mask {
    indices.iter().fold(Mask::EMPTY, |mask, &index| {
        mask.union(Mask::singleton(index).unwrap())
    })
}
fn query(start: &[u16], targets: &[&[u16]], supplies: &[&str]) -> RecoveryBuildQuery {
    assert_eq!(targets.len(), supplies.len());
    let stages = targets
        .iter()
        .zip(supplies)
        .map(|(target, supply)| RecoveryChainStage {
            target: cells(target),
            supply: (*supply).into(),
        })
        .collect::<Vec<_>>();
    RecoveryBuildQuery {
        fields: RecoveryBuildFields::from_chain(12, cells(start), &stages).unwrap(),
        first_supply: supplies[0].into(),
        second_supply: supplies.last().unwrap().to_string(),
        chain_stages: stages,
        all_solutions: true,
        minimum_solutions: false,
        required_solution_keys: Vec::new(),
        minimum_source_identity: None,
        early_limit: CrossStageEarlyLimit::AtMost(0),
        allow_piece_exchange: false,
        hold_enabled: false,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}

#[test]
fn recovery_chain_rejects_probability_multiplication_and_mirror_double_counting() {
    let q = query(
        &[],
        &[&[0, 1, 10, 11], &[20, 21, 30, 31], &[40, 41, 50, 51]],
        &["[IO]", "[IO]", "[IO]"],
    );
    let result = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(
        (result.possible, result.normal_count, result.no_path_count),
        (8, 1, 7)
    );
    assert_eq!(result.normal_probability, 0.125);
    assert_eq!(result.solutions.len(), 2);
    for solution in result.solutions {
        assert_eq!(solution.covered_count, 1);
        assert_eq!(solution.probability, 0.125);
    }
}

#[test]
fn recovery_chain_small_inventory_universe_obeys_hold_and_source_exchange() {
    // Normal target order is I,O,I. There are eight input triples; only IOI
    // matches without exchange. With hold and exchange, IIO and OII also work.
    for hold in [false, true] {
        for exchange in [false, true] {
            let mut q = query(
                &[4, 5, 6, 7, 8, 9],
                &[&[0, 1, 2, 3], &[10, 11, 20, 21], &[14, 15, 16, 17]],
                &["[IO]", "[IO]", "[IO]"],
            );
            q.hold_enabled = hold;
            q.allow_piece_exchange = exchange;
            let report = q.search(&ExecutionControl::default()).unwrap();
            let expected = if hold && exchange { 3 } else { 1 };
            assert_eq!(
                (report.possible, report.normal_count, report.recovery_count),
                (8, expected, 0)
            );
            assert_eq!(report.no_path_count, 8 - expected);
            assert_eq!(report.normal_probability, expected as f64 / 8.0);
        }
    }
}

#[test]
fn recovery_chain_keeps_a_token_held_across_two_boundaries() {
    let mut q = query(
        &[],
        &[&[0, 1, 2, 3], &[4, 5, 6, 7], &[8, 9, 18, 19]],
        &["O", "I", "I"],
    );
    q.hold_enabled = true;
    q.allow_piece_exchange = true;
    let report = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!((report.normal_count, report.recovery_count), (1, 0));
    assert!(!report.solutions.is_empty());
    for solution in report.solutions {
        let path = solution.example.path;
        assert_eq!(
            path.steps.iter().map(|s| s.source_index).collect::<Vec<_>>(),
            [1, 2, 0]
        );
        assert_eq!(
            path.steps.last().unwrap().hold_decision,
            "release-held-at-terminal"
        );
        assert_eq!(path.stage_early_counts, [0, 0]);
    }
    q.allow_piece_exchange = false;
    assert_eq!(
        q.search(&ExecutionControl::default()).unwrap().no_path_count,
        1
    );
}

#[test]
fn recovery_chain_keeps_the_complete_unused_final_suffix() {
    let q = query(
        &[],
        &[&[0, 1, 10, 11], &[20, 21, 30, 31], &[40, 41, 50, 51]],
        &["O", "O", "O[IJLOSTZ]"],
    );
    let report = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(
        (report.possible, report.normal_count, report.no_path_count),
        (7, 7, 0)
    );
    assert_eq!(report.normal_probability, 1.0);
    for solution in report.solutions {
        assert_eq!(solution.covered_count, 7);
        assert_eq!(solution.example.path.steps.len(), 3);
        assert_eq!(solution.example.path.stage_source_lengths, [1, 1, 2]);
    }
}

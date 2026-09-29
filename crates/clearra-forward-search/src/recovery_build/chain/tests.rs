use super::*;
use crate::{RecoveryBuildFields, RecoveryBuildQuery};
fn mask(n: u64) -> Mask {
    Mask::from_words([n, 0, 0, 0])
}
fn query() -> RecoveryChainQuery {
    RecoveryChainQuery {
        height: 8,
        initial: Mask::EMPTY,
        targets: vec![mask(0xc03), mask(0x300c), mask(0xf0)],
        supplies: vec!["O".into(), "I".into(), "O".into()],
        early_limit: CrossStageEarlyLimit::AtMost(1),
        allow_piece_exchange: true,
        hold_enabled: false,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
        minimum_solutions: true,
    }
}
#[test]
fn recovery_chain_three_stages_preserve_repayment_and_actual_early_boundaries() {
    let q = query();
    let c = ExecutionControl::default();
    let r = q.search(&c).unwrap();
    assert_eq!(
        (
            r.possible,
            r.normal_count,
            r.recovery_count,
            r.no_path_count
        ),
        (1, 0, 1, 0)
    );
    assert!(!r.solutions.is_empty());
    for s in &r.solutions {
        assert_eq!(s.witness.early_counts, vec![0, 1]);
        assert_eq!(s.witness.step_stages, vec![0, 2, 1]);
        assert_eq!(s.covered_count, 1);
        assert_eq!(s.witness.steps.len(), 3);
    }
    let mut off = q.clone();
    off.allow_piece_exchange = false;
    assert_eq!(off.search(&c).unwrap().no_path_count, 1);
    off = q;
    off.early_limit = CrossStageEarlyLimit::AtMost(0);
    assert_eq!(off.search(&c).unwrap().no_path_count, 1);
}
#[test]
fn recovery_chain_held_token_survives_two_boundaries_and_keeps_its_origin() {
    let mut q = query();
    q.supplies = vec!["I".into(), "O".into(), "O".into()];
    q.hold_enabled = true;
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!((r.normal_count, r.recovery_count), (1, 0));
    for s in &r.solutions {
        let last = s.witness.steps.last().unwrap();
        assert_eq!(last.source_index, 0);
        assert_eq!(last.hold_decision, "release-held-at-terminal");
        assert_eq!(s.witness.step_stages, vec![0, 1, 2]);
        assert_eq!(s.witness.early_counts, vec![0, 0]);
    }
}
#[test]
fn recovery_chain_probabilities_union_complete_inputs_not_independent_stage_rates() {
    let mut q = query();
    q.supplies = vec!["[IO]".into(), "O".into(), "O".into()];
    q.hold_enabled = true;
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(
        (
            r.possible,
            r.normal_count,
            r.recovery_count,
            r.no_path_count
        ),
        (2, 1, 0, 1)
    );
    assert_eq!(r.normal_probability, 0.5);
    assert_eq!(r.no_path_probability, 0.5);
    assert!(r
        .solutions
        .iter()
        .all(|s| s.covered_count == 1 && s.probability == 0.5));
    let classes = r.coverage_classes.unwrap();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].len(), r.solutions.len());
}
#[test]
fn recovery_chain_four_bag_source_is_compact_without_enumerating_combinations() {
    let mut q = query();
    q.targets.push(mask(0x3c00));
    q.supplies = vec!["P7".into(); 4];
    // Source construction is independent of the deliberately unused geometry.
    let mut diagram = Diagram::default();
    let source = source::Source::new(&q, &mut diagram, &ExecutionControl::default()).unwrap();
    assert_eq!(source.possible, 5040_u128.pow(4));
    assert_eq!(
        source
            .measure(&mut diagram, source.universe, &ExecutionControl::default())
            .unwrap(),
        (5040_u128.pow(4), 1.0)
    );
    assert!(
        diagram.node_count() < 2000,
        "source DAG must not contain one node per queue tuple"
    );
}
#[test]
fn recovery_chain_two_targets_match_the_existing_exact_search() {
    for exchange in [false, true] {
        for hold in [false, true] {
            for early in [0, 1, 2] {
                let mut q = query();
                q.targets = vec![mask(0xf), mask(0xc030)];
                q.supplies = vec!["[IO]".into(), "[IO]".into()];
                q.allow_piece_exchange = exchange;
                q.hold_enabled = hold;
                q.early_limit = CrossStageEarlyLimit::AtMost(early);
                let old = RecoveryBuildQuery {
                    all_solutions: true,
                    minimum_solutions: false,
                    required_solution_keys: vec![],
                    minimum_source_identity: None,
                    fields: RecoveryBuildFields {
                        height: q.height,
                        initial: q.initial,
                        middle: q.targets[0],
                        result: q.targets[1],
                    },
                    first_supply: q.supplies[0].clone(),
                    second_supply: q.supplies[1].clone(),
                    early_limit: q.early_limit,
                    allow_piece_exchange: exchange,
                    hold_enabled: hold,
                    preserve_b2b: false,
                    initial_b2b: true,
                    rule_profile: q.rule_profile,
                    spin_profile: q.spin_profile,
                };
                let c = ExecutionControl::default();
                let a = old.search(&c).unwrap();
                let b = q.search(&c).unwrap();
                assert_eq!(
                    (
                        a.possible,
                        a.normal_count,
                        a.recovery_count,
                        a.no_path_count
                    ),
                    (
                        b.possible,
                        b.normal_count,
                        b.recovery_count,
                        b.no_path_count
                    ),
                    "exchange={exchange} hold={hold} early={early}"
                );
                assert_eq!(a.solutions.len(), b.solutions.len());
                let mut ca = a
                    .solutions
                    .iter()
                    .map(|s| s.covered_count)
                    .collect::<Vec<_>>();
                ca.sort_unstable();
                let mut cb = b
                    .solutions
                    .iter()
                    .map(|s| s.covered_count)
                    .collect::<Vec<_>>();
                cb.sort_unstable();
                assert_eq!(ca, cb);
            }
        }
    }
}
#[test]
fn recovery_chain_consecutive_pc_boundaries_preserve_logical_rows() {
    let mut q = query();
    q.initial = mask(0x3f0 | (0x3f0 << 10));
    q.targets = vec![mask(0xf), mask(0xf << 10), mask(0xc03 << 20)];
    q.supplies = vec!["I".into(), "I".into(), "O".into()];
    q.allow_piece_exchange = false;
    q.preserve_b2b = false;
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let r = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(r.normal_count, 1);
    for s in &r.solutions {
        assert_eq!(s.witness.steps[0].cleared_lines, 1);
        assert_eq!(s.witness.steps[1].cleared_lines, 1);
        assert_eq!(s.witness.steps[2].logical_cells[2], 3);
        assert_eq!(s.witness.steps[2].logical_cells[3], 3);
    }
}
#[test]
fn recovery_chain_invalid_inputs_and_cancellation_are_not_zero_probability() {
    let c = ExecutionControl::default();
    let mut q = query();
    q.targets[1] = q.targets[0];
    assert!(q.search(&c).is_err());
    q = query();
    q.supplies.pop();
    assert!(q.search(&c).is_err());
    let mut search = RecoveryChainSearch::new(query(), &c).unwrap();
    assert!(search.advance(1, &c).is_ok());
    assert!(
        search.finish(&c).is_err(),
        "unfinished enumeration cannot claim completeness"
    );
    let mut search = RecoveryChainSearch::new(query(), &c).unwrap();
    c.cancellation.handle().cancel();
    assert_eq!(search.advance(1, &c), Err(Error::Cancelled));
    assert_eq!(query().search(&c), Err(Error::Cancelled));
}

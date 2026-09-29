use super::*;
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

fn cells(indices: &[u16]) -> Mask {
    indices.iter().fold(Mask::EMPTY, |mask, &cell| {
        mask.union(Mask::singleton(cell).unwrap())
    })
}
fn chain(n: usize) -> RecoveryBuildQuery {
    let stages = (0..n)
        .map(|i| RecoveryChainStage {
            target: cells(&[
                20 * i as u16,
                20 * i as u16 + 1,
                20 * i as u16 + 10,
                20 * i as u16 + 11,
            ]),
            supply: "O".into(),
        })
        .collect::<Vec<_>>();
    RecoveryBuildQuery {
        fields: RecoveryBuildFields::from_chain(12, Mask::EMPTY, &stages).unwrap(),
        first_supply: "O".into(),
        second_supply: "O".into(),
        chain_stages: stages,
        all_solutions: true,
        minimum_solutions: false,
        required_solution_keys: Vec::new(),
        minimum_source_identity: None,
        early_limit: CrossStageEarlyLimit::AtMost(0),
        allow_piece_exchange: false,
        hold_enabled: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}
#[test]
fn recovery_build_initial_symmetry_verifies_both_middle_piece_handednesses() {
    let control = ExecutionControl::default();
    let mut q = chain(3);
    q.chain_stages.clear();
    q.fields = RecoveryBuildFields {
        height: 8,
        initial: Mask::EMPTY,
        middle: cells(&[0, 1, 11, 21]),
        result: cells(&[4, 5, 14, 15]),
    };
    q.first_supply = "[JL]".into();
    q.second_supply = "O".into();
    q.hold_enabled = false;
    let original_successes = [PieceKind::J, PieceKind::L]
        .into_iter()
        .filter(|&piece| {
            RecoveryBuildFixedQuery {
                fields: q.fields.clone(),
                first_supply: vec![piece],
                second_supply: vec![PieceKind::O],
                early_limit: q.early_limit,
                allow_piece_exchange: false,
                hold_enabled: false,
                preserve_b2b: false,
                initial_b2b: true,
                rule_profile: q.rule_profile,
                spin_profile: q.spin_profile,
            }
            .search(&control)
            .unwrap()
            .status
                == RecoveryBuildStatus::Normal
        })
        .count();
    assert_eq!(
        original_successes, 1,
        "independent fixed-field search accepts only one handedness"
    );
    let report = q.search(&control).unwrap();
    assert_eq!(report.possible, 2);
    assert_eq!(report.normal_count, 2);
    assert_eq!(report.no_path_count, 0);
    let middles = report
        .solutions
        .iter()
        .map(|s| s.example.path.middle_target)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        middles.len(),
        2,
        "initial symmetry adds the entire mirrored continuation, not just a rendered image"
    );
    q.fields.initial = cells(&[9]);
    assert_eq!(
        super::chain::orientations(&q, &control).unwrap().len(),
        1,
        "asymmetric start and middle do not add reflections"
    );
}
#[test]
fn recovery_build_chain_counts_and_exact_parallel_catalog_agree() {
    let control = ExecutionControl::default();
    for n in [3, 4, 5] {
        let q = chain(n);
        let expected = super::parallel::search_serial(q.clone(), &control).unwrap();
        assert_eq!(
            (
                expected.possible,
                expected.normal_count,
                expected.recovery_count
            ),
            (1, 1, 0)
        );
        assert_eq!(
            expected.solutions.len(),
            2,
            "left and reflected right stacked O drawings"
        );
        for solution in &expected.solutions {
            assert_eq!(solution.example.path.stage_source_lengths, vec![1; n]);
            assert_eq!(solution.example.path.stage_early_counts, vec![0; n - 1]);
            assert_eq!(solution.example.path.stage_targets.len(), n);
        }
        let mut coordinator = RecoveryBuildParallelCoordinator::new(q, 4).unwrap();
        let mut worker =
            RecoveryBuildParallelWorker::new(&coordinator.worker_initialization()).unwrap();
        let mut replies = Vec::new();
        loop {
            let (kind, task) = coordinator.produce(32, &control).unwrap();
            match kind {
                RecoveryBuildParallelProduce::Batch => {
                    let mut reply = worker.consume(&task, &control).unwrap();
                    while worker.has_pending_work() {
                        reply = worker.advance(&control).unwrap();
                    }
                    replies.push(reply.unwrap());
                }
                RecoveryBuildParallelProduce::Pending => {
                    if let Some(reply) = replies.pop() {
                        coordinator.absorb(&reply, &control).unwrap();
                    }
                }
                RecoveryBuildParallelProduce::Completed => break,
                _ => panic!("unexpected cancellation"),
            }
            if replies.len() > 1 {
                let reply = replies.pop().unwrap();
                coordinator.absorb(&reply, &control).unwrap();
            }
        }
        assert_eq!(coordinator.finish(&control).unwrap(), expected);
    }
}
#[test]
fn recovery_build_chain_source_is_symbolic_beyond_wasm_cartesian_address_space() {
    use super::staged::{diagram::Diagram, source::Source};
    let control = ExecutionControl::default();
    let mut q = chain(5);
    for stage in &mut q.chain_stages {
        stage.supply = "P7".into();
    }
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    let prepared = super::population::PreparedPopulation::new(q).unwrap();
    assert_eq!(prepared.possible, 5040u128.pow(5));
    let mut diagram = Diagram::default();
    let source = Source::for_population(&mut diagram, &prepared, &control).unwrap();
    assert_eq!(
        diagram.count(source.universe, 0, 35).unwrap(),
        5040u128.pow(5)
    );
    assert!(diagram.node_count() < 2500, "no Cartesian queue array");
}
#[test]
fn recovery_build_chain_preserves_per_source_inventory_and_boundary_quotas() {
    // Three independently grounded targets. First source can only build stage 2,
    // second source only stage 1, and the final O builds the last target.
    let control = ExecutionControl::default();
    let mut q = chain(3);
    q.chain_stages[0].target = cells(&[0, 1, 2, 3]);
    q.chain_stages[0].supply = "O".into();
    q.chain_stages[1].target = cells(&[5, 6, 15, 16]);
    q.chain_stages[1].supply = "I".into();
    q.chain_stages[2].target = cells(&[8, 9, 18, 19]);
    q.chain_stages[2].supply = "O".into();
    q.fields = RecoveryBuildFields::from_chain(8, Mask::EMPTY, &q.chain_stages).unwrap();
    q.first_supply = "O".into();
    q.second_supply = "O".into();
    q.hold_enabled = false;
    q.allow_piece_exchange = true;
    assert_eq!(q.search(&control).unwrap().no_path_count, 1);
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    let result = super::parallel::search_serial(q.clone(), &control).unwrap();
    assert_eq!(result.recovery_count, 1);
    assert!(result
        .solutions
        .iter()
        .all(|s| s.example.path.stage_early_counts == vec![1, 0]));
    q.allow_piece_exchange = false;
    assert_eq!(
        q.search(&control).unwrap().no_path_count,
        1,
        "aggregate inventory alone must not erase per-source constraints"
    );
    q.hold_enabled = true;
    // Swap/order correction through hold still cannot change per-source inventory.
    assert_eq!(q.search(&control).unwrap().no_path_count, 1);
}

#[test]
fn recovery_build_chain_row_ownership_survives_a_middle_line_clear() {
    let control = ExecutionControl::default();
    let start = cells(&[4, 5, 6, 7, 8, 9]);
    let mut q = chain(3);
    q.chain_stages = vec![
        RecoveryChainStage {
            target: cells(&[0, 1, 2, 3]),
            supply: "I".into(),
        },
        RecoveryChainStage {
            target: cells(&[10, 11, 20, 21]),
            supply: "O".into(),
        },
        RecoveryChainStage {
            target: cells(&[14, 15, 16, 17]),
            supply: "I".into(),
        },
    ];
    q.fields = RecoveryBuildFields::from_chain(8, start, &q.chain_stages).unwrap();
    q.first_supply = "I".into();
    q.second_supply = "I".into();
    q.hold_enabled = false;
    let report = super::parallel::search_serial(q, &control).unwrap();
    assert_eq!(
        (report.possible, report.normal_count, report.no_path_count),
        (1, 1, 0)
    );
    assert_eq!(
        report.solutions.len(),
        2,
        "the cleared first checkpoint enables both suffix directions"
    );
    for solution in report.solutions {
        let path = solution.example.path;
        assert_eq!(path.stage_source_lengths, vec![1, 1, 1]);
        assert_eq!(path.stage_early_counts, vec![0, 0]);
        assert_eq!(path.steps[0].cleared_rows, 1);
        assert_eq!(path.steps[0].logical_cells[0], 15);
        for (stage, step) in path.steps.iter().enumerate() {
            // Independently form row words from the expected stage's cells.
            // Neither a small row value nor a cleared row becomes a cell ID.
            let expected = Mask::from_words(path.stage_targets[stage]);
            let rows: Vec<u16> = (0..path.steps[stage].logical_cells.len())
                .map(|y| {
                    (0..10).fold(0, |row, x| {
                        row | if expected.contains_index((y * 10 + x) as u16) {
                            1 << x
                        } else {
                            0
                        }
                    })
                })
                .collect();
            assert_eq!(step.logical_cells, rows);
            assert_eq!(step.source_index, stage);
        }
    }
}

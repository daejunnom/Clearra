use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionPartition},
    piece::piece_kind::PieceKind,
};
use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use super::*;

fn fixture(input: &str, example_limit: usize, policy: PcExecutionPolicy) -> Arc<SearchProblem> {
    let expression = QueuePatternExpression::parse(input, 1).expect("one-piece expression");
    let query = PcScenarioQuery::new(
        // An O-shaped cavity, already used by exact compact execution tests.
        PcScenarioBoard::standard_10(2, 0xf3fcf),
        PcQueueInput::pattern_expression(expression),
        PieceWindow::new(1),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_retained_trace_limit(0)
    .with_execution_policy(policy)
    .with_objective(ObjectivePolicy::unique());
    Arc::new(
        ProblemCompiler::compile_scenario_percent(&query)
            .expect("compiled compact PC")
            .with_pc_failed_queue_v2_evidence(example_limit),
    )
}

fn advance_to_terminal(
    session: &mut WasmCompactPcFailedQueueSession,
    control: &ExecutionControl,
) -> Result<WasmPcFailedQueueAdvance, WasmCpuSearchError> {
    for _ in 0..100_000 {
        match session.advance(1, control)? {
            WasmPcFailedQueueAdvance::Pending => {}
            terminal => return Ok(terminal),
        }
    }
    panic!("compact one-piece fixture must terminate");
}

#[test]
fn compact_pc_failed_queue_exact_compiled_ordinals_and_fail_closed_boundaries() {
    let policy = PcExecutionPolicy::mvp_default().with_workers(1);
    let control = ExecutionControl::default();
    assert!(matches!(
        WasmCompactPcFailedQueueSession::new(fixture(
            "[IO]",
            1,
            policy.clone().with_max_memory_mib(Some(0))
        )),
        Err(WasmCpuSearchError::ResourceAdmission { .. })
    ));
    // The full-capacity authority is process-wide; run these fixtures in
    // sequence and dispose of the producer before acquiring the next one.
    for (input, example_limit, success_count, fail_count) in
        [("[IOT]", 1, 1, 2), ("[IT]", 2, 0, 2), ("[IO]", 0, 1, 1)]
    {
        let problem = fixture(input, example_limit, policy.clone());
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("complete original ordinal space");
        let mut session =
            WasmCompactPcFailedQueueSession::new(Arc::clone(&problem)).expect("qualified input");
        let WasmPcFailedQueueAdvance::Completed(result, evidence) =
            advance_to_terminal(&mut session, &control).expect("typed compact completion")
        else {
            panic!("expected successful complete result (including empty family)");
        };
        assert!(evidence.matches_problem_owner(&problem));
        assert!(!evidence.matches_problem_owner(&Arc::new((*problem).clone())));
        assert_eq!(evidence.success_pattern_count(), success_count);
        assert_eq!(evidence.failed_pattern_count(), fail_count);
        assert_eq!(evidence.pattern_count(), universe.pattern_count());
        assert_eq!(
            evidence.success_coverage().words(),
            result.coverage_pattern_words()
        );
        assert_eq!(result.field("status"), Some("percent-executed"));
        assert_eq!(
            result.field("search_output_policy"),
            Some("coverage-summary")
        );
        assert_eq!(result.bool_field("count_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(
            result.field("probability"),
            Some(format_probability(evidence.success_probability().get()).as_str())
        );
        assert!(result.pc_chance_coverage_evidence().is_none());

        let mut expected_failures = Vec::new();
        for ordinal in 0..universe.pattern_count() {
            let queue = universe.sequence_at(ordinal);
            let solves_cavity = queue.as_ref() == [PieceKind::O];
            assert_eq!(
                evidence
                    .success_coverage()
                    .contains(PatternId::new(ordinal)),
                solves_cavity,
                "the bit is in the original supply order, not candidate order"
            );
            if !solves_cavity {
                expected_failures.push((ordinal, queue.into_owned()));
            }
        }
        let observed_failures: Vec<_> = evidence
            .examples()
            .iter()
            .map(|example| (example.pattern_index(), example.pieces().to_vec()))
            .collect();
        expected_failures.truncate(example_limit);
        assert_eq!(observed_failures, expected_failures);
        assert!(matches!(
            session.advance(1, &control),
            Err(WasmCpuSearchError::InvalidProblem {
                reason: "compact_pc_failed_queue_already_finished"
            })
        ));
        assert!(
            matches!(
                WasmCompactPcFailedQueueSession::new(Arc::clone(&problem)),
                Err(WasmCpuSearchError::ResourceAdmission { .. })
            ),
            "terminal evidence is produced under the original retained lease"
        );
    }

    let problem = fixture("[IO]", 1, policy.clone().with_max_candidates(1));
    {
        let mut session = WasmCompactPcFailedQueueSession::new(problem).expect("bounded input");
        assert!(matches!(
            advance_to_terminal(&mut session, &control),
            Err(WasmCpuSearchError::InvalidProblem {
                reason: "compact_pc_failed_queue_source_incomplete"
            })
        ));
    }
    let problem = fixture("[IO]", 1, policy.clone());
    {
        let mut session =
            WasmCompactPcFailedQueueSession::new(Arc::clone(&problem)).expect("qualified input");
        let partition =
            ExecutionControl::default().with_partition(ExecutionPartition::new(0, 2).unwrap());
        assert!(matches!(
            session.advance(1, &partition),
            Err(WasmCpuSearchError::InvalidProblem {
                reason: "compact_pc_failed_queue_partial_execution"
            })
        ));
    }
    {
        let mut session =
            WasmCompactPcFailedQueueSession::new(problem).expect("admission released");
        let token = ExecutionCancellationToken::new();
        token.handle().cancel();
        assert!(matches!(
            session.advance(1, &ExecutionControl::new(token)),
            Ok(WasmPcFailedQueueAdvance::Cancelled)
        ));
    }
    assert!(matches!(
        WasmCompactPcFailedQueueSession::new(fixture("[IO]", 1, policy.with_workers(2))),
        Err(WasmCpuSearchError::Unsupported {
            reason: "compact_pc_failed_queue_requires_one_cpu_worker"
        })
    ));
}

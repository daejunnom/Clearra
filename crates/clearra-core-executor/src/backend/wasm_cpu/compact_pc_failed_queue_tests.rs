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
    let expression = QueuePatternExpression::parse(input, 7).expect("one-piece expression");
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

fn multi_candidate_fixture(workers: usize) -> Arc<SearchProblem> {
    let expression =
        QueuePatternExpression::parse("IIOO[OT]", 7).expect("deterministic five-piece patterns");
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::pattern_expression(expression),
        PieceWindow::new(5),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(5))
    .with_retained_trace_limit(0)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_use_all_logical_processors(true)
            .with_workers(workers),
    )
    .with_objective(ObjectivePolicy::all());
    Arc::new(
        ProblemCompiler::compile_scenario_percent(&query)
            .expect("compiled compact PC")
            .with_pc_failed_queue_v2_evidence(2),
    )
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[test]
fn compact_pc_failed_queue_native_workers_keep_complete_original_coverage() {
    let control = ExecutionControl::default();
    let run = |workers| {
        let problem = multi_candidate_fixture(workers);
        let mut session =
            WasmCompactPcFailedQueueSession::new_for_direct_execution(Arc::clone(&problem))
                .expect("native CPU session");
        let WasmPcFailedQueueAdvance::Completed(result, evidence) =
            advance_to_terminal(&mut session, &control).expect("complete compact result")
        else {
            panic!("complete compact result");
        };
        assert!(evidence.matches_problem_owner(&problem));
        assert_eq!(evidence.pattern_count(), 2);
        assert_eq!(evidence.success_pattern_count(), 1);
        assert_eq!(evidence.failed_pattern_count(), 1);
        (result, evidence)
    };
    let (serial, serial_evidence) = run(1);
    if clearra_core_domain::runtime_cpu_capacity::CpuCapacity::current().hard_limit() < 2 {
        return;
    }
    let (parallel, parallel_evidence) = run(2);
    assert_eq!(parallel.usize_field("workers_used"), Some(2));
    assert_eq!(parallel.bool_field("cpu_parallel_execution"), Some(true));
    assert_eq!(
        serial.coverage_pattern_words(),
        parallel.coverage_pattern_words()
    );
    assert_eq!(
        serial_evidence.source_row_count(),
        parallel_evidence.source_row_count()
    );
    assert_eq!(serial_evidence.examples(), parallel_evidence.examples());
}

#[test]
fn compact_pc_failed_queue_distributed_worker_rows_require_complete_dispatch() {
    use crate::{
        pc_chance_coverage_evidence::DistributedPcChanceCoverageRows, WasmCandidateProducerAdvance,
        WasmCpuCandidateProducer, WasmDistributedVerifier,
    };

    let problem = multi_candidate_fixture(1);
    let control = ExecutionControl::default();
    // A real worker owns a separate WASM realm and capacity ledger. Use a
    // serial transcript for the native fixture to avoid overlapping its
    // worker admission with the coordinator's full-capacity parent lease.
    let partial = {
        let mut producer = WasmCpuCandidateProducer::new_typed(&problem).unwrap();
        let mut worker = WasmDistributedVerifier::new(&problem).unwrap();
        loop {
            match producer.advance(&control).unwrap() {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(packet) => {
                    worker.consume(&packet, &control).unwrap();
                }
                WasmCandidateProducerAdvance::Completed(_) => break,
                WasmCandidateProducerAdvance::Cancelled => panic!("unexpected cancellation"),
            }
        }
        let raw = worker.finish().unwrap();
        let source = raw
            .pc_chance_coverage_evidence()
            .expect("actual worker evidence");
        let transport = DistributedPcChanceCoverageRows::try_from_untrusted_rows(
            source.piece_source_id(),
            source.pattern_universe_id(),
            source.pattern_weight_model_id(),
            source.pattern_count(),
            source.rows().to_vec(),
            source.complete(),
        )
        .unwrap();
        raw.without_pc_chance_transient_evidence()
            .with_distributed_pc_chance_coverage_rows(transport)
    };
    let open_merger = || {
        let mut producer =
            WasmCpuCandidateProducer::new_failed_queue(Arc::clone(&problem), 0).unwrap();
        loop {
            match producer.advance(&control).unwrap() {
                WasmCandidateProducerAdvance::Pending
                | WasmCandidateProducerAdvance::Candidate(_) => {}
                WasmCandidateProducerAdvance::Completed(summary) => {
                    return (producer.into_merger().unwrap(), summary)
                }
                WasmCandidateProducerAdvance::Cancelled => panic!("unexpected cancellation"),
            }
        }
    };
    {
        let (mut merger, summary) = open_merger();
        assert!(summary.candidate_count > 1);
        assert!(
            merger.finish(&summary, 1).is_err(),
            "ordinary terminal cannot seal a complement"
        );
        assert!(
            merger.finish_failed_queue(&summary, 1, &control).is_err(),
            "missing worker"
        );
    }
    {
        let (mut merger, summary) = open_merger();
        merger.absorb(&partial).unwrap();
        assert!(merger.absorb(&partial).is_err(), "duplicate candidate rows");
        assert!(
            merger.finish_failed_queue(&summary, 1, &control).is_err(),
            "failed ingress poisons authority"
        );
    }
    {
        let (mut merger, summary) = open_merger();
        merger.absorb(&partial).unwrap();
        let (result, evidence) = merger.finish_failed_queue(&summary, 1, &control).unwrap();
        assert!(evidence.matches_problem_owner(&problem));
        assert_eq!(
            result.coverage_pattern_words(),
            evidence.success_coverage().words()
        );
        assert_eq!(evidence.success_pattern_count(), 1);
        assert_eq!(evidence.failed_pattern_count(), 1);
        assert!(result.pc_chance_coverage_evidence().is_none());
    }
}

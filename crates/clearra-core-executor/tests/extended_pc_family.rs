//! Bounded actual PC completion, not an empty 24L/sixty-piece enumeration.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    pc::pc_target::PcTarget, piece::piece_kind::PieceKind, solution::ExtendedTilingSolutionKey,
};
use clearra_core_executor::backend::{
    WasmCpuSearchAdvance, WasmCpuSearchBackend, WasmCpuSearchError, WasmCpuSearchSession,
};
use clearra_core_executor::{WasmPcFailedQueueAdvance, WasmPcFailedQueueSession};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
use clearra_rules::profile::{
    builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
    rule_profile::RuleProfile,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;
use std::sync::Arc;

fn forced_query(height: u8, rule: RuleProfile) -> (PcScenarioQuery, Board256Mask, usize) {
    let starts = if height == 7 {
        vec![0, 3]
    } else {
        (0..u16::from(height)).step_by(4).collect()
    };
    let pieces = starts.len();
    let mut holes = Board256Mask::EMPTY;
    for (column, start) in starts.into_iter().enumerate() {
        for row in start..start + 4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column as u16).unwrap());
        }
    }
    let initial = Board256Mask::all_cells(u16::from(height) * 10)
        .unwrap()
        .without(holes);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
        PieceWindow::new(pieces),
    )
    .with_rule(rule)
    .with_exact_pieces(Some(pieces))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_allow_backend_fallback(false),
    );
    (query, initial, pieces)
}

#[test]
fn full_height_pc_family_runs_buildup_and_clears_the_whole_board_in_all_profiles() {
    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        for height in [7, 8, 12, 24] {
            let (query, initial, pieces) = forced_query(height, rule);
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            let result =
                WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                    .unwrap();
            assert_eq!(result.field("actual_backend"), Some("wasm-cpu-pc-extended"));
            assert_eq!(
                result.bool_field("representative_physical_replay_validated"),
                Some(true)
            );
            assert_eq!(result.bool_field("buildup_executed"), Some(true));
            assert_eq!(result.bool_field("count_complete"), Some(true));
            assert_eq!(result.bool_field("build_variant_count_exact"), Some(true));
            assert_eq!(result.field("coverage_probability"), Some("1"));
            let availability = result.execution_report().solution_set_availability();
            assert!(availability.uses_explicit_contract());
            assert!(availability.contract_valid());
            assert!(availability.solution_keys_complete());
            assert!(!availability.solution_page_available());
            assert!(availability.materialized_key_count_matches(1));
            assert_eq!(result.normalized_solution_keys().len(), 1);
            let identity =
                ExtendedTilingSolutionKey::parse_canonical(&result.normalized_solution_keys()[0])
                    .unwrap();
            let mut hasher =
                clearra_core_domain::solution::NormalizedTilingSolutionSetHasher::default();
            hasher.update_extended_canonical_key(identity);
            assert_eq!(
                result.field("normalized_solution_set_hash"),
                Some(hasher.finish().as_str()),
                "the complete PC family must be exportable with the public semantic hash"
            );
            assert_eq!(identity.height(), height);
            assert_eq!(identity.initial_board(), initial);
            assert_eq!(identity.placement_count(), pieces);
            assert_eq!(result.path_steps().len(), pieces);
            assert_eq!(
                result
                    .path_steps()
                    .iter()
                    .map(|step| u16::from(step.cleared_lines()))
                    .sum::<u16>(),
                u16::from(height)
            );
            // An ordinary PC family cannot masquerade as a typed Tiling or
            // minimum/score/replay producer.
            assert!(result.tiling_solution_page_store().is_none());
            assert!(result.pc_chance_coverage_evidence().is_none());
            assert!(result.exact_scoring_execution_batches().is_empty());
        }
    }
}

#[test]
fn full_height_cooperative_and_direct_pc_families_match() {
    for height in [7, 8, 12, 24] {
        let (query, _, _) = forced_query(height, srs_plus());
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let control = ExecutionControl::default();
        let direct = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
        let mut session = WasmCpuSearchSession::new(&problem).unwrap();
        let mut result = None;
        for _ in 0..4096 {
            match session.advance(1, &control).unwrap() {
                WasmCpuSearchAdvance::Pending => {}
                WasmCpuSearchAdvance::Completed(completed) => {
                    result = Some(completed);
                    break;
                }
                WasmCpuSearchAdvance::Cancelled => panic!("no cancellation was requested"),
            }
        }
        let result = result.expect("a bounded forced PC must complete");
        assert_eq!(
            result.normalized_solution_keys(),
            direct.normalized_solution_keys()
        );
        assert_eq!(result.path_steps(), direct.path_steps());
        assert_eq!(
            result.field("build_variant_count"),
            direct.field("build_variant_count")
        );
        assert_eq!(
            result.coverage_pattern_words(),
            direct.coverage_pattern_words()
        );
    }
}

#[test]
fn full_height_score_source_retains_actual_lock_graphs_and_distinct_problem_authority() {
    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        for height in [7, 8, 12, 24] {
            let (query, initial, _) = forced_query(height, rule);
            let query = query.with_objective(ObjectivePolicy::all().with_score_summary());
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            let result =
                WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                    .unwrap();
            let evidence = result
                .pc_score_problem_evidence()
                .expect("executed score problem");
            assert!(evidence.matches_search_problem(&problem));
            assert!(!evidence
                .matches_search_problem(&problem.clone().with_pc_score_portfolio_v2_evidence(),));
            let batch = result
                .full_height_scoring_execution_batch()
                .expect("all physical graphs, not only representative path");
            assert_eq!(batch.height(), height);
            assert_eq!(batch.initial(), initial);
            assert!(batch.execution().complete());
            assert!(!batch.execution().graphs().is_empty());
            for graph in batch.execution().graphs() {
                assert_eq!(graph.candidate_id(), 1);
                assert_eq!(graph.candidate_key(), result.normalized_solution_keys()[0]);
            }
            assert!(result.exact_scoring_execution_batches().is_empty());
            assert!(result.spin_coverage_execution_batches().is_empty());
            assert_eq!(result.bool_field("objective_complete"), Some(false));
            assert!(
                result.checked_resource_retained_bytes().unwrap()
                    >= batch.checked_nested_retained_bytes().unwrap()
            );
            let policy = problem.objective().score();
            let materialized = clearra_postprocess::score_batch::FullHeightScoreCellMaterializer::materialize_with_memory_limit(
                batch, policy, &ExecutionControl::default(),
                result.checked_resource_retained_bytes().unwrap(), 32 * 1024 * 1024,
            ).unwrap();
            assert!(materialized.complete());
            assert_eq!(materialized.cells().len(), 1);
            let cell = &materialized.cells()[0];
            assert_eq!(cell.candidate_id(), 1);
            assert_eq!(cell.pattern_id(), 0);
            // Independently evaluate this forced all-I path. Default T-only
            // spin scoring cannot award any spin to these I pieces.
            let (profile, _) =
                clearra_postprocess::score_profile_with_memory_guard(policy, 0, 1024 * 1024)
                    .unwrap();
            let mut expected = clearra_scoring::model::ScoreModelEvaluator::initial_state(
                clearra_scoring::model::ScoreEvaluationPolicy::tetrio_pc(policy.initial_b2b()),
            );
            for (index, step) in result.path_steps().iter().enumerate() {
                expected = clearra_scoring::model::ScoreModelEvaluator::evaluate_classified_lock(
                    &profile,
                    expected,
                    index,
                    step.cleared_lines(),
                    index + 1 == result.path_steps().len(),
                    None,
                );
            }
            assert_eq!(
                (cell.score(), cell.attack()),
                (expected.score(), expected.attack())
            );
            let matrix = clearra_postprocess::ScoreMatrix::from_materialized_cells(
                materialized.into_cells(),
                &profile,
                batch.execution().patterns().len(),
                true,
            );
            assert!(
                matrix.complete(),
                "the common reducer consumes actual full-height cells"
            );
            let public = result.without_pc_score_transient_evidence();
            assert!(public.full_height_scoring_execution_batch().is_none());
            assert!(public.pc_score_problem_evidence().is_none());
        }
    }
}

#[test]
fn full_height_score_portfolio_retains_both_execution_and_complete_coverage_without_score_only_alias(
) {
    let (query, _, _) = forced_query(24, srs_plus());
    let problem = ProblemCompiler::compile_scenario_pc(
        &query.with_objective(ObjectivePolicy::minimum_cover().with_score_summary()),
    )
    .unwrap()
    .with_pc_score_portfolio_v2_evidence();
    let result =
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap();
    assert!(result
        .pc_score_problem_evidence()
        .unwrap()
        .matches_search_problem(&problem));
    let coverage = result.pc_chance_coverage_evidence().unwrap();
    assert!(coverage.complete());
    assert!(coverage.matches_extended_minimum_source_keys(result.normalized_solution_keys()));
    assert!(result
        .full_height_scoring_execution_batch()
        .unwrap()
        .execution()
        .complete());
    assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
    assert_eq!(result.bool_field("score_summary_complete"), Some(false));
}

#[test]
fn full_height_parent_authorized_score_session_retains_its_terminal_memory_lease() {
    use clearra_core_executor::WasmCpuTerminalResourceAuthority;
    let (query, _, _) = forced_query(24, srs_plus());
    let problem = Arc::new(
        ProblemCompiler::compile_scenario_pc(
            &query.with_objective(ObjectivePolicy::all().with_score_summary()),
        )
        .unwrap(),
    );
    let authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity().unwrap();
    let mut session = WasmCpuSearchSession::new_shared_under_authority(
        Arc::clone(&problem),
        1024 * 1024,
        &authority,
    )
    .unwrap();
    let control = ExecutionControl::default();
    let result = loop {
        match session.advance(64, &control).unwrap() {
            WasmCpuSearchAdvance::Pending => {}
            WasmCpuSearchAdvance::Completed(result) => break result,
            WasmCpuSearchAdvance::Cancelled => panic!("not cancelled"),
        }
    };
    session
        .validate_public_result_memory_with_future(&result, 4096)
        .unwrap();
    assert!(session
        .validate_public_result_memory_with_future(&result, u128::MAX)
        .is_err());
    assert!(result.full_height_scoring_execution_batch().is_some());
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_parallel_score_source_and_common_materializer_keep_the_same_cells() {
    let hardware = std::thread::available_parallelism().map_or(1, usize::from);
    if hardware < 2 {
        return;
    }
    for height in [8, 24] {
        let base = branching_query(height, srs_plus())
            .with_count_policy(PcCountPolicy::CountAll)
            .with_objective(ObjectivePolicy::all().with_score_summary());
        let mut baseline = None;
        for workers in [1, 2] {
            let query = base.clone().with_execution_policy(
                base.execution_policy()
                    .clone()
                    .with_workers(workers)
                    .with_use_all_logical_processors(hardware == 2),
            );
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            let control = ExecutionControl::default();
            let result = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
            assert_eq!(result.usize_field("workers_used"), Some(workers));
            let batch = result.full_height_scoring_execution_batch().unwrap();
            let cells = clearra_postprocess::score_batch::FullHeightScoreCellMaterializer::materialize_with_memory_limit(
                batch, problem.objective().score(), &control,
                result.checked_resource_retained_bytes().unwrap(), 32 * 1024 * 1024,
            ).unwrap();
            assert!(cells.complete());
            assert_eq!(cells.cells().len(), result.normalized_solution_keys().len());
            let signature = cells
                .cells()
                .iter()
                .map(|cell| {
                    (
                        cell.candidate_id(),
                        cell.pattern_id(),
                        cell.trace_identity().to_owned(),
                        cell.score(),
                        cell.attack(),
                    )
                })
                .collect::<Vec<_>>();
            let observed = (result.normalized_solution_keys().to_vec(), signature);
            if let Some(expected) = baseline.as_ref() {
                assert_eq!(&observed, expected);
            } else {
                baseline = Some(observed);
            }
        }
    }
}

#[test]
fn full_height_pc_never_silently_lowers_the_worker_request_or_invents_product_authority() {
    let (query, _, _) = forced_query(24, srs_plus());
    #[cfg(not(all(feature = "parallel", not(target_family = "wasm"))))]
    for workers in [2, 11] {
        let parallel = query.clone().with_execution_policy(
            query
                .execution_policy()
                .clone()
                .with_workers(workers)
                .with_worker_hardware_limit(workers + 1),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&parallel).unwrap();
        let error =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap_err();
        assert_eq!(error.reason(), "extended_pc_family_parallel_not_connected");
    }
    let minimum = ProblemCompiler::compile_scenario_pc(
        &query.with_objective(ObjectivePolicy::minimum_cover()),
    )
    .unwrap();
    let error = WasmCpuSearchBackend::execute_with_control(&minimum, &ExecutionControl::default())
        .unwrap_err();
    assert_eq!(error.reason(), "extended_pc_family_contract_not_connected");
}

#[test]
fn full_height_minimum_source_is_problem_bound_and_deferred_to_the_common_exact_reducer() {
    for height in [7, 8, 12, 24] {
        let (query, _, _) = forced_query(height, srs_plus());
        let query = query
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(ObjectivePolicy::minimum_cover());
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_output_policy(SearchOutputPolicy::Trace)
            .with_pc_minimum_cover_v2_evidence();
        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap();
        let producer = result
            .pc_chance_coverage_evidence()
            .expect("the actual PC producer must retain coverage proof");
        assert!(producer.complete());
        assert!(producer.problem().matches_search_problem(&problem));
        assert!(producer.matches_extended_minimum_source_keys(result.normalized_solution_keys()));
        let mut relabelled = result.normalized_solution_keys().to_vec();
        relabelled[0].push(' ');
        assert!(
            !producer.matches_extended_minimum_source_keys(&relabelled),
            "public keys may not relabel producer-owned coverage proof"
        );
        assert_eq!(producer.row_count(), 1);
        assert_eq!(producer.rows()[0].candidate_id(), 1);
        assert_eq!(
            producer.rows()[0].coverage_bits(),
            result.normalized_solution_coverages()[0].covered_patterns()
        );
        assert_eq!(
            producer.coverage_union().words(),
            result.coverage_pattern_words()
        );
        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(result.bool_field("objective_complete"), Some(false));
        assert_eq!(result.bool_field("objective_search_complete"), Some(true));
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some("deferred-to-coordinator")
        );
        assert!(
            result.normalized_solution_identities().is_empty(),
            "never truncate proof to Board64"
        );
        assert!(result.solution_coverages().is_empty());
        let foreign = ProblemCompiler::compile_scenario_pc(&query.with_rule(srs()))
            .unwrap()
            .with_output_policy(SearchOutputPolicy::Trace)
            .with_pc_minimum_cover_v2_evidence();
        assert!(!producer.problem().matches_search_problem(&foreign));
    }
}

#[test]
fn full_height_chance_uses_complete_problem_bound_build_coverage_without_replay_authority() {
    for height in [7, 8, 12, 24] {
        let (query, _, _) = forced_query(height, srs_plus());
        let query = query
            .with_objective(ObjectivePolicy::unique())
            .with_count_policy(PcCountPolicy::CountUnique);
        let bare = ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_output_policy(SearchOutputPolicy::CoverageSummary);
        assert_eq!(
            WasmCpuSearchBackend::execute_with_control(&bare, &ExecutionControl::default())
                .unwrap_err()
                .reason(),
            "extended_pc_family_contract_not_connected"
        );
        let problem = bare.with_pc_chance_probability_v2_evidence();
        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap();
        let evidence = result.pc_chance_coverage_evidence().unwrap();
        assert!(evidence.complete());
        assert!(evidence.problem().matches_search_problem(&problem));
        assert_eq!(evidence.row_count(), 1);
        assert_eq!(
            evidence.coverage_union().words(),
            result.coverage_pattern_words()
        );
        assert_eq!(result.field("coverage_probability"), Some("1"));
        assert_eq!(
            result.field("search_output_policy"),
            Some("coverage-summary")
        );
        assert_eq!(result.bool_field("objective_complete"), Some(true));
        assert!(result.exact_scoring_execution_batches().is_empty());
        assert!(!evidence.matches_extended_minimum_source_keys(result.normalized_solution_keys()));
    }
}

#[test]
fn full_height_failed_queue_owns_the_executed_problem_and_never_borrows_chance_authority() {
    for height in [7, 8, 12, 24] {
        let (query, initial, pieces) = forced_query(height, srs_plus());
        for succeeds in [true, false] {
            let query = if succeeds {
                query.clone()
            } else {
                PcScenarioQuery::new(
                    PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words())
                        .unwrap(),
                    PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; pieces])),
                    PieceWindow::new(pieces),
                )
                .with_rule(srs_plus())
                .with_exact_pieces(Some(pieces))
                .with_allow_hold(false)
                .with_min_remaining_queue(0)
                .with_execution_policy(query.execution_policy().clone())
            };
            let query = query
                .with_objective(ObjectivePolicy::all())
                .with_count_policy(PcCountPolicy::CountAll);
            let bare = ProblemCompiler::compile_scenario_percent(&query).unwrap();
            assert!(WasmPcFailedQueueSession::new(Arc::new(
                bare.clone().with_pc_chance_probability_v2_evidence()
            ))
            .is_err());
            let problem = Arc::new(bare.with_pc_failed_queue_v2_evidence(1));
            let mut session = WasmPcFailedQueueSession::new(Arc::clone(&problem)).unwrap();
            let control = ExecutionControl::default();
            let mut completed = None;
            for _ in 0..4096 {
                match session.advance(1, &control).unwrap() {
                    WasmPcFailedQueueAdvance::Pending => {}
                    WasmPcFailedQueueAdvance::Completed(result, evidence) => {
                        completed = Some((result, evidence));
                        break;
                    }
                    WasmPcFailedQueueAdvance::Cancelled => panic!("not cancelled"),
                }
            }
            let (result, evidence) = completed.expect("bounded full-height request");
            assert!(evidence.matches_problem_owner(&problem));
            assert_eq!(evidence.success_pattern_count(), usize::from(succeeds));
            assert_eq!(evidence.failed_pattern_count(), usize::from(!succeeds));
            assert_eq!(evidence.examples().len(), usize::from(!succeeds));
            assert_eq!(
                result.coverage_pattern_words(),
                evidence.success_coverage().words()
            );
            assert_eq!(result.field("status"), Some("percent-executed"));
            assert!(
                result.pc_chance_coverage_evidence().is_none(),
                "the consumed private source must not escape"
            );
            assert!(result.exact_scoring_execution_batches().is_empty());
            assert!(evidence.memory_report().admitted_producer_peak_bytes() > 0);
            assert!(
                session.advance(1, &control).is_err(),
                "terminal must be one-shot"
            );
        }
    }
}

#[test]
fn full_height_failed_queue_cancellation_never_returns_unsat_or_a_failure_list() {
    let (query, _, _) = forced_query(24, srs_plus());
    let query = query
        .with_objective(ObjectivePolicy::all())
        .with_count_policy(PcCountPolicy::CountAll);
    let problem = Arc::new(
        ProblemCompiler::compile_scenario_percent(&query)
            .unwrap()
            .with_pc_failed_queue_v2_evidence(1),
    );
    let mut session = WasmPcFailedQueueSession::new(problem).unwrap();
    let control = ExecutionControl::default();
    control.cancellation.handle().cancel();
    assert!(matches!(
        session.advance(1, &control).unwrap(),
        WasmPcFailedQueueAdvance::Cancelled
    ));
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn branching_query(height: u8, rule: RuleProfile) -> PcScenarioQuery {
    let mut holes = Board256Mask::EMPTY;
    for row in 0..4 {
        for column in 0..4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
        }
    }
    let tail = if height == 7 { 3 } else { 4 };
    for row in tail..u16::from(height) {
        let column = 5 + (row - tail) / 4;
        holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
    }
    let pieces = holes.count_ones() as usize / 4;
    let initial = Board256Mask::all_cells(u16::from(height) * 10)
        .unwrap()
        .without(holes);
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
        PieceWindow::new(pieces),
    )
    .with_rule(rule)
    .with_exact_pieces(Some(pieces))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_allow_backend_fallback(false),
    )
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_native_parallel_failed_queue_keeps_complete_source_coverage_and_worker_identity() {
    let hardware = std::thread::available_parallelism().map_or(1, usize::from);
    if hardware < 2 {
        return;
    }
    for height in [8, 24] {
        let query = branching_query(height, srs_plus())
            .with_objective(ObjectivePolicy::all())
            .with_count_policy(PcCountPolicy::CountAll);
        let mut serial_coverage = None;
        for workers in [1, 2] {
            let query = query.clone().with_execution_policy(
                query
                    .execution_policy()
                    .clone()
                    .with_workers(workers)
                    .with_use_all_logical_processors(hardware == 2),
            );
            let problem = Arc::new(
                ProblemCompiler::compile_scenario_percent(&query)
                    .unwrap()
                    .with_pc_failed_queue_v2_evidence(1),
            );
            let mut session = WasmPcFailedQueueSession::new(Arc::clone(&problem)).unwrap();
            let control = ExecutionControl::default();
            let mut completed = None;
            for _ in 0..4096 {
                match session.advance(1, &control).unwrap() {
                    WasmPcFailedQueueAdvance::Pending => {}
                    WasmPcFailedQueueAdvance::Completed(result, evidence) => {
                        completed = Some((result, evidence));
                        break;
                    }
                    WasmPcFailedQueueAdvance::Cancelled => panic!("not cancelled"),
                }
            }
            let (result, evidence) = completed.expect("bounded branching PC request");
            assert_eq!(result.usize_field("workers_used"), Some(workers));
            assert!(evidence.matches_problem_owner(&problem));
            assert_eq!(evidence.success_pattern_count(), 1);
            assert_eq!(evidence.failed_pattern_count(), 0);
            let coverage = evidence.success_coverage().words().to_vec();
            if let Some(serial) = &serial_coverage {
                assert_eq!(&coverage, serial);
            } else {
                serial_coverage = Some(coverage);
            }
        }
    }
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_native_parallel_minimum_retains_the_same_canonical_dictionary_and_coverage_proof() {
    let hardware = std::thread::available_parallelism().map_or(1, usize::from);
    if hardware < 2 {
        return;
    }
    for height in [8, 24] {
        let base = branching_query(height, srs_plus())
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(ObjectivePolicy::minimum_cover());
        let serial_problem = ProblemCompiler::compile_scenario_pc(&base)
            .unwrap()
            .with_output_policy(SearchOutputPolicy::Trace)
            .with_pc_minimum_cover_v2_evidence();
        let serial = WasmCpuSearchBackend::execute_with_control(
            &serial_problem,
            &ExecutionControl::default(),
        )
        .unwrap();
        let parallel_query = base.clone().with_execution_policy(
            base.execution_policy()
                .clone()
                .with_workers(2)
                .with_use_all_logical_processors(hardware == 2),
        );
        let parallel_problem = ProblemCompiler::compile_scenario_pc(&parallel_query)
            .unwrap()
            .with_output_policy(SearchOutputPolicy::Trace)
            .with_pc_minimum_cover_v2_evidence();
        let parallel = WasmCpuSearchBackend::execute_with_control(
            &parallel_problem,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(serial.normalized_solution_keys().len() >= 2);
        assert_eq!(parallel.usize_field("workers_used"), Some(2));
        assert_eq!(
            parallel.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            parallel.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        let producer = parallel.pc_chance_coverage_evidence().unwrap();
        assert!(producer.complete());
        assert!(producer.problem().matches_search_problem(&parallel_problem));
        assert!(producer.matches_extended_minimum_source_keys(serial.normalized_solution_keys()));
        assert_eq!(
            producer.rows(),
            serial.pc_chance_coverage_evidence().unwrap().rows()
        );
    }
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_native_parallel_pc_preserves_buildup_counts_coverage_and_canonical_witness() {
    use clearra_pc_graph::request::PcSolutionProbabilityPolicy;
    let logical_processors = std::thread::available_parallelism().map_or(1, usize::from);
    if logical_processors < 2 {
        return;
    }
    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        for height in [7, 8, 12, 24] {
            for count_policy in [PcCountPolicy::CountAll, PcCountPolicy::CountUnique] {
                let base = branching_query(height, rule)
                    .with_count_policy(count_policy)
                    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
                let serial_problem = ProblemCompiler::compile_scenario_pc(&base).unwrap();
                let serial = WasmCpuSearchBackend::execute_with_control(
                    &serial_problem,
                    &ExecutionControl::default(),
                )
                .unwrap();
                let parallel_query = base.clone().with_execution_policy(
                    base.execution_policy()
                        .clone()
                        .with_workers(2)
                        .with_use_all_logical_processors(logical_processors == 2),
                );
                let parallel_problem =
                    ProblemCompiler::compile_scenario_pc(&parallel_query).unwrap();
                let parallel = WasmCpuSearchBackend::execute_with_control(
                    &parallel_problem,
                    &ExecutionControl::default(),
                )
                .unwrap();
                assert_eq!(parallel.field("workers_requested"), Some("2"));
                assert_eq!(parallel.usize_field("workers_used"), Some(2));
                assert_eq!(parallel.usize_field("cpu_parallel_active_workers"), Some(2));
                assert_eq!(parallel.bool_field("cpu_parallel_execution"), Some(true));
                for name in [
                    "normalized_solution_set_hash",
                    "build_variant_count",
                    "build_variant_count_exact",
                    "packing_candidate_count",
                    "packing_candidate_digest",
                    "count_complete",
                    "probability_complete",
                    "coverage_probability",
                ] {
                    assert_eq!(parallel.field(name), serial.field(name), "{height} {name}");
                }
                assert_eq!(
                    parallel.normalized_solution_keys(),
                    serial.normalized_solution_keys()
                );
                assert_eq!(
                    parallel.normalized_solution_coverages(),
                    serial.normalized_solution_coverages()
                );
                assert_eq!(
                    parallel.solution_probabilities(),
                    serial.solution_probabilities()
                );
                assert_eq!(
                    parallel.coverage_pattern_words(),
                    serial.coverage_pattern_words()
                );
                assert_eq!(parallel.path_steps(), serial.path_steps());
                assert!(parallel.tiling_solution_page_store().is_none());
                assert!(parallel.pc_chance_coverage_evidence().is_none());
                assert!(parallel.exact_scoring_execution_batches().is_empty());
            }
        }
    }
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_native_parallel_cooperative_path_does_not_become_a_serial_one() {
    let logical_processors = std::thread::available_parallelism().map_or(1, usize::from);
    if logical_processors < 2 {
        return;
    }
    let base = branching_query(24, srs_plus()).with_count_policy(PcCountPolicy::CountAll);
    let query = base.clone().with_execution_policy(
        base.execution_policy()
            .clone()
            .with_workers(2)
            .with_use_all_logical_processors(logical_processors == 2),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let expected =
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap();
    let mut session = WasmCpuSearchSession::new(&problem).unwrap();
    match session.advance(1, &ExecutionControl::default()).unwrap() {
        WasmCpuSearchAdvance::Completed(result) => {
            assert_eq!(result.usize_field("workers_used"), Some(2));
            assert_eq!(
                result.normalized_solution_keys(),
                expected.normalized_solution_keys()
            );
            assert_eq!(result.path_steps(), expected.path_steps());
        }
        other => {
            panic!("native parallel execution must use the admitted caller/pool path: {other:?}")
        }
    }
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_parallel_node_budget_is_not_duplicated_per_worker() {
    let base = branching_query(24, srs_plus());
    let query = base.clone().with_execution_policy(
        base.execution_policy()
            .clone()
            .with_workers(2)
            .with_worker_hardware_limit(3)
            .with_max_nodes(1),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let error = match WasmCpuSearchSession::new(&problem) {
        Ok(_) => panic!("finite node credit is not yet a per-worker allowance"),
        Err(error) => error,
    };
    assert_eq!(
        error.reason(),
        "extended_pc_family_parallel_node_budget_not_connected"
    );
}

#[test]
fn full_height_pc_cancellation_precedes_result_publication() {
    let (query, _, _) = forced_query(24, srs_plus());
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let control = ExecutionControl::default();
    control.cancellation.handle().cancel();
    let error = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap_err();
    assert_eq!(error.reason(), "wasm_cpu_search_cancelled");
}

#[test]
fn empty_extended_opening_reaches_finite_catalog_admission_instead_of_a_false_capability() {
    for lines in [8, 24] {
        let query = OpeningPcSearchQuery::new(PcTarget::new(lines).unwrap())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I;
                usize::from(lines) * 10 / 4
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_execution_policy(
                PcExecutionPolicy::default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_workers(1)
                    .with_max_memory_mib(Some(1)),
            );
        let problem = ProblemCompiler::compile_opening_pc(&query).unwrap();
        // Deliberately censor this enormous empty-origin family at a finite
        // catalog boundary. A resource refusal is NOT a completed PC proof.
        let error = match WasmCpuSearchSession::new(&problem) {
            Ok(_) => panic!("a one-MiB empty-origin catalog must not be admitted"),
            Err(error) => error,
        };
        assert!(
            matches!(error, WasmCpuSearchError::ResourceAdmission { .. }),
            "target {lines}: {error:?}"
        );
    }
}

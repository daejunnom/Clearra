//! Bounded actual PC completion, not an empty 24L/sixty-piece enumeration.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    pc::pc_target::PcTarget, piece::piece_kind::PieceKind, solution::ExtendedTilingSolutionKey,
};
use clearra_core_executor::backend::{
    WasmCpuSearchAdvance, WasmCpuSearchBackend, WasmCpuSearchError, WasmCpuSearchSession,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::{
    builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
    rule_profile::RuleProfile,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

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
    .unwrap()
    .with_pc_minimum_cover_v2_evidence();
    let error = WasmCpuSearchBackend::execute_with_control(&minimum, &ExecutionControl::default())
        .unwrap_err();
    assert_eq!(error.reason(), "extended_pc_family_contract_not_connected");
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

// SRP rationale: this module has one change reason: executable behavior coverage for the PC service boundary.
#![cfg_attr(not(feature = "native-c-core"), allow(dead_code, unused_imports))]

use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::service::pc_service::PcService;

#[cfg(feature = "native-c-core")]
#[test]
fn scenario_result_exposes_terminal_supply_and_explicit_solution_availability() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0x1c0701c07),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::S,
            PieceKind::T,
            PieceKind::O,
            PieceKind::I,
            PieceKind::L,
            PieceKind::J,
            PieceKind::Z,
        ])),
        PieceWindow::new(7),
    )
    .with_exact_pieces(Some(7));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let result = PcService::execute(&problem).expect("execution");
    let availability = result.execution_report().solution_set_availability();

    assert_eq!(result.field("search_output_policy"), Some("trace"));
    assert_eq!(
        result.field("supply_window_resolution"),
        Some("projected-terminal-lookahead")
    );
    assert_eq!(result.bool_field("projects_unplaced_lookahead"), Some(true));
    assert_eq!(
        result.bool_field("projects_standard_bag_lookahead"),
        Some(false)
    );
    assert_eq!(result.usize_field("source_sequence_length"), Some(7));
    assert_eq!(result.field("total_possible_pattern_count"), Some("1"));
    assert!(availability.uses_explicit_contract());
    assert!(availability.contract_valid());
    assert!(availability.solution_count_calculated());
    assert!(availability.solution_set_materialized());
    assert_eq!(
        availability.solution_keys_materialized_count(),
        result.normalized_solution_keys().len()
    );
    assert!(availability.solution_keys_complete());
    assert!(!availability.solution_page_available());
}

#[cfg(feature = "native-c-core")]
mod case_pc_service_runs_search_problem_through_packing_buildup_coverage_and_output_model {
    use super::*;

    #[test]
    fn pc_service_runs_search_problem_through_packing_buildup_coverage_and_output_model() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");
        let fields = result.summary_fields();

        assert!(fields.contains(&("problem_layer".to_owned(), "clearra-problem".to_owned())));
        assert!(fields.contains(&(
            "executor_layer".to_owned(),
            "clearra-core-executor".to_owned()
        )));
        assert!(fields.contains(&("packing_runner".to_owned(), "PackingRunner::run".to_owned())));
        assert!(fields.contains(&("buildup_runner".to_owned(), "BuildUpRunner::run".to_owned())));
        assert!(fields.contains(&("buildup_result".to_owned(), "C BuildUpResult".to_owned())));
        assert!(fields.contains(&(
            "packing_candidate_is_solution".to_owned(),
            "false".to_owned()
        )));
        assert!(fields.contains(&(
            "rust_objective_reducer".to_owned(),
            "ObjectiveReducer::reduce".to_owned()
        )));
        assert!(fields.contains(&(
            "rust_output_model".to_owned(),
            "CoreExecutionResult".to_owned()
        )));
        assert!(fields.contains(&(
            "solver_backend".to_owned(),
            expected_solver_backend().to_owned()
        )));
        assert!(fields.contains(&(
            "packing_execution_source".to_owned(),
            expected_packing_execution_source().to_owned()
        )));
        assert!(fields.contains(&(
            "buildup_execution_source".to_owned(),
            expected_buildup_execution_source().to_owned()
        )));
        assert!(fields.contains(&(
            "native_c_core_executed".to_owned(),
            expected_native_c_core_executed().to_owned()
        )));
        assert!(fields.contains(&(
            "native_c_core_fallback_policy".to_owned(),
            expected_native_c_core_fallback_policy().to_owned()
        )));
        assert!(fields.contains(&("chain_class".to_owned(), "opening-2l".to_owned())));
        assert!(fields.contains(&("chain_labels".to_owned(), "2L".to_owned())));
        assert!(fields.contains(&(
            "exact_target_policy".to_owned(),
            "2L-label-clear-to-empty".to_owned()
        )));
        assert!(fields.contains(&(
            "checkpoint_results".to_owned(),
            "not-executed-label-metadata".to_owned()
        )));
        assert!(fields.contains(&(
            "checkpoint_schedule_source".to_owned(),
            "clearra-pc-graph-labels".to_owned()
        )));
        assert!(fields.contains(&("checkpoint_schedule_partitions".to_owned(), "2".to_owned())));
        assert!(fields.contains(&(
            "compact_piece_source_kind".to_owned(),
            clearra_core_ffi::supply::C_PIECE_SOURCE_FIXED_QUEUE.to_string()
        )));
        let compact_supply_provenance_id = field_value(&fields, "compact_supply_provenance_id")
            .and_then(|value| value.parse::<u32>().ok())
            .expect("compact supply provenance id");
        assert_ne!(compact_supply_provenance_id, 0);
        assert!(fields.contains(&(
            "gpu_backend_scope".to_owned(),
            "native-gpu-packing".to_owned()
        )));
        assert!(fields.contains(&("gpu_larger_batch_planner".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_dominance_prefilter".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_shape_union_mask".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_readback_compression".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_result_deterministic".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_backend_available".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_result_cpu_confirmed".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_cpu_reference_match".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("hybrid_scheduler".to_owned(), "false".to_owned())));
        assert!(fields.contains(&(
            "hybrid_gpu_readback_cpu_buildup_overlap".to_owned(),
            "false".to_owned()
        )));
        assert!(fields.contains(&(
            "hybrid_backend_metrics_reported".to_owned(),
            "false".to_owned()
        )));
        assert!(fields.contains(&(
            "hybrid_memory_leak_report_clean".to_owned(),
            "true".to_owned()
        )));
        assert!(fields.contains(&("coverage_pattern_count".to_owned(), "1".to_owned())));
        assert!(fields.contains(&(
            "covered_pattern_count".to_owned(),
            expected_covered_pattern_count().to_owned()
        )));
        assert!(fields.contains(&(
            "probability_complete".to_owned(),
            expected_probability_complete().to_owned()
        )));
        assert!(fields.contains(&(
            "coverage_probability".to_owned(),
            expected_coverage_probability().to_owned()
        )));
        assert_eq!(
            result
                .execution_report()
                .backend_report()
                .backend_requested(),
            "auto"
        );
    }
}

#[cfg(feature = "native-c-core")]
mod case_tiling_service_materializes_raw_geometry_without_buildup {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::*;
    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        solution::{NormalizedTilingSolutionKey, NormalizedTilingSolutionSet},
    };
    use clearra_core_ffi::{
        problem::CPackingProblem, CBuildUpProblemBuilder, CNativeBuildUpEnumerationLimits,
        CoreCNative, NativePackingOutcome, PackingCandidateBatch,
    };
    use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
    use clearra_pc_graph::request::{
        BackendFallbackPolicy, GpuDeviceSelection, PcExecutionPolicy, PcHoldPolicy,
        RequestedSearchBackend,
    };

    use crate::{
        backend::{
            BackendTrustReport, CapabilityQueryError, GpuSearchCapability, GpuUnavailableReason,
            NativePackingExecutorRegistry, PackingBackendOutcome, PackingCandidateProvenance,
            SearchBackendCapabilityProvider, SearchBackendExecutor, SearchBackendExecutorResolver,
            SearchBackendFallbackReason, SelectedSearchBackend,
        },
        packing::{PackingRunner, PackingRunnerError},
        service::{
            pc_service::PcService,
            pc_tiling_materialization::{PcTilingMaterialization, PcTilingMaterializationError},
            PcServiceError,
        },
    };

    #[derive(Clone, Copy)]
    struct NoGpuCapability;

    impl SearchBackendCapabilityProvider for NoGpuCapability {
        fn gpu_capability(
            &self,
            _device: GpuDeviceSelection,
        ) -> Result<GpuSearchCapability, CapabilityQueryError> {
            Ok(GpuSearchCapability::unavailable(
                GpuUnavailableReason::DeviceNotFound,
            ))
        }

        fn prepared_gpu_capability(
            &self,
            _device: GpuDeviceSelection,
        ) -> Result<GpuSearchCapability, CapabilityQueryError> {
            self.gpu_capability(GpuDeviceSelection::Auto)
        }
    }

    #[derive(Clone, Copy)]
    struct ConnectedGpuCapability;

    impl SearchBackendCapabilityProvider for ConnectedGpuCapability {
        fn gpu_capability(
            &self,
            _device: GpuDeviceSelection,
        ) -> Result<GpuSearchCapability, CapabilityQueryError> {
            Ok(GpuSearchCapability::available(0))
        }

        fn prepared_gpu_capability(
            &self,
            _device: GpuDeviceSelection,
        ) -> Result<GpuSearchCapability, CapabilityQueryError> {
            self.gpu_capability(GpuDeviceSelection::Auto)
        }
    }

    fn native_execution_test_guard() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[derive(Clone, Copy)]
    struct RawGeometryExecutor {
        repeat_first_candidate: Option<usize>,
    }

    impl RawGeometryExecutor {
        const fn native() -> Self {
            Self {
                repeat_first_candidate: None,
            }
        }

        const fn repeated(repeat_first_candidate: usize) -> Self {
            Self {
                repeat_first_candidate: Some(repeat_first_candidate),
            }
        }
    }

    impl SearchBackendExecutor for RawGeometryExecutor {
        fn execute_packing(
            &self,
            problem: &CPackingProblem,
            _policy: &PcExecutionPolicy,
            _cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
        ) -> Result<PackingBackendOutcome, PackingRunnerError> {
            let NativePackingOutcome {
                mut candidates,
                resource_report,
                ..
            } = CoreCNative::generate_packing_candidates(problem)
                .map_err(PackingRunnerError::Native)?;
            if let Some(repeat_count) = self.repeat_first_candidate {
                let first = candidates
                    .candidate_at(0)
                    .expect("raw geometry fixture has a candidate");
                candidates = PackingCandidateBatch::from_candidates(
                    candidates.board_width(),
                    candidates.board_height(),
                    (0..repeat_count).map(|candidate_index| {
                        let mut candidate = first;
                        candidate.final_board = candidate_index as u64;
                        candidate
                    }),
                )
                .map_err(PackingRunnerError::CandidateBatch)?;
            }
            Ok(PackingBackendOutcome::raw_geometry_exact(
                SelectedSearchBackend::CpuGeometryExactCover,
                candidates,
                resource_report,
                BackendTrustReport::cpu_exact(),
            ))
        }
    }

    struct RawGeometryResolver(RawGeometryExecutor);

    impl SearchBackendExecutorResolver for RawGeometryResolver {
        fn executor_for(
            &self,
            backend: SelectedSearchBackend,
        ) -> Option<&dyn SearchBackendExecutor> {
            matches!(
                backend,
                SelectedSearchBackend::CpuGeometryExactCover
                    | SelectedSearchBackend::CpuParallelGeometryExactCover
            )
            .then_some(&self.0)
        }

        fn cpu_fallback_executor(&self) -> &dyn SearchBackendExecutor {
            &self.0
        }

        fn supports_native_candidate_streaming(&self) -> bool {
            true
        }

        fn use_resolved_executor_for_test(&self) -> bool {
            true
        }
    }

    struct UnadmittedRawGeometryResolver(RawGeometryExecutor);

    impl SearchBackendExecutorResolver for UnadmittedRawGeometryResolver {
        fn executor_for(
            &self,
            backend: SelectedSearchBackend,
        ) -> Option<&dyn SearchBackendExecutor> {
            matches!(
                backend,
                SelectedSearchBackend::CpuGeometryExactCover
                    | SelectedSearchBackend::CpuParallelGeometryExactCover
            )
            .then_some(&self.0)
        }

        fn cpu_fallback_executor(&self) -> &dyn SearchBackendExecutor {
            &self.0
        }
    }

    fn tiling_problem_with_objective(objective: ObjectivePolicy) -> clearra_problem::SearchProblem {
        tiling_problem_with_objective_and_policy(
            objective,
            PcExecutionPolicy::mvp_default()
                .with_workers(1)
                .with_worker_hardware_limit(1)
                .with_max_candidates(5_000),
        )
    }

    fn tiling_problem_with_objective_and_policy(
        objective: ObjectivePolicy,
        policy: PcExecutionPolicy,
    ) -> clearra_problem::SearchProblem {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_objective(objective)
            .with_execution_policy(policy);
        ProblemCompiler::compile_opening_pc(&query).expect("tiling problem")
    }

    fn canonical_tiling_problem_with_policy(
        policy: PcExecutionPolicy,
    ) -> clearra_problem::SearchProblem {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::tiling())
            .with_execution_policy(policy);
        ProblemCompiler::compile_opening_pc_tiling(&query).expect("canonical tiling problem")
    }

    fn tiling_problem() -> clearra_problem::SearchProblem {
        canonical_tiling_problem_with_policy(
            PcExecutionPolicy::mvp_default()
                .with_workers(1)
                .with_worker_hardware_limit(1)
                .with_max_candidates(5_000),
        )
    }

    fn raw_geometry_packing(
        problem: &clearra_problem::SearchProblem,
        executor: RawGeometryExecutor,
    ) -> crate::packing::PackingRunResult {
        PackingRunner::run_with_components(
            problem,
            &NoGpuCapability,
            &RawGeometryResolver(executor),
        )
        .expect("raw geometry packing")
    }

    fn independently_enumerated_raw_geometry(
        problem: &clearra_problem::SearchProblem,
    ) -> (
        crate::packing::PackingRunResult,
        NormalizedTilingSolutionSet,
        NormalizedTilingSolutionKey,
    ) {
        let packing = raw_geometry_packing(problem, RawGeometryExecutor::native());
        let mut independent_keys = Vec::new();
        let mut rejected_key = None;
        for candidate_index in 0..packing.candidate_count() {
            let candidate = packing
                .candidate_at(candidate_index)
                .expect("raw geometry candidate");
            let identity = candidate
                .standard_board64_tiling_identity(problem.initial_board().occupied_mask())
                .expect("raw geometry identity");
            let key = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
            independent_keys.push(key.clone());

            let buildable = CBuildUpProblemBuilder::from_packing_candidate(
                problem,
                &candidate,
                u32::try_from(candidate_index).expect("candidate index fits native builder"),
                0,
            )
            .ok()
            .and_then(|buildup| {
                CoreCNative::enumerate_buildup_variants(
                    &buildup,
                    &CNativeBuildUpEnumerationLimits::default(),
                )
                .ok()
            })
            .is_some_and(|outcome| outcome.accepted());
            if !buildable && rejected_key.is_none() {
                rejected_key = Some(key);
            }
        }

        (
            packing,
            NormalizedTilingSolutionSet::new(independent_keys),
            rejected_key.expect("fixture includes a BuildUp-rejected raw tiling"),
        )
    }

    #[test]
    fn raw_geometry_candidate_is_materialized_even_when_buildup_rejects_it() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem();
        let (packing, independent_set, rejected_key) =
            independently_enumerated_raw_geometry(&problem);
        let expected_keys = independent_set
            .keys()
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>();

        let result = PcService::finish_with_packing_for_test(&problem, packing)
            .expect("tiling materialization");

        assert_eq!(result.field("buildup_runner"), Some("not-executed"));
        assert_eq!(result.bool_field("buildup_executed"), Some(false));
        assert_eq!(
            result.bool_field("additional_buildup_executed"),
            Some(false)
        );
        assert_eq!(result.field("buildup_backend"), Some("none"));
        assert_eq!(result.field("buildup_backend_owner"), Some("none"));
        assert_eq!(result.usize_field("buildup_workspace_bytes"), Some(0));
        assert_eq!(
            result.field("buildup_workspace_accounting_basis"),
            Some("none-no-buildup")
        );
        assert_eq!(
            result.usize_field("resource_buildup_workspace_bytes"),
            Some(0)
        );
        assert_eq!(
            result.field("resource_buildup_workspace_accounting_basis"),
            Some("none-no-buildup")
        );
        assert_eq!(
            result.bool_field("packing_candidate_is_solution"),
            Some(true)
        );
        assert_eq!(result.bool_field("packing_source_raw_geometry"), Some(true));
        assert_eq!(
            result.bool_field("tiling_materialization_complete"),
            Some(true)
        );
        assert_eq!(result.bool_field("tiling_objective_canonical"), Some(true));
        assert_eq!(
            result.bool_field("tiling_materialization_memory_admission_accounted"),
            Some(true)
        );
        assert_eq!(
            result.pc_tiling_memory_admission_evidence(),
            Some(crate::PcTilingMemoryAdmissionEvidence::NativeInternal)
        );
        assert!(result.pc_tiling_family_publication_contract_is_valid());
        assert_eq!(
            result.field("tiling_materialization_incomplete_reason"),
            Some("none")
        );
        assert_eq!(result.bool_field("buildability_verified"), Some(false));
        assert_eq!(result.bool_field("coverage_calculated"), Some(false));
        assert_eq!(result.bool_field("probability_calculated"), Some(false));
        assert_eq!(result.field_occurrence_count("probability_calculated"), 1);
        assert_eq!(result.field("coverage_probability"), Some("not-calculated"));
        assert_eq!(
            result.usize_field("normalized_unique_solution_count"),
            Some(independent_set.len())
        );
        assert_eq!(result.normalized_solution_keys(), expected_keys.as_slice());
        assert!(result
            .normalized_solution_keys()
            .iter()
            .any(|key| key == rejected_key.as_str()));
        assert_eq!(
            result.field("normalized_solution_set_hash"),
            Some(independent_set.hash())
        );
        assert_eq!(
            result.usize_field("solution_keys_materialized_count"),
            Some(result.normalized_solution_keys().len())
        );
        assert_eq!(result.bool_field("solution_set_materialized"), Some(true));
        assert_eq!(result.bool_field("solution_keys_complete"), Some(true));
        let page_store = result
            .tiling_solution_page_store()
            .expect("complete tiling page authority");
        assert_eq!(page_store.len(), independent_set.len());
        assert_eq!(page_store.normalized_hash(), independent_set.hash());
        assert_eq!(
            page_store.page_keys(0, 100).expect("initial tiling page"),
            expected_keys
        );
        assert_eq!(
            result.usize_field("tiling_initial_page_count"),
            Some(independent_set.len())
        );
        assert_eq!(result.bool_field("tiling_family_complete"), Some(true));
        assert_eq!(
            result.bool_field("tiling_initial_page_covers_family"),
            Some(true)
        );
    }

    #[test]
    fn production_raw_geometry_stream_is_complete_without_running_buildup() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem();
        let (_, _, rejected_key) = independently_enumerated_raw_geometry(&problem);
        let result = PcService::execute(&problem).expect("native tiling execution");

        assert_eq!(result.bool_field("buildup_executed"), Some(false));
        assert_eq!(
            result.bool_field("additional_buildup_executed"),
            Some(false)
        );
        assert_eq!(result.field("buildup_backend"), Some("none"));
        assert_eq!(result.field("buildup_backend_owner"), Some("none"));
        assert_eq!(result.bool_field("packing_source_raw_geometry"), Some(true));
        assert_eq!(
            result.bool_field("packing_source_buildability_preverified"),
            Some(false)
        );
        assert_eq!(
            result.bool_field("tiling_materialization_complete"),
            Some(true)
        );
        assert_eq!(
            result.field("tiling_materialization_incomplete_reason"),
            Some("none")
        );
        assert_eq!(result.bool_field("buildability_verified"), Some(false));
        assert_eq!(result.usize_field("buildup_workspace_bytes"), Some(0));
        assert_eq!(
            result.field("buildup_workspace_accounting_basis"),
            Some("none-no-buildup")
        );
        assert_eq!(
            result.usize_field("resource_buildup_workspace_bytes"),
            Some(0)
        );
        assert_eq!(
            result.field("resource_buildup_workspace_accounting_basis"),
            Some("none-no-buildup")
        );
        assert_eq!(result.bool_field("count_complete"), Some(true));
        assert_eq!(result.bool_field("solution_keys_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(result.field("resource_truncation_reason"), Some("none"));
        assert_eq!(result.usize_field("execution_workers"), Some(1));
        assert_eq!(result.field("selected_model"), Some("bitset-algorithm-x"));
        assert_eq!(
            result.field("packing_algorithm"),
            Some("geometry-exact-cover-candidate-materialization")
        );
        assert!(result
            .normalized_solution_keys()
            .iter()
            .any(|key| key == rejected_key.as_str()));
    }

    #[test]
    fn scenario_pc_canonical_tiling_uses_the_same_product_raw_geometry_stream() {
        let _execution_guard = native_execution_test_guard();
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
            PieceWindow::new(5),
        )
        .with_exact_pieces(Some(5))
        .with_allow_hold(false)
        .with_objective(ObjectivePolicy::tiling())
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_workers(2)
                .with_worker_hardware_limit(2)
                .with_max_candidates(5_000),
        );
        let problem = ProblemCompiler::compile_scenario_pc_tiling(&query)
            .expect("canonical scenario tiling problem");
        let result = PcService::execute(&problem).expect("scenario native tiling execution");

        assert_eq!(result.bool_field("packing_source_raw_geometry"), Some(true));
        assert_eq!(result.bool_field("buildup_executed"), Some(false));
        assert_eq!(
            result.bool_field("tiling_materialization_complete"),
            Some(true)
        );
        assert_eq!(result.usize_field("workers_used"), Some(1));
        assert_eq!(
            result.field("selected_backend"),
            Some("cpu-geometry-exact-cover")
        );
        assert_eq!(result.field("selected_model"), Some("bitset-algorithm-x"));
        assert_eq!(
            result.field("backend_selection_reason"),
            Some("auto-small-scenario-cpu-geometry-exact-cover")
        );
    }

    #[test]
    fn noncanonical_tiling_objective_keeps_buildable_provenance_and_is_incomplete() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem_with_objective(ObjectivePolicy::tiling().with_score_summary());
        let result =
            PcService::execute(&problem).expect("incomplete tiling materialization result");
        assert_eq!(
            result.bool_field("packing_source_raw_geometry"),
            Some(false)
        );
        assert_eq!(
            result.bool_field("packing_source_buildability_preverified"),
            Some(true)
        );
        assert_eq!(result.bool_field("tiling_objective_canonical"), Some(false));
        assert_eq!(
            result.bool_field("tiling_materialization_complete"),
            Some(false)
        );
        assert_eq!(
            result.field("tiling_materialization_incomplete_reason"),
            Some("noncanonical-tiling-objective")
        );
        assert_eq!(result.bool_field("objective_complete"), Some(false));
        assert_eq!(
            result.bool_field("postprocess_scoring_requested"),
            Some(true)
        );
        assert_eq!(
            result.bool_field("postprocess_execution_complete"),
            Some(false)
        );
        assert_eq!(result.bool_field("buildup_executed"), Some(true));
        assert_eq!(
            result.bool_field("additional_buildup_executed"),
            Some(false)
        );
    }

    #[test]
    fn generic_tiling_trace_never_receives_native_pc_tiling_authority() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem_with_objective(ObjectivePolicy::tiling());
        let result = PcService::execute(&problem).expect("generic tiling execution");

        assert_eq!(result.field("search_output_policy"), Some("trace"));
        assert_eq!(result.bool_field("tiling_objective_canonical"), Some(false));
        assert_eq!(
            result.bool_field("tiling_materialization_complete"),
            Some(false)
        );
        assert_eq!(
            result.field("tiling_materialization_incomplete_reason"),
            Some("noncanonical-tiling-objective")
        );
        assert_eq!(result.field("coverage_probability"), Some("not-calculated"));
        assert_eq!(result.bool_field("probability_calculated"), Some(false));
        assert_eq!(result.field_occurrence_count("probability_calculated"), 1);
        assert_eq!(result.bool_field("probability_complete"), Some(false));
        assert_eq!(
            result.bool_field("supply_probability_complete"),
            Some(false)
        );
        assert_eq!(
            result.bool_field("resource_probability_complete"),
            Some(false)
        );
        assert!(result.pc_tiling_memory_admission_evidence().is_none());
        assert!(!result.pc_tiling_family_publication_contract_is_valid());
    }

    #[test]
    fn canonical_tiling_without_a_native_admission_fails_before_materialization() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem();
        let packing = PackingRunner::run_with_components(
            &problem,
            &NoGpuCapability,
            &UnadmittedRawGeometryResolver(RawGeometryExecutor::native()),
        )
        .expect("unadmitted raw geometry fixture");

        assert_eq!(
            PcService::finish_with_packing_for_test(&problem, packing),
            Err(PcServiceError::TilingMaterialization(
                PcTilingMaterializationError::MemoryAccountingUnavailable,
            ))
        );
    }

    #[test]
    fn noncanonical_tiling_rejects_a_raw_fixture_at_the_runner_boundary() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem_with_objective(ObjectivePolicy::tiling().with_score_summary());

        assert_eq!(
            PackingRunner::run_with_components(
                &problem,
                &NoGpuCapability,
                &RawGeometryResolver(RawGeometryExecutor::native()),
            ),
            Err(PackingRunnerError::CandidateProvenanceMismatch {
                expected: PackingCandidateProvenance::BuildabilityPrefiltered,
                actual: PackingCandidateProvenance::RawGeometry,
            })
        );
    }

    #[test]
    fn ordinary_and_all_spin_objectives_keep_buildable_packing_and_buildup() {
        let _execution_guard = native_execution_test_guard();
        for objective in [
            ObjectivePolicy::all(),
            ObjectivePolicy::unique(),
            ObjectivePolicy::minimum_cover(),
            ObjectivePolicy::all()
                .with_score_summary()
                .with_spin_profile(SpinProfileSelection::AllSpin),
        ] {
            let problem = tiling_problem_with_objective(objective);
            let packing = PackingRunner::run(&problem).expect("ordinary product packing");
            assert_eq!(
                packing.candidate_provenance(),
                PackingCandidateProvenance::BuildabilityPrefiltered
            );
            drop(packing);

            let result = PcService::execute(&problem).expect("ordinary product execution");
            assert_eq!(result.bool_field("buildup_executed"), None);
            assert_eq!(result.field("buildup_runner"), Some("BuildUpRunner::run"));
        }
    }

    #[test]
    fn b2b_tiling_override_does_not_inherit_raw_geometry_semantics() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem_with_objective(
            ObjectivePolicy::tiling().with_back_to_back_preservation(SpinProfileSelection::AllSpin),
        );
        let result = PcService::execute(&problem).expect("noncanonical b2b tiling execution");

        assert_eq!(
            result.bool_field("packing_source_raw_geometry"),
            Some(false)
        );
        assert_eq!(result.bool_field("tiling_objective_canonical"), Some(false));
        assert_eq!(
            result.field("tiling_materialization_incomplete_reason"),
            Some("noncanonical-tiling-objective")
        );
    }

    #[test]
    fn explicit_memory_accounts_tiling_materialization_under_the_shared_bound() {
        let _execution_guard = native_execution_test_guard();
        let problem = canonical_tiling_problem_with_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(4)
                .with_worker_hardware_limit(4)
                .with_max_candidates(5_000)
                .with_max_memory_mib(Some(64)),
        );
        let result = PcService::execute(&problem).expect("bounded native tiling execution");

        assert_eq!(result.field("workers_requested"), Some("4"));
        assert_eq!(result.usize_field("workers_used"), Some(1));
        assert_eq!(result.usize_field("execution_workers"), Some(1));
        assert_eq!(
            result.field("selected_backend"),
            Some("cpu-geometry-exact-cover")
        );
        assert_eq!(result.field("selected_model"), Some("bitset-algorithm-x"));
        assert_eq!(
            result.field("packing_algorithm"),
            Some("geometry-exact-cover-candidate-materialization")
        );
        assert_eq!(
            result.field("backend_selection_reason"),
            Some("raw-geometry-deterministic-serial")
        );
        assert_eq!(result.bool_field("packing_source_raw_geometry"), Some(true));
        assert_eq!(
            result.bool_field("tiling_materialization_memory_admission_accounted"),
            Some(true)
        );
        assert_eq!(
            result.bool_field("tiling_materialization_complete"),
            Some(true)
        );
        assert_eq!(
            result.field("tiling_materialization_incomplete_reason"),
            Some("none")
        );
    }

    #[test]
    fn candidate_cap_preserves_raw_provenance_but_reports_resource_truncation_separately() {
        let _execution_guard = native_execution_test_guard();
        let problem = canonical_tiling_problem_with_policy(
            PcExecutionPolicy::mvp_default()
                .with_workers(4)
                .with_worker_hardware_limit(4)
                .with_max_candidates(1),
        );
        let first = PcService::execute(&problem).expect("first truncated native tiling execution");
        let first_keys = first.normalized_solution_keys().to_vec();
        let first_hash = first
            .field("normalized_solution_set_hash")
            .expect("first normalized solution-set hash")
            .to_owned();
        drop(first);
        let result =
            PcService::execute(&problem).expect("second truncated native tiling execution");

        assert_eq!(result.bool_field("packing_source_raw_geometry"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(true));
        assert_eq!(
            result.field("resource_truncation_reason"),
            Some("candidate_budget_exceeded")
        );
        assert_eq!(
            result.field("tiling_materialization_incomplete_reason"),
            Some("candidate_budget_exceeded")
        );
        assert_eq!(
            result.bool_field("tiling_materialization_complete"),
            Some(false)
        );
        assert_eq!(result.normalized_solution_keys(), first_keys.as_slice());
        assert_eq!(
            result.field("normalized_solution_set_hash"),
            Some(first_hash.as_str())
        );
    }

    #[test]
    fn connected_gpu_raw_geometry_fallback_is_explicitly_allowed_or_denied() {
        let _execution_guard = native_execution_test_guard();
        let base_policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Gpu)
            .with_workers(4)
            .with_worker_hardware_limit(4)
            .with_max_candidates(5_000);
        let allowed_problem = canonical_tiling_problem_with_policy(
            base_policy
                .clone()
                .with_backend_fallback(BackendFallbackPolicy::Allow),
        );
        let resolver = NativePackingExecutorRegistry::default();
        let allowed = PackingRunner::run_with_components(
            &allowed_problem,
            &ConnectedGpuCapability,
            &resolver,
        )
        .expect("connected GPU raw execution may use an explicit CPU fallback");

        assert_eq!(
            allowed.candidate_provenance(),
            PackingCandidateProvenance::RawGeometry
        );
        assert_eq!(
            allowed.actual_backend(),
            SelectedSearchBackend::CpuGeometryExactCover
        );
        assert_eq!(allowed.backend_report().workers_used(), 1);
        assert!(allowed.backend_report().backend_fallback_used());
        assert_eq!(
            allowed.backend_report().fallback_reason(),
            Some(SearchBackendFallbackReason::GpuBackendNotConnected)
        );
        drop(allowed);

        let denied_problem = canonical_tiling_problem_with_policy(
            base_policy.with_backend_fallback(BackendFallbackPolicy::Deny),
        );
        let denied =
            PackingRunner::run_with_components(&denied_problem, &ConnectedGpuCapability, &resolver);

        assert!(matches!(
            denied,
            Err(PackingRunnerError::GpuExecutionRejected(resolution))
                if !resolution.fallback_used()
                    && resolution.failure_reason()
                        == Some(SearchBackendFallbackReason::GpuBackendNotConnected)
        ));
    }

    #[test]
    fn pre_loop_cancellation_is_preserved_by_the_pc_service_error() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem();
        let packing = raw_geometry_packing(&problem, RawGeometryExecutor::native());
        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        let control = ExecutionControl::new(cancellation);

        assert_eq!(
            PcService::finish_with_packing_and_control_for_test(&problem, packing, &control),
            Err(PcServiceError::TilingMaterialization(
                PcTilingMaterializationError::ExecutionCancelled,
            ))
        );
    }

    #[test]
    fn identity_hash_and_key_poll_checkpoints_observe_real_cancellation() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem();
        let packing = raw_geometry_packing(&problem, RawGeometryExecutor::native());
        for (target_stage, target_completed) in [
            ("identities", 1_usize),
            ("hash", 0),
            ("hash", 1),
            ("keys", 1),
        ] {
            let cancellation = ExecutionCancellationToken::new();
            let handle = cancellation.handle();
            let control = ExecutionControl::new(cancellation);
            let mut checkpoint_observed = false;
            let error = PcTilingMaterialization::from_packing_with_poll_observer_for_test(
                &problem,
                &packing,
                &control,
                1,
                &mut |stage, completed, _| {
                    if stage == target_stage && completed == target_completed {
                        checkpoint_observed = true;
                        handle.cancel();
                    }
                },
            )
            .expect_err("checkpoint cancellation must stop tiling materialization");

            assert!(
                checkpoint_observed,
                "missing {target_stage} checkpoint {target_completed}"
            );
            assert_eq!(error, PcTilingMaterializationError::ExecutionCancelled);
        }
    }

    #[test]
    fn production_poll_stride_checks_4096_without_checking_4095() {
        let _execution_guard = native_execution_test_guard();
        let problem = tiling_problem();
        let packing = raw_geometry_packing(&problem, RawGeometryExecutor::repeated(4_097));
        assert_eq!(packing.candidate_count(), 4_097);
        assert!(packing.execution_memory_bound().is_some());
        let control = ExecutionControl::default();
        let mut identity_checkpoints = Vec::new();

        PcTilingMaterialization::from_packing_with_production_poll_observer_for_test(
            &problem,
            &packing,
            &control,
            &mut |stage, completed, _| {
                if stage == "identities" {
                    identity_checkpoints.push(completed);
                }
            },
        )
        .expect("uncancelled boundary materialization");

        assert!(identity_checkpoints.contains(&4_096));
        assert!(!identity_checkpoints.contains(&4_095));
        assert_eq!(identity_checkpoints.first(), Some(&0));
        assert_eq!(identity_checkpoints.last(), Some(&4_097));
    }
}

#[cfg(feature = "native-c-core")]
mod case_product_acceptance_opening_2l_empty_fixture_uses_full_solver_flow {
    use super::*;

    #[test]
    fn product_acceptance_opening_2l_empty_fixture_uses_full_solver_flow() {
        assert!(
            include_str!("../../../../tests/fixtures/pc/opening_2l_empty.json")
                .contains("opening_2l_empty")
        );
        assert!(
            include_str!("../../../../tests/golden/pc/opening_2l_empty.json")
                .contains("packing_candidate_is_solution=false")
        );

        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");

        assert!(result.solution_found());
        assert_eq!(result.field("problem_preset"), Some("opening-pc"));
        assert_eq!(result.field("compiled_goal"), Some("clear-to-empty"));
        assert_eq!(result.field("compiled_piece_window"), Some("5"));
        assert_eq!(result.field("compiled_exact_pieces"), Some("5"));
        assert_eq!(
            result.field("compiled_initial_board_mask"),
            Some("0x0000000000000000")
        );
        assert_eq!(result.field("packing_candidate_is_solution"), Some("false"));
        assert_eq!(result.field("coverage_result"), Some("rust-coverage"));
        assert_eq!(
            result.field("objective_result"),
            Some("rust-objective-reducer")
        );
        assert_eq!(
            result.field("coverage_probability"),
            Some(expected_coverage_probability())
        );
        assert_eq!(
            result.field("count_complete"),
            Some(expected_probability_complete())
        );
        assert!(
            result
                .execution_report()
                .objective_result()
                .total_solution_count()
                > 0
        );
    }
}

#[cfg(feature = "native-c-core")]
mod case_product_acceptance_opening_4l_fixture_compiles_deterministic_schedule {
    use super::*;

    #[test]
    fn product_acceptance_opening_4l_fixture_compiles_deterministic_schedule() {
        let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

        assert_eq!(problem.preset().as_str(), "opening-pc");
        assert_eq!(
            problem.exact_target_policy().target(),
            Some(PcTarget::four_lines())
        );
        assert_eq!(problem.piece_window().max_pieces(), 10);
        assert_eq!(problem.exact_pieces(), Some(10));
        let schedule = problem.checkpoint_schedule().expect("4L schedule");
        assert_eq!(schedule.label(), "4L");
        assert_eq!(schedule.partition_labels(), vec!["4", "2+2"]);
        assert_eq!(schedule.checkpoint_count(), 3);
    }
}

mod case_product_acceptance_continuation_fixture_exports_next_pc_token_after_2l {
    use super::*;

    #[test]
    fn product_acceptance_continuation_fixture_exports_next_pc_token_after_2l() {
        assert!(include_str!(
            "../../../../tests/fixtures/continuation/pc_then_next_pc_available.json"
        )
        .contains("pc_then_next_pc_available"));
        assert!(
            include_str!("../../../../tests/golden/continuation/next_pc_available.json")
                .contains("continuation_token_version=pc2")
        );

        let fixed_pieces = vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ];
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(
                fixed_pieces.clone(),
            )))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let pc_query = problem
            .scenario()
            .pc_query()
            .expect("opening problem preserves pc query");
        let fields = crate::service::pc_continuation_fields::opening_continuation_fields(
            pc_query,
            Some(&fixed_pieces),
            5,
        );

        assert_eq!(field_value(&fields, "remaining_queue_len"), Some("5"));
        assert_eq!(
            field_value(&fields, "remaining_queue_preview"),
            Some("IIOOO")
        );
        assert_eq!(field_value(&fields, "next_pc_available"), Some("true"));
        assert_eq!(field_value(&fields, "next_pc_candidate"), Some("2L"));
        assert_eq!(
            field_value(&fields, "continuation_token_available"),
            Some("true")
        );
        assert_eq!(
            field_value(&fields, "continuation_token_version"),
            Some("pc2")
        );
        assert!(field_value(&fields, "continuation_token")
            .is_some_and(|token| token.starts_with("pc2:l2:")));
    }
}

#[cfg(feature = "native-c-core")]
mod case_product_acceptance_scenario_simple_4l_fixture_solves_visible_tall_board {
    use super::*;
    use clearra_core_domain::solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, NormalizedTilingSolutionSet, PiecePlacementMask,
    };

    #[test]
    fn product_acceptance_scenario_simple_4l_fixture_solves_visible_tall_board() {
        assert!(
            include_str!("../../../../tests/fixtures/pc/scenario_simple_4l.json")
                .contains("scenario_simple_4l")
        );
        assert!(
            include_str!("../../../../tests/golden/pc/scenario_simple_4l.json")
                .contains("scenario_replay_token_version=sr2")
        );

        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_retained_trace_limit(1);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");

        assert!(result.solution_found());
        assert_eq!(result.field("problem_preset"), Some("scenario-pc"));
        assert_eq!(result.field("board_width"), Some("10"));
        assert_eq!(result.field("visible_height"), Some("4"));
        assert_eq!(
            result.field("initial_board_mask"),
            Some("0x00000000000003f0")
        );
        assert_eq!(result.field("piece_window"), Some("1"));
        assert_eq!(result.field("exact_pieces"), Some("1"));
        assert_eq!(
            result.field("actual_solution_set_contract"),
            Some("normalized-tiling-set")
        );
        assert_eq!(result.field("normalized_unique_solution_count"), Some("1"));
        let independent_key = NormalizedTilingSolutionKey::from_placements(
            0x3f0,
            [PiecePlacementMask::new(PieceKind::I, 0x0f)],
        )
        .expect("one horizontal I fills the only four empty cells");
        assert_eq!(
            independent_key.as_str(),
            "ctk1|initial=00000000000003f0|placements=I:000000000000000f"
        );
        let independent_set = NormalizedTilingSolutionSet::new([independent_key.clone()]);
        let independent_hash = independent_set.hash();
        assert_eq!(
            result
                .execution_report()
                .objective_result()
                .total_solution_count(),
            1,
            "an empty hold cannot store the final current without a next piece"
        );
        assert_eq!(
            result.normalized_solution_keys(),
            &[independent_key.as_str().to_owned()]
        );
        assert_eq!(
            result.field("normalized_solution_set_hash"),
            Some(independent_hash)
        );
        assert_eq!(result.path_steps().len(), 1);
        assert_eq!(result.path_steps()[0].piece(), PieceKind::I);
        assert_eq!(result.path_steps()[0].hold(), "none");
        assert_eq!(
            result.field("coverage_probability"),
            Some(expected_coverage_probability())
        );
        assert_eq!(result.field("scenario_replay_token_version"), Some("sr2"));
        assert!(result
            .field("scenario_replay_token")
            .is_some_and(|token| token.starts_with("sr2:w10:v4:")));
    }
}

#[cfg(feature = "native-c-core")]
mod case_completed_initial_row_pc_equivalence {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::service::pc_service::PcService;

    #[test]
    fn raw_completed_row_matches_normalized_result_and_ctk_initial_board() {
        let query = |board| {
            PcScenarioQuery::new(
                board,
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                    PieceKind::O,
                    PieceKind::I,
                    PieceKind::I,
                ])),
                PieceWindow::new(3),
            )
            .with_exact_pieces(Some(3))
            .with_retained_trace_limit(1)
        };
        let raw = query(PcScenarioBoard::standard_10(2, 0x0000_0000_0003_ffff));
        let normalized = query(PcScenarioBoard::standard_10(2, 0xff));
        let raw_problem = ProblemCompiler::compile_scenario_pc(&raw).expect("raw problem");
        let normalized_problem =
            ProblemCompiler::compile_scenario_pc(&normalized).expect("normalized problem");

        assert_eq!(raw_problem, normalized_problem);
        let raw_result = PcService::execute(&raw_problem).expect("raw execution");
        let normalized_result =
            PcService::execute(&normalized_problem).expect("normalized execution");

        assert_eq!(raw_result, normalized_result);
        assert_eq!(raw_result.field("visible_height"), Some("2"));
        assert_eq!(
            raw_result.field("initial_board_mask"),
            Some("0x00000000000000ff")
        );
        assert!(!raw_result.normalized_solution_keys().is_empty());
        assert!(raw_result
            .normalized_solution_keys()
            .iter()
            .all(|key| key.starts_with("ctk1|initial=00000000000000ff|placements=")));
    }
}

#[cfg(not(feature = "native-c-core"))]
mod case_pc_service_preserves_scenario_fixture_trace_key_contract {
    use super::*;

    #[cfg(not(feature = "native-c-core"))]
    #[test]
    fn pc_service_rejects_execution_when_native_runtime_is_unavailable() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_retained_trace_limit(1);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        assert_eq!(
            PcService::execute(&problem),
            Err(crate::service::pc_service::PcServiceError::Packing(
                crate::packing::PackingRunnerError::Native(
                    clearra_core_ffi::NativeCoreError::Unavailable
                )
            ))
        );
    }
}

#[cfg(feature = "native-c-core")]
mod case_pc_service_native_scenario_uses_native_trace_key {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::service::pc_service::PcService;

    #[cfg(feature = "native-c-core")]
    #[test]
    fn pc_service_native_scenario_uses_native_trace_key() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_retained_trace_limit(1);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");

        assert!(result.solution_found());
        assert!(result.summary_fields().contains(&(
            "retained_trace_key_source".to_owned(),
            "native-c-core".to_owned()
        )));
    }
}

fn expected_solver_backend() -> &'static str {
    "core-c-cpu-packing-cpu-buildup"
}

fn expected_packing_execution_source() -> &'static str {
    "native-cpu-packing"
}

fn expected_buildup_execution_source() -> &'static str {
    "native-cpu-buildup"
}

fn expected_native_c_core_executed() -> &'static str {
    "true"
}

fn expected_native_c_core_fallback_policy() -> &'static str {
    "native-required-no-fallback"
}

fn expected_coverage_probability() -> &'static str {
    "1.0"
}

fn expected_covered_pattern_count() -> &'static str {
    "1"
}

fn expected_probability_complete() -> &'static str {
    "true"
}

fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(field_key, _)| field_key == key)
        .map(|(_, value)| value.as_str())
}

#[cfg(feature = "native-c-core")]
mod case_pc_service_hands_replay_seed_to_app_post_processing_without_running_scoring {
    use super::*;

    #[test]
    fn pc_service_hands_replay_seed_to_app_post_processing_without_running_scoring() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::all().with_score_summary());
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");
        let fields = result.summary_fields();

        assert_eq!(result.field("score_post_processing"), None);
        assert_eq!(result.field("score_event_basis"), None);
        assert!(fields.contains(&(
            "postprocess_scoring_requested".to_owned(),
            "true".to_owned()
        )));
        assert!(fields.contains(&(
            "postprocess_execution_owner".to_owned(),
            "clearra-app->clearra-postprocess".to_owned()
        )));
        assert!(result.postprocess_replay_trace().is_some());
        assert_eq!(result.field("objective_best_score_by_pattern_count"), None);
    }
}

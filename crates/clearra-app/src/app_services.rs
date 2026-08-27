//! SRP rationale: this module has one change reason: the application-service execution contract
//! that coordinates domain services and post-processing without owning their algorithms.

use std::{
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use clearra_build_coverage::query::build_coverage_query::BuildCoverageQuery;
use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    resource::ResourceReport as CoreResourceReport,
};
#[cfg(not(target_family = "wasm"))]
use clearra_core_executor::WasmSetupParallelCoordinator;
use clearra_core_executor::{
    CoreExecutionError, CoreExecutionResult, CoreExecutor, CorePostProcessExecution,
    CorePostProcessScoreCell, CorePostProcessSpinCoverage, PcFailedQueueEvidence,
    PcFailedQueueEvidenceError, PcFailedQueueExecutionError, PercentService, PercentServiceError,
    WasmBuildProbabilityBackend, WasmCpuSearchBackend, WasmCpuSearchError, WasmSetupSearchBackend,
};
use clearra_i18n::{LanguageId, LanguagePreference, LanguageResolver};
use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
#[cfg(not(target_family = "wasm"))]
use clearra_pc_graph::request::WorkerPolicy;
use clearra_postprocess::{
    ExactScoringExecutionMaterializer, SpinCoverageTarget, TSpinCoverageMaterializationError,
    TSpinCoverageOnlyMaterializer,
};
use clearra_problem::{
    BuildProbabilityFinesseRequest, BuildSolutionProbabilityPolicy, SearchProblem, SetupSearchQuery,
};
use clearra_scoring::profile::SpinProfileId;

use crate::{
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    build_solution_probability_result::declared_build_solution_probability_policy,
    commands::core_execution_error_response,
    diagnostics::AppDiagnosticReport,
    execution_constraint_postprocess::{
        apply_build_execution_constraints_with_memory_guard,
        apply_build_worker_execution_constraints_with_memory_guard,
        apply_execution_constraints_with_memory_guard,
    },
    io::{AppFilePolicy, AppFileResolver},
    pc_failed_queue_result::PcFailedQueueCompiledAuthority,
    pc_score_minimum_cover_result::ValidatedPcScorePortfolioExecutionEvidence,
    pc_score_postprocess::{
        apply_pc_postprocess, apply_pc_postprocess_with_derivation_and_memory_guard,
        score_policy_from_result, score_profile_for_policy, PcScoreDerivation,
    },
    pc_score_summary_result::{PcScoreCompiledAuthority, ValidatedPcScoreExecutionEvidence},
    pc_tiling_family_result::PcTilingCompiledAuthority,
    resource_contract::resource_report_from_core_domain,
    search_output_surface_postprocess::finalize_coverage_summary_public_surface_with_memory_guard,
    solution_set_audit_postprocess::attach_solution_set_audit_with_memory_guard,
};

#[cfg(all(not(target_family = "wasm"), feature = "parallel"))]
pub use crate::native_build_probability_execution::host_runtime::{
    register_native_build_probability_host, register_system_native_build_probability_host,
    NativeBuildProbabilityAdmissionProvider, NativeBuildProbabilityAdmissionRequest,
    NativeBuildProbabilityHostProviderError, NativeBuildProbabilityHostRegistration,
    NativeBuildProbabilityHostRegistrationError, NativeBuildProbabilityProviderMeasurement,
    SystemNativeBuildProbabilityAdmissionProvider,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AppPcFailedQueueExecutionError {
    Core(CoreExecutionError),
    EvidenceMemoryAdmission(CoreResourceReport),
    Evidence(PcFailedQueueEvidenceError),
}

impl AppPcFailedQueueExecutionError {
    pub(crate) fn into_response(self) -> AppResponse {
        match self {
            Self::Core(error) => core_execution_error_response(error),
            Self::EvidenceMemoryAdmission(resource_report) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    "pc_failed_queue_evidence_memory_admission_failed",
                ),
            )
            .with_resource_report(resource_report_from_core_domain(&resource_report)),
            Self::Evidence(error) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ExecutionFailed,
                    format!("pc_failed_queue_evidence_failed: {error:?}"),
                ),
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppServices {
    core_executor: AppCoreExecutorService,
    file_resolver: AppFileResolver,
    language_resolver: AppLanguageResolverService,
    clock: AppClock,
    diagnostic_sink: AppDiagnosticSink,
}

impl AppServices {
    pub fn new() -> Self {
        Self::default()
    }
}
impl AppServices {
    pub fn with_core_executor(mut self, core_executor: AppCoreExecutorService) -> Self {
        self.core_executor = core_executor;
        self
    }
}
impl AppServices {
    pub fn with_file_resolver(mut self, file_resolver: AppFileResolver) -> Self {
        self.file_resolver = file_resolver;
        self
    }
}
impl AppServices {
    pub fn with_language_resolver(mut self, language_resolver: AppLanguageResolverService) -> Self {
        self.language_resolver = language_resolver;
        self
    }
}
impl AppServices {
    pub fn with_clock(mut self, clock: AppClock) -> Self {
        self.clock = clock;
        self
    }
}
impl AppServices {
    pub fn with_diagnostic_sink(mut self, diagnostic_sink: AppDiagnosticSink) -> Self {
        self.diagnostic_sink = diagnostic_sink;
        self
    }
}
impl AppServices {
    pub fn core_executor(&self) -> &AppCoreExecutorService {
        &self.core_executor
    }
}
impl AppServices {
    pub fn file_resolver(&self) -> &AppFileResolver {
        &self.file_resolver
    }
}
impl AppServices {
    pub fn file_resolver_for(&self, policy: &AppFilePolicy) -> AppFileResolver {
        self.file_resolver.with_policy(policy.clone())
    }
}
impl AppServices {
    pub fn language_resolver(&self) -> &AppLanguageResolverService {
        &self.language_resolver
    }
}
impl AppServices {
    pub fn clock(&self) -> &AppClock {
        &self.clock
    }
}
impl AppServices {
    pub fn diagnostic_sink(&self) -> &AppDiagnosticSink {
        &self.diagnostic_sink
    }
}

impl Default for AppServices {
    fn default() -> Self {
        Self {
            core_executor: AppCoreExecutorService::default(),
            file_resolver: AppFileResolver::default(),
            language_resolver: AppLanguageResolverService::default(),
            clock: AppClock::default(),
            diagnostic_sink: AppDiagnosticSink::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppCoreExecutorBackend {
    NativeCore,
    WasmCpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppCoreExecutorService {
    backend: AppCoreExecutorBackend,
}

impl AppCoreExecutorService {
    pub const fn wasm_cpu() -> Self {
        Self {
            backend: AppCoreExecutorBackend::WasmCpu,
        }
    }
}

impl AppCoreExecutorService {
    pub fn service_name(&self) -> &'static str {
        match self.backend {
            AppCoreExecutorBackend::NativeCore => "clearra-core-executor",
            AppCoreExecutorBackend::WasmCpu => "clearra-wasm-cpu-search-backend",
        }
    }

    pub(crate) const fn supports_cooperative_wasm_search(&self) -> bool {
        matches!(self.backend, AppCoreExecutorBackend::WasmCpu)
    }

    pub(crate) fn postprocess_search_result(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        let result = self.materialize_terminal_replay_partition(result, control)?;
        let pc_save_requested = result.bool_field("postprocess_pc_save_requested") == Some(true);
        let mut memory_guard = |_: &CoreExecutionResult, _: u128| Ok(());
        let result =
            apply_execution_constraints_with_memory_guard(result, control, &mut memory_guard)?;
        let result = if result.field("search_kind") == Some("build-probability") {
            let result = apply_build_spin_postprocess(result, control)?;
            if result.field("postprocess_scoring_requested") == Some("true") {
                // Build score products share the exact replay/scoring matrix
                // implementation with PC, but retain a distinct request and
                // result contract.  Running the reducer here keeps score
                // evidence behind the same executed Build problem instead of
                // fabricating a Host-side score from candidate summaries.
                apply_pc_postprocess(result, control)
            } else {
                Ok(result)
            }
        } else {
            apply_pc_postprocess(result, control)
        }?;
        let result = if pc_save_requested {
            // Save ties are ordinary winner lists. The generic solution-set
            // audit carries unrelated portfolio metadata, so it is not part
            // of either save product's public result.
            result
        } else {
            attach_solution_set_audit_with_memory_guard(result, &mut memory_guard)?
        };
        finalize_coverage_summary_public_surface_with_memory_guard(result, &mut memory_guard)
    }

    fn materialize_terminal_replay_partition(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if result.bool_field("postprocess_pc_save_requested") != Some(true)
            && result.bool_field("postprocess_pc_path_requested") != Some(true)
        {
            return Ok(result);
        }
        let pattern_weights = result.postprocess_pattern_weights().to_vec();
        let mut executions = Vec::new();
        let mut complete = !result.exact_scoring_execution_batches().is_empty();
        for batch in result.exact_scoring_execution_batches() {
            let materialized =
                ExactScoringExecutionMaterializer::materialize_terminal_replays(batch, control)
                    .map_err(|_| CoreExecutionError::Cancelled)?;
            complete &= materialized.complete();
            for aggregate in materialized.aggregates() {
                executions.extend(aggregate.executions().iter().map(|execution| {
                    CorePostProcessExecution::new(
                        aggregate.candidate_id(),
                        execution.pattern_id(),
                        execution.trace_identity(),
                        execution.replay_trace().clone(),
                    )
                }));
            }
        }
        executions.sort_unstable_by(|left, right| {
            left.candidate_id()
                .cmp(&right.candidate_id())
                .then_with(|| left.pattern_id().cmp(&right.pattern_id()))
                .then_with(|| left.trace_identity().cmp(right.trace_identity()))
        });
        Ok(result.with_postprocess_execution_batch(executions, complete, pattern_weights))
    }

    /// Runs the exact generic post-processing/public-surface path while moving
    /// the single chance transient out of that boundary, then reattaches it
    /// solely for the closed App product finalizer. The finalizer consumes it
    /// before returning any response.
    pub(crate) fn postprocess_pc_chance_result_before_public_surface(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        let (result, chance_evidence) = result.into_pc_chance_transient_parts();
        let result = self.postprocess_search_result(result, control)?;
        Ok(match chance_evidence {
            Some(evidence) => result.with_pc_chance_transient_evidence(evidence),
            None => result,
        })
    }

    pub fn materialize_pc_scoring_partition(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if result.field("search_kind") == Some("build-probability")
            || result.field("postprocess_scoring_requested") != Some("true")
        {
            return Ok(result);
        }
        let score_policy = score_policy_from_result(&result);
        let profile_id = score_profile_for_policy(score_policy).id().to_owned();
        let mut cells = Vec::new();
        let mut complete = !result.exact_scoring_execution_batches().is_empty();
        for batch in result.exact_scoring_execution_batches() {
            let materialized = ExactScoringExecutionMaterializer::materialize_score_cells(
                batch,
                score_policy,
                control,
            )
            .map_err(|_| CoreExecutionError::Cancelled)?;
            complete &= materialized.complete();
            cells.extend(materialized.scored_executions().iter().map(|execution| {
                CorePostProcessScoreCell::new(
                    execution.candidate_identity(),
                    execution.pattern_id(),
                    execution.trace_identity(),
                    execution.score(),
                    execution.attack(),
                )
            }));
        }
        cells.sort_unstable();
        cells.dedup();
        Ok(result.with_postprocess_score_cells(cells, complete, profile_id))
    }

    pub fn materialize_distributed_postprocess_partition(
        &self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.materialize_distributed_postprocess_partition_inner(result, None, control, |_, _| {
            Ok(())
        })
    }

    /// Preserves the shared verifier and memory authority through a worker's
    /// distributed post-process terminal boundary.
    pub fn materialize_distributed_postprocess_partition_with_memory_guard(
        &self,
        result: CoreExecutionResult,
        expected_build_solution_probability_policy: BuildSolutionProbabilityPolicy,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.materialize_distributed_postprocess_partition_inner(
            result,
            Some(expected_build_solution_probability_policy),
            control,
            &mut memory_guard,
        )
    }

    fn materialize_distributed_postprocess_partition_inner(
        &self,
        result: CoreExecutionResult,
        expected_build_solution_probability_policy: Option<BuildSolutionProbabilityPolicy>,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        memory_guard(&result, 0)?;
        let build_partition = result.field("search_kind") == Some("build-probability");
        let result = if build_partition {
            let expected_policy = match expected_build_solution_probability_policy {
                Some(policy) => policy,
                None => declared_build_solution_probability_policy(&result).map_err(|error| {
                    CoreExecutionError::RuntimeUnavailable {
                        component: error.input_component(),
                    }
                })?,
            };
            apply_build_worker_execution_constraints_with_memory_guard(
                result,
                expected_policy,
                control,
                &mut memory_guard,
            )?
        } else {
            apply_execution_constraints_with_memory_guard(result, control, &mut memory_guard)?
        };
        memory_guard(&result, 0)?;
        if result.field("search_kind") != Some("build-probability") {
            let result = self.materialize_pc_scoring_partition(result, control)?;
            memory_guard(&result, 0)?;
            return Ok(result);
        }
        if result.field("postprocess_build_spin_requested") != Some("true") {
            return Ok(result);
        }
        let Some((target_id, target)) = build_spin_coverage_target(&result) else {
            return Ok(result);
        };
        let pass_index = result
            .usize_field("build_distributed_pass_index")
            .unwrap_or(0);
        let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
        let coverage_word_count = pattern_count.div_ceil(u64::BITS as usize);
        let coverage_future_bytes = (coverage_word_count as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            })?;
        let candidate_capacity = result
            .exact_scoring_execution_batches()
            .iter()
            .map(|batch| batch.graphs().len())
            .chain(
                result
                    .spin_coverage_execution_batches()
                    .iter()
                    .map(|batch| batch.graphs().len()),
            )
            .try_fold(0_usize, usize::checked_add)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            })?;
        let candidate_outer_bytes = (candidate_capacity as u128)
            .checked_mul(core::mem::size_of::<String>() as u128)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            })?;
        let aggregate_future_bytes = coverage_future_bytes
            .checked_add(candidate_outer_bytes)
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<CorePostProcessSpinCoverage>() as u128)
            })
            .and_then(|bytes| bytes.checked_add(target_id.len() as u128))
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            })?;
        memory_guard(&result, aggregate_future_bytes)?;
        let mut coverage_words = Vec::new();
        coverage_words
            .try_reserve_exact(coverage_word_count)
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_coverage_allocation_failed",
            })?;
        coverage_words.resize(coverage_word_count, 0_u64);
        let mut candidate_keys = Vec::new();
        candidate_keys
            .try_reserve_exact(candidate_capacity)
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_candidate_allocation_failed",
            })?;
        let mut witnessed_pattern_count = 0_u128;
        let mut complete = !result.exact_scoring_execution_batches().is_empty()
            || !result.spin_coverage_execution_batches().is_empty();
        for batch in result.exact_scoring_execution_batches() {
            let projection = TSpinCoverageOnlyMaterializer::checked_target_memory_projection(batch)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                })?;
            let aggregate_live =
                checked_spin_partition_aggregate_bytes(&coverage_words, &candidate_keys)?;
            let bounded_cap = aggregate_live
                .checked_add(projection.required_peak_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                })?;
            memory_guard(&result, bounded_cap)?;
            let (materialized, _) =
                TSpinCoverageOnlyMaterializer::materialize_target_with_memory_limit(
                    batch,
                    target,
                    0..batch.patterns().len(),
                    control,
                    aggregate_live,
                    bounded_cap,
                )
                .map_err(core_error_from_spin_materialization)?;
            for (target_word, source_word) in coverage_words
                .iter_mut()
                .zip(materialized.covered_patterns().words())
            {
                *target_word |= source_word;
            }
            let (covered, mut keys, witnessed, materialized_complete) =
                materialized.into_summary_parts();
            drop(covered);
            candidate_keys.append(&mut keys);
            witnessed_pattern_count = witnessed_pattern_count.checked_add(witnessed).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_witness_count_overflow",
                },
            )?;
            complete &= materialized_complete;
        }
        for batch in result.spin_coverage_execution_batches() {
            let projection = TSpinCoverageOnlyMaterializer::checked_spin_batch_memory_projection(
                batch,
            )
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            })?;
            let aggregate_live =
                checked_spin_partition_aggregate_bytes(&coverage_words, &candidate_keys)?;
            let bounded_cap = aggregate_live
                .checked_add(projection.required_peak_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                })?;
            memory_guard(&result, bounded_cap)?;
            let (materialized, _) =
                TSpinCoverageOnlyMaterializer::materialize_spin_batch_with_memory_limit(
                    batch,
                    target,
                    0..batch.patterns().len(),
                    control,
                    aggregate_live,
                    bounded_cap,
                )
                .map_err(core_error_from_spin_materialization)?;
            for (target_word, source_word) in coverage_words
                .iter_mut()
                .zip(materialized.covered_patterns().words())
            {
                *target_word |= source_word;
            }
            let (covered, mut keys, witnessed, materialized_complete) =
                materialized.into_summary_parts();
            drop(covered);
            candidate_keys.append(&mut keys);
            witnessed_pattern_count = witnessed_pattern_count.checked_add(witnessed).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_witness_count_overflow",
                },
            )?;
            complete &= materialized_complete;
        }
        memory_guard(
            &result,
            checked_spin_partition_aggregate_bytes(&coverage_words, &candidate_keys)?
                .checked_add(core::mem::size_of::<CorePostProcessSpinCoverage>() as u128)
                .and_then(|bytes| bytes.checked_add(target_id.len() as u128))
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                })?,
        )?;
        let shard = CorePostProcessSpinCoverage::new(
            target_id,
            pass_index,
            pattern_count,
            coverage_words,
            candidate_keys,
            witnessed_pattern_count,
            complete,
        );
        let mut shards = Vec::new();
        shards
            .try_reserve_exact(1)
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_shard_allocation_failed",
            })?;
        shards.push(shard);
        let result = result.with_postprocess_spin_coverages(shards);
        memory_guard(&result, 0)?;
        Ok(result)
    }
}

fn checked_spin_partition_aggregate_bytes(
    coverage_words: &Vec<u64>,
    candidate_keys: &Vec<String>,
) -> Result<u128, CoreExecutionError> {
    let mut bytes = (coverage_words.capacity() as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .and_then(|words| {
            words.checked_add(
                (candidate_keys.capacity() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)?,
            )
        })
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_memory_projection_overflow",
        })?;
    for key in candidate_keys {
        bytes = bytes.checked_add(key.capacity() as u128).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            },
        )?;
    }
    Ok(bytes)
}

fn core_error_from_spin_materialization(
    error: TSpinCoverageMaterializationError,
) -> CoreExecutionError {
    match error {
        TSpinCoverageMaterializationError::Cancelled => CoreExecutionError::Cancelled,
        TSpinCoverageMaterializationError::ProjectionOverflow => {
            CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            }
        }
        TSpinCoverageMaterializationError::MemoryCapacityExceeded { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_capacity_exceeded",
            }
        }
    }
}
impl AppCoreExecutorService {
    pub fn execute_setup_with_workers_and_control(
        &self,
        query: &SetupSearchQuery,
        workers: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        #[cfg(not(target_family = "wasm"))]
        {
            let workers = workers.max(1).min(WorkerPolicy::hardware_worker_limit());
            if query
                .queue_observation_policy()
                .requires_observation_policy()
            {
                return WasmSetupSearchBackend::execute_with_observation_workers_and_control(
                    query, workers, control,
                )
                .map_err(core_error_from_wasm);
            }
            if workers > 1 {
                return WasmSetupParallelCoordinator::execute_native(query, workers, control)
                    .map_err(core_error_from_wasm);
            }
        }
        self.execute_setup_with_control(query, control)
    }

    pub fn execute_setup_with_control(
        &self,
        query: &SetupSearchQuery,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        WasmSetupSearchBackend::execute_with_control(query, control).map_err(core_error_from_wasm)
    }
}
impl AppCoreExecutorService {
    pub fn execute(
        &self,
        problem: &SearchProblem,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_with_cancellation(problem, &ExecutionCancellationToken::new())
    }
}
impl AppCoreExecutorService {
    pub fn execute_with_cancellation(
        &self,
        problem: &SearchProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_with_control(problem, &ExecutionControl::new(cancellation.clone()))
    }
}
impl AppCoreExecutorService {
    pub fn execute_with_control(
        &self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        let result = self.execute_search_backend_with_control(problem, control)?;
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        control.report_progress("postprocess", 0, Some(1));
        let result = self.postprocess_search_result(result, control)?;
        control.report_progress("postprocess", 1, Some(1));
        Ok(result)
    }

    /// Canonical native `pc.tiling` seam. Native Core owns materialization
    /// admission and returns the complete family together with unforgeable
    /// producer evidence. Generic PC post-processing must not reinterpret the
    /// bounded initial page as the complete family or strip that evidence.
    pub(crate) fn execute_native_pc_tiling_with_control(
        &self,
        authority: &PcTilingCompiledAuthority,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if !matches!(self.backend, AppCoreExecutorBackend::NativeCore) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_tiling_native_execution_backend_mismatch",
            });
        }
        if authority.terminal_resource_authority().is_some() {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_tiling_native_memory_authority_mismatch",
            });
        }
        let result = self.execute_search_backend_with_control(authority.problem(), control)?;
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        Ok(result)
    }

    /// Closed execution seam for `pc.chance`. It deliberately delays only the
    /// final public-surface strip so the typed App authority can validate the
    /// producer-owned transient evidence first.
    pub(crate) fn execute_pc_chance_with_control(
        &self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        let result = self.execute_search_backend_with_control(problem, control)?;
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        control.report_progress("postprocess", 0, Some(1));
        let result = self.postprocess_pc_chance_result_before_public_surface(result, control)?;
        control.report_progress("postprocess", 1, Some(1));
        Ok(result)
    }

    /// Native-only closed execution seam for `pc.failed-queue`. The Core
    /// service produces the result and its evidence from one execution. App
    /// post-processing runs only after that producer has returned, and the raw
    /// evidence never crosses the command boundary.
    pub(crate) fn execute_pc_failed_queue_with_control(
        &self,
        authority: &PcFailedQueueCompiledAuthority,
        control: &ExecutionControl,
    ) -> Result<(CoreExecutionResult, PcFailedQueueEvidence), AppPcFailedQueueExecutionError> {
        if !matches!(self.backend, AppCoreExecutorBackend::NativeCore) {
            return Err(AppPcFailedQueueExecutionError::Core(
                CoreExecutionError::RuntimeUnavailable {
                    component: "pc_failed_queue_wasm_typed_execution_unavailable",
                },
            ));
        }
        if control.is_cancelled() {
            return Err(AppPcFailedQueueExecutionError::Core(
                CoreExecutionError::Cancelled,
            ));
        }
        let execution = PercentService::execute_failed_queue_with_control(
            authority.problem_arc(),
            authority.failed_pattern_limit(),
            control,
        )
        .map_err(|error| app_error_from_pc_failed_queue_execution(error, control))?;
        if control.is_cancelled() {
            return Err(AppPcFailedQueueExecutionError::Core(
                CoreExecutionError::Cancelled,
            ));
        }
        let (result, evidence) = execution.into_parts();
        control.report_progress("postprocess", 0, Some(1));
        let result = self
            .postprocess_search_result(result, control)
            .map_err(AppPcFailedQueueExecutionError::Core)?;
        control.report_progress("postprocess", 1, Some(1));
        Ok((result, evidence))
    }

    /// Closed typed score seam. Native replay output is deliberately rejected:
    /// it does not carry producer-owned full executed-problem evidence. The
    /// WASM path executes the exact problem `Arc` retained by the authority.
    pub(crate) fn execute_pc_score_with_control(
        &self,
        authority: &PcScoreCompiledAuthority,
        external_retained_context_bytes: Option<u128>,
        control: &ExecutionControl,
    ) -> Result<(CoreExecutionResult, ValidatedPcScoreExecutionEvidence), CoreExecutionError> {
        if !matches!(self.backend, AppCoreExecutorBackend::WasmCpu) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_native_executed_problem_evidence_unavailable",
            });
        }
        let external_retained_context_bytes =
            external_retained_context_bytes.ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_direct_external_retained_context_missing",
            })?;
        let external_retained_upper_bound_bytes = authority
            .checked_external_retained_upper_bound_bytes(external_retained_context_bytes)
            .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                component: error.component(),
            })?;
        let executed_problem = authority.problem_arc();
        WasmCpuSearchBackend::execute_shared_under_authority_with_control_and_terminal(
            Arc::clone(&executed_problem),
            external_retained_upper_bound_bytes,
            authority.terminal_resource_authority(),
            control,
            |result, terminal_authority| {
                let result = result.map_err(core_error_from_wasm)?;
                let terminal_authority =
                    terminal_authority.ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "pc_score_wasm_terminal_authority_missing",
                    })?;
                self.postprocess_pc_score_wasm_result_with_memory_guard(
                    authority,
                    &executed_problem,
                    result,
                    control,
                    |stage_result, checked_future_bytes| {
                        terminal_authority
                            .validate_public_result_memory_with_future(
                                stage_result,
                                checked_future_bytes,
                            )
                            .map_err(core_error_from_wasm)
                    },
                )
            },
        )
    }

    /// Distinct `pc.score-minimals` execution seam. It reuses the exact score
    /// producer, then retains a separately validated all-optimal portfolio
    /// authority before the private replay owners are released.
    pub(crate) fn execute_pc_score_minimals_with_control(
        &self,
        authority: &PcScoreCompiledAuthority,
        external_retained_context_bytes: Option<u128>,
        control: &ExecutionControl,
    ) -> Result<
        (
            CoreExecutionResult,
            ValidatedPcScorePortfolioExecutionEvidence,
        ),
        CoreExecutionError,
    > {
        if !matches!(self.backend, AppCoreExecutorBackend::WasmCpu) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_minimals_native_executed_problem_evidence_unavailable",
            });
        }
        let external_retained_context_bytes =
            external_retained_context_bytes.ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_minimals_direct_external_retained_context_missing",
            })?;
        let external_retained_upper_bound_bytes = authority
            .checked_external_retained_upper_bound_bytes(external_retained_context_bytes)
            .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                component: error.component(),
            })?;
        let executed_problem = authority.problem_arc();
        WasmCpuSearchBackend::execute_shared_under_authority_with_control_and_terminal(
            Arc::clone(&executed_problem),
            external_retained_upper_bound_bytes,
            authority.terminal_resource_authority(),
            control,
            |result, terminal_authority| {
                let result = result.map_err(core_error_from_wasm)?;
                let terminal_authority =
                    terminal_authority.ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "pc_score_minimals_wasm_terminal_authority_missing",
                    })?;
                self.postprocess_pc_score_minimals_wasm_result_with_memory_guard(
                    authority,
                    &executed_problem,
                    result,
                    control,
                    |stage_result, checked_future_bytes| {
                        terminal_authority
                            .validate_public_result_memory_with_future(
                                stage_result,
                                checked_future_bytes,
                            )
                            .map_err(core_error_from_wasm)
                    },
                )
            },
        )
    }

    /// Canonical direct WASM tiling seam. The request-level authority is
    /// acquired before compilation and remains borrowed through Core's
    /// terminal memory validation. The closed tiling result needs no generic
    /// BuildUp/score post-processing.
    pub(crate) fn execute_pc_tiling_with_control(
        &self,
        authority: &PcTilingCompiledAuthority,
        external_retained_context_bytes: Option<u128>,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if !matches!(self.backend, AppCoreExecutorBackend::WasmCpu) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_tiling_wasm_terminal_authority_not_required",
            });
        }
        let context_bytes =
            external_retained_context_bytes.ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "pc_tiling_direct_external_retained_context_missing",
            })?;
        let external_bound = authority
            .checked_external_retained_upper_bound_bytes(context_bytes)
            .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                component: error.component(),
            })?;
        let executed_problem = authority.problem_arc();
        let terminal_resource_authority = authority.terminal_resource_authority().ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_tiling_wasm_terminal_authority_missing",
            },
        )?;
        WasmCpuSearchBackend::execute_shared_under_authority_with_control_and_terminal(
            Arc::clone(&executed_problem),
            external_bound,
            terminal_resource_authority,
            control,
            |result, terminal_authority| {
                let result = result.map_err(core_error_from_wasm)?;
                let terminal_authority =
                    terminal_authority.ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "pc_tiling_wasm_terminal_session_missing",
                    })?;
                terminal_authority
                    .validate_public_result_memory(&result)
                    .map_err(core_error_from_wasm)?;
                if !Arc::ptr_eq(&executed_problem, &authority.problem_arc()) {
                    return Err(CoreExecutionError::RuntimeUnavailable {
                        component: "pc_tiling_executed_problem_owner_mismatch",
                    });
                }
                Ok(result)
            },
        )
    }

    /// Shared direct/cooperative score finalizer. The same live search-session
    /// lease guards raw validation, typed derivation, evidence binding, and the
    /// terminal public projection that physically drops every private owner.
    pub(crate) fn postprocess_pc_score_wasm_result_with_memory_guard(
        &self,
        authority: &PcScoreCompiledAuthority,
        executed_problem: &std::sync::Arc<SearchProblem>,
        result: CoreExecutionResult,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<(CoreExecutionResult, ValidatedPcScoreExecutionEvidence), CoreExecutionError> {
        memory_guard(&result, 0)?;
        authority
            .validate_raw_wasm_execution(executed_problem, &result)
            .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                component: error.component(),
            })?;
        let result =
            apply_execution_constraints_with_memory_guard(result, control, &mut memory_guard)?;
        memory_guard(&result, 0)?;
        let (result, derivation) = apply_pc_postprocess_with_derivation_and_memory_guard(
            result,
            control,
            &mut memory_guard,
        )?
        .into_parts();
        memory_guard(&result, 0)?;
        let derivation = derivation.ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_derivation_evidence_missing",
        })?;
        let evidence = authority
            .validate_postprocessed_result(executed_problem, &result, &derivation)
            .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                component: error.component(),
            })?;
        let result = result
            .try_into_fail_closed_public_solution_surface_with_memory_guard(|live, future| {
                memory_guard(live, future)
            })
            .map_err(map_pc_score_public_surface_error)?;
        if !evidence.matches_core_result(&result) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_public_surface_evidence_mismatch",
            });
        }
        memory_guard(&result, 0)?;
        Ok((result, evidence))
    }

    pub(crate) fn postprocess_pc_score_minimals_wasm_result_with_memory_guard(
        &self,
        authority: &PcScoreCompiledAuthority,
        executed_problem: &std::sync::Arc<SearchProblem>,
        result: CoreExecutionResult,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<
        (
            CoreExecutionResult,
            ValidatedPcScorePortfolioExecutionEvidence,
        ),
        CoreExecutionError,
    > {
        memory_guard(&result, 0)?;
        authority
            .validate_raw_wasm_execution(executed_problem, &result)
            .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                component: error.component(),
            })?;
        let result =
            apply_execution_constraints_with_memory_guard(result, control, &mut memory_guard)?;
        memory_guard(&result, 0)?;
        let (result, derivation) = apply_pc_postprocess_with_derivation_and_memory_guard(
            result,
            control,
            &mut memory_guard,
        )?
        .into_parts();
        memory_guard(&result, 0)?;
        let derivation = derivation.ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_minimals_derivation_evidence_missing",
        })?;
        let score_execution = authority
            .validate_postprocessed_result(executed_problem, &result, &derivation)
            .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                component: error.component(),
            })?;
        let evidence =
            ValidatedPcScorePortfolioExecutionEvidence::validate(score_execution, &derivation)
                .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                    component: error.as_str(),
                })?;
        let portfolio_retained_bytes = evidence
            .checked_incremental_retained_capacity_bytes()
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_minimals_retained_memory_overflow",
            })?;
        memory_guard(&result, portfolio_retained_bytes)?;
        let result = result
            .try_into_fail_closed_public_solution_surface_with_memory_guard(|live, future| {
                let future = future.checked_add(portfolio_retained_bytes).ok_or(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "pc_score_minimals_future_memory_overflow",
                    },
                )?;
                memory_guard(live, future)
            })
            .map_err(map_pc_score_public_surface_error)?;
        if !evidence.matches_core_result(&result) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_minimals_public_surface_evidence_mismatch",
            });
        }
        memory_guard(&result, portfolio_retained_bytes)?;
        Ok((result, evidence))
    }

    fn execute_search_backend_with_control(
        &self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        if matches!(self.backend, AppCoreExecutorBackend::NativeCore)
            && problem.objective().execution_constraints().requested()
        {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "native_core_execution_constraints_not_supported",
            });
        }
        if matches!(self.backend, AppCoreExecutorBackend::NativeCore)
            && problem.allowed_colored_solution_identities().is_some()
        {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "native_core_supplied_solution_filter_not_supported",
            });
        }
        let result = match self.backend {
            AppCoreExecutorBackend::NativeCore => {
                CoreExecutor::execute_with_control(problem, control)
            }
            AppCoreExecutorBackend::WasmCpu => {
                WasmCpuSearchBackend::execute_with_control(problem, control)
                    .map_err(core_error_from_wasm)
            }
        }?;
        Ok(result)
    }
}

fn app_error_from_pc_failed_queue_execution(
    error: PcFailedQueueExecutionError,
    control: &ExecutionControl,
) -> AppPcFailedQueueExecutionError {
    if control.is_cancelled() {
        return AppPcFailedQueueExecutionError::Core(CoreExecutionError::Cancelled);
    }
    match error {
        PcFailedQueueExecutionError::Percent(error) => {
            if let Some((stage, status, resource_report)) = error.resource_incomplete() {
                return AppPcFailedQueueExecutionError::Core(
                    CoreExecutionError::ResourceIncomplete {
                        stage,
                        status,
                        resource_report,
                    },
                );
            }
            if let Some(component) = error.unsupported_reason() {
                return AppPcFailedQueueExecutionError::Core(
                    CoreExecutionError::RuntimeUnavailable { component },
                );
            }
            if let PercentServiceError::Packing(
                clearra_core_executor::packing::PackingRunnerError::BackendExecutorUnavailable {
                    reason,
                    ..
                },
            ) = error
            {
                return AppPcFailedQueueExecutionError::Core(
                    CoreExecutionError::RuntimeUnavailable { component: reason },
                );
            }
            AppPcFailedQueueExecutionError::Core(CoreExecutionError::Pc(format!(
                "Percent({error:?})"
            )))
        }
        PcFailedQueueExecutionError::Evidence(PcFailedQueueEvidenceError::MemoryAdmission(
            resource_report,
        )) => AppPcFailedQueueExecutionError::EvidenceMemoryAdmission(resource_report),
        PcFailedQueueExecutionError::Evidence(
            PcFailedQueueEvidenceError::MemoryAuthorityUnavailable,
        ) => AppPcFailedQueueExecutionError::Core(CoreExecutionError::RuntimeUnavailable {
            component: "pc_failed_queue_memory_authority_unavailable",
        }),
        PcFailedQueueExecutionError::Evidence(error) => {
            AppPcFailedQueueExecutionError::Evidence(error)
        }
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_coverage(
        &self,
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_build_coverage_with_cancellation(
            problem,
            query,
            &ExecutionCancellationToken::new(),
        )
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_coverage_with_cancellation(
        &self,
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_build_coverage_with_control(
            problem,
            query,
            &ExecutionControl::new(cancellation.clone()),
        )
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_coverage_with_control(
        &self,
        problem: &SearchProblem,
        query: &BuildCoverageQuery,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        match self.backend {
            AppCoreExecutorBackend::NativeCore => {
                CoreExecutor::execute_build_coverage_with_control(problem, query, control)
            }
            AppCoreExecutorBackend::WasmCpu => Err(CoreExecutionError::RuntimeUnavailable {
                component: "wasm_cpu_build_coverage_not_connected",
            }),
        }
    }
}

impl Default for AppCoreExecutorService {
    fn default() -> Self {
        Self {
            backend: AppCoreExecutorBackend::NativeCore,
        }
    }
}

fn core_error_from_wasm(error: WasmCpuSearchError) -> CoreExecutionError {
    error.into_core_execution_error()
}

fn validate_wasm_build_probability_terminal<T, A>(
    result: Result<T, WasmCpuSearchError>,
    authority: Option<A>,
) -> Result<(T, A), CoreExecutionError> {
    let result = result.map_err(core_error_from_wasm)?;
    let authority = authority.ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "wasm_build_probability_terminal_authority_missing",
    })?;
    Ok((result, authority))
}

fn map_pc_score_public_surface_error(
    error: clearra_core_executor::core_execution_result::CoreResultFieldReplacementError<
        CoreExecutionError,
    >,
) -> CoreExecutionError {
    match error {
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_public_surface_memory_projection_overflow",
            }
        }
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::AllocationFailed { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_public_surface_allocation_failed",
            }
        }
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => error,
    }
}
impl AppCoreExecutorService {
    pub fn execute_build_probability_with_control(
        &self,
        problem: &SearchProblem,
        field: clearra_problem::BuildProbabilityField,
        aggregation: clearra_problem::BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.execute_build_probability_with_optional_score_derivation_with_control(
            problem,
            field,
            aggregation,
            finesse,
            solution_probability_policy,
            control,
            false,
        )
        .map(|(result, _)| result)
    }

    /// Executes one Build query and retains the typed score derivation while
    /// the producer-owned terminal memory authority is still alive.
    ///
    /// This is deliberately separate from the legacy scalar Build route: a
    /// score product must never reconstruct candidate winners from flattened
    /// Host fields after the exact replay batch has been released.
    pub(crate) fn execute_build_probability_with_score_derivation_with_control(
        &self,
        problem: &SearchProblem,
        field: clearra_problem::BuildProbabilityField,
        aggregation: clearra_problem::BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
        control: &ExecutionControl,
    ) -> Result<(CoreExecutionResult, PcScoreDerivation), CoreExecutionError> {
        let (result, derivation) = self
            .execute_build_probability_with_optional_score_derivation_with_control(
                problem,
                field,
                aggregation,
                finesse,
                solution_probability_policy,
                control,
                true,
            )?;
        let derivation = derivation.ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "build_score_derivation_evidence_missing",
        })?;
        Ok((result, derivation))
    }

    fn execute_build_probability_with_optional_score_derivation_with_control(
        &self,
        problem: &SearchProblem,
        field: clearra_problem::BuildProbabilityField,
        aggregation: clearra_problem::BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
        control: &ExecutionControl,
        retain_private_score_authority: bool,
    ) -> Result<(CoreExecutionResult, Option<PcScoreDerivation>), CoreExecutionError> {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        #[cfg(all(not(target_family = "wasm"), feature = "parallel"))]
        if problem.backend_request().workers() > 1 {
            if !crate::native_build_probability_execution::host_runtime::native_build_probability_host_registered()
            {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "native_build_probability_host_provider_not_registered",
                });
            }
            if retain_private_score_authority {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "native_build_probability_score_derivation_not_supported",
                });
            }
            return crate::native_build_probability_execution::host_runtime::run_registered_native_build_probability(
                *self,
                problem,
                field,
                aggregation,
                &finesse,
                solution_probability_policy,
                control,
            )
            .map(|result| (result, None));
        }
        // A native thread pool is product-reachable only through the registered
        // durable host boundary above. A one-worker WasmCpu request retains its
        // exact single-session implementation; a multi-worker request without
        // provider authority has already failed closed instead of downgrading.
        match self.backend {
            AppCoreExecutorBackend::WasmCpu => {
                WasmBuildProbabilityBackend::execute_with_control_and_terminal(
                    problem,
                    field,
                    aggregation,
                    finesse,
                    control,
                    |result, authority| {
                        let (result, authority) =
                            validate_wasm_build_probability_terminal(result, authority)?;
                        let mut terminal_memory_guard =
                            |stage_result: &CoreExecutionResult, checked_future_bytes| {
                                authority
                                    .validate_public_result_memory_with_future(
                                        stage_result,
                                        checked_future_bytes,
                                    )
                                    .map_err(core_error_from_wasm)
                            };
                        self.materialize_build_probability_public_result_with_derivation_and_memory_guard(
                            result,
                            solution_probability_policy,
                            control,
                            retain_private_score_authority,
                            &mut terminal_memory_guard,
                        )
                    },
                )
            }
            AppCoreExecutorBackend::NativeCore => Err(CoreExecutionError::RuntimeUnavailable {
                component: "native_build_probability_backend_not_connected",
            }),
        }
    }

    pub(crate) fn materialize_build_probability_public_result(
        &self,
        result: CoreExecutionResult,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.materialize_build_probability_public_result_with_memory_guard(
            result,
            solution_probability_policy,
            control,
            |_, _| Ok(()),
        )
    }

    pub(crate) fn materialize_build_probability_public_result_with_memory_guard(
        &self,
        result: CoreExecutionResult,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        self.materialize_build_probability_public_result_with_derivation_and_memory_guard(
            result,
            solution_probability_policy,
            control,
            false,
            &mut memory_guard,
        )
        .map(|(result, _)| result)
    }

    fn materialize_build_probability_public_result_with_derivation_and_memory_guard(
        &self,
        result: CoreExecutionResult,
        solution_probability_policy: BuildSolutionProbabilityPolicy,
        control: &ExecutionControl,
        retain_private_score_authority: bool,
        memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<(CoreExecutionResult, Option<PcScoreDerivation>), CoreExecutionError> {
        memory_guard(&result, 0)?;
        let result = apply_build_execution_constraints_with_memory_guard(
            result,
            solution_probability_policy,
            control,
            memory_guard,
        )?;
        let result = apply_build_spin_postprocess_with_memory_guard(result, control, memory_guard)?;
        let (result, derivation) = if result.field("postprocess_scoring_requested") == Some("true")
        {
            apply_pc_postprocess_with_derivation_and_memory_guard(result, control, memory_guard)?
                .into_parts()
        } else {
            (result, None)
        };
        memory_guard(&result, 0)?;
        let result = attach_solution_set_audit_with_memory_guard(result, memory_guard)?;
        let result = if retain_private_score_authority && derivation.is_some() {
            result
        } else {
            finalize_coverage_summary_public_surface_with_memory_guard(result, memory_guard)?
        };
        Ok((result, derivation))
    }
}
fn apply_build_spin_postprocess(
    result: CoreExecutionResult,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    apply_build_spin_postprocess_with_memory_guard(result, control, &mut |_, _| Ok(()))
}

fn apply_build_spin_postprocess_with_memory_guard(
    result: CoreExecutionResult,
    control: &ExecutionControl,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    if result.field("postprocess_build_spin_requested") != Some("true") {
        return Ok(result);
    }
    let Some((target_id, target)) = build_spin_coverage_target(&result) else {
        return Ok(result);
    };
    let field_prefix = "spin_search";
    let batches = result.exact_scoring_execution_batches();
    let spin_batches = result.spin_coverage_execution_batches();
    let shards = result.postprocess_spin_coverages();
    if batches.is_empty() && spin_batches.is_empty() && shards.is_empty() {
        let shapes = [
            (
                prefixed_field_len(field_prefix, "probability_complete"),
                "false".len(),
            ),
            (
                prefixed_field_len(field_prefix, "accuracy"),
                "unavailable".len(),
            ),
            (
                prefixed_field_len(field_prefix, "incomplete_reason"),
                "exact_execution_graph_not_materialized".len(),
            ),
        ];
        memory_guard(&result, checked_field_request_bytes(&shapes)?)?;
        let fields = vec![
            (
                format!("{field_prefix}_probability_complete"),
                "false".to_owned(),
            ),
            (format!("{field_prefix}_accuracy"), "unavailable".to_owned()),
            (
                format!("{field_prefix}_incomplete_reason"),
                "exact_execution_graph_not_materialized".to_owned(),
            ),
        ];
        return try_replace_fields_with_memory_guard(result, fields, memory_guard);
    }
    let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    let batch_count = batches.len().checked_add(spin_batches.len()).ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_memory_projection_overflow",
        },
    )?;
    let aggregate_projection = (word_count as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .and_then(|bytes| {
            bytes.checked_add(
                (batch_count as u128).checked_mul(core::mem::size_of::<usize>() as u128)?,
            )
        })
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_memory_projection_overflow",
        })?;
    memory_guard(&result, aggregate_projection)?;
    let mut covered_words = Vec::new();
    covered_words.try_reserve_exact(word_count).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_coverage_allocation_failed",
        }
    })?;
    covered_words.resize(word_count, 0_u64);
    let mut candidate_counts = Vec::new();
    candidate_counts
        .try_reserve_exact(batch_count)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_candidate_allocation_failed",
        })?;
    candidate_counts.resize(batch_count, 0_usize);
    memory_guard(
        &result,
        checked_spin_count_aggregate_bytes(&covered_words, &candidate_counts)?,
    )?;
    let mut witnessed_pattern_count = 0_u128;
    let mut materialization_complete = true;
    if shards.is_empty() {
        for (batch_index, batch) in batches.iter().enumerate() {
            let projection = TSpinCoverageOnlyMaterializer::checked_target_memory_projection(batch)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                })?;
            let aggregate_live =
                checked_spin_count_aggregate_bytes(&covered_words, &candidate_counts)?;
            let bounded_cap = aggregate_live
                .checked_add(projection.required_peak_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                })?;
            memory_guard(&result, bounded_cap)?;
            let (materialized, _) =
                TSpinCoverageOnlyMaterializer::materialize_target_with_memory_limit(
                    batch,
                    target,
                    0..batch.patterns().len(),
                    control,
                    aggregate_live,
                    bounded_cap,
                )
                .map_err(core_error_from_spin_materialization)?;
            union_words_checked(&mut covered_words, materialized.covered_patterns().words())?;
            candidate_counts[batch_index] = materialized.candidate_keys().count();
            witnessed_pattern_count = witnessed_pattern_count
                .checked_add(materialized.witnessed_pattern_count())
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_witness_count_overflow",
                })?;
            materialization_complete &= materialized.complete();
        }
        for (batch_offset, batch) in spin_batches.iter().enumerate() {
            let batch_index = batches.len() + batch_offset;
            let projection = TSpinCoverageOnlyMaterializer::checked_spin_batch_memory_projection(
                batch,
            )
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            })?;
            let aggregate_live =
                checked_spin_count_aggregate_bytes(&covered_words, &candidate_counts)?;
            let bounded_cap = aggregate_live
                .checked_add(projection.required_peak_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                })?;
            memory_guard(&result, bounded_cap)?;
            let (materialized, _) =
                TSpinCoverageOnlyMaterializer::materialize_spin_batch_with_memory_limit(
                    batch,
                    target,
                    0..batch.patterns().len(),
                    control,
                    aggregate_live,
                    bounded_cap,
                )
                .map_err(core_error_from_spin_materialization)?;
            union_words_checked(&mut covered_words, materialized.covered_patterns().words())?;
            candidate_counts[batch_index] = materialized.candidate_keys().count();
            witnessed_pattern_count = witnessed_pattern_count
                .checked_add(materialized.witnessed_pattern_count())
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_witness_count_overflow",
                })?;
            materialization_complete &= materialized.complete();
        }
    } else {
        for shard in shards {
            materialization_complete &= shard.complete() && shard.target_id() == target_id;
            if shard.pattern_count() != pattern_count
                || shard.covered_pattern_words().len()
                    != shard.pattern_count().div_ceil(u64::BITS as usize)
            {
                materialization_complete = false;
                continue;
            }
            union_words_checked(&mut covered_words, shard.covered_pattern_words())?;
            witnessed_pattern_count = witnessed_pattern_count
                .checked_add(shard.witnessed_pattern_count())
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_witness_count_overflow",
                })?;
        }
    }
    let original_candidate_count = if shards.is_empty() {
        candidate_counts.first().copied().unwrap_or(0)
    } else {
        distinct_shard_candidate_count(shards, 0)?
    };
    let mirror_candidate_count = if shards.is_empty() {
        candidate_counts.get(1).copied().unwrap_or(0)
    } else {
        distinct_shard_candidate_count(shards, 1)?
    };
    let symmetry_batch_count = if shards.is_empty() {
        batch_count
    } else {
        distinct_shard_pass_count(shards).max(batch_count)
    };
    let covered_pattern_count = covered_pattern_count(&covered_words, pattern_count);
    let mut probability = 0.0_f64;
    let mut weights_complete =
        pattern_count > 0 && result.postprocess_pattern_weights().len() == pattern_count;
    for (pattern_index, weight) in result.postprocess_pattern_weights().iter().enumerate() {
        match weight.parse::<f64>() {
            Ok(weight) => {
                if pattern_is_covered(&covered_words, pattern_index) {
                    probability += weight;
                }
            }
            Err(_) => weights_complete = false,
        }
    }
    probability = if weights_complete {
        probability.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let search_complete = result.bool_field("probability_complete").unwrap_or(false);
    let complete = materialization_complete && search_complete && weights_complete;
    let probability_len = if probability == 0.0 {
        1
    } else {
        checked_display_len(probability)?
    };
    let accuracy = if complete { "exact" } else { "incomplete" };
    let coverage_basis = result
        .field("coverage_basis")
        .unwrap_or("original-field-patterns");
    let incomplete_reason = if complete {
        "none"
    } else if !weights_complete {
        "pattern_weights_not_materialized"
    } else if !search_complete {
        "build_probability_incomplete"
    } else {
        "execution_graph_incomplete"
    };
    let distribution = if shards.is_empty() {
        "coordinator"
    } else {
        "worker-partitions"
    };
    let rule_profile = target.spin_profile().id().as_str();
    let rule_len = rule_profile
        .len()
        .checked_add("-first-success".len())
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_memory_projection_overflow",
        })?;
    let shapes = [
        (
            prefixed_field_len(field_prefix, "candidate_count"),
            checked_display_len(original_candidate_count)?,
        ),
        (
            prefixed_field_len(field_prefix, "mirror_candidate_count"),
            checked_display_len(mirror_candidate_count)?,
        ),
        (
            prefixed_field_len(field_prefix, "symmetry_batch_count"),
            checked_display_len(symmetry_batch_count)?,
        ),
        (
            prefixed_field_len(field_prefix, "covered_pattern_count"),
            checked_display_len(covered_pattern_count)?,
        ),
        (
            prefixed_field_len(field_prefix, "witnessed_pattern_count"),
            checked_display_len(witnessed_pattern_count)?,
        ),
        (
            prefixed_field_len(field_prefix, "evaluation_basis"),
            "candidate-pattern-existence".len(),
        ),
        (
            prefixed_field_len(field_prefix, "path_multiplicity_counted"),
            "false".len(),
        ),
        (
            prefixed_field_len(field_prefix, "probability"),
            probability_len,
        ),
        (
            prefixed_field_len(field_prefix, "probability_complete"),
            if complete {
                "true".len()
            } else {
                "false".len()
            },
        ),
        (prefixed_field_len(field_prefix, "accuracy"), accuracy.len()),
        (prefixed_field_len(field_prefix, "rule"), rule_len),
        (
            prefixed_field_len(field_prefix, "coverage_basis"),
            coverage_basis.len(),
        ),
        (
            prefixed_field_len(field_prefix, "incomplete_reason"),
            incomplete_reason.len(),
        ),
        ("spin_coverage_target".len(), target_id.len()),
        (
            "spin_coverage_execution_distribution".len(),
            distribution.len(),
        ),
    ];
    drop(covered_words);
    drop(candidate_counts);
    memory_guard(&result, checked_field_request_bytes(&shapes)?)?;
    let rule_id = format!("{rule_profile}-first-success");
    let fields = vec![
        (
            format!("{field_prefix}_candidate_count"),
            original_candidate_count.to_string(),
        ),
        (
            format!("{field_prefix}_mirror_candidate_count"),
            mirror_candidate_count.to_string(),
        ),
        (
            format!("{field_prefix}_symmetry_batch_count"),
            symmetry_batch_count.to_string(),
        ),
        (
            format!("{field_prefix}_covered_pattern_count"),
            covered_pattern_count.to_string(),
        ),
        (
            format!("{field_prefix}_witnessed_pattern_count"),
            witnessed_pattern_count.to_string(),
        ),
        (
            format!("{field_prefix}_evaluation_basis"),
            "candidate-pattern-existence".to_owned(),
        ),
        (
            format!("{field_prefix}_path_multiplicity_counted"),
            "false".to_owned(),
        ),
        (
            format!("{field_prefix}_probability"),
            if probability == 0.0 {
                "0".to_owned()
            } else {
                probability.to_string()
            },
        ),
        (
            format!("{field_prefix}_probability_complete"),
            complete.to_string(),
        ),
        (
            format!("{field_prefix}_accuracy"),
            if complete { "exact" } else { "incomplete" }.to_owned(),
        ),
        (format!("{field_prefix}_rule"), rule_id),
        (
            format!("{field_prefix}_coverage_basis"),
            result
                .field("coverage_basis")
                .unwrap_or("original-field-patterns")
                .to_owned(),
        ),
        (
            format!("{field_prefix}_incomplete_reason"),
            incomplete_reason.to_owned(),
        ),
        ("spin_coverage_target".to_owned(), target_id.to_owned()),
        (
            "spin_coverage_execution_distribution".to_owned(),
            distribution.to_owned(),
        ),
    ];
    try_replace_fields_with_memory_guard(result, fields, memory_guard)
}

fn checked_spin_count_aggregate_bytes(
    covered_words: &Vec<u64>,
    candidate_counts: &Vec<usize>,
) -> Result<u128, CoreExecutionError> {
    (covered_words.capacity() as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .and_then(|bytes| {
            bytes.checked_add(
                (candidate_counts.capacity() as u128)
                    .checked_mul(core::mem::size_of::<usize>() as u128)?,
            )
        })
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_memory_projection_overflow",
        })
}

fn union_words_checked(target: &mut [u64], source: &[u64]) -> Result<(), CoreExecutionError> {
    if target.len() != source.len() {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_coverage_shape_mismatch",
        });
    }
    for (target, source) in target.iter_mut().zip(source) {
        *target |= source;
    }
    Ok(())
}

fn distinct_shard_candidate_count(
    shards: &[CorePostProcessSpinCoverage],
    pass_index: usize,
) -> Result<usize, CoreExecutionError> {
    let mut count = 0_usize;
    for (shard_index, shard) in shards.iter().enumerate() {
        if shard.pass_index() != pass_index {
            continue;
        }
        for (key_index, key) in shard.candidate_keys().iter().enumerate() {
            let seen_in_prior_shard = shards[..shard_index].iter().any(|prior| {
                prior.pass_index() == pass_index
                    && prior.candidate_keys().iter().any(|prior| prior == key)
            });
            let seen_in_current = shard.candidate_keys()[..key_index]
                .iter()
                .any(|prior| prior == key);
            if !seen_in_prior_shard && !seen_in_current {
                count = count
                    .checked_add(1)
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "build_probability_postprocess_candidate_count_overflow",
                    })?;
            }
        }
    }
    Ok(count)
}

fn distinct_shard_pass_count(shards: &[CorePostProcessSpinCoverage]) -> usize {
    shards
        .iter()
        .enumerate()
        .filter(|(index, shard)| {
            !shards[..*index]
                .iter()
                .any(|prior| prior.pass_index() == shard.pass_index())
        })
        .count()
}

fn pattern_is_covered(words: &[u64], pattern_index: usize) -> bool {
    words
        .get(pattern_index / u64::BITS as usize)
        .is_some_and(|word| word & (1_u64 << (pattern_index % u64::BITS as usize)) != 0)
}

fn covered_pattern_count(words: &[u64], pattern_count: usize) -> usize {
    (0..pattern_count)
        .filter(|pattern| pattern_is_covered(words, *pattern))
        .count()
}

fn prefixed_field_len(prefix: &str, suffix: &str) -> usize {
    prefix.len() + 1 + suffix.len()
}

fn checked_field_request_bytes(fields: &[(usize, usize)]) -> Result<u128, CoreExecutionError> {
    let mut bytes = (fields.len() as u128)
        .checked_mul(core::mem::size_of::<(String, String)>() as u128)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "build_probability_postprocess_memory_projection_overflow",
        })?;
    for (key, value) in fields {
        bytes = bytes
            .checked_add(*key as u128)
            .and_then(|bytes| bytes.checked_add(*value as u128))
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "build_probability_postprocess_memory_projection_overflow",
            })?;
    }
    Ok(bytes)
}

#[derive(Default)]
struct CheckedDisplayLength(Option<usize>);

impl fmt::Write for CheckedDisplayLength {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0 = self.0.unwrap_or(0).checked_add(value.len());
        self.0.map(|_| ()).ok_or(fmt::Error)
    }
}

fn checked_display_len(value: impl fmt::Display) -> Result<usize, CoreExecutionError> {
    use fmt::Write;

    let mut counter = CheckedDisplayLength(Some(0));
    write!(&mut counter, "{value}").map_err(|_| CoreExecutionError::RuntimeUnavailable {
        component: "build_probability_postprocess_memory_projection_overflow",
    })?;
    counter.0.ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "build_probability_postprocess_memory_projection_overflow",
    })
}

fn try_replace_fields_with_memory_guard(
    result: CoreExecutionResult,
    fields: Vec<(String, String)>,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    result
        .try_with_replaced_fields_with_memory_guard(fields, |live, future| {
            memory_guard(live, future)
        })
        .map_err(|error| match error {
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow => {
                CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_memory_projection_overflow",
                }
            }
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::AllocationFailed { .. } => {
                CoreExecutionError::RuntimeUnavailable {
                    component: "build_probability_postprocess_field_allocation_failed",
                }
            }
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => error,
        })
}

fn build_spin_coverage_target(
    result: &CoreExecutionResult,
) -> Option<(&'static str, SpinCoverageTarget)> {
    if result.field("build_probability_aggregation") != Some("spin") {
        return None;
    }
    let selection = result
        .field("spin_profile_requested")
        .and_then(SpinProfileSelection::parse)
        .unwrap_or(SpinProfileSelection::TSpins);
    let profile_id = spin_profile_id(selection);
    let target_id = match selection {
        SpinProfileSelection::TSpins => "spin:t-spins",
        SpinProfileSelection::TSpinsPlus => "spin:t-spins-plus",
        SpinProfileSelection::AllSpin => "spin:all-spin",
        SpinProfileSelection::AllSpinPlus => "spin:all-spin-plus",
        SpinProfileSelection::AllMini => "spin:all-mini",
        SpinProfileSelection::AllMiniPlus => "spin:all-mini-plus",
    };
    Some((target_id, SpinCoverageTarget::any_line_clear(profile_id)))
}

fn spin_profile_id(selection: SpinProfileSelection) -> SpinProfileId {
    match selection {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    }
}

#[cfg(test)]
mod execution_constraint_backend_tests {
    #[cfg(feature = "native-c-core")]
    use clearra_core_domain::solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, PiecePlacementMask,
    };
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind, solution::StandardBoard64ColoredTilingIdentity,
    };
    use clearra_core_executor::{
        CoreExecutionError, CoreExecutionResult, CorePostProcessSpinCoverage, WasmCpuSearchError,
    };
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_replay::SpinCoverageExecutionBatch;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        apply_build_spin_postprocess_with_memory_guard, validate_wasm_build_probability_terminal,
        AppCoreExecutorService,
    };

    #[test]
    fn build_probability_terminal_validation_preserves_backend_error_before_missing_authority() {
        let error = validate_wasm_build_probability_terminal(
            Err::<(), _>(WasmCpuSearchError::InvalidProblem {
                reason: "test_build_probability_problem_error",
            }),
            None::<()>,
        )
        .expect_err("the backend error must remain authoritative");

        assert_eq!(
            error,
            CoreExecutionError::Pc("test_build_probability_problem_error".to_owned())
        );
    }

    #[test]
    fn build_probability_terminal_validation_rejects_missing_success_authority() {
        let error = validate_wasm_build_probability_terminal(Ok(()), None::<()>)
            .expect_err("a successful result still requires terminal authority");

        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "wasm_build_probability_terminal_authority_missing",
            }
        );
    }

    fn shard_spin_result() -> CoreExecutionResult {
        CoreExecutionResult::new(
            vec![
                (
                    "postprocess_build_spin_requested".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "build_probability_aggregation".to_owned(),
                    "spin".to_owned(),
                ),
                ("spin_profile_requested".to_owned(), "t-spins".to_owned()),
                ("coverage_pattern_count".to_owned(), "1".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                (
                    "coverage_basis".to_owned(),
                    "original-field-patterns".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_postprocess_execution_batch(Vec::new(), true, vec!["1".to_owned()])
        .with_spin_coverage_execution_batch(Some(SpinCoverageExecutionBatch::new(
            Vec::new(),
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            Vec::new(),
            true,
        )))
        .with_postprocess_spin_coverages(vec![CorePostProcessSpinCoverage::new(
            "spin:t-spins",
            0,
            1,
            vec![1],
            vec!["candidate".to_owned()],
            1,
            true,
        )])
    }

    #[test]
    fn shard_spin_aggregate_actual_capacity_guard_accepts_exact_peak_and_rejects_peak_minus_one() {
        let expected_actual = (core::mem::size_of::<u64>() + core::mem::size_of::<usize>()) as u128;
        let mut observed = Vec::new();
        apply_build_spin_postprocess_with_memory_guard(
            shard_spin_result(),
            &ExecutionControl::default(),
            &mut |_, future| {
                observed.push(future);
                Ok(())
            },
        )
        .expect("unbounded shard aggregation");
        assert_eq!(observed[0], expected_actual);
        assert_eq!(observed[1], expected_actual);

        let mut exact_call = 0_usize;
        apply_build_spin_postprocess_with_memory_guard(
            shard_spin_result(),
            &ExecutionControl::default(),
            &mut |_, future| {
                let current_call = exact_call;
                exact_call += 1;
                if current_call == 1 && future > expected_actual {
                    return Err(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_guard_rejected",
                    });
                }
                Ok(())
            },
        )
        .expect("the allocator-visible aggregate must fit its exact cap");
        assert!(exact_call >= 2);

        let mut rejected_call = 0_usize;
        let error = apply_build_spin_postprocess_with_memory_guard(
            shard_spin_result(),
            &ExecutionControl::default(),
            &mut |_, future| {
                let current_call = rejected_call;
                rejected_call += 1;
                if current_call == 1 && future > expected_actual - 1 {
                    return Err(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_guard_rejected",
                    });
                }
                Ok(())
            },
        )
        .expect_err("the allocator-visible aggregate must reject peak minus one");
        assert_eq!(rejected_call, 2);
        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_guard_rejected",
            }
        );
    }

    #[test]
    fn native_core_fails_closed_for_execution_constraints() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
        );
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("opening PC problem");

        let error = AppCoreExecutorService::default()
            .execute_with_control(&problem, &ExecutionControl::default())
            .expect_err("NativeCore must not silently skip execution constraints");

        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "native_core_execution_constraints_not_supported",
            }
        );
    }

    fn scenario_problem_with_supplied_solution_filter(
        with_filter: bool,
    ) -> clearra_problem::SearchProblem {
        let mut query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        if with_filter {
            let selected_identity =
                StandardBoard64ColoredTilingIdentity::from_piece_masks(0, [0; 7])
                    .expect("canonical supplied-solution identity");
            query = query.with_allowed_colored_solution_identities([selected_identity]);
        }
        ProblemCompiler::compile_scenario_pc(&query).expect("scenario PC problem")
    }

    #[test]
    fn native_core_fails_closed_before_ignoring_supplied_solution_filter() {
        let problem = scenario_problem_with_supplied_solution_filter(true);

        let error = AppCoreExecutorService::default()
            .execute_with_control(&problem, &ExecutionControl::default())
            .expect_err("NativeCore must not silently ignore the supplied-solution filter");

        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "native_core_supplied_solution_filter_not_supported",
            }
        );
    }

    #[test]
    #[cfg(feature = "native-c-core")]
    fn ordinary_native_pc_request_does_not_inherit_supplied_filter_admission() {
        let problem = scenario_problem_with_supplied_solution_filter(false);
        assert!(problem.allowed_colored_solution_identities().is_none());

        let result = AppCoreExecutorService::default()
            .execute_with_control(&problem, &ExecutionControl::default())
            .expect("an ordinary PC request must reach and complete NativeCore execution");
        let expected_key = NormalizedTilingSolutionKey::from_placements(
            0x3f0,
            [PiecePlacementMask::new(PieceKind::I, 0x0f)],
        )
        .expect("one horizontal I fills the only four empty cells");

        assert!(result.solution_found());
        assert_eq!(result.field("status"), Some("scenario-searched"));
        assert_eq!(
            result.field("actual_solution_set_contract"),
            Some("normalized-tiling-set")
        );
        assert_eq!(result.field("normalized_unique_solution_count"), Some("1"));
        assert_eq!(
            result.normalized_solution_keys(),
            &[expected_key.as_str().to_owned()]
        );
    }

    #[test]
    fn wasm_cpu_keeps_executing_supplied_solution_filter_requests() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let unfiltered_problem = scenario_problem_with_supplied_solution_filter(false);
        let filtered_problem = scenario_problem_with_supplied_solution_filter(true);

        let unfiltered = AppCoreExecutorService::wasm_cpu()
            .execute_with_control(&unfiltered_problem, &ExecutionControl::default())
            .expect("WASM CPU unfiltered control request");
        let filtered = AppCoreExecutorService::wasm_cpu()
            .execute_with_control(&filtered_problem, &ExecutionControl::default())
            .expect("WASM CPU owns supplied-solution filter semantics");

        assert!(unfiltered.solution_found());
        assert!(!filtered.solution_found());
        assert!(filtered.normalized_solution_identities().is_empty());
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppLanguageResolverService;

impl AppLanguageResolverService {
    pub fn service_name(&self) -> &'static str {
        "clearra-i18n-language-resolver"
    }
}
impl AppLanguageResolverService {
    pub fn resolve(&self, preference: &LanguagePreference) -> LanguageId {
        LanguageResolver::resolve(preference)
    }
}
impl AppLanguageResolverService {
    pub fn resolve_from_selected(&self, selected: Option<LanguageId>) -> LanguageId {
        LanguageResolver::resolve_from_selected(selected)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppClock;

impl AppClock {
    pub fn service_name(&self) -> &'static str {
        "system-clock"
    }
}
impl AppClock {
    pub fn unix_timestamp_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppDiagnosticSink;

impl AppDiagnosticSink {
    pub fn service_name(&self) -> &'static str {
        "app-diagnostic-sink"
    }
}
impl AppDiagnosticSink {
    pub fn observe(&self, _report: &AppDiagnosticReport) {}
}

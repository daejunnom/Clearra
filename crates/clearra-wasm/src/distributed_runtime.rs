//! SRP rationale: this module has one change reason: the distributed WASM coordinator state
//! machine shared by producer, verifier, merge, cancellation, and terminal transitions.

use clearra_app::{
    AppCommand, AppCoreExecutorService, AppRequest, DistributedForwardPreparation,
    DistributedSearchPreparation, DistributedSetupPreparation, ExecutionControl,
    PreparedDistributedForwardSearch, PreparedDistributedPcScoreCompletion,
    PreparedDistributedSearch, PreparedDistributedSearchCompletion, PreparedDistributedSetupSearch,
    ProductCapabilityContract,
};
#[cfg(feature = "webgpu-search")]
use clearra_core_executor::WasmWebGpuCandidateProducer;
use clearra_core_executor::{
    CoreExecutionError, WasmBuildProbabilityCandidateProducer,
    WasmBuildProbabilityDistributedResultMerger, WasmBuildProbabilityDistributedVerifier,
    WasmCandidatePacket, WasmCandidateProducerAdvance, WasmCpuCandidateProducer,
    WasmCpuSearchBackend, WasmCpuSearchError, WasmDistributedGeometrySummary,
    WasmDistributedProgress, WasmDistributedResultMerger, WasmDistributedVerifier,
    WasmProductSearchBackend, WasmSetupParallelCoordinator, WasmSetupParallelProduce,
    WasmSetupParallelWorker, WasmTilingRootAdvance, WasmTilingRootProducer,
    WasmTilingRootResultMerger, WasmTilingRootWorker,
};
use clearra_forward_search::{
    ForwardParallelCoordinator, ForwardParallelProduce, ForwardParallelWorker,
};
use clearra_pc_graph::request::RequestedSearchBackend;
use clearra_problem::BuildSolutionProbabilityPolicy;

use crate::{
    distributed_wire::{
        checked_candidate_vec_retained_bytes,
        decode_build_probability_candidate_batch_with_memory_guard,
        decode_build_probability_partial_results_with_memory_guard, decode_candidate_batch,
        decode_partial_results_with_memory_guard, decode_tiling_root_chunk, encode_candidate_batch,
        encode_candidate_batch_with_memory_guard, encode_partial_results,
        encode_partial_results_with_memory_guard, encode_tiling_root_chunk, is_tiling_root_chunk,
        GuardedDistributedWireError,
    },
    json_event_envelope::{serialize_governed_worker_events, serialize_worker_events},
    BackendStatus, GovernedWasmExecutionResult, GovernedWasmJson, GovernedWasmWorkerEvents,
    JobProgress, WasmCommandRuntime, WasmCommandRuntimeError, WasmExecutionResult,
    WasmWorkerJobEvent, WasmWorkerJobId,
};

const MIN_DISTRIBUTED_TARGET_PIECES: usize = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum WasmDistributedMode {
    Serial = 0,
    CpuMulti = 1,
    WebGpuMulti = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum WasmDistributedRequestedBackend {
    Auto = 0,
    Cpu = 1,
    Gpu = 2,
    Hybrid = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum WasmDistributedFallbackReason {
    None = 0,
    GpuKernelUnavailable = 1,
    GpuDeviceNotFound = 2,
}

pub enum WasmDistributedPreparation {
    Serial,
    Ready(WasmExecutionResult),
    Coordinator(WasmDistributedCoordinator),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WasmDistributedProducerAdvance {
    Pending,
    Initialization(Vec<u8>),
    Batch(Vec<u8>),
    Completed,
    Cancelled,
}

pub struct WasmDistributedCoordinator {
    prepared: Option<DistributedPreparedSearch>,
    producer: Option<DistributedCandidateProducer>,
    merger: Option<DistributedResultMerger>,
    pending_build_completion_summary: Option<WasmDistributedGeometrySummary>,
    summary: Option<WasmDistributedGeometrySummary>,
    completed_progress: WasmDistributedProgress,
    forward_completed: bool,
    worker_count: usize,
    verification_required: bool,
    webgpu_requested: bool,
    mode: WasmDistributedMode,
    requested_backend: WasmDistributedRequestedBackend,
    preparation_fallback_reason: WasmDistributedFallbackReason,
    backend_execution_override: Option<clearra_core_executor::WasmDistributedBackendExecution>,
    control: ExecutionControl,
}

enum DistributedCandidateProducer {
    Cpu(WasmCpuCandidateProducer),
    Tiling(WasmTilingRootProducer),
    BuildProbability(WasmBuildProbabilityCandidateProducer),
    Forward(ForwardParallelCoordinator),
    Setup(WasmSetupParallelCoordinator),
    #[cfg(feature = "webgpu-search")]
    WebGpu(WasmWebGpuCandidateProducer),
}

pub struct WasmDistributedVerifierRuntime {
    verifier: DistributedVerifier,
    postprocessor: AppCoreExecutorService,
    build_solution_probability_policy: Option<BuildSolutionProbabilityPolicy>,
    pending_candidates: Vec<WasmCandidatePacket>,
    pending_candidate_cursor: usize,
    pending_external_retained_bytes: u128,
    control: ExecutionControl,
}

enum DistributedVerifier {
    Pc(WasmDistributedVerifier),
    Tiling(WasmTilingRootWorker),
    BuildProbability(WasmBuildProbabilityDistributedVerifier),
    Forward(ForwardParallelWorker),
    Setup(WasmSetupParallelWorker),
}

enum DistributedResultMerger {
    Pc(WasmDistributedResultMerger),
    Tiling(WasmTilingRootResultMerger),
    BuildProbability(WasmBuildProbabilityDistributedResultMerger),
    Forward(ForwardParallelCoordinator),
}

enum DistributedPreparedSearch {
    Core(PreparedDistributedSearch),
    Forward(PreparedDistributedForwardSearch),
    Setup(PreparedDistributedSetupSearch),
}

pub struct WasmDistributedVerifierConsume {
    pub candidate_count: usize,
    pub partial: Option<Vec<u8>>,
    pub has_pending_work: bool,
}

impl WasmDistributedCoordinator {
    pub fn prepare(
        runtime: &WasmCommandRuntime,
        command_text: &str,
    ) -> Result<WasmDistributedPreparation, WasmCommandRuntimeError> {
        if raw_distributed_requests_finite_memory(command_text) {
            return Err(finite_authority_unavailable_error());
        }
        Self::prepare_after_raw_finite_precheck(runtime, command_text)
    }

    fn prepare_after_raw_finite_precheck(
        runtime: &WasmCommandRuntime,
        command_text: &str,
    ) -> Result<WasmDistributedPreparation, WasmCommandRuntimeError> {
        let prepared = runtime.prepare_command_text(command_text)?;
        let (request, webgpu_requested) = prepared.into_parts();
        if matches!(request.command(), AppCommand::Setup(_)) {
            let requested_workers = usize::from(request.resource_budget().workers()).max(1);
            if requested_workers < 2 {
                return Ok(WasmDistributedPreparation::Serial);
            }
            let prepared = match runtime
                .app_context()
                .prepare_distributed_setup_search(request)
            {
                DistributedSetupPreparation::Ready(response) => {
                    return Ok(WasmDistributedPreparation::Ready(
                        WasmExecutionResult::from_app_response(response, false),
                    ));
                }
                DistributedSetupPreparation::Search(prepared) => prepared,
            };
            if prepared
                .query()
                .queue_observation_policy()
                .requires_observation_policy()
            {
                return Ok(WasmDistributedPreparation::Serial);
            }
            let producer = WasmSetupParallelCoordinator::new(prepared.query(), requested_workers)
                .map_err(|error| {
                distributed_error("E_WASM_DISTRIBUTED_SETUP_START", error.reason())
            })?;
            if producer.task_count() < 2 {
                return Ok(WasmDistributedPreparation::Serial);
            }
            let worker_count = requested_workers.min(producer.task_count() + 1);
            return Ok(WasmDistributedPreparation::Coordinator(Self {
                prepared: Some(DistributedPreparedSearch::Setup(prepared)),
                producer: Some(DistributedCandidateProducer::Setup(producer)),
                merger: None,
                pending_build_completion_summary: None,
                summary: None,
                completed_progress: WasmDistributedProgress::default(),
                forward_completed: false,
                worker_count,
                verification_required: true,
                webgpu_requested: false,
                mode: WasmDistributedMode::CpuMulti,
                requested_backend: WasmDistributedRequestedBackend::Cpu,
                preparation_fallback_reason: WasmDistributedFallbackReason::None,
                backend_execution_override: None,
                control: ExecutionControl::default(),
            }));
        }
        if matches!(
            request.command(),
            AppCommand::Damage(_) | AppCommand::SpinFinder(_)
        ) {
            let workers = usize::from(request.resource_budget().workers()).max(1);
            let query = match request.command() {
                AppCommand::Damage(command) => command.query(),
                AppCommand::SpinFinder(command) => command.query(),
                _ => {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_STATE",
                        "forward command classification changed during preparation",
                    ));
                }
            };
            if !ForwardParallelCoordinator::is_worthwhile(query, workers) {
                return Ok(WasmDistributedPreparation::Serial);
            }
            let prepared = match runtime
                .app_context()
                .prepare_distributed_forward_search(request)
            {
                DistributedForwardPreparation::Ready(response) => {
                    return Ok(WasmDistributedPreparation::Ready(
                        WasmExecutionResult::from_app_response(response, false),
                    ));
                }
                DistributedForwardPreparation::Search(prepared) => prepared,
            };
            let producer = ForwardParallelCoordinator::new(prepared.query().clone(), workers)
                .map_err(|error| {
                    distributed_error("E_WASM_DISTRIBUTED_FORWARD_START", error.reason())
                })?;
            return Ok(WasmDistributedPreparation::Coordinator(Self {
                prepared: Some(DistributedPreparedSearch::Forward(prepared)),
                producer: Some(DistributedCandidateProducer::Forward(producer)),
                merger: None,
                pending_build_completion_summary: None,
                summary: None,
                completed_progress: WasmDistributedProgress::default(),
                forward_completed: false,
                worker_count: workers,
                verification_required: true,
                webgpu_requested: false,
                mode: WasmDistributedMode::CpuMulti,
                requested_backend: WasmDistributedRequestedBackend::Cpu,
                preparation_fallback_reason: WasmDistributedFallbackReason::None,
                backend_execution_override: None,
                control: ExecutionControl::default(),
            }));
        }
        if !request_needs_distributed_execution(&request) {
            return Ok(WasmDistributedPreparation::Serial);
        }
        let prepared = match runtime.app_context().prepare_distributed_search(request) {
            DistributedSearchPreparation::Ready(response) => {
                return Ok(WasmDistributedPreparation::Ready(
                    WasmExecutionResult::from_app_response(response, webgpu_requested),
                ));
            }
            DistributedSearchPreparation::Search(prepared) => prepared,
        };
        let problem = prepared.problem();
        if problem
            .queue_observation_policy()
            .requires_observation_policy()
        {
            return Ok(WasmDistributedPreparation::Serial);
        }
        let build_probability_request = prepared.build_probability_request();
        let build_probability_finesse_request = prepared.build_probability_finesse_request();
        let build_probability_finesse_requested =
            build_probability_finesse_request.is_some_and(|(metric, _)| metric.requested());
        if build_probability_finesse_requested
            && build_probability_request
                .is_some_and(|(_, aggregation)| aggregation.is_tiling_only())
        {
            // Tiling-only workers return roots rather than authoritative BuildUp
            // successes. Finesse reconstruction must only consume candidates that
            // passed the normal BuildUp verifier, so keep this combination serial.
            return Ok(WasmDistributedPreparation::Serial);
        }
        let worker_count = problem.backend_policy().workers();
        let distributed_worthwhile = build_probability_request.map_or_else(
            || WasmCpuSearchBackend::distributed_execution_is_worthwhile(problem),
            |(field, _)| build_probability_distributed_execution_is_worthwhile(field, worker_count),
        );
        if worker_count < 2 || !distributed_worthwhile {
            return Ok(WasmDistributedPreparation::Serial);
        }
        let requested_backend = problem.backend_policy().requested_backend();
        let distributed_requested_backend = requested_backend.into();
        let explicit_gpu = requested_backend == RequestedSearchBackend::Gpu;
        let webgpu_connected = build_probability_request.is_none()
            && problem.backend_policy().runtime_webgpu_available()
            && cfg!(feature = "webgpu-search");
        if explicit_gpu && !webgpu_connected && !problem.backend_policy().allow_backend_fallback() {
            return Ok(WasmDistributedPreparation::Serial);
        }
        let unavailable_gpu = (explicit_gpu && !webgpu_connected).then(|| {
            if build_probability_request.is_some()
                || problem.backend_policy().runtime_webgpu_available()
            {
                (
                    "gpu_kernel_unavailable",
                    WasmDistributedFallbackReason::GpuKernelUnavailable,
                )
            } else {
                (
                    "gpu_device_not_found",
                    WasmDistributedFallbackReason::GpuDeviceNotFound,
                )
            }
        });
        let backend_execution_override = unavailable_gpu.map(|(reason, _)| {
            clearra_core_executor::WasmDistributedBackendExecution::CpuFallback {
                reason,
                failure_class: "unavailable",
                failure_stage: "capability-query",
                discarded_partial_gpu_result: false,
                original_gpu_result_incomplete: false,
            }
        });
        let preparation_fallback_reason = unavailable_gpu
            .map(|(_, reason)| reason)
            .unwrap_or(WasmDistributedFallbackReason::None);
        let selected_product_backend = build_probability_request
            .is_none()
            .then(|| WasmCpuSearchBackend::selected_product_backend(problem));
        let (producer, mode, worker_count) = if let Some((field, aggregation)) =
            build_probability_request
        {
            let (finesse_metric, finesse_pattern_knowledge) =
                build_probability_finesse_request.unwrap_or_default();
            let verifier_count = worker_count.saturating_sub(1);
            let producer =
                match WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
                    problem,
                    field,
                    aggregation,
                    finesse_metric,
                    finesse_pattern_knowledge,
                    verifier_count,
                    0,
                ) {
                    Ok(producer) => producer,
                    Err(WasmCpuSearchError::ResourceAdmission { .. }) => {
                        return Ok(WasmDistributedPreparation::Serial);
                    }
                    Err(error) => {
                        return Err(distributed_error(
                            "E_WASM_DISTRIBUTED_START",
                            error.reason(),
                        ));
                    }
                };
            (
                DistributedCandidateProducer::BuildProbability(producer),
                WasmDistributedMode::CpuMulti,
                worker_count,
            )
        } else if problem.objective().kind()
            == clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
            && selected_product_backend == Some(WasmProductSearchBackend::Cpu)
        {
            let producer = match prepared
                .pc_tiling_terminal_resource_authority()
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_START", reason))?
            {
                Some((authority, checked_external_retained_upper_bound_bytes)) => {
                    WasmTilingRootProducer::new_shared_under_authority(
                        prepared.problem_arc(),
                        checked_external_retained_upper_bound_bytes,
                        authority,
                    )
                }
                None => WasmTilingRootProducer::new(problem),
            }
            .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_START", reason))?;
            if producer.root_count() < 2 {
                return Ok(WasmDistributedPreparation::Serial);
            }
            let worker_count = worker_count.min(producer.root_count().saturating_add(1));
            (
                DistributedCandidateProducer::Tiling(producer),
                WasmDistributedMode::CpuMulti,
                worker_count,
            )
        } else {
            match selected_product_backend.unwrap_or(WasmProductSearchBackend::Cpu) {
                WasmProductSearchBackend::Cpu => {
                    let score_authority = prepared
                        .pc_score_terminal_resource_authority()
                        .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_START", reason))?;
                    let producer = match match score_authority {
                        Some((authority, checked_external_retained_upper_bound_bytes)) => {
                            WasmCpuCandidateProducer::new_shared_under_terminal_authority(
                                prepared.problem_arc(),
                                checked_external_retained_upper_bound_bytes,
                                authority,
                            )
                        }
                        None => WasmCpuCandidateProducer::new_typed(problem),
                    } {
                        Ok(producer) => producer,
                        Err(WasmCpuSearchError::ResourceAdmission { .. }) => {
                            return Ok(WasmDistributedPreparation::Serial);
                        }
                        Err(error) => {
                            return Err(distributed_error(
                                "E_WASM_DISTRIBUTED_START",
                                error.reason(),
                            ));
                        }
                    };
                    (
                        DistributedCandidateProducer::Cpu(producer),
                        WasmDistributedMode::CpuMulti,
                        worker_count,
                    )
                }
                WasmProductSearchBackend::WebGpu => {
                    #[cfg(feature = "webgpu-search")]
                    {
                        (
                            DistributedCandidateProducer::WebGpu(
                                WasmWebGpuCandidateProducer::new(problem).map_err(|error| {
                                    distributed_search_error("E_WASM_DISTRIBUTED_START", error)
                                })?,
                            ),
                            WasmDistributedMode::WebGpuMulti,
                            worker_count,
                        )
                    }
                    #[cfg(not(feature = "webgpu-search"))]
                    {
                        return Ok(WasmDistributedPreparation::Serial);
                    }
                }
            }
        };
        let verification_required = producer.verification_required();
        Ok(WasmDistributedPreparation::Coordinator(Self {
            prepared: Some(DistributedPreparedSearch::Core(prepared)),
            producer: Some(producer),
            merger: None,
            pending_build_completion_summary: None,
            summary: None,
            completed_progress: WasmDistributedProgress::default(),
            forward_completed: false,
            worker_count,
            verification_required,
            webgpu_requested,
            mode,
            requested_backend: distributed_requested_backend,
            preparation_fallback_reason,
            backend_execution_override,
            control: ExecutionControl::default(),
        }))
    }

    #[cfg(test)]
    fn prepare_finite_transport_fixture_for_test(
        runtime: &WasmCommandRuntime,
        command_text: &str,
    ) -> Result<WasmDistributedPreparation, WasmCommandRuntimeError> {
        // This fixture deliberately performs compatibility parsing and ProblemCompiler
        // work outside the claimed finite authority. It exists only to exercise the
        // already-built distributed producer, terminal carrier, event, and JSON
        // transitions; no production raw or prepared entry point calls this path.
        Self::prepare_after_raw_finite_precheck(runtime, command_text)
    }

    /// Creates an in-process verifier under this coordinator's admitted
    /// BuildProbability authority when that producer owns the matching build
    /// request. Other distributed families retain the standalone preparation
    /// path and its independent fail-closed admission.
    pub fn prepare_in_process_verifier(
        &self,
        runtime: &WasmCommandRuntime,
        command_text: &str,
    ) -> Result<WasmDistributedVerifierRuntime, WasmCommandRuntimeError> {
        if let (
            Some(DistributedCandidateProducer::BuildProbability(producer)),
            Some(DistributedPreparedSearch::Core(prepared)),
        ) = (&self.producer, &self.prepared)
        {
            if let Some((field, aggregation)) = prepared.build_probability_request() {
                let solution_probability_policy = prepared
                    .build_probability_solution_probability_policy()
                    .ok_or_else(|| {
                        distributed_error(
                            "E_WASM_DISTRIBUTED_VERIFIER_START",
                            "build probability solution policy authority is unavailable",
                        )
                    })?;
                let verifier = producer
                    .new_delegated_verifier(field, aggregation)
                    .map_err(|error| {
                        distributed_search_error("E_WASM_DISTRIBUTED_VERIFIER_START", error)
                    })?;
                return Ok(WasmDistributedVerifierRuntime {
                    verifier: DistributedVerifier::BuildProbability(verifier),
                    postprocessor: *runtime.app_context().services().core_executor(),
                    build_solution_probability_policy: Some(solution_probability_policy),
                    pending_candidates: Vec::new(),
                    pending_candidate_cursor: 0,
                    pending_external_retained_bytes: 0,
                    control: self.control.clone(),
                });
            }
        }
        if let (
            Some(DistributedCandidateProducer::Tiling(_)),
            Some(DistributedPreparedSearch::Core(prepared)),
        ) = (&self.producer, &self.prepared)
        {
            let verifier = WasmTilingRootWorker::new(prepared.problem())
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFIER_START", reason))?;
            return Ok(WasmDistributedVerifierRuntime {
                verifier: DistributedVerifier::Tiling(verifier),
                postprocessor: *runtime.app_context().services().core_executor(),
                build_solution_probability_policy: None,
                pending_candidates: Vec::new(),
                pending_candidate_cursor: 0,
                pending_external_retained_bytes: 0,
                control: self.control.clone(),
            });
        }
        if let (
            Some(DistributedCandidateProducer::Cpu(_)),
            Some(DistributedPreparedSearch::Core(prepared)),
        ) = (&self.producer, &self.prepared)
        {
            if let Some((authority, checked_external_retained_upper_bound_bytes)) = prepared
                .pc_score_terminal_resource_authority()
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFIER_START", reason))?
            {
                let verifier = WasmDistributedVerifier::new_shared_under_terminal_authority(
                    prepared.problem_arc(),
                    checked_external_retained_upper_bound_bytes,
                    authority,
                )
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFIER_START", reason))?;
                return Ok(WasmDistributedVerifierRuntime {
                    verifier: DistributedVerifier::Pc(verifier),
                    postprocessor: *runtime.app_context().services().core_executor(),
                    build_solution_probability_policy: None,
                    pending_candidates: Vec::new(),
                    pending_candidate_cursor: 0,
                    pending_external_retained_bytes: 0,
                    control: self.control.clone(),
                });
            }
        }
        WasmDistributedVerifierRuntime::prepare(runtime, command_text)
    }

    pub const fn mode(&self) -> WasmDistributedMode {
        self.mode
    }

    pub const fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub const fn verification_required(&self) -> bool {
        self.verification_required
    }

    pub fn tiling_geometry_parallel(&self) -> bool {
        matches!(self.producer, Some(DistributedCandidateProducer::Tiling(_)))
    }

    pub const fn requested_backend(&self) -> WasmDistributedRequestedBackend {
        self.requested_backend
    }

    pub const fn preparation_fallback_reason(&self) -> WasmDistributedFallbackReason {
        self.preparation_fallback_reason
    }

    pub fn worker_initialization(&self) -> Option<Vec<u8>> {
        match self.producer.as_ref() {
            Some(DistributedCandidateProducer::Forward(producer)) => {
                Some(producer.worker_initialization())
            }
            _ => None,
        }
    }

    pub fn worker_initialization_deferred(&self) -> bool {
        matches!(self.producer, Some(DistributedCandidateProducer::Setup(_)))
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        if let Some(producer) = &self.producer {
            return producer.progress();
        }
        if let Some(progress) = self
            .merger
            .as_ref()
            .and_then(DistributedResultMerger::tiling_progress)
        {
            return progress;
        }
        self.summary
            .as_ref()
            .or(self.pending_build_completion_summary.as_ref())
            .map_or(self.completed_progress, |summary| WasmDistributedProgress {
                geometry_nodes: summary.expanded_nodes,
                candidates: summary.candidate_count,
                candidate_family_count: summary.candidate_family_count,
                pass_count: 1,
                ..WasmDistributedProgress::default()
            })
    }

    pub fn cancel(&self) {
        self.control.cancellation.handle().cancel();
    }

    pub fn advance_producer(
        &mut self,
        work_budget: usize,
        batch_capacity: usize,
    ) -> Result<WasmDistributedProducerAdvance, WasmCommandRuntimeError> {
        if let Some(summary) = self.pending_build_completion_summary.take() {
            self.summary = Some(summary);
            return Ok(WasmDistributedProducerAdvance::Completed);
        }
        if self.summary.is_some() || self.forward_completed {
            return Ok(WasmDistributedProducerAdvance::Completed);
        }
        if matches!(self.producer, Some(DistributedCandidateProducer::Setup(_))) {
            let status = match self.producer.as_mut() {
                Some(DistributedCandidateProducer::Setup(producer)) => producer
                    .advance(work_budget, batch_capacity, &self.control)
                    .map_err(|error| {
                        distributed_error("E_WASM_DISTRIBUTED_SETUP_PRODUCER", error.reason())
                    })?,
                _ => {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_STATE",
                        "setup task producer is not active",
                    ));
                }
            };
            return match status {
                WasmSetupParallelProduce::Pending => Ok(WasmDistributedProducerAdvance::Pending),
                WasmSetupParallelProduce::Initialization(bytes) => {
                    Ok(WasmDistributedProducerAdvance::Initialization(bytes))
                }
                WasmSetupParallelProduce::Batch(bytes) => {
                    Ok(WasmDistributedProducerAdvance::Batch(bytes))
                }
                WasmSetupParallelProduce::Completed => {
                    self.forward_completed = true;
                    Ok(WasmDistributedProducerAdvance::Completed)
                }
                WasmSetupParallelProduce::Cancelled => {
                    Ok(WasmDistributedProducerAdvance::Cancelled)
                }
            };
        }
        if matches!(
            self.producer,
            Some(DistributedCandidateProducer::Forward(_))
        ) {
            let (status, batch) = match self.producer.as_mut() {
                Some(DistributedCandidateProducer::Forward(producer)) => producer
                    .produce(batch_capacity, &self.control)
                    .map_err(|error| {
                        distributed_error("E_WASM_DISTRIBUTED_FORWARD_PRODUCER", error.reason())
                    })?,
                _ => {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_STATE",
                        "forward candidate producer is not active",
                    ));
                }
            };
            return match status {
                ForwardParallelProduce::Pending => Ok(WasmDistributedProducerAdvance::Pending),
                ForwardParallelProduce::Batch => Ok(WasmDistributedProducerAdvance::Batch(batch)),
                ForwardParallelProduce::Cancelled => Ok(WasmDistributedProducerAdvance::Cancelled),
                ForwardParallelProduce::Completed => {
                    self.completed_progress = self
                        .producer
                        .as_ref()
                        .map_or_else(WasmDistributedProgress::default, |producer| {
                            producer.progress()
                        });
                    let producer = self.producer.take().ok_or_else(|| {
                        distributed_error(
                            "E_WASM_DISTRIBUTED_STATE",
                            "forward candidate producer disappeared before completion",
                        )
                    })?;
                    self.merger = Some(producer.into_merger().map_err(|error| {
                        distributed_search_error("E_WASM_DISTRIBUTED_FORWARD_MERGER", error)
                    })?);
                    self.forward_completed = true;
                    Ok(WasmDistributedProducerAdvance::Completed)
                }
            };
        }
        let tiling_root_tasks =
            matches!(self.producer, Some(DistributedCandidateProducer::Tiling(_)));
        let producer = self.producer.as_mut().ok_or_else(|| {
            distributed_error(
                "E_WASM_DISTRIBUTED_STATE",
                "candidate producer is not active",
            )
        })?;
        let mut candidates = Vec::<WasmCandidatePacket>::new();
        let batch_capacity = if tiling_root_tasks {
            1
        } else {
            batch_capacity.max(1)
        };
        reserve_candidate_batch_storage(producer, &mut candidates, batch_capacity)?;
        for _ in 0..work_budget.max(1) {
            let external_candidate_bytes = if let DistributedCandidateProducer::BuildProbability(
                build_producer,
            ) = &*producer
            {
                checked_candidate_vec_retained_bytes(&candidates)
                    .ok_or_else(|| build_candidate_memory_projection_error(build_producer))
                    .map_err(|error| {
                        distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error)
                    })?
            } else {
                0
            };
            match producer
                .advance_with_external_retained(&self.control, external_candidate_bytes)
                .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_PRODUCER", error))?
            {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(candidate) => {
                    validate_candidate_before_push(producer, &candidates, &candidate)?;
                    candidates.push(candidate);
                    validate_candidate_batch_actual(producer, &candidates)?;
                    if candidates.len() == batch_capacity {
                        return Ok(WasmDistributedProducerAdvance::Batch(
                            encode_candidate_batch_for_producer(producer, &candidates)?,
                        ));
                    }
                }
                WasmCandidateProducerAdvance::Completed(mut summary) => {
                    if let Some(execution) = self.backend_execution_override.clone() {
                        summary.backend_execution = execution;
                    }
                    let encoded = (!candidates.is_empty())
                        .then(|| encode_candidate_batch_for_producer(producer, &candidates))
                        .transpose()?;
                    if let DistributedCandidateProducer::BuildProbability(build_producer) =
                        &*producer
                    {
                        let transition_external_bytes =
                            match checked_build_candidate_transition_retained_bytes(
                                &candidates,
                                encoded.as_ref(),
                            ) {
                                Some(bytes) => bytes,
                                None => {
                                    let error =
                                        build_candidate_memory_projection_error(build_producer);
                                    drop(self.producer.take());
                                    drop(summary);
                                    drop(encoded);
                                    drop(candidates);
                                    return Err(distributed_search_error(
                                        "E_WASM_DISTRIBUTED_CANDIDATE_MEMORY",
                                        error,
                                    ));
                                }
                            };
                        let producer = match self.producer.take() {
                            Some(DistributedCandidateProducer::BuildProbability(producer)) => {
                                producer
                            }
                            producer => {
                                self.producer = producer;
                                return Err(distributed_error(
                                    "E_WASM_DISTRIBUTED_STATE",
                                    "build candidate producer disappeared before completion",
                                ));
                            }
                        };
                        // `into_merger` moves the producer Vec owners and clones
                        // only the shared pattern-weight Arc. It allocates no new
                        // backing storage. Reauthorize the merger immediately
                        // afterward against the actual candidate and wire
                        // capacities that still coexist at this boundary.
                        let merger = match producer.into_merger() {
                            Ok(merger) => merger,
                            Err(reason) => {
                                drop(summary);
                                drop(encoded);
                                drop(candidates);
                                return Err(distributed_error("E_WASM_DISTRIBUTED_MERGER", reason));
                            }
                        };
                        if let Err(error) =
                            merger.validate_external_result_memory(transition_external_bytes, 0)
                        {
                            // The failure response allocates its message. Release
                            // both the new authority and every external batch
                            // owner before constructing that response.
                            drop(merger);
                            drop(summary);
                            drop(encoded);
                            drop(candidates);
                            return Err(distributed_search_error(
                                "E_WASM_DISTRIBUTED_MERGER",
                                error,
                            ));
                        }
                        drop(candidates);
                        self.merger = Some(DistributedResultMerger::BuildProbability(merger));
                        if let Some(encoded) = encoded {
                            self.pending_build_completion_summary = Some(summary);
                            return Ok(WasmDistributedProducerAdvance::Batch(encoded));
                        }
                        self.summary = Some(summary);
                        return Ok(WasmDistributedProducerAdvance::Completed);
                    }
                    let producer = self.producer.take().ok_or_else(|| {
                        distributed_error(
                            "E_WASM_DISTRIBUTED_STATE",
                            "candidate producer disappeared before completion",
                        )
                    })?;
                    self.merger = Some(producer.into_merger().map_err(|error| {
                        distributed_search_error("E_WASM_DISTRIBUTED_MERGER", error)
                    })?);
                    self.summary = Some(summary);
                    if let Some(encoded) = encoded {
                        return Ok(WasmDistributedProducerAdvance::Batch(encoded));
                    }
                    return Ok(WasmDistributedProducerAdvance::Completed);
                }
                WasmCandidateProducerAdvance::Cancelled => {
                    return Ok(WasmDistributedProducerAdvance::Cancelled);
                }
            }
        }
        if candidates.is_empty() {
            Ok(WasmDistributedProducerAdvance::Pending)
        } else {
            Ok(WasmDistributedProducerAdvance::Batch(
                encode_candidate_batch_for_producer(producer, &candidates)?,
            ))
        }
    }

    pub fn producer_completed(&self) -> bool {
        self.summary.is_some() || self.forward_completed
    }

    pub fn absorb_partial(&mut self, input: &[u8]) -> Result<(), WasmCommandRuntimeError> {
        self.absorb_partial_with_external_retained(input, input.len() as u128)
    }

    /// Absorbs one borrowed partial while the caller-owned transfer buffer's
    /// allocator-visible storage remains live. For byte vectors this value is
    /// normally `transfer_input.capacity() as u128`, not merely the slice len.
    pub fn absorb_partial_with_external_retained(
        &mut self,
        input: &[u8],
        external_retained_bytes: u128,
    ) -> Result<(), WasmCommandRuntimeError> {
        if is_tiling_root_chunk(input) {
            let chunk = decode_tiling_root_chunk(input).map_err(|error| {
                distributed_error("E_WASM_DISTRIBUTED_TILING_PARTIAL_INVALID", error.reason())
            })?;
            if let Some(DistributedCandidateProducer::Tiling(producer)) = self.producer.as_mut() {
                producer.absorb(&chunk).map_err(|reason| {
                    distributed_error("E_WASM_DISTRIBUTED_TILING_MERGE", reason)
                })?;
                return Ok(());
            }
            let merger = self.merger.as_mut().ok_or_else(|| {
                distributed_error(
                    "E_WASM_DISTRIBUTED_STATE",
                    "tiling result merger is not ready",
                )
            })?;
            merger
                .absorb_tiling_chunk(&chunk)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_TILING_MERGE", reason))?;
            return Ok(());
        }
        if let Some(DistributedCandidateProducer::Setup(producer)) = self.producer.as_mut() {
            producer.absorb(input).map_err(|error| {
                distributed_error("E_WASM_DISTRIBUTED_SETUP_MERGE", error.reason())
            })?;
            return Ok(());
        }
        if let Some(DistributedCandidateProducer::Forward(producer)) = self.producer.as_mut() {
            producer.absorb(input, &self.control).map_err(|error| {
                distributed_error("E_WASM_DISTRIBUTED_FORWARD_MERGE", error.reason())
            })?;
            return Ok(());
        }
        let merger = self.merger.as_mut().ok_or_else(|| {
            distributed_error("E_WASM_DISTRIBUTED_STATE", "result merger is not ready")
        })?;
        if let DistributedResultMerger::BuildProbability(merger) = merger {
            absorb_build_probability_partial_batch_with_external_retained(
                input,
                external_retained_bytes,
                merger,
            )
            .map_err(|error| match error {
                GuardedDistributedWireError::Wire(error) => {
                    distributed_error("E_WASM_DISTRIBUTED_PARTIAL_INVALID", error.reason())
                }
                GuardedDistributedWireError::MemoryGuard(error) => {
                    distributed_search_error("E_WASM_DISTRIBUTED_MERGE", error)
                }
            })?;
            return Ok(());
        }
        if external_retained_bytes < input.len() as u128 {
            return Err(distributed_error(
                "E_WASM_DISTRIBUTED_PARTIAL_INVALID",
                crate::distributed_wire::DistributedWireError::decode_memory_projection_overflow()
                    .reason(),
            ));
        }
        let results = decode_partial_results_with_memory_guard(input, |checked_future_bytes| {
            merger.validate_external_result_memory(external_retained_bytes, checked_future_bytes)
        })
        .map_err(|error| match error {
            GuardedDistributedWireError::Wire(error) => {
                distributed_error("E_WASM_DISTRIBUTED_PARTIAL_INVALID", error.reason())
            }
            GuardedDistributedWireError::MemoryGuard(reason) => {
                distributed_error("E_WASM_DISTRIBUTED_MERGE", reason)
            }
        })?;
        let decoded_bytes =
            checked_worker_result_vec_retained_bytes(&results).ok_or_else(|| {
                distributed_error(
                "E_WASM_DISTRIBUTED_PARTIAL_INVALID",
                crate::distributed_wire::DistributedWireError::decode_memory_projection_overflow()
                    .reason(),
            )
            })?;
        let external_result_bytes = external_retained_bytes
            .checked_add(decoded_bytes)
            .ok_or_else(|| {
                distributed_error(
                    "E_WASM_DISTRIBUTED_PARTIAL_INVALID",
                    crate::distributed_wire::DistributedWireError::decode_memory_projection_overflow()
                        .reason(),
                )
            })?;
        merger
            .validate_external_result_memory(external_result_bytes, 0)
            .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_MERGE", reason))?;
        for result in &results {
            let absorb_future_bytes = result.checked_resource_retained_bytes().ok_or_else(|| {
                distributed_error(
                    "E_WASM_DISTRIBUTED_PARTIAL_INVALID",
                    crate::distributed_wire::DistributedWireError::decode_memory_projection_overflow()
                        .reason(),
                )
            })?;
            merger
                .validate_external_result_memory(external_result_bytes, absorb_future_bytes)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_MERGE", reason))?;
            merger
                .absorb(result)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_MERGE", reason))?;
            merger
                .validate_external_result_memory(external_result_bytes, 0)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_MERGE", reason))?;
        }
        Ok(())
    }

    /// Compatibility terminal for unlimited distributed searches. A finite
    /// Build fails closed instead of discarding its non-cloneable authority;
    /// callers must use one of the consuming `finish_governed*` methods.
    pub fn finish(
        mut self,
        workers_used: usize,
    ) -> Result<WasmExecutionResult, WasmCommandRuntimeError> {
        if matches!(self.producer, Some(DistributedCandidateProducer::Setup(_))) {
            let producer = match self.producer.take() {
                Some(DistributedCandidateProducer::Setup(producer)) => producer,
                _ => {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_STATE",
                        "setup result coordinator is not ready",
                    ));
                }
            };
            let result = producer
                .finish_with_control(workers_used, &self.control)
                .map_err(|error| {
                    distributed_error("E_WASM_DISTRIBUTED_SETUP_FINISH", error.reason())
                })?;
            let prepared = match self.prepared.take() {
                Some(DistributedPreparedSearch::Setup(prepared)) => prepared,
                _ => {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_STATE",
                        "setup app search is not prepared",
                    ));
                }
            };
            return Ok(WasmExecutionResult::from_app_response(
                prepared.complete(result),
                false,
            ));
        }
        if matches!(self.merger, Some(DistributedResultMerger::Forward(_))) {
            let merger = match self.merger.take() {
                Some(DistributedResultMerger::Forward(merger)) => merger,
                _ => {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_STATE",
                        "forward result merger is not ready",
                    ));
                }
            };
            let report = merger
                .finish_with_control(workers_used.min(self.worker_count).max(1), &self.control)
                .map_err(|error| {
                    distributed_error("E_WASM_DISTRIBUTED_FORWARD_FINISH", error.reason())
                })?;
            let prepared = match self.prepared.take() {
                Some(DistributedPreparedSearch::Forward(prepared)) => prepared,
                _ => {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_STATE",
                        "forward app search is not prepared",
                    ));
                }
            };
            return Ok(WasmExecutionResult::from_app_response(
                prepared.complete(report),
                false,
            ));
        }
        let summary = self.summary.take().ok_or_else(|| {
            distributed_error(
                "E_WASM_DISTRIBUTED_STATE",
                "geometry producer has not completed",
            )
        })?;
        let prepared = match self.prepared.take() {
            Some(DistributedPreparedSearch::Core(prepared)) => prepared,
            _ => {
                return Err(distributed_error(
                    "E_WASM_DISTRIBUTED_STATE",
                    "core app search is not prepared",
                ));
            }
        };
        let workers_used = workers_used.min(self.worker_count).max(1);
        let control = &self.control;
        let merger = self.merger.take().ok_or_else(|| {
            distributed_error("E_WASM_DISTRIBUTED_STATE", "result merger is not ready")
        })?;
        let pc_score = prepared.is_pc_score();
        let response = match merger {
            DistributedResultMerger::BuildProbability(merger) => {
                stage_build_probability_completion(
                    merger,
                    &summary,
                    workers_used,
                    control,
                    prepared,
                )?
                .complete()
                .map_err(distributed_terminal_core_error)?
            }
            merger if pc_score => {
                stage_pc_score_completion(merger, &summary, workers_used, control, prepared)?
                    .complete()
            }
            mut merger => {
                let result = merger
                    .finish(&summary, workers_used, control)
                    .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_FINISH", reason))?;
                prepared.complete(result, control)
            }
        };
        Ok(WasmExecutionResult::from_app_response(
            response,
            self.webgpu_requested,
        ))
    }

    /// Consumes a completed finite distributed Build and preserves its single
    /// memory authority through the App-to-WASM transport transition.
    ///
    /// Compatibility/unlimited completions and every non-Build distributed
    /// family fail closed: only a finite Build can return a governed result.
    pub fn finish_governed(
        mut self,
        workers_used: usize,
    ) -> Result<GovernedWasmExecutionResult, WasmCommandRuntimeError> {
        let summary = match self.summary.take() {
            Some(summary) => summary,
            None => {
                drop(self);
                return Err(WasmCommandRuntimeError::new(
                    "E_WASM_DISTRIBUTED_GOVERNED_STATE",
                    String::new(),
                ));
            }
        };
        let prepared = match self.prepared.take() {
            Some(DistributedPreparedSearch::Core(prepared)) => prepared,
            prepared => {
                drop(prepared);
                drop(summary);
                drop(self);
                return Err(WasmCommandRuntimeError::new(
                    "E_WASM_DISTRIBUTED_GOVERNED_KIND",
                    String::new(),
                ));
            }
        };
        let merger = match self.merger.take() {
            Some(DistributedResultMerger::BuildProbability(merger)) => merger,
            merger => {
                drop(merger);
                drop(prepared);
                drop(summary);
                drop(self);
                return Err(WasmCommandRuntimeError::new(
                    "E_WASM_DISTRIBUTED_GOVERNED_KIND",
                    String::new(),
                ));
            }
        };
        let workers_used = workers_used.min(self.worker_count).max(1);
        let webgpu_requested = self.webgpu_requested;
        let completion = match stage_build_probability_completion(
            merger,
            &summary,
            workers_used,
            &self.control,
            prepared,
        ) {
            Ok(completion) => completion,
            Err(error) => {
                drop(summary);
                drop(self);
                return Err(error);
            }
        };

        // The staged completion owns the final Core result. Destroy the
        // coordinator shell and its control/request metadata before the App
        // authority constructs the response, then consume that response at the
        // WASM boundary without cloning any result field.
        drop(summary);
        drop(self);
        let governed = completion
            .complete_governed()
            .map_err(distributed_terminal_core_error)?;
        WasmExecutionResult::try_from_governed_app_response_for_distributed_finish(
            governed,
            webgpu_requested,
        )
    }

    /// Consumes a completed finite distributed Build and moves its terminal
    /// response into the stable `Progress(2/2) + FinalResponse` worker schema.
    pub fn finish_governed_events(
        self,
        workers_used: usize,
        job_id: WasmWorkerJobId,
    ) -> Result<GovernedWasmWorkerEvents, WasmCommandRuntimeError> {
        GovernedWasmWorkerEvents::try_from_final_result(job_id, self.finish_governed(workers_used)?)
    }

    /// Consumes a completed finite distributed Build through the final worker
    /// event batch and into one governed JSON owner without cloning response
    /// payloads at either transition.
    pub fn finish_governed_json(
        self,
        workers_used: usize,
        job_id: WasmWorkerJobId,
    ) -> Result<GovernedWasmJson, WasmCommandRuntimeError> {
        serialize_governed_worker_events(self.finish_governed_events(workers_used, job_id)?)
    }
}

fn stage_build_probability_completion(
    mut merger: WasmBuildProbabilityDistributedResultMerger,
    summary: &WasmDistributedGeometrySummary,
    workers_used: usize,
    control: &ExecutionControl,
    prepared: PreparedDistributedSearch,
) -> Result<PreparedDistributedSearchCompletion, WasmCommandRuntimeError> {
    let completion = merger.finish_with_control_and_terminal(
        summary,
        workers_used,
        control,
        |result, authority| {
            result.map(|result| {
                prepared.complete_with_memory_guard(
                    result,
                    control,
                    |stage_result, checked_future_bytes| {
                        authority
                            .validate_public_result_memory_with_future(
                                stage_result,
                                checked_future_bytes,
                            )
                            .map_err(|component| CoreExecutionError::RuntimeUnavailable {
                                component,
                            })
                    },
                )
            })
        },
    );

    // The static Core marker may cross this point, but neither a rich WASM
    // error nor either App-response form may be constructed until the merger's
    // live-byte authority is gone.
    after_authority_drop(merger, || {
        completion.map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_FINISH", reason))
    })
}

fn stage_pc_score_completion(
    mut merger: DistributedResultMerger,
    summary: &WasmDistributedGeometrySummary,
    workers_used: usize,
    control: &ExecutionControl,
    prepared: PreparedDistributedSearch,
) -> Result<PreparedDistributedPcScoreCompletion, WasmCommandRuntimeError> {
    let result = match merger.finish(summary, workers_used, control) {
        Ok(result) => result,
        Err(reason) => {
            // Neither the rich WASM error nor an App response may coexist with
            // the child merger lease or its parent typed-product authority.
            drop(merger);
            drop(prepared);
            return Err(distributed_error("E_WASM_DISTRIBUTED_FINISH", reason));
        }
    };
    let completion = prepared.complete_pc_score_with_memory_guard(
        result,
        control,
        |stage_result, checked_future_bytes| {
            merger
                .validate_public_result_memory_with_future(stage_result, checked_future_bytes)
                .map_err(|component| CoreExecutionError::RuntimeUnavailable { component })
        },
    );
    after_authority_drop(merger, || Ok(completion))
}

trait BuildProbabilityPartialIngressAuthority {
    type Error;

    fn validate_external_result_memory(
        &self,
        external_result_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), Self::Error>;

    fn absorb_with_external_retained(
        &mut self,
        result: &clearra_core_executor::CoreExecutionResult,
        external_result_bytes: u128,
    ) -> Result<(), Self::Error>;
}

impl BuildProbabilityPartialIngressAuthority for WasmBuildProbabilityDistributedResultMerger {
    type Error = WasmCpuSearchError;

    fn validate_external_result_memory(
        &self,
        external_result_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), Self::Error> {
        WasmBuildProbabilityDistributedResultMerger::validate_external_result_memory(
            self,
            external_result_bytes,
            checked_future_bytes,
        )
    }

    fn absorb_with_external_retained(
        &mut self,
        result: &clearra_core_executor::CoreExecutionResult,
        external_result_bytes: u128,
    ) -> Result<(), Self::Error> {
        WasmBuildProbabilityDistributedResultMerger::absorb_with_external_retained(
            self,
            result,
            external_result_bytes,
        )
    }
}

fn absorb_build_probability_partial_batch<A>(
    input: &[u8],
    authority: &mut A,
) -> Result<(), GuardedDistributedWireError<A::Error>>
where
    A: BuildProbabilityPartialIngressAuthority,
{
    absorb_build_probability_partial_batch_with_external_retained(
        input,
        input.len() as u128,
        authority,
    )
}

fn absorb_build_probability_partial_batch_with_external_retained<A>(
    input: &[u8],
    input_bytes: u128,
    authority: &mut A,
) -> Result<(), GuardedDistributedWireError<A::Error>>
where
    A: BuildProbabilityPartialIngressAuthority,
{
    if input_bytes < input.len() as u128 {
        return Err(GuardedDistributedWireError::Wire(
            crate::distributed_wire::DistributedWireError::decode_memory_projection_overflow(),
        ));
    }
    let results = decode_build_probability_partial_results_with_memory_guard(
        input,
        |checked_future_bytes| {
            authority.validate_external_result_memory(input_bytes, checked_future_bytes)
        },
    )?;
    let decoded_bytes = checked_worker_result_vec_retained_bytes(&results).ok_or_else(|| {
        GuardedDistributedWireError::Wire(
            crate::distributed_wire::DistributedWireError::decode_memory_projection_overflow(),
        )
    })?;
    let external_result_bytes = input_bytes.checked_add(decoded_bytes).ok_or_else(|| {
        GuardedDistributedWireError::Wire(
            crate::distributed_wire::DistributedWireError::decode_memory_projection_overflow(),
        )
    })?;
    authority
        .validate_external_result_memory(external_result_bytes, 0)
        .map_err(GuardedDistributedWireError::MemoryGuard)?;
    for result in &results {
        authority
            .absorb_with_external_retained(result, external_result_bytes)
            .map_err(GuardedDistributedWireError::MemoryGuard)?;
    }
    Ok(())
}

fn reserve_candidate_batch_storage(
    producer: &DistributedCandidateProducer,
    candidates: &mut Vec<WasmCandidatePacket>,
    batch_capacity: usize,
) -> Result<(), WasmCommandRuntimeError> {
    let DistributedCandidateProducer::BuildProbability(producer) = producer else {
        candidates.reserve(batch_capacity);
        return Ok(());
    };
    let requested_outer_bytes = (batch_capacity as u128)
        .checked_mul(core::mem::size_of::<WasmCandidatePacket>() as u128)
        .ok_or_else(|| build_candidate_memory_projection_error(producer))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))?;
    producer
        .validate_external_result_memory(requested_outer_bytes)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))?;
    candidates.try_reserve_exact(batch_capacity).map_err(|_| {
        distributed_error(
            "E_WASM_DISTRIBUTED_CANDIDATE_STORAGE",
            "candidate_batch_allocation_failed",
        )
    })?;
    let actual_outer_bytes = checked_candidate_vec_retained_bytes(candidates)
        .ok_or_else(|| build_candidate_memory_projection_error(producer))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))?;
    producer
        .validate_external_result_memory(actual_outer_bytes)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))
}

fn validate_candidate_before_push(
    producer: &DistributedCandidateProducer,
    candidates: &Vec<WasmCandidatePacket>,
    candidate: &WasmCandidatePacket,
) -> Result<(), WasmCommandRuntimeError> {
    let DistributedCandidateProducer::BuildProbability(producer) = producer else {
        return Ok(());
    };
    let external_bytes = checked_candidate_vec_retained_bytes(candidates)
        .and_then(|bytes| bytes.checked_add(candidate.checked_nested_retained_bytes()?))
        .ok_or_else(|| build_candidate_memory_projection_error(producer))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))?;
    producer
        .validate_external_result_memory(external_bytes)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))
}

fn validate_candidate_batch_actual(
    producer: &DistributedCandidateProducer,
    candidates: &Vec<WasmCandidatePacket>,
) -> Result<(), WasmCommandRuntimeError> {
    let DistributedCandidateProducer::BuildProbability(producer) = producer else {
        return Ok(());
    };
    let external_bytes = checked_candidate_vec_retained_bytes(candidates)
        .ok_or_else(|| build_candidate_memory_projection_error(producer))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))?;
    producer
        .validate_external_result_memory(external_bytes)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error))
}

fn encode_candidate_batch_for_producer(
    producer: &DistributedCandidateProducer,
    candidates: &Vec<WasmCandidatePacket>,
) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    let DistributedCandidateProducer::BuildProbability(producer) = producer else {
        return Ok(encode_candidate_batch(candidates));
    };
    let candidate_bytes = checked_candidate_vec_retained_bytes(candidates)
        .ok_or_else(|| build_candidate_memory_projection_error(producer))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_ENCODE", error))?;
    encode_candidate_batch_with_memory_guard(candidates, |wire_bytes| {
        let external_bytes = candidate_bytes
            .checked_add(wire_bytes)
            .ok_or_else(|| build_candidate_memory_projection_error(producer))?;
        producer.validate_external_result_memory(external_bytes)
    })
    .map_err(|error| match error {
        GuardedDistributedWireError::Wire(error) => {
            distributed_error("E_WASM_DISTRIBUTED_CANDIDATE_ENCODE", error.reason())
        }
        GuardedDistributedWireError::MemoryGuard(error) => {
            distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_ENCODE", error)
        }
    })
}

fn checked_build_candidate_transition_retained_bytes(
    candidates: &Vec<WasmCandidatePacket>,
    encoded: Option<&Vec<u8>>,
) -> Option<u128> {
    checked_candidate_vec_retained_bytes(candidates)?.checked_add(
        encoded
            .map(|wire| wire.capacity() as u128)
            .unwrap_or_default(),
    )
}

fn after_authority_drop<Authority, Output>(
    authority: Authority,
    complete: impl FnOnce() -> Output,
) -> Output {
    drop(authority);
    complete()
}

fn build_candidate_memory_projection_error(
    producer: &WasmBuildProbabilityCandidateProducer,
) -> WasmCpuSearchError {
    producer
        .validate_external_result_memory(u128::MAX)
        .expect_err("overflow-sized build candidate storage is unavailable")
}

impl From<RequestedSearchBackend> for WasmDistributedRequestedBackend {
    fn from(value: RequestedSearchBackend) -> Self {
        match value {
            RequestedSearchBackend::Auto => Self::Auto,
            RequestedSearchBackend::Cpu => Self::Cpu,
            RequestedSearchBackend::Gpu => Self::Gpu,
            RequestedSearchBackend::Hybrid => Self::Hybrid,
        }
    }
}

impl DistributedCandidateProducer {
    fn verification_required(&self) -> bool {
        match self {
            Self::Cpu(producer) => producer.verification_required(),
            Self::Tiling(_) => true,
            Self::BuildProbability(_) | Self::Forward(_) | Self::Setup(_) => true,
            #[cfg(feature = "webgpu-search")]
            Self::WebGpu(producer) => producer.verification_required(),
        }
    }

    fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, WasmCpuSearchError> {
        self.advance_with_external_retained(control, 0)
    }

    fn advance_with_external_retained(
        &mut self,
        control: &ExecutionControl,
        external_retained_bytes: u128,
    ) -> Result<WasmCandidateProducerAdvance, WasmCpuSearchError> {
        match self {
            Self::Cpu(producer) => producer.advance(control).map_err(invalid_search_error),
            Self::Tiling(producer) => producer.advance(control).map_err(invalid_search_error),
            Self::BuildProbability(producer) => {
                producer.advance_with_external_retained(control, external_retained_bytes)
            }
            Self::Forward(_) => Err(invalid_search_error(
                "forward_producer_requires_batch_advance",
            )),
            Self::Setup(_) => Err(invalid_search_error(
                "setup_producer_requires_task_batch_advance",
            )),
            #[cfg(feature = "webgpu-search")]
            Self::WebGpu(producer) => producer.advance(control),
        }
    }

    fn into_merger(self) -> Result<DistributedResultMerger, WasmCpuSearchError> {
        match self {
            Self::Cpu(producer) => producer
                .into_merger()
                .map(DistributedResultMerger::Pc)
                .map_err(invalid_search_error),
            Self::Tiling(producer) => producer
                .into_merger()
                .map(DistributedResultMerger::Tiling)
                .map_err(invalid_search_error),
            Self::BuildProbability(producer) => producer
                .into_merger()
                .map(DistributedResultMerger::BuildProbability)
                .map_err(invalid_search_error),
            Self::Forward(producer) => Ok(DistributedResultMerger::Forward(producer)),
            Self::Setup(_) => Err(invalid_search_error(
                "setup_producer_owns_its_result_merger",
            )),
            #[cfg(feature = "webgpu-search")]
            Self::WebGpu(producer) => producer.into_merger().map(DistributedResultMerger::Pc),
        }
    }

    fn progress(&self) -> WasmDistributedProgress {
        match self {
            Self::Cpu(producer) => producer.progress(),
            Self::Tiling(producer) => producer.progress(),
            Self::BuildProbability(producer) => producer.progress(),
            Self::Forward(producer) => {
                let progress = producer.progress();
                WasmDistributedProgress {
                    geometry_nodes: usize::try_from(progress.visited_states).unwrap_or(usize::MAX),
                    candidates: usize::try_from(progress.tasks_dispatched).unwrap_or(usize::MAX),
                    pass_index: progress.patterns_completed,
                    pass_count: progress.pattern_count.max(1),
                    layer_index: progress.layer_index,
                    layer_count: progress.layer_count,
                    layer_done: progress.layer_done,
                    layer_total: progress.layer_total,
                    ..WasmDistributedProgress::default()
                }
            }
            Self::Setup(producer) => {
                let (pass_index, pass_count, layer_index, layer_count, layer_done, layer_total) =
                    producer.build_progress();
                WasmDistributedProgress {
                    geometry_nodes: producer.geometry_nodes(),
                    candidates: producer.dispatched_conditions(),
                    candidate_family_count: Some(producer.task_count() as u128),
                    build_nodes: producer.partial_build_nodes(),
                    coverage_checks: producer.received_conditions(),
                    pass_index,
                    pass_count,
                    layer_index,
                    layer_count,
                    layer_done,
                    layer_total,
                }
            }
            #[cfg(feature = "webgpu-search")]
            Self::WebGpu(producer) => producer.progress(),
        }
    }
}

impl DistributedVerifier {
    fn consume(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
    ) -> Result<(), &'static str> {
        match self {
            Self::Pc(verifier) => verifier.consume(candidate, control),
            Self::Tiling(_) => Err("tiling_verifier_requires_root_task_batch"),
            Self::BuildProbability(verifier) => verifier.consume(candidate, control),
            Self::Forward(_) => Err("forward_verifier_requires_forward_task_wire"),
            Self::Setup(_) => Err("setup_verifier_requires_setup_task_wire"),
        }
    }

    fn finish(&mut self) -> Result<Vec<clearra_core_executor::CoreExecutionResult>, &'static str> {
        match self {
            Self::Pc(verifier) => verifier.finish().map(|result| vec![result]),
            Self::Tiling(verifier) => {
                if verifier.has_pending_work() {
                    Err("wasm_tiling_root_worker_finish_pending")
                } else {
                    Ok(Vec::new())
                }
            }
            Self::BuildProbability(verifier) => verifier.finish(),
            Self::Forward(_) => Ok(Vec::new()),
            Self::Setup(_) => Ok(Vec::new()),
        }
    }

    fn progress(&self) -> WasmDistributedProgress {
        match self {
            Self::Pc(verifier) => verifier.progress(),
            Self::Tiling(verifier) => {
                let progress = verifier.progress();
                WasmDistributedProgress {
                    candidates: progress.candidates,
                    build_nodes: progress.geometry_nodes,
                    coverage_checks: progress.coverage_checks,
                    candidate_family_count: progress.candidate_family_count,
                    ..WasmDistributedProgress::default()
                }
            }
            Self::BuildProbability(verifier) => verifier.progress(),
            Self::Forward(verifier) => {
                let progress = verifier.progress();
                WasmDistributedProgress {
                    candidates: usize::try_from(progress.tasks_completed).unwrap_or(usize::MAX),
                    build_nodes: usize::try_from(progress.visited_states).unwrap_or(usize::MAX),
                    coverage_checks: usize::try_from(progress.generated_locks)
                        .unwrap_or(usize::MAX),
                    ..WasmDistributedProgress::default()
                }
            }
            Self::Setup(_) => WasmDistributedProgress::default(),
        }
    }
}

impl DistributedResultMerger {
    fn tiling_progress(&self) -> Option<WasmDistributedProgress> {
        match self {
            Self::Pc(merger) => merger.tiling_progress(),
            Self::Tiling(merger) => merger.progress(),
            Self::BuildProbability(_) | Self::Forward(_) => None,
        }
    }

    fn absorb(
        &mut self,
        result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<(), &'static str> {
        match self {
            Self::Pc(merger) => merger.absorb(result),
            Self::Tiling(_) => Err("tiling_merger_requires_tiling_chunk"),
            Self::BuildProbability(merger) => merger.absorb(result),
            Self::Forward(_) => Err("forward_merger_requires_forward_result_wire"),
        }
    }

    fn validate_external_result_memory(
        &self,
        external_retained_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), &'static str> {
        match self {
            Self::Pc(merger) => merger
                .validate_external_result_memory(external_retained_bytes, checked_future_bytes),
            Self::Tiling(_) => Err("tiling_merger_requires_tiling_chunk"),
            Self::BuildProbability(_) => {
                Err("build_probability_merger_requires_build_partial_ingress")
            }
            Self::Forward(_) => Err("forward_merger_requires_forward_result_wire"),
        }
    }

    fn absorb_tiling_chunk(
        &mut self,
        chunk: &clearra_core_executor::WasmTilingRootChunk,
    ) -> Result<(), &'static str> {
        match self {
            Self::Pc(merger) => merger.absorb_tiling_chunk(chunk),
            Self::Tiling(merger) => merger.absorb(chunk),
            Self::BuildProbability(_) => Err("tiling_chunk_requires_pc_result_merger"),
            Self::Forward(_) => Err("tiling_chunk_requires_pc_result_merger"),
        }
    }

    fn validate_public_result_memory_with_future(
        &self,
        result: &clearra_core_executor::CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), &'static str> {
        match self {
            Self::Pc(merger) => {
                merger.validate_public_result_memory_with_future(result, checked_future_bytes)
            }
            Self::Tiling(_) | Self::BuildProbability(_) | Self::Forward(_) => {
                Err("distributed_pc_score_terminal_merger_kind_mismatch")
            }
        }
    }

    fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
        control: &ExecutionControl,
    ) -> Result<clearra_core_executor::CoreExecutionResult, &'static str> {
        match self {
            Self::Pc(merger) => merger.finish(summary, workers_used),
            Self::Tiling(merger) => merger.finish(summary, workers_used),
            Self::BuildProbability(merger) => {
                merger.finish_with_control(summary, workers_used, control)
            }
            Self::Forward(_) => Err("forward_merger_requires_forward_finish"),
        }
    }
}

impl WasmDistributedVerifierRuntime {
    pub fn prepare(
        runtime: &WasmCommandRuntime,
        command_text: &str,
    ) -> Result<Self, WasmCommandRuntimeError> {
        if raw_distributed_requests_finite_memory(command_text) {
            return Err(finite_authority_unavailable_error());
        }
        Self::prepare_after_raw_finite_precheck(runtime, command_text)
    }

    fn prepare_after_raw_finite_precheck(
        runtime: &WasmCommandRuntime,
        command_text: &str,
    ) -> Result<Self, WasmCommandRuntimeError> {
        let prepared = runtime.prepare_command_text(command_text)?;
        let (request, _) = prepared.into_parts();
        let prepared = match runtime.app_context().prepare_distributed_search(request) {
            DistributedSearchPreparation::Search(prepared) => prepared,
            DistributedSearchPreparation::Ready(_) => {
                return Err(distributed_error(
                    "E_WASM_DISTRIBUTED_VERIFIER_START",
                    "command did not compile to a distributed search",
                ));
            }
        };
        let build_solution_probability_policy =
            prepared.build_probability_solution_probability_policy();
        let verifier =
            if let Some((field, aggregation)) = prepared.build_probability_request() {
                DistributedVerifier::BuildProbability(
                    WasmBuildProbabilityDistributedVerifier::new_typed(
                        prepared.problem(),
                        field,
                        aggregation,
                    )
                    .map_err(|error| {
                        distributed_search_error("E_WASM_DISTRIBUTED_VERIFIER_START", error)
                    })?,
                )
            } else if prepared.problem().objective().kind()
                == clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
            {
                DistributedVerifier::Tiling(WasmTilingRootWorker::new(prepared.problem()).map_err(
                    |reason| distributed_error("E_WASM_DISTRIBUTED_VERIFIER_START", reason),
                )?)
            } else {
                let problem = prepared.into_worker_problem();
                DistributedVerifier::Pc(WasmDistributedVerifier::new(problem.as_ref()).map_err(
                    |reason| distributed_error("E_WASM_DISTRIBUTED_VERIFIER_START", reason),
                )?)
            };
        Ok(Self {
            verifier,
            postprocessor: *runtime.app_context().services().core_executor(),
            build_solution_probability_policy,
            pending_candidates: Vec::new(),
            pending_candidate_cursor: 0,
            pending_external_retained_bytes: 0,
            control: ExecutionControl::default(),
        })
    }

    pub fn prepare_forward(
        runtime: &WasmCommandRuntime,
        initialization: &[u8],
    ) -> Result<Self, WasmCommandRuntimeError> {
        let verifier = if WasmSetupParallelWorker::accepts_initialization(initialization) {
            DistributedVerifier::Setup(WasmSetupParallelWorker::new(initialization).map_err(
                |error| {
                    distributed_error("E_WASM_DISTRIBUTED_SETUP_VERIFIER_START", error.reason())
                },
            )?)
        } else {
            DistributedVerifier::Forward(ForwardParallelWorker::new(initialization).map_err(
                |error| {
                    distributed_error("E_WASM_DISTRIBUTED_FORWARD_VERIFIER_START", error.reason())
                },
            )?)
        };
        Ok(Self {
            verifier,
            postprocessor: *runtime.app_context().services().core_executor(),
            build_solution_probability_policy: None,
            pending_candidates: Vec::new(),
            pending_candidate_cursor: 0,
            pending_external_retained_bytes: 0,
            control: ExecutionControl::default(),
        })
    }

    pub fn consume(
        &mut self,
        input: &[u8],
    ) -> Result<WasmDistributedVerifierConsume, WasmCommandRuntimeError> {
        self.consume_with_external_retained(input, input.len() as u128)
    }

    /// Consumes one borrowed candidate batch while the caller-owned transfer
    /// buffer remains live at its allocator-visible retained capacity.
    pub fn consume_with_external_retained(
        &mut self,
        input: &[u8],
        external_retained_bytes: u128,
    ) -> Result<WasmDistributedVerifierConsume, WasmCommandRuntimeError> {
        if self.has_pending_candidates() {
            return Err(distributed_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed verifier candidate batch is still pending",
            ));
        }
        if let DistributedVerifier::Tiling(verifier) = &mut self.verifier {
            let candidates = decode_candidate_batch(input).map_err(|error| {
                distributed_error("E_WASM_DISTRIBUTED_TILING_TASK_INVALID", error.reason())
            })?;
            let mut roots = Vec::new();
            roots.try_reserve_exact(candidates.len()).map_err(|_| {
                distributed_error(
                    "E_WASM_DISTRIBUTED_TILING_TASK_INVALID",
                    "wasm_tiling_root_batch_storage_unavailable",
                )
            })?;
            for candidate in &candidates {
                if !candidate.row_ids().is_empty() {
                    return Err(distributed_error(
                        "E_WASM_DISTRIBUTED_TILING_TASK_INVALID",
                        "wasm_tiling_root_task_rows_must_be_empty",
                    ));
                }
                roots.push((
                    candidate.pass_index(),
                    u32::try_from(candidate.ordinal()).map_err(|_| {
                        distributed_error(
                            "E_WASM_DISTRIBUTED_TILING_TASK_INVALID",
                            "wasm_tiling_root_ordinal_overflow",
                        )
                    })?,
                    candidate.target_index(),
                ));
            }
            verifier.enqueue(&roots).map_err(|reason| {
                distributed_error("E_WASM_DISTRIBUTED_TILING_TASK_INVALID", reason)
            })?;
            let (partial, has_pending_work) = advance_tiling_worker(verifier, &self.control)?;
            return Ok(WasmDistributedVerifierConsume {
                candidate_count: candidates.len(),
                partial,
                has_pending_work,
            });
        }
        if let DistributedVerifier::Forward(verifier) = &mut self.verifier {
            let (candidate_count, partial) =
                verifier.consume(input, &self.control).map_err(|error| {
                    distributed_error("E_WASM_DISTRIBUTED_FORWARD_VERIFY", error.reason())
                })?;
            return Ok(WasmDistributedVerifierConsume {
                candidate_count,
                partial: Some(partial),
                has_pending_work: false,
            });
        }
        if let DistributedVerifier::Setup(verifier) = &mut self.verifier {
            let (candidate_count, partial) =
                verifier.consume(input, &self.control).map_err(|error| {
                    distributed_error("E_WASM_DISTRIBUTED_SETUP_VERIFY", error.reason())
                })?;
            return Ok(WasmDistributedVerifierConsume {
                candidate_count,
                partial: Some(partial),
                has_pending_work: false,
            });
        }
        if let DistributedVerifier::BuildProbability(verifier) = &mut self.verifier {
            if external_retained_bytes < input.len() as u128 {
                return Err(distributed_error(
                    "E_WASM_DISTRIBUTED_CANDIDATE_MEMORY",
                    "wasm_distributed_raw_retained_bytes_below_input_length",
                ));
            }
            let input_bytes = external_retained_bytes;
            let candidates = decode_build_probability_candidate_batch_with_memory_guard(
                input,
                |decoded_future_bytes| {
                    let external_bytes = input_bytes
                        .checked_add(decoded_future_bytes)
                        .ok_or_else(|| build_worker_memory_projection_error(verifier))?;
                    verifier.validate_external_result_memory(external_bytes)
                },
            )
            .map_err(|error| match error {
                GuardedDistributedWireError::Wire(error) => {
                    distributed_error("E_WASM_DISTRIBUTED_CANDIDATE_INVALID", error.reason())
                }
                GuardedDistributedWireError::MemoryGuard(error) => {
                    distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error)
                }
            })?;
            let external_retained_bytes = checked_candidate_vec_retained_bytes(&candidates)
                .and_then(|bytes| bytes.checked_add(input_bytes))
                .ok_or_else(|| build_worker_memory_projection_error(verifier))
                .map_err(|error| {
                    distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error)
                })?;
            verifier
                .validate_external_result_memory(external_retained_bytes)
                .map_err(|error| {
                    distributed_search_error("E_WASM_DISTRIBUTED_CANDIDATE_MEMORY", error)
                })?;
            self.begin_pending_candidates(candidates, external_retained_bytes);
            return self.advance_pending_candidate();
        }
        let candidates = decode_candidate_batch(input).map_err(|error| {
            distributed_error("E_WASM_DISTRIBUTED_CANDIDATE_INVALID", error.reason())
        })?;
        self.begin_pending_candidates(candidates, 0);
        self.advance_pending_candidate()
    }

    fn begin_pending_candidates(
        &mut self,
        candidates: Vec<WasmCandidatePacket>,
        external_retained_bytes: u128,
    ) {
        debug_assert!(!self.has_pending_candidates());
        self.pending_candidates = candidates;
        self.pending_candidate_cursor = 0;
        self.pending_external_retained_bytes = external_retained_bytes;
    }

    fn has_pending_candidates(&self) -> bool {
        self.pending_candidate_cursor < self.pending_candidates.len()
    }

    /// Verifies at most one complete candidate per WASM entry. Candidate
    /// verification is the smallest existing exact-authority transaction: its
    /// reachability, coverage, and result reduction share mutable workspaces
    /// and must commit together. Keeping the remaining decoded batch behind a
    /// cursor lets the worker yield and publish a heartbeat between those
    /// transactions without weakening that authority.
    fn advance_pending_candidate(
        &mut self,
    ) -> Result<WasmDistributedVerifierConsume, WasmCommandRuntimeError> {
        if !self.has_pending_candidates() {
            self.pending_candidates.clear();
            self.pending_candidate_cursor = 0;
            self.pending_external_retained_bytes = 0;
            return Ok(WasmDistributedVerifierConsume {
                candidate_count: 0,
                partial: None,
                has_pending_work: false,
            });
        }

        let candidate = &self.pending_candidates[self.pending_candidate_cursor];
        match &mut self.verifier {
            DistributedVerifier::Pc(verifier) => verifier
                .consume(candidate, &self.control)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFY", reason))?,
            DistributedVerifier::BuildProbability(verifier) => verifier
                .consume_with_external_retained(
                    candidate,
                    &self.control,
                    self.pending_external_retained_bytes,
                )
                .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY", error))?,
            DistributedVerifier::Tiling(_)
            | DistributedVerifier::Forward(_)
            | DistributedVerifier::Setup(_) => {
                return Err(distributed_error(
                    "E_WASM_DISTRIBUTED_STATE",
                    "distributed verifier has no pending candidate cursor",
                ));
            }
        }
        self.pending_candidate_cursor += 1;
        let has_pending_work = self.has_pending_candidates();
        if !has_pending_work {
            self.pending_candidates.clear();
            self.pending_candidate_cursor = 0;
            self.pending_external_retained_bytes = 0;
        }
        Ok(WasmDistributedVerifierConsume {
            candidate_count: 1,
            partial: None,
            has_pending_work,
        })
    }

    pub fn continue_work(
        &mut self,
    ) -> Result<WasmDistributedVerifierConsume, WasmCommandRuntimeError> {
        if self.has_pending_candidates() {
            return self.advance_pending_candidate();
        }
        let DistributedVerifier::Tiling(verifier) = &mut self.verifier else {
            return Err(distributed_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed verifier has no resumable work",
            ));
        };
        let (partial, has_pending_work) = advance_tiling_worker(verifier, &self.control)?;
        Ok(WasmDistributedVerifierConsume {
            candidate_count: 0,
            partial,
            has_pending_work,
        })
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        self.verifier.progress()
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, WasmCommandRuntimeError> {
        if self.has_pending_candidates() {
            return Err(distributed_error(
                "E_WASM_DISTRIBUTED_STATE",
                "distributed verifier cannot finish with pending candidates",
            ));
        }
        if matches!(self.verifier, DistributedVerifier::Tiling(_)) {
            self.verifier
                .finish()
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", reason))?;
            return Ok(Vec::new());
        }
        if matches!(
            self.verifier,
            DistributedVerifier::Forward(_) | DistributedVerifier::Setup(_)
        ) {
            return Ok(Vec::new());
        }
        if matches!(&self.verifier, DistributedVerifier::BuildProbability(_)) {
            let _solution_probability_policy =
                self.build_solution_probability_policy.ok_or_else(|| {
                    distributed_error(
                        "E_WASM_DISTRIBUTED_VERIFY_FINISH",
                        "build probability solution policy authority is unavailable",
                    )
                })?;
            let DistributedVerifier::BuildProbability(verifier) = &mut self.verifier else {
                unreachable!("build verifier kind was checked above");
            };
            let results = verifier
                .finish()
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", reason))?;
            return finish_build_probability_worker_results(verifier, results);
        }
        let results = self
            .verifier
            .finish()
            .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", reason))?;
        let results = results
            .into_iter()
            .map(|result| {
                self.postprocessor
                    .materialize_distributed_postprocess_partition(result, &self.control)
                    .map_err(|error| {
                        distributed_error(
                            "E_WASM_DISTRIBUTED_SCORE_PARTITION",
                            format!("{error:?}"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        encode_partial_results(&results)
            .map_err(|error| distributed_error("E_WASM_DISTRIBUTED_PARTIAL_ENCODE", error.reason()))
    }
}

fn finish_build_probability_worker_results(
    verifier: &mut WasmBuildProbabilityDistributedVerifier,
    mut results: Vec<clearra_core_executor::CoreExecutionResult>,
) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    let source_bytes = checked_worker_result_vec_retained_bytes(&results)
        .ok_or_else(|| build_worker_memory_projection_error(verifier))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;
    verifier
        .validate_external_result_memory(source_bytes)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;

    let requested_processed_outer_bytes = (results.len() as u128)
        .checked_mul(core::mem::size_of::<clearra_core_executor::CoreExecutionResult>() as u128)
        .ok_or_else(|| build_worker_memory_projection_error(verifier))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;
    let source_and_requested_bytes = source_bytes
        .checked_add(requested_processed_outer_bytes)
        .ok_or_else(|| build_worker_memory_projection_error(verifier))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;
    verifier
        .validate_external_result_memory(source_and_requested_bytes)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;

    let mut processed = Vec::new();
    processed.try_reserve_exact(results.len()).map_err(|_| {
        distributed_error(
            "E_WASM_DISTRIBUTED_RESULT_STORAGE",
            "build probability processed result storage allocation failed",
        )
    })?;
    let allocated_bytes = checked_worker_result_vec_retained_bytes(&results)
        .and_then(|bytes| bytes.checked_add(checked_worker_result_vec_retained_bytes(&processed)?))
        .ok_or_else(|| build_worker_memory_projection_error(verifier))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;
    verifier
        .validate_external_result_memory(allocated_bytes)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;

    while let Some(result) = results.pop() {
        // A Build worker owns an unmaterialized partial. App finalization is a
        // coordinator-only transition: materializing here would add final-only
        // solution-probability fields which the strict worker wire correctly
        // rejects. The representative path is likewise not worker-wire
        // authority; requested finesse and post-process evidence are rebuilt
        // by the coordinator's retained Build finalizers and App terminal.
        let result = if result.path_steps().is_empty() {
            result
        } else {
            let source_result_bytes = result
                .checked_resource_retained_bytes()
                .ok_or_else(|| build_worker_memory_projection_error(verifier))
                .map_err(|error| {
                    distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error)
                })?;
            let transition_bytes = checked_worker_result_vec_retained_bytes(&results)
                .and_then(|bytes| {
                    bytes.checked_add(checked_worker_result_vec_retained_bytes(&processed)?)
                })
                .and_then(|bytes| bytes.checked_add(source_result_bytes))
                // `with_path_steps` rebuilds the derived execution report while
                // the source result is still live. A complete second result is
                // a conservative, allocator-independent upper bound for that
                // short transition.
                .and_then(|bytes| bytes.checked_add(source_result_bytes))
                .ok_or_else(|| build_worker_memory_projection_error(verifier))
                .map_err(|error| {
                    distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error)
                })?;
            verifier
                .validate_external_result_memory(transition_bytes)
                .map_err(|error| {
                    distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error)
                })?;
            result.with_path_steps(Vec::new())
        };
        let external_bytes = checked_worker_result_vec_retained_bytes(&results)
            .and_then(|bytes| {
                bytes.checked_add(checked_worker_result_vec_retained_bytes(&processed)?)
            })
            .and_then(|bytes| bytes.checked_add(result.checked_resource_retained_bytes()?))
            .ok_or_else(|| build_worker_memory_projection_error(verifier))
            .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;
        verifier
            .validate_external_result_memory(external_bytes)
            .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_VERIFY_FINISH", error))?;
        processed.push(result);
    }
    drop(results);
    processed.reverse();

    let processed_bytes = checked_worker_result_vec_retained_bytes(&processed)
        .ok_or_else(|| build_worker_memory_projection_error(verifier))
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_PARTIAL_ENCODE", error))?;
    let encoded = encode_partial_results_with_memory_guard(&processed, |wire_future_bytes| {
        let external_bytes = processed_bytes
            .checked_add(wire_future_bytes)
            .ok_or_else(|| build_worker_memory_projection_error(verifier))?;
        verifier.validate_external_result_memory(external_bytes)
    })
    .map_err(|error| match error {
        GuardedDistributedWireError::Wire(error) => {
            distributed_error("E_WASM_DISTRIBUTED_PARTIAL_ENCODE", error.reason())
        }
        GuardedDistributedWireError::MemoryGuard(error) => {
            distributed_search_error("E_WASM_DISTRIBUTED_PARTIAL_ENCODE", error)
        }
    })?;
    drop(processed);
    verifier
        .validate_external_result_memory(encoded.capacity() as u128)
        .map_err(|error| distributed_search_error("E_WASM_DISTRIBUTED_PARTIAL_ENCODE", error))?;
    Ok(encoded)
}

fn checked_worker_result_vec_retained_bytes(
    results: &Vec<clearra_core_executor::CoreExecutionResult>,
) -> Option<u128> {
    let result_inline = core::mem::size_of::<clearra_core_executor::CoreExecutionResult>() as u128;
    let mut retained = (results.capacity() as u128).checked_mul(result_inline)?;
    for result in results {
        retained = retained.checked_add(
            result
                .checked_resource_retained_bytes()?
                .checked_sub(result_inline)?,
        )?;
    }
    Some(retained)
}

fn build_worker_memory_projection_error(
    verifier: &WasmBuildProbabilityDistributedVerifier,
) -> WasmCpuSearchError {
    verifier
        .validate_external_result_memory(u128::MAX)
        .expect_err("overflow-sized build worker terminal storage is unavailable")
}

fn advance_tiling_worker(
    verifier: &mut WasmTilingRootWorker,
    control: &ExecutionControl,
) -> Result<(Option<Vec<u8>>, bool), WasmCommandRuntimeError> {
    let chunk = match verifier
        .advance(16_384, control)
        .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_TILING_GEOMETRY", reason))?
    {
        WasmTilingRootAdvance::Pending(chunk) | WasmTilingRootAdvance::Completed(chunk) => chunk,
        WasmTilingRootAdvance::Cancelled => {
            return Err(distributed_error(
                "E_WASM_DISTRIBUTED_TILING_GEOMETRY",
                "wasm_cpu_search_cancelled",
            ));
        }
    };
    let partial = (!chunk.is_empty()).then(|| encode_tiling_root_chunk(&chunk));
    Ok((partial, verifier.has_pending_work()))
}

pub fn serialize_distributed_final_events(
    job_id: u64,
    result: &WasmExecutionResult,
) -> Result<String, WasmCommandRuntimeError> {
    let job_id = WasmWorkerJobId::new(job_id);
    serialize_worker_events(&[
        WasmWorkerJobEvent::Progress {
            job_id,
            progress: JobProgress::new(2, 2, "AppResponse completed").with_backend_status(
                BackendStatus::from_execution(result.app_response(), result.webgpu_backend()),
            ),
        },
        WasmWorkerJobEvent::FinalResponse {
            job_id,
            response: result.app_response().clone(),
            webgpu_backend: result.webgpu_backend().clone(),
            search_report: result.search_report().cloned(),
        },
    ])
}

fn raw_distributed_requests_finite_memory(command_text: &str) -> bool {
    command_text
        .split_whitespace()
        .any(|token| token == "--max-memory-mib")
}

fn finite_authority_unavailable_error() -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_FINITE_AUTHORITY_UNAVAILABLE", String::new())
}

fn distributed_error(code: &'static str, reason: impl Into<String>) -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new(code, reason)
}

fn distributed_search_error(
    code: &'static str,
    error: WasmCpuSearchError,
) -> WasmCommandRuntimeError {
    let reason = error.reason();
    match error {
        WasmCpuSearchError::ResourceAdmission { resource_report } => {
            WasmCommandRuntimeError::new(code, reason).with_resource_report(resource_report)
        }
        _ => WasmCommandRuntimeError::new(code, reason),
    }
}

fn distributed_terminal_core_error(error: CoreExecutionError) -> WasmCommandRuntimeError {
    let reason = error
        .unsupported_reason()
        .unwrap_or("distributed_final_core_result_unavailable");
    drop(error);
    distributed_error("E_WASM_DISTRIBUTED_FINISH", reason)
}

const fn invalid_search_error(reason: &'static str) -> WasmCpuSearchError {
    WasmCpuSearchError::InvalidProblem { reason }
}

fn request_needs_distributed_execution(request: &AppRequest) -> bool {
    if matches!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcScore | ProductCapabilityContract::PcScoreMinimals)
    ) {
        return false;
    }
    let (workers, required_piece_count) = match request.command() {
        AppCommand::Pc(command) => (
            command.query().execution_policy().workers(),
            usize::from(command.query().target().lines()) * 10 / 4,
        ),
        AppCommand::Path(command) => (
            command.query().execution_policy().workers(),
            usize::from(command.query().target().lines()) * 10 / 4,
        ),
        AppCommand::Scenario(command) => {
            let query = command.query();
            let board = query.initial_board();
            let board_cells = usize::from(board.width()) * usize::from(board.visible_height());
            let required_cells =
                board_cells.saturating_sub(board.occupied_mask().count_ones() as usize);
            (
                query.execution_policy().workers(),
                query.exact_pieces().unwrap_or(required_cells / 4),
            )
        }
        AppCommand::BuildProbability(command) => {
            if command.query().finesse_score().is_some() {
                return false;
            }
            return command.query().core_query().execution_policy().workers() >= 2
                && command.query().target_piece_count() >= MIN_DISTRIBUTED_TARGET_PIECES;
        }
        _ => return false,
    };
    workers >= 2 && required_piece_count >= 7
}

fn build_probability_distributed_execution_is_worthwhile(
    field: clearra_problem::BuildProbabilityField,
    worker_count: usize,
) -> bool {
    worker_count >= 2 && field.target_piece_count() >= MIN_DISTRIBUTED_TARGET_PIECES
}

#[cfg(test)]
mod build_probability_partial_ingress_tests {
    use std::{
        cell::{Cell, RefCell},
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
        rc::Rc,
    };

    use crate::distributed_wire::decode_partial_results;
    use clearra_core_executor::CoreExecutionResult;

    use super::*;

    struct RecordingIngressAuthority {
        cap_bytes: u128,
        retained_bytes: u128,
        observations: RefCell<Vec<u128>>,
        absorbed: Vec<(String, u128)>,
    }

    impl RecordingIngressAuthority {
        fn new(cap_bytes: u128) -> Self {
            Self {
                cap_bytes,
                retained_bytes: 0,
                observations: RefCell::new(Vec::new()),
                absorbed: Vec::new(),
            }
        }

        fn authorize(
            &self,
            external_result_bytes: u128,
            checked_future_bytes: u128,
        ) -> Result<(), &'static str> {
            let observed = self
                .retained_bytes
                .checked_add(external_result_bytes)
                .and_then(|bytes| bytes.checked_add(checked_future_bytes))
                .ok_or("memory-projection-overflow")?;
            self.observations.borrow_mut().push(observed);
            (observed <= self.cap_bytes)
                .then_some(())
                .ok_or("memory-budget-exceeded")
        }
    }

    impl BuildProbabilityPartialIngressAuthority for RecordingIngressAuthority {
        type Error = &'static str;

        fn validate_external_result_memory(
            &self,
            external_result_bytes: u128,
            checked_future_bytes: u128,
        ) -> Result<(), Self::Error> {
            self.authorize(external_result_bytes, checked_future_bytes)
        }

        fn absorb_with_external_retained(
            &mut self,
            result: &CoreExecutionResult,
            external_result_bytes: u128,
        ) -> Result<(), Self::Error> {
            self.authorize(external_result_bytes, 0)?;
            self.absorbed.push((
                result
                    .field("build_distributed_pass_index")
                    .unwrap_or("missing")
                    .to_owned(),
                external_result_bytes,
            ));
            self.retained_bytes = self
                .retained_bytes
                .checked_add(17)
                .ok_or("memory-projection-overflow")?;
            Ok(())
        }
    }

    fn sibling_results() -> Vec<CoreExecutionResult> {
        [0, 1]
            .into_iter()
            .map(|pass_index| {
                CoreExecutionResult::new(
                    vec![
                        ("search_kind".to_owned(), "build-probability".to_owned()),
                        (
                            "build_distributed_pass_index".to_owned(),
                            pass_index.to_string(),
                        ),
                        (
                            "worker_payload".to_owned(),
                            format!("sibling-{pass_index}-payload"),
                        ),
                    ],
                    Vec::new(),
                )
            })
            .collect()
    }

    const FINITE_DISTRIBUTED_BUILD: &str = "clearra build-probability \
        --base-mask 0x0 --target-mask 0xfc3f3fcff --height 4 \
        --queue OOOOOOO --no-hold --no-mirror --workers 2 \
        --max-memory-mib 256";
    const FINITE_SERIAL_BUILD: &str = "clearra build-probability \
        --base-mask 0x0 --target-mask 0xf --height 4 --queue I \
        --no-hold --no-mirror --workers 1 --max-memory-mib 64";

    #[test]
    fn public_raw_distributed_finite_entries_fail_before_parser_authority() {
        let runtime = WasmCommandRuntime::default();
        assert!(raw_distributed_requests_finite_memory(
            "not even a Clearra command --max-memory-mib 64"
        ));
        assert!(raw_distributed_requests_finite_memory(
            "not-even-clearra\u{2003}--max-memory-mib\u{2003}64"
        ));
        assert!(!raw_distributed_requests_finite_memory(
            "clearra build-probability --max-memory-mib=64"
        ));
        assert!(!raw_distributed_requests_finite_memory(
            "clearra build-probability --max-memory-mib-extra 64"
        ));

        for command in [
            "not even a Clearra command --max-memory-mib 64",
            "not-even-clearra\u{2003}--max-memory-mib\u{2003}64",
        ] {
            let coordinator_error = match WasmDistributedCoordinator::prepare(&runtime, command) {
                Err(error) => error,
                Ok(_) => panic!("raw finite coordinator preparation must remain inactive"),
            };
            assert_eq!(
                coordinator_error.code(),
                "E_WASM_FINITE_AUTHORITY_UNAVAILABLE"
            );
            assert!(coordinator_error.message().is_empty());
            assert_eq!(coordinator_error.message_capacity_for_test(), 0);
            assert!(coordinator_error.resource_report().is_none());

            let verifier_error = match WasmDistributedVerifierRuntime::prepare(&runtime, command) {
                Err(error) => error,
                Ok(_) => panic!("raw finite verifier preparation must remain inactive"),
            };
            assert_eq!(verifier_error.code(), "E_WASM_FINITE_AUTHORITY_UNAVAILABLE");
            assert!(verifier_error.message().is_empty());
            assert_eq!(verifier_error.message_capacity_for_test(), 0);
            assert!(verifier_error.resource_report().is_none());
        }
    }

    fn finishable_build_coordinator(
        runtime: &WasmCommandRuntime,
        command: &str,
    ) -> WasmDistributedCoordinator {
        let preparation =
            WasmDistributedCoordinator::prepare_finite_transport_fixture_for_test(runtime, command)
                .expect("test-only distributed Build transport fixture");
        let mut coordinator = match preparation {
            WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
            _ => panic!("finite Build fixture must use the distributed coordinator"),
        };
        let mut verifier = coordinator
            .prepare_in_process_verifier(runtime, command)
            .expect("distributed Build verifier");
        loop {
            match coordinator
                .advance_producer(16_384, 16)
                .expect("distributed Build producer")
            {
                WasmDistributedProducerAdvance::Pending
                | WasmDistributedProducerAdvance::Initialization(_) => {}
                WasmDistributedProducerAdvance::Batch(batch) => {
                    let mut consumed = verifier
                        .consume_with_external_retained(&batch, batch.capacity() as u128)
                        .expect("Build candidate batch");
                    if let Some(partial) = consumed.partial.take() {
                        coordinator
                            .absorb_partial_with_external_retained(
                                &partial,
                                partial.capacity() as u128,
                            )
                            .expect("streamed Build partial");
                    }
                    while consumed.has_pending_work {
                        consumed = verifier.continue_work().expect("continued Build work");
                        if let Some(partial) = consumed.partial.take() {
                            coordinator
                                .absorb_partial_with_external_retained(
                                    &partial,
                                    partial.capacity() as u128,
                                )
                                .expect("continued Build partial");
                        }
                    }
                }
                WasmDistributedProducerAdvance::Completed => break,
                WasmDistributedProducerAdvance::Cancelled => {
                    panic!("finite Build fixture was cancelled")
                }
            }
        }
        let partial = verifier.finish().expect("final Build worker partial");
        if !partial.is_empty() {
            coordinator
                .absorb_partial_with_external_retained(&partial, partial.capacity() as u128)
                .expect("final Build partial merge");
        }
        coordinator
    }

    fn serial_governed_result() -> GovernedWasmExecutionResult {
        let runtime = WasmCommandRuntime::default();
        // Test-only precondition: compatibility parsing is intentionally
        // outside the claimed authority. Only the already-prepared bridge is
        // under test here; this fixture is not public raw-text evidence.
        let prepared = runtime
            .prepare_command_text(FINITE_SERIAL_BUILD)
            .expect("prepare finite serial Build fixture");
        runtime
            .execute_prepared_governed(prepared)
            .expect("already-prepared finite serial Build fixture")
    }

    #[test]
    fn build_probability_ingress_counts_input_all_siblings_and_merger_state_at_exact_peak() {
        let source = sibling_results();
        let input = encode_partial_results(&source).expect("encode sibling ingress batch");
        let decoded = decode_partial_results(&input).expect("decode expected sibling batch");
        let decoded_bytes = checked_worker_result_vec_retained_bytes(&decoded)
            .expect("checked decoded sibling bytes");
        let expected_external = (input.len() as u128)
            .checked_add(decoded_bytes)
            .expect("checked input and decoded bytes");

        let mut observed = RecordingIngressAuthority::new(u128::MAX);
        absorb_build_probability_partial_batch(&input, &mut observed)
            .expect("unbounded ingress observation");
        assert_eq!(
            observed.absorbed,
            vec![
                ("0".to_owned(), expected_external),
                ("1".to_owned(), expected_external)
            ]
        );
        assert!(
            expected_external
                > input.len() as u128
                    + decoded[0]
                        .checked_resource_retained_bytes()
                        .expect("first sibling bytes"),
            "the absorb authority must retain the input, outer Vec, and every decoded sibling"
        );
        let observations = observed.observations.into_inner();
        let peak = observations
            .into_iter()
            .max()
            .expect("ingress memory observations");

        let mut exact = RecordingIngressAuthority::new(peak);
        absorb_build_probability_partial_batch(&input, &mut exact)
            .expect("the exact observed ingress peak must fit");
        assert_eq!(exact.absorbed.len(), 2, "round-trip sibling parity");

        let mut below = RecordingIngressAuthority::new(peak - 1);
        assert!(matches!(
            absorb_build_probability_partial_batch(&input, &mut below),
            Err(GuardedDistributedWireError::MemoryGuard(
                "memory-budget-exceeded"
            ))
        ));
    }

    #[test]
    fn build_probability_ingress_counts_raw_transfer_capacity_slack_at_exact_peak() {
        let source = sibling_results();
        let encoded = encode_partial_results(&source).expect("encode sibling ingress batch");
        let mut input = Vec::with_capacity(encoded.len() + 4_096);
        input.extend_from_slice(&encoded);
        assert!(
            input.capacity() > input.len(),
            "the transfer owner has slack"
        );
        let decoded = decode_partial_results(&input).expect("decode expected sibling batch");
        let decoded_bytes = checked_worker_result_vec_retained_bytes(&decoded)
            .expect("checked decoded sibling bytes");
        let expected_external = (input.capacity() as u128)
            .checked_add(decoded_bytes)
            .expect("checked raw input and decoded bytes");

        let mut observed = RecordingIngressAuthority::new(u128::MAX);
        absorb_build_probability_partial_batch_with_external_retained(
            &input,
            input.capacity() as u128,
            &mut observed,
        )
        .expect("unbounded ingress observation");
        assert_eq!(
            observed.absorbed,
            vec![
                ("0".to_owned(), expected_external),
                ("1".to_owned(), expected_external)
            ]
        );
        let peak = observed
            .observations
            .into_inner()
            .into_iter()
            .max()
            .expect("ingress memory observations");

        let mut exact = RecordingIngressAuthority::new(peak);
        absorb_build_probability_partial_batch_with_external_retained(
            &input,
            input.capacity() as u128,
            &mut exact,
        )
        .expect("the exact raw-capacity ingress peak must fit");

        let mut below = RecordingIngressAuthority::new(peak - 1);
        assert!(matches!(
            absorb_build_probability_partial_batch_with_external_retained(
                &input,
                input.capacity() as u128,
                &mut below,
            ),
            Err(GuardedDistributedWireError::MemoryGuard(
                "memory-budget-exceeded"
            ))
        ));

        let mut impossible_raw_owner = RecordingIngressAuthority::new(u128::MAX);
        let error = absorb_build_probability_partial_batch_with_external_retained(
            &input,
            input.len() as u128 - 1,
            &mut impossible_raw_owner,
        )
        .expect_err("a raw owner smaller than the borrowed slice must fail closed");
        match error {
            GuardedDistributedWireError::Wire(error) => {
                assert_eq!(error.reason(), "partial_decode_memory_projection_overflow")
            }
            GuardedDistributedWireError::MemoryGuard(error) => {
                panic!("raw-owner shape failed through the memory guard: {error}")
            }
        }
        assert!(
            impossible_raw_owner.observations.into_inner().is_empty(),
            "a raw retained-byte claim below the borrowed slice length fails before decoding"
        );
    }

    #[test]
    fn build_candidate_transition_counts_actual_outer_nested_and_wire_capacities() {
        let mut first_rows = Vec::with_capacity(9);
        first_rows.extend([3_u32, 7]);
        let first_row_capacity = first_rows.capacity();
        let mut second_rows = Vec::with_capacity(13);
        second_rows.extend([11_u32, 17, 23]);
        let second_row_capacity = second_rows.capacity();
        let mut candidates = Vec::with_capacity(5);
        candidates.push(WasmCandidatePacket::for_pass(1, 0, 2, first_rows));
        candidates.push(WasmCandidatePacket::for_pass(4, 1, 6, second_rows));
        let mut wire = Vec::with_capacity(97);
        wire.extend([0_u8; 7]);

        let expected = (candidates.capacity() as u128)
            .checked_mul(core::mem::size_of::<WasmCandidatePacket>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    ((first_row_capacity + second_row_capacity) as u128)
                        .checked_mul(core::mem::size_of::<u32>() as u128)?,
                )
            })
            .and_then(|bytes| bytes.checked_add(wire.capacity() as u128))
            .expect("checked candidate transition bytes");
        let observed = checked_build_candidate_transition_retained_bytes(&candidates, Some(&wire))
            .expect("candidate transition projection");

        assert_eq!(observed, expected);
        assert!(observed <= expected, "the exact boundary must admit");
        assert!(
            observed > expected - 1,
            "one byte below the exact boundary must reject"
        );
    }

    #[test]
    fn terminal_completion_callback_runs_only_after_authority_drop() {
        struct DropProbe(Rc<Cell<bool>>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let dropped = Rc::new(Cell::new(false));
        let completed = after_authority_drop(DropProbe(Rc::clone(&dropped)), || {
            assert!(dropped.get(), "completion observed a live authority");
            "complete"
        });

        assert_eq!(completed, "complete");
        assert!(dropped.get());
    }

    #[test]
    fn finite_build_governed_json_matches_the_legacy_terminal_schema() {
        let runtime = WasmCommandRuntime::default()
            .with_host_capabilities(crate::WasmHostCapabilities::new(4, false, false));
        let job_id = WasmWorkerJobId::new(0x1_0000_0042);

        let governed = finishable_build_coordinator(&runtime, FINITE_DISTRIBUTED_BUILD)
            .finish_governed(2)
            .expect("finite distributed governed result");
        assert!(governed.result().tiling_solution_page_store().is_none());
        let compatibility = serialize_distributed_final_events(job_id.get(), governed.result())
            .expect("legacy terminal schema fixture");
        let events = GovernedWasmWorkerEvents::try_from_final_result(job_id, governed)
            .expect("governed terminal events");
        assert_eq!(events.events().len(), 2);
        assert!(matches!(
            &events.events()[0],
            WasmWorkerJobEvent::Progress {
                job_id: observed_job_id,
                progress,
            } if *observed_job_id == job_id
                && progress.done == 2
                && progress.total == 2
                && progress.label == "AppResponse completed"
        ));
        assert!(matches!(
            &events.events()[1],
            WasmWorkerJobEvent::FinalResponse {
                job_id: observed_job_id,
                ..
            } if *observed_job_id == job_id
        ));
        let governed_json =
            serialize_governed_worker_events(events).expect("governed distributed terminal JSON");
        assert_eq!(governed_json.json(), compatibility);
        drop(governed_json);

        let direct = finishable_build_coordinator(&runtime, FINITE_DISTRIBUTED_BUILD)
            .finish_governed_json(2, job_id)
            .expect("direct governed distributed terminal JSON");
        assert_eq!(direct.json().matches("\"schema_version\":1").count(), 2);
        assert_eq!(direct.json().matches("\"job_id\":4294967362").count(), 2);
        assert!(direct.json().contains("\"event\":\"progress\""));
        assert!(direct.json().contains("\"event\":\"final_response\""));
    }

    #[test]
    fn legacy_finish_rejects_a_finite_distributed_build_authority() {
        let runtime = WasmCommandRuntime::default()
            .with_host_capabilities(crate::WasmHostCapabilities::new(4, false, false));
        let error = finishable_build_coordinator(&runtime, FINITE_DISTRIBUTED_BUILD)
            .finish(2)
            .expect_err("legacy finish must not discard finite response authority");

        assert_eq!(error.code(), "E_WASM_DISTRIBUTED_FINISH");
        assert_eq!(
            error.message(),
            "distributed_finite_response_requires_governed_completion"
        );
    }

    #[test]
    fn governed_terminal_events_accept_exact_peak_and_reject_peak_minus_one() {
        let measured_result = serial_governed_result();
        assert!(measured_result
            .result()
            .tiling_solution_page_store()
            .is_none());
        let source_actual = measured_result.authority().actual_retained_bytes();
        let transport_heap = measured_result
            .result()
            .checked_transport_retained_capacity_bytes()
            .expect("finite transport heap");
        let measured_events = GovernedWasmWorkerEvents::try_from_final_result(
            WasmWorkerJobId::new(7),
            measured_result,
        )
        .expect("measure terminal event transition");
        let target_allocation_bytes = measured_events
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<GovernedWasmWorkerEvents>() as u128)
            .and_then(|bytes| bytes.checked_sub(transport_heap))
            .expect("event target allocation bytes");
        let governed_metadata = (core::mem::size_of::<GovernedWasmExecutionResult>()
            .max(core::mem::size_of::<(
                WasmExecutionResult,
                crate::WasmExecutionMemoryAuthority,
            )>())
            .max(core::mem::size_of::<
                Result<GovernedWasmExecutionResult, WasmCommandRuntimeError>,
            >()) as u128)
            .checked_sub(core::mem::size_of::<WasmExecutionResult>() as u128)
            .expect("governed result transition metadata");
        let exact_peak = source_actual
            .checked_add(governed_metadata)
            .and_then(|bytes| {
                bytes.checked_add(
                    core::mem::size_of::<std::collections::VecDeque<WasmWorkerJobEvent>>()
                        .max(core::mem::size_of::<
                            Option<std::collections::VecDeque<WasmWorkerJobEvent>>,
                        >()) as u128,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<Vec<WasmWorkerJobEvent>>() as u128)
            })
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<JobProgress>() as u128))
            .and_then(|bytes| bytes.checked_add(target_allocation_bytes))
            .expect("event exact peak");
        let event_payload_heap = measured_events
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<GovernedWasmWorkerEvents>() as u128)
            .expect("event payload heap");
        let returned_event_carrier = core::mem::size_of::<GovernedWasmWorkerEvents>()
            .max(core::mem::size_of::<
                Result<GovernedWasmWorkerEvents, WasmCommandRuntimeError>,
            >())
            .max(core::mem::size_of::<(
                Vec<WasmWorkerJobEvent>,
                Option<std::sync::Arc<clearra_core_executor::TilingSolutionPageStore>>,
                u128,
                u128,
            )>()) as u128;
        let returned_peak = event_payload_heap
            .checked_add(returned_event_carrier)
            .expect("event return peak");
        let exact_peak = exact_peak.max(returned_peak);
        drop(measured_events);

        let exact = GovernedWasmWorkerEvents::try_from_final_result(
            WasmWorkerJobId::new(7),
            serial_governed_result().with_memory_limit_for_test(exact_peak),
        )
        .expect("exact terminal event peak is admitted");
        drop(exact);
        let error = GovernedWasmWorkerEvents::try_from_final_result(
            WasmWorkerJobId::new(7),
            serial_governed_result().with_memory_limit_for_test(exact_peak - 1),
        )
        .expect_err("terminal event peak minus one is rejected");
        assert_eq!(error.code(), "E_WASM_EVENT_MEMORY_LIMIT");
    }

    #[test]
    fn governed_terminal_json_accepts_exact_peak_and_reject_peak_minus_one() {
        let measured_events = GovernedWasmWorkerEvents::try_from_final_result(
            WasmWorkerJobId::new(11),
            serial_governed_result(),
        )
        .expect("measure terminal events");
        let source_actual = measured_events.actual_retained_bytes();
        let measured_json = serialize_governed_worker_events(measured_events)
            .expect("measure governed JSON transition");
        assert!(measured_json
            .completed_tiling_solution_page_store()
            .is_none());
        let source_wrapper_inline = core::mem::size_of::<GovernedWasmWorkerEvents>() as u128;
        let source_payload_heap = source_actual
            .checked_sub(source_wrapper_inline)
            .expect("event payload heap");
        let json_payload_heap = measured_json
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<GovernedWasmJson>() as u128)
            .expect("JSON payload heap");
        let construction_peak = source_payload_heap
            .checked_add(crate::json_event_envelope::governed_event_source_carrier_inline_bytes())
            .and_then(|bytes| {
                bytes.checked_add(crate::json_event_envelope::json_build_carrier_inline_bytes())
            })
            .and_then(|bytes| bytes.checked_add(json_payload_heap))
            .expect("JSON construction peak");
        let returned_peak = json_payload_heap
            .checked_add(crate::json_event_envelope::governed_json_returned_carrier_inline_bytes())
            .expect("JSON return peak");
        let exact_peak = construction_peak.max(returned_peak);
        let expected_len = measured_json.json().len();
        let mut expected_hasher = DefaultHasher::new();
        measured_json.json().hash(&mut expected_hasher);
        let expected_hash = expected_hasher.finish();
        drop(measured_json);

        let exact_events = GovernedWasmWorkerEvents::try_from_final_result(
            WasmWorkerJobId::new(11),
            serial_governed_result(),
        )
        .expect("exact JSON event fixture")
        .with_memory_limit_for_test(exact_peak);
        let exact = serialize_governed_worker_events(exact_events)
            .expect("exact governed JSON peak is admitted");
        let mut exact_hasher = DefaultHasher::new();
        exact.json().hash(&mut exact_hasher);
        assert_eq!(exact.json().len(), expected_len);
        assert_eq!(exact_hasher.finish(), expected_hash);
        drop(exact);

        let below_events = GovernedWasmWorkerEvents::try_from_final_result(
            WasmWorkerJobId::new(11),
            serial_governed_result(),
        )
        .expect("below JSON event fixture")
        .with_memory_limit_for_test(exact_peak - 1);
        let error = serialize_governed_worker_events(below_events)
            .expect_err("governed JSON peak minus one is rejected");
        assert_eq!(error.code(), "E_WASM_JSON_MEMORY_LIMIT");
    }

    #[test]
    fn final_build_batch_requires_a_later_completed_signal() {
        let summary = WasmDistributedGeometrySummary {
            candidate_count: 7,
            candidate_digest: 11,
            candidate_family_count: Some(13),
            expanded_nodes: 17,
            peak_frontier: 19,
            domain_pruned_states: 23,
            hall_pruned_states: 29,
            column_pruned_states: 31,
            component_compositions: 37,
            truncated_reason: None,
            backend_execution: clearra_core_executor::WasmDistributedBackendExecution::Cpu,
        };
        let mut coordinator = WasmDistributedCoordinator {
            prepared: None,
            producer: None,
            merger: None,
            pending_build_completion_summary: Some(summary),
            summary: None,
            completed_progress: WasmDistributedProgress::default(),
            forward_completed: false,
            worker_count: 2,
            verification_required: true,
            webgpu_requested: false,
            mode: WasmDistributedMode::CpuMulti,
            requested_backend: WasmDistributedRequestedBackend::Cpu,
            preparation_fallback_reason: WasmDistributedFallbackReason::None,
            backend_execution_override: None,
            control: ExecutionControl::default(),
        };

        assert!(!coordinator.producer_completed());
        assert_eq!(coordinator.progress().candidates, 7);
        assert_eq!(
            coordinator
                .advance_producer(1, 1)
                .expect("report pending Build completion"),
            WasmDistributedProducerAdvance::Completed
        );
        assert!(coordinator.producer_completed());
        assert!(coordinator.pending_build_completion_summary.is_none());
        assert_eq!(
            coordinator
                .summary
                .as_ref()
                .map(|value| value.candidate_count),
            Some(7)
        );
    }

    #[test]
    fn build_tiling_only_uses_build_probability_authorities() {
        let runtime = WasmCommandRuntime::default()
            .with_host_capabilities(crate::WasmHostCapabilities::new(4, false, false));
        let command = "clearra build-probability --base-mask 0x0 \
            --target-mask 0xffffffffff --height 4 --queue OTSZJLIOTI \
            --no-hold --no-mirror --tiling-only --workers 2";
        let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
            .expect("distributed Build tiling preparation");
        let coordinator = match preparation {
            WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
            _ => panic!("Build tiling must retain the distributed Build authority"),
        };

        assert!(matches!(
            coordinator.producer.as_ref(),
            Some(DistributedCandidateProducer::BuildProbability(_))
        ));
        assert!(!coordinator.tiling_geometry_parallel());
        let verifier = coordinator
            .prepare_in_process_verifier(&runtime, command)
            .expect("delegated Build verifier");
        assert!(matches!(
            verifier.verifier,
            DistributedVerifier::BuildProbability(_)
        ));
    }

    #[test]
    fn pc_verifier_returns_to_the_host_between_candidate_transactions() {
        let runtime = WasmCommandRuntime::default()
            .with_host_capabilities(crate::WasmHostCapabilities::new(4, false, false));
        let command = "clearra pc --lines 4 --count unique \
            --backend cpu --workers 2";
        let preparation = WasmDistributedCoordinator::prepare(&runtime, command)
            .expect("distributed PC preparation");
        let mut coordinator = match preparation {
            WasmDistributedPreparation::Coordinator(coordinator) => coordinator,
            _ => panic!("two-worker PC must use the distributed coordinator"),
        };
        let mut verifier = coordinator
            .prepare_in_process_verifier(&runtime, command)
            .expect("distributed PC verifier");
        let batch = (0..100_000)
            .find_map(|_| {
                match coordinator
                    .advance_producer(16_384, 16)
                    .expect("PC geometry producer")
                {
                    WasmDistributedProducerAdvance::Batch(batch) => Some(batch),
                    WasmDistributedProducerAdvance::Pending
                    | WasmDistributedProducerAdvance::Initialization(_) => None,
                    WasmDistributedProducerAdvance::Completed => {
                        panic!("PC geometry completed before emitting a batch")
                    }
                    WasmDistributedProducerAdvance::Cancelled => {
                        panic!("PC geometry was unexpectedly cancelled")
                    }
                }
            })
            .expect("bounded PC geometry must emit a candidate batch");
        let expected_candidates = decode_candidate_batch(&batch)
            .expect("canonical candidate batch")
            .len();
        assert!(expected_candidates > 1, "fixture must exercise the cursor");

        let mut consumed = verifier
            .consume(&batch)
            .expect("first candidate transaction");
        assert_eq!(consumed.candidate_count, 1);
        assert!(consumed.has_pending_work);
        let finish_error = verifier
            .finish()
            .expect_err("pending candidate batches must reject early finish");
        assert_eq!(finish_error.code(), "E_WASM_DISTRIBUTED_STATE");

        let mut consumed_candidates = consumed.candidate_count;
        while consumed.has_pending_work {
            consumed = verifier
                .continue_work()
                .expect("continued candidate transaction");
            assert!(consumed.candidate_count <= 1);
            consumed_candidates += consumed.candidate_count;
        }
        assert_eq!(consumed_candidates, expected_candidates);
        assert!(!verifier.has_pending_candidates());
    }
}

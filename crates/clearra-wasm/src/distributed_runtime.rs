use clearra_app::{
    AppCommand, AppCoreExecutorService, AppRequest, DistributedForwardPreparation,
    DistributedSearchPreparation, DistributedSetupPreparation, ExecutionControl,
    PreparedDistributedForwardSearch, PreparedDistributedSearch, PreparedDistributedSetupSearch,
};
#[cfg(feature = "webgpu-search")]
use clearra_core_executor::WasmWebGpuCandidateProducer;
use clearra_core_executor::{
    WasmBuildProbabilityCandidateProducer, WasmBuildProbabilityDistributedResultMerger,
    WasmBuildProbabilityDistributedVerifier, WasmCandidatePacket, WasmCandidateProducerAdvance,
    WasmCpuCandidateProducer, WasmCpuSearchBackend, WasmDistributedGeometrySummary,
    WasmDistributedProgress, WasmDistributedResultMerger, WasmDistributedVerifier,
    WasmProductSearchBackend, WasmSetupParallelCoordinator, WasmSetupParallelProduce,
    WasmSetupParallelWorker,
};
use clearra_forward_search::{
    ForwardParallelCoordinator, ForwardParallelProduce, ForwardParallelWorker,
};
use clearra_pc_graph::request::RequestedSearchBackend;

use crate::{
    distributed_wire::{
        decode_candidate_batch, decode_partial_results, encode_candidate_batch,
        encode_partial_results,
    },
    json_event_envelope::serialize_worker_events,
    BackendStatus, JobProgress, WasmCommandRuntime, WasmCommandRuntimeError, WasmExecutionResult,
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
    summary: Option<WasmDistributedGeometrySummary>,
    completed_progress: WasmDistributedProgress,
    forward_completed: bool,
    worker_count: usize,
    webgpu_requested: bool,
    mode: WasmDistributedMode,
    requested_backend: WasmDistributedRequestedBackend,
    preparation_fallback_reason: WasmDistributedFallbackReason,
    backend_execution_override: Option<clearra_core_executor::WasmDistributedBackendExecution>,
    control: ExecutionControl,
}

enum DistributedCandidateProducer {
    Cpu(WasmCpuCandidateProducer),
    BuildProbability(WasmBuildProbabilityCandidateProducer),
    Forward(ForwardParallelCoordinator),
    Setup(WasmSetupParallelCoordinator),
    #[cfg(feature = "webgpu-search")]
    WebGpu(WasmWebGpuCandidateProducer),
}

pub struct WasmDistributedVerifierRuntime {
    verifier: DistributedVerifier,
    postprocessor: AppCoreExecutorService,
    control: ExecutionControl,
}

enum DistributedVerifier {
    Pc(WasmDistributedVerifier),
    BuildProbability(WasmBuildProbabilityDistributedVerifier),
    Forward(ForwardParallelWorker),
    Setup(WasmSetupParallelWorker),
}

enum DistributedResultMerger {
    Pc(WasmDistributedResultMerger),
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
}

impl WasmDistributedCoordinator {
    pub fn prepare(
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
                summary: None,
                completed_progress: WasmDistributedProgress::default(),
                forward_completed: false,
                worker_count,
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
                summary: None,
                completed_progress: WasmDistributedProgress::default(),
                forward_completed: false,
                worker_count: workers,
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
        let explicit_gpu = matches!(
            requested_backend,
            RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
        );
        let webgpu_connected = build_probability_request.is_none()
            && problem.backend_policy().runtime_webgpu_available()
            && cfg!(feature = "webgpu-search");
        if explicit_gpu && !webgpu_connected && !problem.backend_policy().allow_backend_fallback() {
            return Ok(WasmDistributedPreparation::Serial);
        }
        let backend_execution_override = (explicit_gpu && !webgpu_connected).then_some(
            clearra_core_executor::WasmDistributedBackendExecution::CpuFallback {
                reason: "gpu_kernel_unavailable",
                failure_class: "unavailable",
                failure_stage: "capability-query",
                discarded_partial_gpu_result: false,
                original_gpu_result_incomplete: false,
            },
        );
        let preparation_fallback_reason = if backend_execution_override.is_some() {
            WasmDistributedFallbackReason::GpuKernelUnavailable
        } else {
            WasmDistributedFallbackReason::None
        };
        let (producer, mode) = if let Some((field, aggregation)) = build_probability_request {
            (
                DistributedCandidateProducer::BuildProbability(
                    WasmBuildProbabilityCandidateProducer::new(problem, field, aggregation)
                        .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_START", reason))?,
                ),
                WasmDistributedMode::CpuMulti,
            )
        } else {
            match WasmCpuSearchBackend::selected_product_backend(problem) {
                WasmProductSearchBackend::Cpu => (
                    DistributedCandidateProducer::Cpu(
                        WasmCpuCandidateProducer::new(problem).map_err(|reason| {
                            distributed_error("E_WASM_DISTRIBUTED_START", reason)
                        })?,
                    ),
                    WasmDistributedMode::CpuMulti,
                ),
                WasmProductSearchBackend::WebGpu => {
                    #[cfg(feature = "webgpu-search")]
                    {
                        (
                            DistributedCandidateProducer::WebGpu(
                                WasmWebGpuCandidateProducer::new(problem).map_err(|reason| {
                                    distributed_error("E_WASM_DISTRIBUTED_START", reason)
                                })?,
                            ),
                            WasmDistributedMode::WebGpuMulti,
                        )
                    }
                    #[cfg(not(feature = "webgpu-search"))]
                    {
                        return Ok(WasmDistributedPreparation::Serial);
                    }
                }
            }
        };
        Ok(WasmDistributedPreparation::Coordinator(Self {
            prepared: Some(DistributedPreparedSearch::Core(prepared)),
            producer: Some(producer),
            merger: None,
            summary: None,
            completed_progress: WasmDistributedProgress::default(),
            forward_completed: false,
            worker_count,
            webgpu_requested,
            mode,
            requested_backend: distributed_requested_backend,
            preparation_fallback_reason,
            backend_execution_override,
            control: ExecutionControl::default(),
        }))
    }

    pub const fn mode(&self) -> WasmDistributedMode {
        self.mode
    }

    pub const fn worker_count(&self) -> usize {
        self.worker_count
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
        self.summary
            .as_ref()
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
                    self.merger = Some(producer.into_merger().map_err(|reason| {
                        distributed_error("E_WASM_DISTRIBUTED_FORWARD_MERGER", reason)
                    })?);
                    self.forward_completed = true;
                    Ok(WasmDistributedProducerAdvance::Completed)
                }
            };
        }
        let producer = self.producer.as_mut().ok_or_else(|| {
            distributed_error(
                "E_WASM_DISTRIBUTED_STATE",
                "candidate producer is not active",
            )
        })?;
        let mut candidates = Vec::<WasmCandidatePacket>::new();
        let batch_capacity = batch_capacity.max(1);
        candidates.reserve(batch_capacity);
        for _ in 0..work_budget.max(1) {
            match producer
                .advance(&self.control)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_PRODUCER", reason))?
            {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(candidate) => {
                    candidates.push(candidate);
                    if candidates.len() == batch_capacity {
                        return Ok(WasmDistributedProducerAdvance::Batch(
                            encode_candidate_batch(&candidates),
                        ));
                    }
                }
                WasmCandidateProducerAdvance::Completed(mut summary) => {
                    if let Some(execution) = self.backend_execution_override.clone() {
                        summary.backend_execution = execution;
                    }
                    let producer = self.producer.take().ok_or_else(|| {
                        distributed_error(
                            "E_WASM_DISTRIBUTED_STATE",
                            "candidate producer disappeared before completion",
                        )
                    })?;
                    self.merger = Some(producer.into_merger().map_err(|reason| {
                        distributed_error("E_WASM_DISTRIBUTED_MERGER", reason)
                    })?);
                    self.summary = Some(summary);
                    if candidates.is_empty() {
                        return Ok(WasmDistributedProducerAdvance::Completed);
                    }
                    return Ok(WasmDistributedProducerAdvance::Batch(
                        encode_candidate_batch(&candidates),
                    ));
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
                encode_candidate_batch(&candidates),
            ))
        }
    }

    pub fn producer_completed(&self) -> bool {
        self.summary.is_some() || self.forward_completed
    }

    pub fn absorb_partial(&mut self, input: &[u8]) -> Result<(), WasmCommandRuntimeError> {
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
        let results = decode_partial_results(input).map_err(|error| {
            distributed_error("E_WASM_DISTRIBUTED_PARTIAL_INVALID", error.reason())
        })?;
        let merger = self.merger.as_mut().ok_or_else(|| {
            distributed_error("E_WASM_DISTRIBUTED_STATE", "result merger is not ready")
        })?;
        for result in &results {
            merger
                .absorb(result)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_MERGE", reason))?;
        }
        Ok(())
    }

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
            let result = producer.finish(workers_used).map_err(|error| {
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
            let report = merger.finish().map_err(|error| {
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
        let result = self
            .merger
            .as_mut()
            .ok_or_else(|| {
                distributed_error("E_WASM_DISTRIBUTED_STATE", "result merger is not ready")
            })?
            .finish(&summary, workers_used.min(self.worker_count).max(1))
            .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_FINISH", reason))?;
        let response = match self.prepared.take() {
            Some(DistributedPreparedSearch::Core(prepared)) => {
                prepared.complete(result, &self.control)
            }
            _ => {
                return Err(distributed_error(
                    "E_WASM_DISTRIBUTED_STATE",
                    "core app search is not prepared",
                ));
            }
        };
        Ok(WasmExecutionResult::from_app_response(
            response,
            self.webgpu_requested,
        ))
    }
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
    fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, &'static str> {
        match self {
            Self::Cpu(producer) => producer.advance(control),
            Self::BuildProbability(producer) => producer.advance(control),
            Self::Forward(_) => Err("forward_producer_requires_batch_advance"),
            Self::Setup(_) => Err("setup_producer_requires_task_batch_advance"),
            #[cfg(feature = "webgpu-search")]
            Self::WebGpu(producer) => producer.advance(control),
        }
    }

    fn into_merger(self) -> Result<DistributedResultMerger, &'static str> {
        match self {
            Self::Cpu(producer) => producer.into_merger().map(DistributedResultMerger::Pc),
            Self::BuildProbability(producer) => producer
                .into_merger()
                .map(DistributedResultMerger::BuildProbability),
            Self::Forward(producer) => Ok(DistributedResultMerger::Forward(producer)),
            Self::Setup(_) => Err("setup_producer_owns_its_result_merger"),
            #[cfg(feature = "webgpu-search")]
            Self::WebGpu(producer) => producer.into_merger().map(DistributedResultMerger::Pc),
        }
    }

    fn progress(&self) -> WasmDistributedProgress {
        match self {
            Self::Cpu(producer) => producer.progress(),
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
            Self::Setup(producer) => WasmDistributedProgress {
                geometry_nodes: producer.geometry_nodes(),
                candidates: producer.dispatched_conditions(),
                candidate_family_count: Some(producer.task_count() as u128),
                build_nodes: producer.partial_build_nodes(),
                coverage_checks: producer.received_conditions(),
                ..WasmDistributedProgress::default()
            },
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
            Self::BuildProbability(verifier) => verifier.consume(candidate, control),
            Self::Forward(_) => Err("forward_verifier_requires_forward_task_wire"),
            Self::Setup(_) => Err("setup_verifier_requires_setup_task_wire"),
        }
    }

    fn finish(&mut self) -> Result<Vec<clearra_core_executor::CoreExecutionResult>, &'static str> {
        match self {
            Self::Pc(verifier) => verifier.finish().map(|result| vec![result]),
            Self::BuildProbability(verifier) => verifier.finish(),
            Self::Forward(_) => Ok(Vec::new()),
            Self::Setup(_) => Ok(Vec::new()),
        }
    }

    fn progress(&self) -> WasmDistributedProgress {
        match self {
            Self::Pc(verifier) => verifier.progress(),
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
    fn absorb(
        &mut self,
        result: &clearra_core_executor::CoreExecutionResult,
    ) -> Result<(), &'static str> {
        match self {
            Self::Pc(merger) => merger.absorb(result),
            Self::BuildProbability(merger) => merger.absorb(result),
            Self::Forward(_) => Err("forward_merger_requires_forward_result_wire"),
        }
    }

    fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<clearra_core_executor::CoreExecutionResult, &'static str> {
        match self {
            Self::Pc(merger) => merger.finish(summary, workers_used),
            Self::BuildProbability(merger) => merger.finish(summary, workers_used),
            Self::Forward(_) => Err("forward_merger_requires_forward_finish"),
        }
    }
}

impl WasmDistributedVerifierRuntime {
    pub fn prepare(
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
        let verifier = if let Some((field, aggregation)) = prepared.build_probability_request() {
            DistributedVerifier::BuildProbability(
                WasmBuildProbabilityDistributedVerifier::new(
                    prepared.problem(),
                    field,
                    aggregation,
                )
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFIER_START", reason))?,
            )
        } else {
            DistributedVerifier::Pc(
                WasmDistributedVerifier::new(prepared.problem()).map_err(|reason| {
                    distributed_error("E_WASM_DISTRIBUTED_VERIFIER_START", reason)
                })?,
            )
        };
        Ok(Self {
            verifier,
            postprocessor: *runtime.app_context().services().core_executor(),
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
            control: ExecutionControl::default(),
        })
    }

    pub fn consume(
        &mut self,
        input: &[u8],
    ) -> Result<WasmDistributedVerifierConsume, WasmCommandRuntimeError> {
        if let DistributedVerifier::Forward(verifier) = &mut self.verifier {
            let (candidate_count, partial) =
                verifier.consume(input, &self.control).map_err(|error| {
                    distributed_error("E_WASM_DISTRIBUTED_FORWARD_VERIFY", error.reason())
                })?;
            return Ok(WasmDistributedVerifierConsume {
                candidate_count,
                partial: Some(partial),
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
            });
        }
        let candidates = decode_candidate_batch(input).map_err(|error| {
            distributed_error("E_WASM_DISTRIBUTED_CANDIDATE_INVALID", error.reason())
        })?;
        for candidate in &candidates {
            self.verifier
                .consume(candidate, &self.control)
                .map_err(|reason| distributed_error("E_WASM_DISTRIBUTED_VERIFY", reason))?;
        }
        Ok(WasmDistributedVerifierConsume {
            candidate_count: candidates.len(),
            partial: None,
        })
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        self.verifier.progress()
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, WasmCommandRuntimeError> {
        if matches!(
            self.verifier,
            DistributedVerifier::Forward(_) | DistributedVerifier::Setup(_)
        ) {
            return Ok(Vec::new());
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
        Ok(encode_partial_results(&results))
    }
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

fn distributed_error(code: &'static str, reason: impl Into<String>) -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new(code, reason)
}

fn request_needs_distributed_execution(request: &AppRequest) -> bool {
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

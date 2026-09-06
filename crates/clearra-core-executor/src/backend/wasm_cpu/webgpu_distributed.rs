use std::{sync::Arc, task::Poll};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

use crate::backend::{GpuExecutionFailure, GpuFailureDisposition, WasmCpuSearchError};

use super::{
    distributed::{
        WasmCandidatePacket, WasmCandidateProducerAdvance, WasmCpuCandidateProducer,
        WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
        WasmDistributedResultMerger,
    },
    mix_digest,
    result::WasmExactSearchSession,
    webgpu_search::{
        adapter_selection, compile_batches, poll_once, run_gpu, GpuReduction, GpuRunFailure,
        GpuRunFuture,
    },
    WasmExactSearchError,
};

pub struct WasmWebGpuCandidateProducer {
    problem: Arc<SearchProblem>,
    exact: Option<WasmExactSearchSession>,
    phase: WebGpuProducerPhase,
    fallback_allowed: bool,
    fallback_execution: Option<WasmDistributedBackendExecution>,
    fallback_producer: Option<WasmCpuCandidateProducer>,
    verification_required: bool,
    candidate_count: usize,
    candidate_digest: u64,
    summary: Option<WasmDistributedGeometrySummary>,
}

// The active GPU producer phase is long-lived and transitions by ownership move.
#[allow(clippy::large_enum_variant)]
enum WebGpuProducerPhase {
    Preparing {
        selection: clearra_webgpu::WebGpuAdapterSelection,
        warmup_requested: bool,
    },
    Prepared {
        batches: Vec<clearra_webgpu::WebGpuGeometryExactCoverBatch>,
        selection: clearra_webgpu::WebGpuAdapterSelection,
        warmup_requested: bool,
    },
    Dispatching(GpuRunFuture),
    Reducing(GpuReduction),
    CpuFallback(WasmCpuCandidateProducer),
    Finished,
}

impl WasmWebGpuCandidateProducer {
    pub fn new(problem: &SearchProblem) -> Result<Self, WasmCpuSearchError> {
        let verification_required = problem.objective().kind()
            != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling;
        let exact = WasmExactSearchSession::new_external_geometry(problem).map_err(map_error)?;
        Ok(Self {
            problem: Arc::new(problem.clone()),
            exact: Some(exact),
            phase: WebGpuProducerPhase::Preparing {
                selection: adapter_selection(problem.backend_policy().gpu_device()),
                warmup_requested: problem.backend_policy().gpu_warmup(),
            },
            fallback_allowed: problem.backend_policy().allow_backend_fallback(),
            fallback_execution: None,
            fallback_producer: None,
            verification_required,
            candidate_count: 0,
            candidate_digest: 0,
            summary: None,
        })
    }

    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, WasmCpuSearchError> {
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
        }
        if let Some(summary) = self.summary.clone() {
            return Ok(WasmCandidateProducerAdvance::Completed(summary));
        }
        loop {
            let phase = std::mem::replace(&mut self.phase, WebGpuProducerPhase::Finished);
            match phase {
                WebGpuProducerPhase::Preparing {
                    selection,
                    warmup_requested,
                } => {
                    let exact = self
                        .exact
                        .as_mut()
                        .ok_or_else(|| invalid("webgpu_exact_session_missing"))?;
                    if !exact
                        .advance_external_geometry_preparation(control)
                        .map_err(map_error)?
                    {
                        self.phase = WebGpuProducerPhase::Preparing {
                            selection,
                            warmup_requested,
                        };
                        return Ok(WasmCandidateProducerAdvance::Pending);
                    }
                    let batches =
                        compile_batches(exact, self.problem.backend_policy()).map_err(map_error)?;
                    self.phase = WebGpuProducerPhase::Prepared {
                        batches,
                        selection,
                        warmup_requested,
                    };
                    return Ok(WasmCandidateProducerAdvance::Pending);
                }
                WebGpuProducerPhase::Prepared {
                    batches,
                    selection,
                    warmup_requested,
                } => {
                    self.phase = WebGpuProducerPhase::Dispatching(Box::pin(run_gpu(
                        batches,
                        selection,
                        warmup_requested,
                        control.clone(),
                    )));
                    return Ok(WasmCandidateProducerAdvance::Pending);
                }
                WebGpuProducerPhase::Dispatching(mut future) => match poll_once(&mut future) {
                    Poll::Pending => {
                        self.phase = WebGpuProducerPhase::Dispatching(future);
                        return Ok(WasmCandidateProducerAdvance::Pending);
                    }
                    Poll::Ready(Ok(result)) => {
                        self.phase = WebGpuProducerPhase::Reducing(result.reduction);
                    }
                    Poll::Ready(Err(GpuRunFailure::Cancelled)) => {
                        return Ok(WasmCandidateProducerAdvance::Cancelled);
                    }
                    Poll::Ready(Err(GpuRunFailure::TrustMismatch(reason))) => {
                        return Err(invalid(reason));
                    }
                    Poll::Ready(Err(GpuRunFailure::Execution(failure))) => {
                        if self.start_cpu_fallback(failure)? {
                            continue;
                        }
                        return Err(invalid(failure_reason(failure)));
                    }
                },
                WebGpuProducerPhase::Reducing(mut reduction) => {
                    let candidate_budget = self.problem.backend_request().max_candidates();
                    if candidate_budget != 0 && self.candidate_count >= candidate_budget {
                        let summary =
                            self.gpu_summary(&reduction, Some("candidate_budget_exceeded"));
                        self.summary = Some(summary.clone());
                        self.phase = WebGpuProducerPhase::Finished;
                        return Ok(WasmCandidateProducerAdvance::Completed(summary));
                    }
                    if reduction.path_cursor.is_none() {
                        let Some(graph) = reduction.graphs.get(reduction.graph_cursor) else {
                            let summary = self.gpu_summary(&reduction, None);
                            self.summary = Some(summary.clone());
                            self.phase = WebGpuProducerPhase::Finished;
                            return Ok(WasmCandidateProducerAdvance::Completed(summary));
                        };
                        reduction.path_cursor = Some(graph.graph.path_cursor());
                    }
                    let cursor = reduction
                        .path_cursor
                        .as_mut()
                        .ok_or_else(|| invalid("webgpu_path_cursor_missing"))?;
                    match cursor.next_path() {
                        Ok(Some(path)) => {
                            let graph = &reduction.graphs[reduction.graph_cursor];
                            let target_index = graph
                                .target_begin
                                .checked_add(path.batch_index() as usize)
                                .and_then(|index| u32::try_from(index).ok())
                                .ok_or_else(|| invalid("webgpu_target_index_overflow"))?;
                            let exact = self
                                .exact
                                .as_ref()
                                .ok_or_else(|| invalid("webgpu_exact_session_missing"))?;
                            let identity_hash = exact
                                .external_candidate_identity_hash(
                                    target_index,
                                    path.operation_indices(),
                                )
                                .map_err(map_error)?;
                            let ordinal = self.candidate_count as u64;
                            self.candidate_count = self.candidate_count.saturating_add(1);
                            self.candidate_digest =
                                mix_digest(self.candidate_digest, identity_hash);
                            let candidate = WasmCandidatePacket::new(
                                ordinal,
                                target_index,
                                path.operation_indices().to_vec(),
                            );
                            self.phase = WebGpuProducerPhase::Reducing(reduction);
                            if !self.verification_required {
                                let exact = self
                                    .exact
                                    .as_mut()
                                    .ok_or_else(|| invalid("webgpu_exact_session_missing"))?;
                                match exact
                                    .process_external_candidate_with_ordinal(
                                        candidate.target_index(),
                                        candidate.row_ids(),
                                        candidate.ordinal(),
                                        control,
                                    )
                                    .map_err(map_error)?
                                {
                                    Some(super::result::ExactSearchAdvance::Cancelled) => {
                                        return Ok(WasmCandidateProducerAdvance::Cancelled);
                                    }
                                    Some(super::result::ExactSearchAdvance::Completed(_)) => {
                                        return Err(invalid(
                                            "wasm_tiling_producer_completed_early",
                                        ));
                                    }
                                    Some(super::result::ExactSearchAdvance::Pending) | None => {}
                                }
                                return Ok(WasmCandidateProducerAdvance::Pending);
                            }
                            return Ok(WasmCandidateProducerAdvance::Candidate(candidate));
                        }
                        Ok(None) => {
                            reduction.graph_cursor += 1;
                            reduction.path_cursor = None;
                            self.phase = WebGpuProducerPhase::Reducing(reduction);
                        }
                        Err(_) => return Err(invalid("webgpu_solution_graph_invalid")),
                    }
                }
                WebGpuProducerPhase::CpuFallback(mut producer) => {
                    match producer.advance(control).map_err(invalid)? {
                        WasmCandidateProducerAdvance::Completed(mut summary) => {
                            summary.backend_execution = self
                                .fallback_execution
                                .clone()
                                .ok_or_else(|| invalid("webgpu_cpu_fallback_metadata_missing"))?;
                            self.fallback_producer = Some(producer);
                            self.summary = Some(summary.clone());
                            self.phase = WebGpuProducerPhase::Finished;
                            return Ok(WasmCandidateProducerAdvance::Completed(summary));
                        }
                        advance => {
                            self.phase = WebGpuProducerPhase::CpuFallback(producer);
                            return Ok(advance);
                        }
                    }
                }
                WebGpuProducerPhase::Finished => {
                    return Err(invalid("wasm_distributed_geometry_already_finished"));
                }
            }
        }
    }

    pub fn into_merger(mut self) -> Result<WasmDistributedResultMerger, WasmCpuSearchError> {
        if self.summary.is_none() {
            return Err(invalid("wasm_distributed_geometry_not_finished"));
        }
        if let Some(producer) = self.fallback_producer.take() {
            return producer.into_merger().map_err(invalid);
        }
        let exact = self
            .exact
            .take()
            .ok_or_else(|| invalid("webgpu_exact_session_missing"))?;
        Ok(WasmDistributedResultMerger::from_session(
            exact.into_distributed_finalizer().map_err(map_error)?,
        ))
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        if let Some(producer) = &self.fallback_producer {
            return producer.progress();
        }
        let geometry_nodes = match &self.phase {
            WebGpuProducerPhase::Reducing(reduction) => reduction.expanded_records,
            WebGpuProducerPhase::CpuFallback(producer) => {
                return producer.progress();
            }
            _ => self
                .summary
                .as_ref()
                .map_or(0, |summary| summary.expanded_nodes),
        };
        WasmDistributedProgress {
            geometry_nodes,
            candidates: self.candidate_count,
            candidate_family_count: self
                .summary
                .as_ref()
                .and_then(|summary| summary.candidate_family_count),
            pass_count: 1,
            ..WasmDistributedProgress::default()
        }
    }

    pub fn target_preparation_pending(&self) -> bool {
        match &self.phase {
            WebGpuProducerPhase::Preparing { .. } => self
                .exact
                .as_ref()
                .is_some_and(WasmExactSearchSession::geometry_target_preparation_pending),
            WebGpuProducerPhase::CpuFallback(producer) => producer.target_preparation_pending(),
            _ => false,
        }
    }

    pub fn verification_required(&self) -> bool {
        self.fallback_producer
            .as_ref()
            .map_or(self.verification_required, |producer| {
                producer.verification_required()
            })
    }

    fn start_cpu_fallback(
        &mut self,
        failure: GpuExecutionFailure,
    ) -> Result<bool, WasmCpuSearchError> {
        let fallback_policy = if self.fallback_allowed {
            clearra_pc_graph::request::BackendFallbackPolicy::Allow
        } else {
            clearra_pc_graph::request::BackendFallbackPolicy::Deny
        };
        let resolution = failure.resolve(fallback_policy);
        if !resolution.fallback_used() {
            self.phase = WebGpuProducerPhase::Finished;
            return Ok(false);
        }
        self.fallback_execution = Some(WasmDistributedBackendExecution::CpuFallback {
            reason: resolution
                .backend_fallback_reason()
                .map_or("webgpu_fallback", |reason| reason.as_str()),
            failure_class: resolution.class().as_str(),
            failure_stage: resolution.stage().as_str(),
            discarded_partial_gpu_result: resolution.discarded_partial_gpu_result(),
            original_gpu_result_incomplete: resolution.original_gpu_result_incomplete(),
        });
        self.exact = None;
        self.phase =
            WebGpuProducerPhase::CpuFallback(WasmCpuCandidateProducer::new_typed(&self.problem)?);
        Ok(true)
    }

    fn gpu_summary(
        &self,
        reduction: &GpuReduction,
        truncated_reason: Option<&'static str>,
    ) -> WasmDistributedGeometrySummary {
        WasmDistributedGeometrySummary {
            candidate_count: self.candidate_count,
            candidate_digest: self.candidate_digest,
            candidate_family_count: Some(self.candidate_count as u128),
            expanded_nodes: reduction.expanded_records,
            peak_frontier: reduction.peak_frontier_states,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            truncated_reason,
            backend_execution: WasmDistributedBackendExecution::WebGpu {
                adapter_index: reduction.adapter_index,
                adapter_name: reduction.adapter_name.clone(),
                adapter_type: reduction.adapter_type,
                adapter_backend: reduction.adapter_backend.clone(),
                peak_gpu_bytes: reduction.peak_gpu_bytes,
                shader_hash: reduction.shader_hash.clone(),
                shader_version: reduction.shader_version,
                warmup_performed: reduction.warmup_performed,
                session_reused: reduction.session_reused,
            },
        }
    }
}

fn failure_reason(failure: GpuExecutionFailure) -> &'static str {
    match failure
        .resolve(clearra_pc_graph::request::BackendFallbackPolicy::Deny)
        .disposition()
    {
        GpuFailureDisposition::Unavailable => "webgpu_backend_unavailable",
        GpuFailureDisposition::TransientFailure => "webgpu_transient_before_commit",
        GpuFailureDisposition::Incomplete => "webgpu_resource_incomplete",
        GpuFailureDisposition::InvalidRequest => "webgpu_invalid_request",
        GpuFailureDisposition::RejectedMismatch => "webgpu_trust_mismatch",
        GpuFailureDisposition::FatalInternal => "webgpu_fatal_internal",
        GpuFailureDisposition::CpuFallback | GpuFailureDisposition::CpuRerunAfterIncomplete => {
            "webgpu_fallback_resolution_inconsistent"
        }
    }
}

fn map_error(error: WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        WasmExactSearchError::InvalidProblem(reason) => invalid(reason),
        WasmExactSearchError::ResourceAdmission(resource_report) => {
            WasmCpuSearchError::resource_admission(*resource_report)
        }
        WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}

const fn invalid(reason: &'static str) -> WasmCpuSearchError {
    WasmCpuSearchError::InvalidProblem { reason }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::resource::{
        ExecutionAvailability, ExecutionAvailabilityReason, ResourceReport,
    };

    use super::*;

    #[test]
    fn resource_admission_preserves_the_typed_report() {
        let report = ResourceReport::admission_failure(
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded)
                .with_pattern_evidence(1_058_400, 1_058_400, 132_304)
                .with_required_memory_bytes(17_066_704),
        );

        let mapped = map_error(WasmExactSearchError::resource_admission(report));
        assert_eq!(mapped, WasmCpuSearchError::resource_admission(report));
        assert_eq!(mapped.reason(), "shared_execution_memory_exhausted");
        assert!(!report.execution_started());
        assert!(!report.result_complete());
    }
}

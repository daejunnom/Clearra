//! Request-owned failed-queue completion of the full-height PC family.
//! No public result field can create coverage or source-completeness evidence.
use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

use super::{ExactSearchAdvance, ExtendedPcSearchSession};
use crate::{
    backend::wasm_cpu_search_backend::map_error,
    pc_failed_queue_evidence::{
        PcFailedQueueEvidenceProducer, PcFailedQueueExecutionAuthority,
        PcFailedQueueProducerAdmission, PcFailedQueueSourceCompleteness,
    },
    CoreExecutionResult, PcFailedQueueEvidence, PcFailedQueueEvidenceError, WasmCpuSearchError,
};

// The completed source result and its evidence move once to the App validator.
#[allow(clippy::large_enum_variant)]
pub enum WasmPcFailedQueueAdvance {
    Pending,
    Completed(CoreExecutionResult, PcFailedQueueEvidence),
    Cancelled,
}

pub struct WasmPcFailedQueueSession {
    engine: ExtendedPcSearchSession,
    authority: PcFailedQueueExecutionAuthority,
    example_limit: usize,
    finished: bool,
}

impl WasmPcFailedQueueSession {
    pub fn new(problem: Arc<SearchProblem>) -> Result<Self, WasmCpuSearchError> {
        let example_limit = problem
            .pc_chance_evidence_policy()
            .pc_failed_queue_example_limit()
            .ok_or(WasmCpuSearchError::InvalidProblem {
                reason: "extended_pc_failed_queue_purpose_required",
            })?;
        if !matches!(
            problem.backend_policy().requested_backend().as_str(),
            "cpu" | "auto"
        ) {
            return Err(WasmCpuSearchError::Unsupported {
                reason: "extended_pc_family_requires_cpu_backend",
            });
        }
        // The engine accounts both original input and its execution snapshot.
        // Also admit the typed session and the request-token Arc header/pointee
        // before catalog compilation, not after the expensive allocation.
        let external = (core::mem::size_of::<Self>() + 3 * core::mem::size_of::<usize>()) as u128;
        let engine =
            ExtendedPcSearchSession::new_with_coexisting_retained_bytes(&problem, external)
                .map_err(map_error)?;
        Ok(Self {
            engine,
            authority: PcFailedQueueExecutionAuthority::new(problem),
            example_limit,
            finished: false,
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<WasmPcFailedQueueAdvance, WasmCpuSearchError> {
        if self.finished {
            return Err(WasmCpuSearchError::InvalidProblem {
                reason: "extended_pc_failed_queue_already_finished",
            });
        }
        if control.is_cancelled() {
            return Ok(WasmPcFailedQueueAdvance::Cancelled);
        }
        match self
            .engine
            .advance(work_budget, control)
            .map_err(map_error)?
        {
            ExactSearchAdvance::Pending => Ok(WasmPcFailedQueueAdvance::Pending),
            ExactSearchAdvance::Cancelled => Ok(WasmPcFailedQueueAdvance::Cancelled),
            ExactSearchAdvance::Completed(result) => {
                self.finished = true;
                if control.is_cancelled() {
                    return Ok(WasmPcFailedQueueAdvance::Cancelled);
                }
                let source = result.pc_chance_coverage_evidence().ok_or(
                    WasmCpuSearchError::InvalidProblem {
                        reason: "extended_pc_failed_queue_source_evidence_missing",
                    },
                )?;
                if !source.complete()
                    || !source
                        .problem()
                        .matches_search_problem(self.authority.problem())
                    || source.coverage_union().words() != result.coverage_pattern_words()
                {
                    return Err(WasmCpuSearchError::InvalidProblem {
                        reason: "extended_pc_failed_queue_source_evidence_mismatch",
                    });
                }
                let observed = self
                    .engine
                    .checked_failed_queue_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(result.checked_resource_retained_bytes()?))
                    .ok_or(WasmCpuSearchError::InvalidProblem {
                        reason: "extended_pc_failed_queue_memory_projection_unavailable",
                    })?;
                let evidence = PcFailedQueueEvidenceProducer::produce(
                    self.authority.clone(),
                    source.rows(),
                    self.example_limit,
                    PcFailedQueueSourceCompleteness::new(true, true, true, true),
                    PcFailedQueueProducerAdmission::new(
                        self.engine.failed_queue_memory_bound(),
                        observed,
                    ),
                )
                .map_err(|error| match error {
                    PcFailedQueueEvidenceError::MemoryAdmission(report) => {
                        WasmCpuSearchError::resource_admission(*report)
                    }
                    _ => WasmCpuSearchError::InvalidProblem {
                        reason: "extended_pc_failed_queue_evidence_invalid",
                    },
                })?;
                Ok(WasmPcFailedQueueAdvance::Completed(
                    result.without_pc_chance_transient_evidence(),
                    evidence,
                ))
            }
        }
    }
}

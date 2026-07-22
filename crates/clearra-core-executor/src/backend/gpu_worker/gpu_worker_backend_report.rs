use crate::backend::{
    GpuExecutionFailureClass, GpuExecutionFailureStage, GpuFallbackBackend, GpuTrustState,
    SearchBackendFallbackReason,
};

use super::{GpuWorkerBackpressure, GpuWorkerExactnessGate, GpuWorkerResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerBackendReport {
    trust_state: GpuTrustState,
    exactness_gate: GpuWorkerExactnessGate,
    candidate_count: u32,
    fallback_reason: Option<SearchBackendFallbackReason>,
    gpu_failure_class: Option<GpuExecutionFailureClass>,
    gpu_failure_stage: Option<GpuExecutionFailureStage>,
    fallback_used: bool,
    fallback_backend: Option<GpuFallbackBackend>,
    discarded_partial_gpu_result: bool,
    cpu_confirm_required: bool,
    can_source_exact_probability: bool,
    can_accept_build_variant: bool,
    memory_ticket_id: u64,
    fence_epoch: u64,
    backpressure: GpuWorkerBackpressure,
}

impl GpuWorkerBackendReport {
    pub fn from_result(result: &GpuWorkerResult, exactness_gate: GpuWorkerExactnessGate) -> Self {
        Self {
            trust_state: result.trust_state(),
            exactness_gate,
            candidate_count: result.candidate_count(),
            fallback_reason: result.fallback_reason(),
            gpu_failure_class: result.failure().map(|failure| failure.class()),
            gpu_failure_stage: result.failure().map(|failure| failure.stage()),
            fallback_used: result
                .failure()
                .is_some_and(|failure| failure.fallback_used()),
            fallback_backend: result
                .failure()
                .and_then(|failure| failure.fallback_backend()),
            discarded_partial_gpu_result: result
                .failure()
                .is_some_and(|failure| failure.discarded_partial_gpu_result()),
            cpu_confirm_required: result.cpu_confirm_required(),
            can_source_exact_probability: exactness_gate.can_source_exact_probability(),
            can_accept_build_variant: exactness_gate.can_accept_build_variant(),
            memory_ticket_id: result.memory_ticket_id(),
            fence_epoch: result.fence_epoch(),
            backpressure: result.backpressure(),
        }
    }
}
impl GpuWorkerBackendReport {
    pub fn trust_state(self) -> GpuTrustState {
        self.trust_state
    }
}
impl GpuWorkerBackendReport {
    pub fn status_label(self) -> &'static str {
        if let Some(class) = self.gpu_failure_class {
            return match class {
                GpuExecutionFailureClass::Unavailable => "unavailable",
                GpuExecutionFailureClass::TransientBeforeCommit
                | GpuExecutionFailureClass::FatalInternal => "failed",
                GpuExecutionFailureClass::ResourceIncomplete => "incomplete",
                GpuExecutionFailureClass::InvalidRequest => "rejected-invalid-request",
                GpuExecutionFailureClass::TrustMismatch => "rejected-mismatch",
            };
        }
        match self.trust_state {
            GpuTrustState::GpuComputedCpuConfirmed
            | GpuTrustState::DeterministicReferenceMatched => "connected",
            GpuTrustState::GpuComputedMismatch => "rejected-mismatch",
            GpuTrustState::FallbackUsed { .. }
            | GpuTrustState::Unavailable
            | GpuTrustState::GpuComputedUnconfirmed
            | GpuTrustState::NotUsed => "unavailable",
        }
    }
}
impl GpuWorkerBackendReport {
    pub fn exactness_gate(self) -> GpuWorkerExactnessGate {
        self.exactness_gate
    }
}
impl GpuWorkerBackendReport {
    pub fn candidate_count(self) -> u32 {
        self.candidate_count
    }
}
impl GpuWorkerBackendReport {
    pub fn fallback_reason(self) -> Option<SearchBackendFallbackReason> {
        self.fallback_reason
    }
}
impl GpuWorkerBackendReport {
    pub const fn gpu_failure_class(self) -> Option<GpuExecutionFailureClass> {
        self.gpu_failure_class
    }

    pub const fn gpu_failure_stage(self) -> Option<GpuExecutionFailureStage> {
        self.gpu_failure_stage
    }

    pub const fn fallback_used(self) -> bool {
        self.fallback_used
    }

    pub const fn fallback_backend(self) -> Option<GpuFallbackBackend> {
        self.fallback_backend
    }

    pub const fn backend_fallback_reason(self) -> Option<SearchBackendFallbackReason> {
        self.fallback_reason
    }

    pub const fn discarded_partial_gpu_result(self) -> bool {
        self.discarded_partial_gpu_result
    }
}
impl GpuWorkerBackendReport {
    pub fn cpu_confirm_required(self) -> bool {
        self.cpu_confirm_required
    }
}
impl GpuWorkerBackendReport {
    pub fn can_source_exact_probability(self) -> bool {
        self.can_source_exact_probability
    }
}
impl GpuWorkerBackendReport {
    pub fn can_accept_build_variant(self) -> bool {
        self.can_accept_build_variant
    }
}
impl GpuWorkerBackendReport {
    pub fn memory_ticket_id(self) -> u64 {
        self.memory_ticket_id
    }
}
impl GpuWorkerBackendReport {
    pub fn fence_epoch(self) -> u64 {
        self.fence_epoch
    }
}
impl GpuWorkerBackendReport {
    pub fn backpressure(self) -> GpuWorkerBackpressure {
        self.backpressure
    }
}

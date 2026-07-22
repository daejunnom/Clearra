use crate::backend::{
    GpuExecutionFailureResolution, GpuFailureDisposition, GpuTrustState,
    SearchBackendFallbackReason,
};

use super::{GpuMemoryTicket, GpuWorkerBackpressure};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuExecutionCompletion {
    Unconfirmed,
    CpuConfirmed,
    DeterministicReferenceMatched,
}

impl GpuExecutionCompletion {
    const fn trust_state(self) -> GpuTrustState {
        match self {
            Self::Unconfirmed => GpuTrustState::GpuComputedUnconfirmed,
            Self::CpuConfirmed => GpuTrustState::GpuComputedCpuConfirmed,
            Self::DeterministicReferenceMatched => GpuTrustState::DeterministicReferenceMatched,
        }
    }

    const fn cpu_confirm_required(self) -> bool {
        matches!(self, Self::Unconfirmed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuWorkerResult {
    request_id: u64,
    candidate_count: u32,
    trust_state: GpuTrustState,
    cpu_confirm_required: bool,
    failure: Option<GpuExecutionFailureResolution>,
    memory_ticket_id: u64,
    fence_epoch: u64,
    scope_epoch: u64,
    byte_budget: u64,
    backpressure: GpuWorkerBackpressure,
}

impl GpuWorkerResult {
    pub fn completed(
        request_id: u64,
        candidate_count: u32,
        completion: GpuExecutionCompletion,
        memory_ticket: GpuMemoryTicket,
        backpressure: GpuWorkerBackpressure,
    ) -> Self {
        Self {
            request_id,
            candidate_count,
            trust_state: completion.trust_state(),
            cpu_confirm_required: completion.cpu_confirm_required(),
            failure: None,
            memory_ticket_id: memory_ticket.id(),
            fence_epoch: memory_ticket.scope_epoch().value(),
            scope_epoch: memory_ticket.scope_epoch().value(),
            byte_budget: memory_ticket.byte_budget(),
            backpressure,
        }
    }

    pub fn from_failure(
        request_id: u64,
        partial_candidate_count: u32,
        failure: GpuExecutionFailureResolution,
        memory_ticket: GpuMemoryTicket,
        backpressure: GpuWorkerBackpressure,
    ) -> Self {
        let trust_state = match failure.disposition() {
            GpuFailureDisposition::CpuFallback | GpuFailureDisposition::CpuRerunAfterIncomplete => {
                GpuTrustState::FallbackUsed {
                    reason: failure
                        .backend_fallback_reason()
                        .expect("fallback resolution owns a fallback reason"),
                }
            }
            GpuFailureDisposition::Unavailable => GpuTrustState::Unavailable,
            GpuFailureDisposition::TransientFailure | GpuFailureDisposition::Incomplete => {
                GpuTrustState::GpuComputedUnconfirmed
            }
            GpuFailureDisposition::RejectedMismatch => GpuTrustState::GpuComputedMismatch,
            GpuFailureDisposition::InvalidRequest | GpuFailureDisposition::FatalInternal => {
                GpuTrustState::NotUsed
            }
        };
        Self {
            request_id,
            candidate_count: partial_candidate_count,
            trust_state,
            cpu_confirm_required: matches!(
                failure.disposition(),
                GpuFailureDisposition::TransientFailure | GpuFailureDisposition::Incomplete
            ),
            failure: Some(failure),
            memory_ticket_id: memory_ticket.id(),
            fence_epoch: memory_ticket.scope_epoch().value(),
            scope_epoch: memory_ticket.scope_epoch().value(),
            byte_budget: memory_ticket.byte_budget(),
            backpressure,
        }
    }

    #[cfg(test)]
    pub fn new(
        request_id: u64,
        candidate_count: u32,
        trust_state: GpuTrustState,
        cpu_confirm_required: bool,
        fallback_reason: Option<SearchBackendFallbackReason>,
        memory_ticket: GpuMemoryTicket,
        backpressure: GpuWorkerBackpressure,
    ) -> Self {
        Self {
            request_id,
            candidate_count,
            trust_state,
            cpu_confirm_required,
            failure: None,
            memory_ticket_id: memory_ticket.id(),
            fence_epoch: memory_ticket.scope_epoch().value(),
            scope_epoch: memory_ticket.scope_epoch().value(),
            byte_budget: memory_ticket.byte_budget(),
            backpressure,
        }
        .with_test_failure_metadata(fallback_reason)
    }

    #[cfg(test)]
    fn with_test_failure_metadata(
        mut self,
        fallback_reason: Option<SearchBackendFallbackReason>,
    ) -> Self {
        use clearra_pc_graph::request::BackendFallbackPolicy;

        use crate::backend::{GpuExecutionFailure, GpuExecutionFailureStage};

        self.failure = match self.trust_state {
            GpuTrustState::FallbackUsed { reason } => Some(
                GpuExecutionFailure::unavailable(GpuExecutionFailureStage::CapabilityQuery, reason)
                    .resolve(BackendFallbackPolicy::Allow),
            ),
            GpuTrustState::Unavailable => fallback_reason.map(|reason| {
                GpuExecutionFailure::unavailable(GpuExecutionFailureStage::CapabilityQuery, reason)
                    .resolve(BackendFallbackPolicy::Deny)
            }),
            GpuTrustState::GpuComputedMismatch => Some(
                GpuExecutionFailure::trust_mismatch(GpuExecutionFailureStage::CpuReferenceConfirm)
                    .resolve(BackendFallbackPolicy::Deny),
            ),
            GpuTrustState::NotUsed
            | GpuTrustState::GpuComputedUnconfirmed
            | GpuTrustState::GpuComputedCpuConfirmed
            | GpuTrustState::DeterministicReferenceMatched => None,
        };
        self
    }
}
impl GpuWorkerResult {
    pub fn request_id(&self) -> u64 {
        self.request_id
    }
}
impl GpuWorkerResult {
    pub fn candidate_count(&self) -> u32 {
        self.candidate_count
    }
}
impl GpuWorkerResult {
    pub fn trust_state(&self) -> GpuTrustState {
        self.trust_state
    }
}
impl GpuWorkerResult {
    pub fn can_source_exact_probability(&self) -> bool {
        self.trust_state.can_source_exact_probability()
    }
}
impl GpuWorkerResult {
    pub fn cpu_confirm_required(&self) -> bool {
        self.cpu_confirm_required
    }
}
impl GpuWorkerResult {
    pub fn fallback_reason(&self) -> Option<SearchBackendFallbackReason> {
        self.failure
            .and_then(GpuExecutionFailureResolution::backend_fallback_reason)
    }
}
impl GpuWorkerResult {
    pub fn failure(&self) -> Option<GpuExecutionFailureResolution> {
        self.failure
    }
}
impl GpuWorkerResult {
    pub fn gpu_failure_reason(&self) -> Option<SearchBackendFallbackReason> {
        self.failure
            .and_then(GpuExecutionFailureResolution::failure_reason)
    }
}
impl GpuWorkerResult {
    pub fn memory_ticket_id(&self) -> u64 {
        self.memory_ticket_id
    }
}
impl GpuWorkerResult {
    pub fn fence_epoch(&self) -> u64 {
        self.fence_epoch
    }
}
impl GpuWorkerResult {
    pub fn scope_epoch(&self) -> u64 {
        self.scope_epoch
    }
}
impl GpuWorkerResult {
    pub fn byte_budget(&self) -> u64 {
        self.byte_budget
    }
}
impl GpuWorkerResult {
    pub fn backpressure(&self) -> GpuWorkerBackpressure {
        self.backpressure
    }
}

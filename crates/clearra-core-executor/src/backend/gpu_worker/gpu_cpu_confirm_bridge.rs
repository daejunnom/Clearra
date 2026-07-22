use super::{GpuWorkerExactnessGate, GpuWorkerReduction};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuCpuConfirmBridgeError {
    UnconfirmedCandidate,
    MismatchedCandidate,
    BackendFallback,
    BackendUnavailable,
    UnsupportedReduction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuCpuConfirmBridgeDecision {
    exactness_gate: GpuWorkerExactnessGate,
    cpu_confirmed: bool,
    can_enter_cpu_buildup_queue: bool,
    can_create_coverage_row: bool,
    candidate_is_solution: bool,
}

impl GpuCpuConfirmBridgeDecision {
    pub const fn confirmed_for_buildup_queue(exactness_gate: GpuWorkerExactnessGate) -> Self {
        Self {
            exactness_gate,
            cpu_confirmed: true,
            can_enter_cpu_buildup_queue: true,
            can_create_coverage_row: false,
            candidate_is_solution: false,
        }
    }
}
impl GpuCpuConfirmBridgeDecision {
    pub const fn exactness_gate(self) -> GpuWorkerExactnessGate {
        self.exactness_gate
    }
}
impl GpuCpuConfirmBridgeDecision {
    pub const fn cpu_confirmed(self) -> bool {
        self.cpu_confirmed
    }
}
impl GpuCpuConfirmBridgeDecision {
    pub const fn can_enter_cpu_buildup_queue(self) -> bool {
        self.can_enter_cpu_buildup_queue
    }
}
impl GpuCpuConfirmBridgeDecision {
    pub const fn can_create_coverage_row(self) -> bool {
        self.can_create_coverage_row
    }
}
impl GpuCpuConfirmBridgeDecision {
    pub const fn candidate_is_solution(self) -> bool {
        self.candidate_is_solution
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuCpuConfirmBridge;

impl GpuCpuConfirmBridge {
    pub fn route_reduction(
        reduction: &GpuWorkerReduction,
    ) -> Result<GpuCpuConfirmBridgeDecision, GpuCpuConfirmBridgeError> {
        match reduction {
            GpuWorkerReduction::ExactCandidateSource { report, .. } => Ok(
                GpuCpuConfirmBridgeDecision::confirmed_for_buildup_queue(report.exactness_gate()),
            ),
            GpuWorkerReduction::PrefilterOnly { .. } => {
                Err(GpuCpuConfirmBridgeError::UnconfirmedCandidate)
            }
            GpuWorkerReduction::RejectedMismatch { .. } => {
                Err(GpuCpuConfirmBridgeError::MismatchedCandidate)
            }
            GpuWorkerReduction::Fallback { .. } => Err(GpuCpuConfirmBridgeError::BackendFallback),
            GpuWorkerReduction::Unavailable { .. } => {
                Err(GpuCpuConfirmBridgeError::BackendUnavailable)
            }
            GpuWorkerReduction::Unsupported { .. }
            | GpuWorkerReduction::ValidationRejected { .. }
            | GpuWorkerReduction::RuntimeFailure { .. }
            | GpuWorkerReduction::ResourceIncomplete { .. }
            | GpuWorkerReduction::InvalidRequest { .. }
            | GpuWorkerReduction::FatalInternal { .. } => {
                Err(GpuCpuConfirmBridgeError::UnsupportedReduction)
            }
        }
    }
}

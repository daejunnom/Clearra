use super::{GpuCpuConfirmBridgeDecision, GpuCpuConfirmBridgeError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuWorkerBuildUpMode {
    VerifyFirst,
    EnumerateVariants,
    CountVariants,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuWorkerBuildResultBridgeError {
    CpuConfirm(GpuCpuConfirmBridgeError),
    CandidateCannotEnterBuildUpQueue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerBuildResultBridge {
    mode: GpuWorkerBuildUpMode,
    confirmed_candidate_count: u32,
    build_variant_count: u32,
    count_complete: bool,
    trace_retained: bool,
    can_source_coverage_rows: bool,
    verify_first_used_for_coverage: bool,
}

impl GpuWorkerBuildResultBridge {
    pub fn from_confirmed_decision(
        decision: GpuCpuConfirmBridgeDecision,
        mode: GpuWorkerBuildUpMode,
        confirmed_candidate_count: u32,
        build_variant_count: u32,
        count_complete: bool,
    ) -> Result<Self, GpuWorkerBuildResultBridgeError> {
        if !decision.can_enter_cpu_buildup_queue() {
            return Err(GpuWorkerBuildResultBridgeError::CandidateCannotEnterBuildUpQueue);
        }

        Ok(Self {
            mode,
            confirmed_candidate_count,
            build_variant_count,
            count_complete,
            trace_retained: mode != GpuWorkerBuildUpMode::CountVariants,
            can_source_coverage_rows: mode == GpuWorkerBuildUpMode::EnumerateVariants
                && build_variant_count > 0,
            verify_first_used_for_coverage: false,
        })
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn from_cpu_confirm_error(
        error: GpuCpuConfirmBridgeError,
    ) -> GpuWorkerBuildResultBridgeError {
        GpuWorkerBuildResultBridgeError::CpuConfirm(error)
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn mode(self) -> GpuWorkerBuildUpMode {
        self.mode
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn confirmed_candidate_count(self) -> u32 {
        self.confirmed_candidate_count
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn build_variant_count(self) -> u32 {
        self.build_variant_count
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn count_complete(self) -> bool {
        self.count_complete
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn trace_retained(self) -> bool {
        self.trace_retained
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn can_source_coverage_rows(self) -> bool {
        self.can_source_coverage_rows
    }
}
impl GpuWorkerBuildResultBridge {
    pub const fn verify_first_used_for_coverage(self) -> bool {
        self.verify_first_used_for_coverage
    }
}

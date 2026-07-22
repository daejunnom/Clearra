use clearra_pc_graph::request::BackendFallbackPolicy;

use super::SearchBackendFallbackReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuExecutionFailureClass {
    Unavailable,
    TransientBeforeCommit,
    ResourceIncomplete,
    InvalidRequest,
    TrustMismatch,
    FatalInternal,
}

impl GpuExecutionFailureClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::TransientBeforeCommit => "transient-before-commit",
            Self::ResourceIncomplete => "resource-incomplete",
            Self::InvalidRequest => "invalid-request",
            Self::TrustMismatch => "trust-mismatch",
            Self::FatalInternal => "fatal-internal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuExecutionFailureStage {
    CapabilityQuery,
    BatchPlanning,
    MemoryReservation,
    Submission,
    KernelExecution,
    Readback,
    HostReduction,
    ExactConfirm,
    CpuReferenceConfirm,
    ResultCommit,
}

impl GpuExecutionFailureStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapabilityQuery => "capability-query",
            Self::BatchPlanning => "batch-planning",
            Self::MemoryReservation => "memory-reservation",
            Self::Submission => "submission",
            Self::KernelExecution => "kernel-execution",
            Self::Readback => "readback",
            Self::HostReduction => "host-reduction",
            Self::ExactConfirm => "exact-confirm",
            Self::CpuReferenceConfirm => "cpu-reference-confirm",
            Self::ResultCommit => "result-commit",
        }
    }

    const fn is_before_commit(self) -> bool {
        !matches!(self, Self::ResultCommit)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuPartialResultDisposition {
    NotProduced,
    Discarded,
    RetainedIncomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuFallbackBackend {
    Cpu,
}

impl GpuFallbackBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuFailureDisposition {
    CpuFallback,
    CpuRerunAfterIncomplete,
    Unavailable,
    TransientFailure,
    Incomplete,
    InvalidRequest,
    RejectedMismatch,
    FatalInternal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuExecutionFailureConstructionError {
    TransientStageIsNotBeforeCommit,
    ResourceFailureHasNoPartialResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuExecutionFailure {
    class: GpuExecutionFailureClass,
    stage: GpuExecutionFailureStage,
    failure_reason: Option<SearchBackendFallbackReason>,
    partial_result: GpuPartialResultDisposition,
}

impl GpuExecutionFailure {
    pub const fn unavailable(
        stage: GpuExecutionFailureStage,
        reason: SearchBackendFallbackReason,
    ) -> Self {
        Self {
            class: GpuExecutionFailureClass::Unavailable,
            stage,
            failure_reason: Some(reason),
            partial_result: GpuPartialResultDisposition::NotProduced,
        }
    }

    pub fn transient_before_commit(
        stage: GpuExecutionFailureStage,
        partial_result: GpuPartialResultDisposition,
    ) -> Result<Self, GpuExecutionFailureConstructionError> {
        if !stage.is_before_commit() {
            return Err(GpuExecutionFailureConstructionError::TransientStageIsNotBeforeCommit);
        }
        Ok(Self {
            class: GpuExecutionFailureClass::TransientBeforeCommit,
            stage,
            failure_reason: Some(SearchBackendFallbackReason::GpuTransientBeforeCommit),
            partial_result,
        })
    }

    pub fn resource_incomplete(
        stage: GpuExecutionFailureStage,
        partial_result: GpuPartialResultDisposition,
    ) -> Result<Self, GpuExecutionFailureConstructionError> {
        if matches!(partial_result, GpuPartialResultDisposition::NotProduced) {
            return Err(GpuExecutionFailureConstructionError::ResourceFailureHasNoPartialResult);
        }
        Ok(Self {
            class: GpuExecutionFailureClass::ResourceIncomplete,
            stage,
            failure_reason: Some(SearchBackendFallbackReason::GpuResourceIncomplete),
            partial_result,
        })
    }

    pub const fn invalid_request(stage: GpuExecutionFailureStage) -> Self {
        Self::terminal(GpuExecutionFailureClass::InvalidRequest, stage)
    }

    pub const fn trust_mismatch(stage: GpuExecutionFailureStage) -> Self {
        Self::terminal(GpuExecutionFailureClass::TrustMismatch, stage)
    }

    pub const fn fatal_internal(stage: GpuExecutionFailureStage) -> Self {
        Self::terminal(GpuExecutionFailureClass::FatalInternal, stage)
    }

    const fn terminal(class: GpuExecutionFailureClass, stage: GpuExecutionFailureStage) -> Self {
        Self {
            class,
            stage,
            failure_reason: None,
            partial_result: GpuPartialResultDisposition::NotProduced,
        }
    }

    pub fn resolve(self, fallback_policy: BackendFallbackPolicy) -> GpuExecutionFailureResolution {
        let fallback_allowed = fallback_policy.is_allowed();
        let transient_result_is_safe_to_abandon = matches!(
            self.partial_result,
            GpuPartialResultDisposition::NotProduced | GpuPartialResultDisposition::Discarded
        );
        let disposition = match self.class {
            GpuExecutionFailureClass::Unavailable if fallback_allowed => {
                GpuFailureDisposition::CpuFallback
            }
            GpuExecutionFailureClass::Unavailable => GpuFailureDisposition::Unavailable,
            GpuExecutionFailureClass::TransientBeforeCommit
                if fallback_allowed && transient_result_is_safe_to_abandon =>
            {
                GpuFailureDisposition::CpuFallback
            }
            GpuExecutionFailureClass::TransientBeforeCommit => {
                GpuFailureDisposition::TransientFailure
            }
            GpuExecutionFailureClass::ResourceIncomplete if fallback_allowed => {
                GpuFailureDisposition::CpuRerunAfterIncomplete
            }
            GpuExecutionFailureClass::ResourceIncomplete => GpuFailureDisposition::Incomplete,
            GpuExecutionFailureClass::InvalidRequest => GpuFailureDisposition::InvalidRequest,
            GpuExecutionFailureClass::TrustMismatch => GpuFailureDisposition::RejectedMismatch,
            GpuExecutionFailureClass::FatalInternal => GpuFailureDisposition::FatalInternal,
        };
        let discarded_partial_gpu_result =
            matches!(self.partial_result, GpuPartialResultDisposition::Discarded)
                || matches!(disposition, GpuFailureDisposition::CpuRerunAfterIncomplete);
        GpuExecutionFailureResolution {
            class: self.class,
            stage: self.stage,
            disposition,
            failure_reason: self.failure_reason,
            discarded_partial_gpu_result,
            original_gpu_result_incomplete: matches!(
                self.class,
                GpuExecutionFailureClass::ResourceIncomplete
            ) || matches!(
                self.partial_result,
                GpuPartialResultDisposition::RetainedIncomplete
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuExecutionFailureResolution {
    class: GpuExecutionFailureClass,
    stage: GpuExecutionFailureStage,
    disposition: GpuFailureDisposition,
    failure_reason: Option<SearchBackendFallbackReason>,
    discarded_partial_gpu_result: bool,
    original_gpu_result_incomplete: bool,
}

impl GpuExecutionFailureResolution {
    pub const fn class(self) -> GpuExecutionFailureClass {
        self.class
    }

    pub const fn stage(self) -> GpuExecutionFailureStage {
        self.stage
    }

    pub const fn disposition(self) -> GpuFailureDisposition {
        self.disposition
    }

    pub const fn fallback_used(self) -> bool {
        matches!(
            self.disposition,
            GpuFailureDisposition::CpuFallback | GpuFailureDisposition::CpuRerunAfterIncomplete
        )
    }

    pub const fn fallback_backend(self) -> Option<GpuFallbackBackend> {
        if self.fallback_used() {
            Some(GpuFallbackBackend::Cpu)
        } else {
            None
        }
    }

    pub const fn backend_fallback_reason(self) -> Option<SearchBackendFallbackReason> {
        if self.fallback_used() {
            self.failure_reason
        } else {
            None
        }
    }

    pub const fn failure_reason(self) -> Option<SearchBackendFallbackReason> {
        self.failure_reason
    }

    pub const fn discarded_partial_gpu_result(self) -> bool {
        self.discarded_partial_gpu_result
    }

    pub const fn original_gpu_result_incomplete(self) -> bool {
        self.original_gpu_result_incomplete
    }
}

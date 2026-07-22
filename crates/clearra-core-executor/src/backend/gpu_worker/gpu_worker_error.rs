use crate::backend::{GpuExecutionFailure, GpuExecutionFailureStage, SearchBackendFallbackReason};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GpuWorkerError {
    InvalidMemoryTicket { reason: &'static str },
    MissingMemoryTicket,
    CpuConfirmRequiredForGpuBatch,
    WorkerUnavailable { reason: &'static str },
    MemoryTicketMismatch { expected: u64, actual: u64 },
    SubmissionRequestMismatch { expected: u64, actual: u64 },
}

impl GpuWorkerError {
    pub fn into_execution_failure(self, stage: GpuExecutionFailureStage) -> GpuExecutionFailure {
        match self {
            Self::WorkerUnavailable { .. } => GpuExecutionFailure::unavailable(
                stage,
                SearchBackendFallbackReason::GpuBackendNotConnected,
            ),
            Self::InvalidMemoryTicket { .. }
            | Self::MissingMemoryTicket
            | Self::CpuConfirmRequiredForGpuBatch
            | Self::MemoryTicketMismatch { .. }
            | Self::SubmissionRequestMismatch { .. } => GpuExecutionFailure::invalid_request(stage),
        }
    }
}

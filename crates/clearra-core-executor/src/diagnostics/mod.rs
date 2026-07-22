#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExecutorSecurityDiagnosticCode;

impl ExecutorSecurityDiagnosticCode {
    pub const GPU_WORKER_MISSING_MEMORY_TICKET: &'static str = "E_GPU_WORKER_MISSING_MEMORY_TICKET";
    pub const GPU_FENCE_EPOCH_MISSING: &'static str = "E_GPU_FENCE_EPOCH_MISSING";
    pub const GPU_UNCONFIRMED_PROBABILITY_SOURCE: &'static str =
        "E_GPU_UNCONFIRMED_PROBABILITY_SOURCE";
    pub const BACKEND_FALLBACK_USED: &'static str = "W_BACKEND_FALLBACK_USED";
    pub const TRACE_RETENTION_TRUNCATED: &'static str = "W_TRACE_RETENTION_TRUNCATED";
    pub const OBSERVED_QUEUE_PROBABILITY_INCOMPLETE: &'static str =
        "W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE";
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutorSecurityDiagnostic {
    MissingGpuMemoryTicket,
    MissingGpuFenceEpoch,
    UnconfirmedGpuProbabilitySource,
    BackendFallbackUsed,
    TraceRetentionTruncated,
    ObservedQueueProbabilityIncomplete,
}

impl ExecutorSecurityDiagnostic {
    pub const fn code(self) -> &'static str {
        match self {
            Self::MissingGpuMemoryTicket => {
                ExecutorSecurityDiagnosticCode::GPU_WORKER_MISSING_MEMORY_TICKET
            }
            Self::MissingGpuFenceEpoch => ExecutorSecurityDiagnosticCode::GPU_FENCE_EPOCH_MISSING,
            Self::UnconfirmedGpuProbabilitySource => {
                ExecutorSecurityDiagnosticCode::GPU_UNCONFIRMED_PROBABILITY_SOURCE
            }
            Self::BackendFallbackUsed => ExecutorSecurityDiagnosticCode::BACKEND_FALLBACK_USED,
            Self::TraceRetentionTruncated => {
                ExecutorSecurityDiagnosticCode::TRACE_RETENTION_TRUNCATED
            }
            Self::ObservedQueueProbabilityIncomplete => {
                ExecutorSecurityDiagnosticCode::OBSERVED_QUEUE_PROBABILITY_INCOMPLETE
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executor_security_diagnostics_expose_stable_codes() {
        assert_eq!(
            ExecutorSecurityDiagnostic::MissingGpuMemoryTicket.code(),
            "E_GPU_WORKER_MISSING_MEMORY_TICKET"
        );
        assert_eq!(
            ExecutorSecurityDiagnostic::UnconfirmedGpuProbabilitySource.code(),
            "E_GPU_UNCONFIRMED_PROBABILITY_SOURCE"
        );
        assert_eq!(
            ExecutorSecurityDiagnostic::BackendFallbackUsed.code(),
            "W_BACKEND_FALLBACK_USED"
        );
    }
}

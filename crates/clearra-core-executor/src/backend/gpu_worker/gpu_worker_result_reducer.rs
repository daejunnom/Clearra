use crate::backend::{
    GpuExecutionFailureClass, GpuFailureDisposition, GpuTrustState, SearchBackendFallbackReason,
};

use super::{
    validate_gpu_worker_result, GpuWorkerBackendReport, GpuWorkerExactnessGate, GpuWorkerResult,
    GpuWorkerResultValidationError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GpuWorkerReduction {
    PrefilterOnly {
        cpu_confirm_required: bool,
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    ExactCandidateSource {
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    Fallback {
        reason: SearchBackendFallbackReason,
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    Unavailable {
        reason: SearchBackendFallbackReason,
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    RejectedMismatch {
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    RuntimeFailure {
        class: GpuExecutionFailureClass,
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    ResourceIncomplete {
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    InvalidRequest {
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    FatalInternal {
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    Unsupported {
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
    ValidationRejected {
        error: GpuWorkerResultValidationError,
        report: GpuWorkerBackendReport,
        result: GpuWorkerResult,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuWorkerResultReducer;

impl GpuWorkerResultReducer {
    pub fn reduce(result: GpuWorkerResult) -> GpuWorkerReduction {
        let gate = GpuWorkerExactnessGate::for_trust_state(result.trust_state());
        let report = GpuWorkerBackendReport::from_result(&result, gate);

        if let Err(error) = validate_gpu_worker_result(&result) {
            return GpuWorkerReduction::ValidationRejected {
                error,
                report,
                result,
            };
        }

        if let Some(failure) = result.failure() {
            return match failure.disposition() {
                GpuFailureDisposition::CpuFallback
                | GpuFailureDisposition::CpuRerunAfterIncomplete => GpuWorkerReduction::Fallback {
                    reason: failure
                        .backend_fallback_reason()
                        .expect("validated fallback reason"),
                    report,
                    result,
                },
                GpuFailureDisposition::Unavailable => GpuWorkerReduction::Unavailable {
                    reason: failure
                        .failure_reason()
                        .expect("validated unavailable reason"),
                    report,
                    result,
                },
                GpuFailureDisposition::TransientFailure => GpuWorkerReduction::RuntimeFailure {
                    class: failure.class(),
                    report,
                    result,
                },
                GpuFailureDisposition::Incomplete => {
                    GpuWorkerReduction::ResourceIncomplete { report, result }
                }
                GpuFailureDisposition::InvalidRequest => {
                    GpuWorkerReduction::InvalidRequest { report, result }
                }
                GpuFailureDisposition::RejectedMismatch => {
                    GpuWorkerReduction::RejectedMismatch { report, result }
                }
                GpuFailureDisposition::FatalInternal => {
                    GpuWorkerReduction::FatalInternal { report, result }
                }
            };
        }

        match result.trust_state() {
            GpuTrustState::GpuComputedUnconfirmed => GpuWorkerReduction::PrefilterOnly {
                cpu_confirm_required: true,
                report,
                result,
            },
            GpuTrustState::GpuComputedCpuConfirmed
            | GpuTrustState::DeterministicReferenceMatched => {
                GpuWorkerReduction::ExactCandidateSource { report, result }
            }
            GpuTrustState::FallbackUsed { .. }
            | GpuTrustState::Unavailable
            | GpuTrustState::GpuComputedMismatch => GpuWorkerReduction::ValidationRejected {
                error: GpuWorkerResultValidationError::FailureTrustStateMismatch,
                report,
                result,
            },
            GpuTrustState::NotUsed => GpuWorkerReduction::Unsupported { report, result },
        }
    }
}

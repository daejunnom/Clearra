use crate::backend::{
    GpuExecutionFailureClass, GpuFailureDisposition, GpuTrustState, SearchBackendFallbackReason,
};

use super::GpuWorkerResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuWorkerResultValidationError {
    MissingMemoryTicket,
    MissingFenceEpoch,
    MissingScopeEpoch,
    MissingByteBudget,
    MissingFallbackReason,
    MissingUnavailableReason,
    CpuConfirmEvidenceMissing,
    FailureTrustStateMismatch,
}

pub fn validate_gpu_worker_result(
    result: &GpuWorkerResult,
) -> Result<(), GpuWorkerResultValidationError> {
    if result.memory_ticket_id() == 0 {
        return Err(GpuWorkerResultValidationError::MissingMemoryTicket);
    }
    if result.fence_epoch() == 0 {
        return Err(GpuWorkerResultValidationError::MissingFenceEpoch);
    }
    if result.scope_epoch() == 0 {
        return Err(GpuWorkerResultValidationError::MissingScopeEpoch);
    }
    if result.byte_budget() == 0 {
        return Err(GpuWorkerResultValidationError::MissingByteBudget);
    }

    if let Some(failure) = result.failure() {
        let trust_matches = match failure.disposition() {
            GpuFailureDisposition::CpuFallback | GpuFailureDisposition::CpuRerunAfterIncomplete => {
                matches!(result.trust_state(), GpuTrustState::FallbackUsed { .. })
            }
            GpuFailureDisposition::Unavailable => {
                result.trust_state() == GpuTrustState::Unavailable
            }
            GpuFailureDisposition::TransientFailure | GpuFailureDisposition::Incomplete => {
                result.trust_state() == GpuTrustState::GpuComputedUnconfirmed
            }
            GpuFailureDisposition::RejectedMismatch => {
                result.trust_state() == GpuTrustState::GpuComputedMismatch
            }
            GpuFailureDisposition::InvalidRequest | GpuFailureDisposition::FatalInternal => {
                result.trust_state() == GpuTrustState::NotUsed
            }
        };
        if !trust_matches {
            return Err(GpuWorkerResultValidationError::FailureTrustStateMismatch);
        }
        if failure.class() == GpuExecutionFailureClass::TrustMismatch && failure.fallback_used() {
            return Err(GpuWorkerResultValidationError::FailureTrustStateMismatch);
        }
    }

    match result.trust_state() {
        GpuTrustState::FallbackUsed { reason } => {
            if result.fallback_reason() != Some(reason) {
                return Err(GpuWorkerResultValidationError::MissingFallbackReason);
            }
        }
        GpuTrustState::Unavailable => {
            if result.gpu_failure_reason().is_none() {
                return Err(GpuWorkerResultValidationError::MissingUnavailableReason);
            }
        }
        GpuTrustState::GpuComputedCpuConfirmed => {
            if result.cpu_confirm_required() {
                return Err(GpuWorkerResultValidationError::CpuConfirmEvidenceMissing);
            }
        }
        GpuTrustState::GpuComputedUnconfirmed
        | GpuTrustState::GpuComputedMismatch
        | GpuTrustState::DeterministicReferenceMatched
        | GpuTrustState::NotUsed => {}
    }
    Ok(())
}

pub fn unavailable_reason_label(reason: Option<SearchBackendFallbackReason>) -> &'static str {
    reason
        .map(SearchBackendFallbackReason::as_str)
        .unwrap_or("gpu_unavailable_reason_missing")
}

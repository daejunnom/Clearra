use crate::backend::{GpuExecutionFailure, GpuExecutionFailureStage, SearchBackendFallbackReason};

use super::GpuBackendKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuBackendError {
    BackendUnavailable {
        kind: GpuBackendKind,
        reason: &'static str,
    },
    UserProvidedShaderPathRejected,
}

impl GpuBackendError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::BackendUnavailable { reason, .. } => reason,
            Self::UserProvidedShaderPathRejected => "user_provided_shader_path_rejected",
        }
    }

    pub fn into_execution_failure(self, stage: GpuExecutionFailureStage) -> GpuExecutionFailure {
        match self {
            Self::BackendUnavailable { reason, .. } => {
                GpuExecutionFailure::unavailable(stage, unavailable_fallback_reason(reason))
            }
            Self::UserProvidedShaderPathRejected => GpuExecutionFailure::invalid_request(stage),
        }
    }
}

fn unavailable_fallback_reason(reason: &'static str) -> SearchBackendFallbackReason {
    match reason {
        "gpu_device_not_found" => SearchBackendFallbackReason::GpuDeviceNotFound,
        "gpu_kernel_unavailable" => SearchBackendFallbackReason::GpuKernelUnavailable,
        "gpu_binding_unavailable" => SearchBackendFallbackReason::GpuBindingUnavailable,
        "gpu_feature_disabled" => SearchBackendFallbackReason::GpuFeatureDisabled,
        _ => SearchBackendFallbackReason::GpuBackendNotConnected,
    }
}

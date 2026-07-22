use clearra_pc_graph::request::BackendFallbackPolicy;

use crate::backend::GpuExecutionFailureStage;

use super::{GpuBackendCapability, GpuBackendError, GpuBackendKind, GpuBackendSelection};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuDeviceSelector;

impl GpuDeviceSelector {
    pub fn select_default() -> GpuBackendSelection {
        Self::select(GpuBackendKind::NativeCompute, true)
            .expect("native GPU unavailable must reduce to explicit CPU fallback")
    }
}
impl GpuDeviceSelector {
    pub fn select(
        requested: GpuBackendKind,
        allow_fallback: bool,
    ) -> Result<GpuBackendSelection, GpuBackendError> {
        let capability = GpuBackendCapability::for_kind(requested);
        if capability.is_available() {
            return Ok(GpuBackendSelection::available(requested, capability));
        }

        let reason = capability
            .unavailable_reason()
            .unwrap_or("gpu_backend_unavailable");
        if allow_fallback {
            let failure = GpuBackendError::BackendUnavailable {
                kind: requested,
                reason,
            }
            .into_execution_failure(GpuExecutionFailureStage::CapabilityQuery)
            .resolve(BackendFallbackPolicy::Allow);
            return Ok(GpuBackendSelection::from_failure(
                requested,
                GpuBackendKind::Disabled,
                GpuBackendCapability::for_kind(GpuBackendKind::Disabled),
                failure,
                reason,
            ));
        }

        Err(GpuBackendError::BackendUnavailable {
            kind: requested,
            reason,
        })
    }
}
impl GpuDeviceSelector {
    pub fn reject_user_provided_shader_path(
        shader_path: Option<&str>,
    ) -> Result<(), GpuBackendError> {
        if matches!(shader_path, Some(path) if !path.trim().is_empty()) {
            return Err(GpuBackendError::UserProvidedShaderPathRejected);
        }
        Ok(())
    }
}

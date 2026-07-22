use crate::backend::{
    GpuExecutionFailureClass, GpuExecutionFailureResolution, GpuExecutionFailureStage,
    GpuFallbackBackend, SearchBackendFallbackReason,
};

use super::{GpuBackendCapability, GpuBackendKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuBackendSelection {
    requested: GpuBackendKind,
    selected: GpuBackendKind,
    capability: GpuBackendCapability,
    failure: Option<GpuExecutionFailureResolution>,
    capability_reason: Option<&'static str>,
}

impl GpuBackendSelection {
    pub(super) const fn available(
        requested: GpuBackendKind,
        capability: GpuBackendCapability,
    ) -> Self {
        Self {
            requested,
            selected: requested,
            capability,
            failure: None,
            capability_reason: None,
        }
    }

    pub(super) const fn from_failure(
        requested: GpuBackendKind,
        selected: GpuBackendKind,
        capability: GpuBackendCapability,
        failure: GpuExecutionFailureResolution,
        capability_reason: &'static str,
    ) -> Self {
        Self {
            requested,
            selected,
            capability,
            failure: Some(failure),
            capability_reason: Some(capability_reason),
        }
    }
}
impl GpuBackendSelection {
    pub fn requested(self) -> GpuBackendKind {
        self.requested
    }
}
impl GpuBackendSelection {
    pub fn selected(self) -> GpuBackendKind {
        self.selected
    }
}
impl GpuBackendSelection {
    pub fn capability(self) -> GpuBackendCapability {
        self.capability
    }
}
impl GpuBackendSelection {
    pub fn fallback_used(self) -> bool {
        self.failure.is_some_and(|failure| failure.fallback_used())
    }
}
impl GpuBackendSelection {
    pub fn fallback_reason(self) -> Option<&'static str> {
        self.fallback_used()
            .then_some(self.capability_reason)
            .flatten()
    }

    pub const fn gpu_failure_class(self) -> Option<GpuExecutionFailureClass> {
        match self.failure {
            Some(failure) => Some(failure.class()),
            None => None,
        }
    }

    pub const fn gpu_failure_stage(self) -> Option<GpuExecutionFailureStage> {
        match self.failure {
            Some(failure) => Some(failure.stage()),
            None => None,
        }
    }

    pub const fn fallback_backend(self) -> Option<GpuFallbackBackend> {
        match self.failure {
            Some(failure) => failure.fallback_backend(),
            None => None,
        }
    }

    pub const fn backend_fallback_reason(self) -> Option<SearchBackendFallbackReason> {
        match self.failure {
            Some(failure) => failure.backend_fallback_reason(),
            None => None,
        }
    }

    pub const fn discarded_partial_gpu_result(self) -> bool {
        match self.failure {
            Some(failure) => failure.discarded_partial_gpu_result(),
            None => false,
        }
    }
}

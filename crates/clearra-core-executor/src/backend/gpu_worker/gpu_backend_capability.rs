use super::GpuBackendKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuBackendCapability {
    kind: GpuBackendKind,
    available: bool,
    unavailable_reason: Option<&'static str>,
    accepts_user_shader_path: bool,
}

impl GpuBackendCapability {
    pub const fn unavailable(kind: GpuBackendKind, reason: &'static str) -> Self {
        Self {
            kind,
            available: false,
            unavailable_reason: Some(reason),
            accepts_user_shader_path: false,
        }
    }
}
impl GpuBackendCapability {
    pub fn for_kind(kind: GpuBackendKind) -> Self {
        match kind {
            GpuBackendKind::NativeCompute => {
                Self::unavailable(kind, "native_gpu_backend_not_built")
            }
            GpuBackendKind::Disabled => Self::unavailable(kind, "gpu_feature_disabled"),
        }
    }
}
impl GpuBackendCapability {
    pub fn kind(self) -> GpuBackendKind {
        self.kind
    }
}
impl GpuBackendCapability {
    pub fn contract_label(self) -> &'static str {
        if self.available {
            return self.kind.as_str();
        }

        match self.kind {
            GpuBackendKind::NativeCompute => "native-gpu-unavailable",
            GpuBackendKind::Disabled => "disabled",
        }
    }
}
impl GpuBackendCapability {
    pub fn is_available(self) -> bool {
        self.available
    }
}
impl GpuBackendCapability {
    pub fn unavailable_reason(self) -> Option<&'static str> {
        self.unavailable_reason
    }
}
impl GpuBackendCapability {
    pub fn accepts_user_shader_path(self) -> bool {
        self.accepts_user_shader_path
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuBackendKind {
    NativeCompute,
    Disabled,
}

impl GpuBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeCompute => "native-gpu",
            Self::Disabled => "disabled",
        }
    }
}
impl GpuBackendKind {
    pub fn is_real_gpu_api(self) -> bool {
        self == Self::NativeCompute
    }
}

impl Default for GpuBackendKind {
    fn default() -> Self {
        Self::Disabled
    }
}

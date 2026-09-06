#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuBackendKind {
    NativeCompute,
    #[default]
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

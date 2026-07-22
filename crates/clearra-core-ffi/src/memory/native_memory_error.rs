use super::memory_abi::CClrMemStatus;
use crate::diagnostics::CoreFfiDiagnosticCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMemoryError {
    BindingUnavailable,
    InvalidArgument,
    OutOfMemory,
    DoubleRelease,
    Aborted,
    CanaryCorrupted,
    DebugPoisoned,
    NotFound,
    InvalidState,
}

impl NativeMemoryError {
    pub fn from_status(status: CClrMemStatus) -> Option<Self> {
        match status {
            CClrMemStatus::Ok => None,
            CClrMemStatus::InvalidArgument => Some(Self::InvalidArgument),
            CClrMemStatus::OutOfMemory => Some(Self::OutOfMemory),
            CClrMemStatus::DoubleRelease => Some(Self::DoubleRelease),
            CClrMemStatus::Aborted => Some(Self::Aborted),
            CClrMemStatus::CanaryCorrupted => Some(Self::CanaryCorrupted),
            CClrMemStatus::DebugPoisoned => Some(Self::DebugPoisoned),
            CClrMemStatus::NotFound => Some(Self::NotFound),
            CClrMemStatus::InvalidState => Some(Self::InvalidState),
        }
    }
}
impl NativeMemoryError {
    pub fn to_release_diagnostic_material(self) -> NativeMemoryReleaseDiagnosticMaterial {
        let diagnostic_code = match self {
            Self::DoubleRelease => CoreFfiDiagnosticCode::CORE_MEMORY_CONTEXT_DOUBLE_RELEASE,
            Self::InvalidArgument | Self::InvalidState => {
                CoreFfiDiagnosticCode::CORE_MEMORY_SCOPE_INVALID
            }
            Self::BindingUnavailable | Self::OutOfMemory | Self::Aborted | Self::NotFound => {
                CoreFfiDiagnosticCode::CORE_MEMORY_LEAK_DETECTED
            }
            Self::CanaryCorrupted | Self::DebugPoisoned => {
                CoreFfiDiagnosticCode::CORE_MEMORY_LEAK_DETECTED
            }
        };
        NativeMemoryReleaseDiagnosticMaterial {
            error: self,
            diagnostic_code,
            released_state: "released",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMemoryReleaseDiagnosticMaterial {
    pub error: NativeMemoryError,
    pub diagnostic_code: &'static str,
    pub released_state: &'static str,
}

#[cfg(test)]
#[path = "native_memory_error_tests.rs"]
mod tests;

use crate::memory::CClrMemStatus;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoreFfiDiagnosticCode;

impl CoreFfiDiagnosticCode {
    pub const CORE_MEMORY_CONTEXT_DOUBLE_RELEASE: &'static str =
        "E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE";
    pub const CORE_MEMORY_SCOPE_INVALID: &'static str = "E_CORE_MEMORY_SCOPE_INVALID";
    pub const CORE_MEMORY_LEAK_DETECTED: &'static str = "E_CORE_MEMORY_LEAK_DETECTED";
    pub const CORE_FFI_BUFFER_BOUNDS: &'static str = "E_CORE_FFI_BUFFER_BOUNDS";
    pub const CORE_INVALID_NATIVE_VIEW: &'static str = "E_CORE_INVALID_NATIVE_VIEW";
    pub const KICK_EVIDENCE_BUFFER_EXHAUSTED: &'static str = "E_KICK_EVIDENCE_BUFFER_EXHAUSTED";
}

pub fn memory_status_diagnostic_code(status: CClrMemStatus) -> Option<&'static str> {
    match status {
        CClrMemStatus::Ok => None,
        CClrMemStatus::DoubleRelease => {
            Some(CoreFfiDiagnosticCode::CORE_MEMORY_CONTEXT_DOUBLE_RELEASE)
        }
        CClrMemStatus::InvalidArgument | CClrMemStatus::InvalidState => {
            Some(CoreFfiDiagnosticCode::CORE_MEMORY_SCOPE_INVALID)
        }
        CClrMemStatus::OutOfMemory
        | CClrMemStatus::Aborted
        | CClrMemStatus::CanaryCorrupted
        | CClrMemStatus::DebugPoisoned
        | CClrMemStatus::NotFound => Some(CoreFfiDiagnosticCode::CORE_MEMORY_LEAK_DETECTED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_status_maps_to_specific_security_diagnostic_code() {
        assert_eq!(
            memory_status_diagnostic_code(CClrMemStatus::DoubleRelease),
            Some("E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE")
        );
        assert_eq!(
            memory_status_diagnostic_code(CClrMemStatus::InvalidState),
            Some("E_CORE_MEMORY_SCOPE_INVALID")
        );
        assert_eq!(memory_status_diagnostic_code(CClrMemStatus::Ok), None);
    }
}

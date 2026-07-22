use super::*;

#[test]
fn native_memory_error_maps_c_status() {
    assert_eq!(NativeMemoryError::from_status(CClrMemStatus::Ok), None);
    assert_eq!(
        NativeMemoryError::from_status(CClrMemStatus::InvalidState),
        Some(NativeMemoryError::InvalidState)
    );
}

#[test]
fn native_memory_release_error_maps_to_diagnostic_material() {
    let material = NativeMemoryError::DoubleRelease.to_release_diagnostic_material();

    assert_eq!(material.error, NativeMemoryError::DoubleRelease);
    assert_eq!(
        material.diagnostic_code,
        "E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE"
    );
    assert_eq!(material.released_state, "released");
}

#[test]
fn native_memory_error_uses_specific_c_status_diagnostic_code() {
    assert_eq!(
        crate::diagnostics::memory_status_diagnostic_code(CClrMemStatus::InvalidArgument),
        Some("E_CORE_MEMORY_SCOPE_INVALID")
    );
}

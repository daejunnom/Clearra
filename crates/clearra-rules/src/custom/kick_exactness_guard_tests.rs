use super::*;

#[test]
fn imported_verified_kick_supports_exact_180_after_verification() {
    let guard = CustomKickExactnessGuard::imported_verified(true);

    assert_eq!(guard.source_kind(), KickProfileSourceKind::ImportedVerified);
    assert!(guard.verified());
    assert!(guard.supports_exact_180());
}

#[test]
fn unverified_custom_kick_rejected_before_c_execution() {
    let guard = CustomKickExactnessGuard::unverified_custom();

    assert_eq!(
        guard.disabled_reason(),
        Some("unverified_custom_kick_rejected_before_c_execution")
    );
    assert_eq!(
        guard.validate_before_c_execution(),
        Err(CustomKickExecutionError::UnverifiedCustomKickRejectedBeforeCExecution)
    );
}

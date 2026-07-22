use super::mvp3_capability_registry::{
    Mvp3CapabilityError, Mvp3CapabilityId, Mvp3CapabilityReport,
};

#[test]
fn mvp3_capability_report_lists_all_generalization_features() {
    let report = Mvp3CapabilityReport::current();

    assert!(report.mvp3_capability_report_lists_all_generalization_features());
}

#[test]
fn unsupported_features_do_not_execute_runtime() {
    let report = Mvp3CapabilityReport::current();

    assert!(report.unsupported_features_do_not_execute_runtime());
    assert_eq!(
        report.assert_runtime_execution_allowed(Mvp3CapabilityId::CustomPieceSchema),
        Err(Mvp3CapabilityError::RuntimeExecutionRequiresRuntimeConnected)
    );
}

#[test]
fn unsupported_features_emit_disabled_reason() {
    let report = Mvp3CapabilityReport::current();

    assert!(report.unsupported_features_emit_disabled_reason());
}

#[test]
fn standard_fast_path_unchanged() {
    let report = Mvp3CapabilityReport::current();

    assert!(report.standard_fast_path_unchanged());
}

#[test]
fn exact_claim_requires_exact_supported() {
    let report = Mvp3CapabilityReport::current();

    assert_eq!(
        report.assert_exact_claim_allowed(Mvp3CapabilityId::GenericGpuDescriptor),
        Err(Mvp3CapabilityError::ExactClaimRequiresExactSupported)
    );
}

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport,
    },
    scope::mvp_scope_guard::MvpScopeGuard,
};

#[test]
fn error_diagnostics_disable_mvp_scope() {
    let mut report = DiagnosticReport::new();
    report.push(Diagnostic::new(
        DiagnosticCode::EPcTargetUnsupportedMvp,
        "outside MVP",
    ));

    let guard = MvpScopeGuard::from_report(&report);

    assert!(!guard.is_allowed());
    assert_eq!(guard.disabled_reasons().len(), 1);
    assert_eq!(
        guard.disabled_reasons()[0].code(),
        DiagnosticCode::EPcTargetUnsupportedMvp
    );
}

#[test]
fn warning_diagnostics_do_not_disable_mvp_scope() {
    let mut report = DiagnosticReport::new();
    report.push(Diagnostic::new(
        DiagnosticCode::WMinimumCoverGreedyFallback,
        "approximate",
    ));

    let guard = MvpScopeGuard::from_report(&report);

    assert!(guard.is_allowed());
}

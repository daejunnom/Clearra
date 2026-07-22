use super::*;
use crate::{app_response::AppStatus, AppErrorCode};
use clearra_core_executor::CoreExecutionResult;

#[test]
fn resource_cap_never_returns_complete_probability() {
    let result = CoreExecutionResult::new(
        vec![
            ("resource_truncated".to_owned(), "true".to_owned()),
            (
                "resource_truncation_reason".to_owned(),
                "frontier_budget_exceeded".to_owned(),
            ),
            (
                "resource_probability_complete".to_owned(),
                "true".to_owned(),
            ),
        ],
        Vec::new(),
    );

    let report = resource_report_from_core_result(&result);

    assert!(report.truncated());
    assert!(!report.probability_complete());
}

#[test]
fn c_packing_capacity_status_maps_to_incomplete_resource_report() {
    let error = AppError::new(AppErrorCode::ExecutionFailed, "Native(PackingStatus(6))");

    let report = resource_report_from_failure(AppStatus::ExecutionFailed, &error);
    let diagnostics = resource_diagnostics_from_failure(&error);

    assert!(report.truncated());
    assert_eq!(
        report.truncation_reason(),
        Some("packing_capacity_exceeded")
    );
    assert!(!report.probability_complete());
    assert!(diagnostics.contains_code(DiagnosticCode::ECorePackingFailed));
    let diagnostic = diagnostics
        .diagnostics()
        .first()
        .expect("truncation diagnostic");
    assert!(diagnostic.evidence().iter().any(|evidence| {
        evidence.key() == "truncation_reason" && evidence.value() == "packing_capacity_exceeded"
    }));
}

#[test]
fn c_buildup_truncation_status_maps_to_count_incomplete_diagnostic() {
    let error = AppError::new(
        AppErrorCode::ExecutionFailed,
        "CLR_BUILDUP_ENUMERATION_TRUNCATED",
    );

    let report = resource_report_from_failure(AppStatus::ExecutionFailed, &error);
    let diagnostics = resource_diagnostics_from_failure(&error);

    assert!(report.truncated());
    assert_eq!(
        report.truncation_reason(),
        Some("buildup_enumeration_truncated")
    );
    assert!(!report.probability_complete());
    assert!(diagnostics.contains_code(DiagnosticCode::WBuildUpEnumerationTruncated));
}

#[test]
fn c_coverage_capacity_status_maps_to_probability_incomplete_diagnostic() {
    let error = AppError::new(
        AppErrorCode::ExecutionFailed,
        "CLR_COVERAGE_CAPACITY_EXCEEDED",
    );

    let report = resource_report_from_failure(AppStatus::ExecutionFailed, &error);
    let diagnostics = resource_diagnostics_from_failure(&error);

    assert!(report.truncated());
    assert_eq!(
        report.truncation_reason(),
        Some("coverage_capacity_exceeded")
    );
    assert!(!report.probability_complete());
    assert!(diagnostics.contains_code(DiagnosticCode::ECoverageCapacityExceeded));
}

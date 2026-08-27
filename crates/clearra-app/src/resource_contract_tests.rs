use super::*;
use crate::{app_response::AppStatus, AppErrorCode};
use clearra_core_executor::CoreExecutionResult;
use clearra_host_contract::{
    ExecutionAvailabilityReason, ExecutionAvailabilityState, ExecutionCompletenessState,
};

#[test]
fn spin_structure_report_is_recorded_as_executed_with_exact_completeness() {
    for (complete, expected_availability, expected_completeness) in [
        (
            true,
            ExecutionAvailabilityState::Available,
            ExecutionCompletenessState::Complete,
        ),
        (
            false,
            ExecutionAvailabilityState::Incomplete,
            ExecutionCompletenessState::Incomplete,
        ),
    ] {
        let render_model = crate::render::AppRenderModel::SpinStructure(
            clearra_spin_structure_search::SpinStructureReport {
                complete,
                ..Default::default()
            },
        );

        let report = resource_report_from_render_model(&render_model);

        assert!(report.solver_executed());
        assert_eq!(
            report.execution_availability().state(),
            expected_availability
        );
        assert_eq!(report.result_completeness(), expected_completeness);
        assert!(!report.probability_complete());
    }
}

#[test]
fn typed_core_admission_failure_preserves_exact_evidence_and_not_executed_axis() {
    let availability = clearra_core_domain::resource::ExecutionAvailability::unavailable(
        clearra_core_domain::resource::ExecutionAvailabilityReason::DensePatternRepresentationUnavailable,
    )
    .with_pattern_evidence(35_384_428_800, 35_384_428_800, 4_423_053_600)
    .with_required_memory_bytes(8_846_107_200);
    let core = clearra_core_domain::resource::ResourceReport::admission_failure(availability);

    let report = resource_report_from_core_domain(&core);

    assert!(!report.solver_executed());
    assert_eq!(
        report.execution_availability().state(),
        ExecutionAvailabilityState::Unavailable
    );
    assert_eq!(
        report.execution_availability().descriptor_pattern_count(),
        Some("35384428800")
    );
    assert_eq!(
        report.execution_availability().dense_pattern_count(),
        Some("35384428800")
    );
    assert_eq!(
        report.execution_availability().required_dense_bytes(),
        Some("4423053600")
    );
    assert_eq!(
        report.execution_availability().required_memory_bytes(),
        Some("8846107200")
    );
    assert_eq!(
        report.result_completeness(),
        ExecutionCompletenessState::NotExecuted
    );
}

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
    assert_eq!(
        report.execution_availability().state(),
        ExecutionAvailabilityState::Exhausted
    );
    assert_eq!(
        report.result_completeness(),
        ExecutionCompletenessState::Incomplete
    );
}

#[test]
fn dense_preflight_unavailability_is_not_reported_as_executed_or_complete() {
    let error = AppError::new(
        AppErrorCode::ExecutionFailed,
        "Pc(\"dense_pattern_representation_unavailable\")",
    );

    let report = resource_report_from_failure(AppStatus::ExecutionFailed, &error);

    assert!(!report.solver_executed());
    assert_eq!(
        report.execution_availability().state(),
        ExecutionAvailabilityState::Unavailable
    );
    assert_eq!(
        report.execution_availability().reason(),
        Some(ExecutionAvailabilityReason::DensePatternRepresentationUnavailable)
    );
    assert_eq!(
        report.result_completeness(),
        ExecutionCompletenessState::NotExecuted
    );
}

#[test]
fn shared_lease_contention_is_deferred_not_empty_success_or_exhaustion() {
    let error = AppError::new(
        AppErrorCode::ExecutionFailed,
        "RuntimeUnavailable(shared_execution_resource_deferred)",
    );

    let report = resource_report_from_failure(AppStatus::ExecutionFailed, &error);

    assert!(!report.solver_executed());
    assert_eq!(
        report.execution_availability().state(),
        ExecutionAvailabilityState::Deferred
    );
    assert_eq!(
        report.result_completeness(),
        ExecutionCompletenessState::NotExecuted
    );
}

#[test]
fn incomplete_zero_count_remains_incomplete_even_when_preflight_was_available() {
    let result = CoreExecutionResult::new(
        vec![
            (
                "execution_availability_state".to_owned(),
                "available".to_owned(),
            ),
            ("unique_solution_count".to_owned(), "0".to_owned()),
            ("solution_count_calculated".to_owned(), "true".to_owned()),
            ("count_complete".to_owned(), "false".to_owned()),
            (
                "count_truncated_reason".to_owned(),
                "partial_execution".to_owned(),
            ),
        ],
        Vec::new(),
    );

    let report = resource_report_from_core_result(&result);

    assert_eq!(
        report.execution_availability().state(),
        ExecutionAvailabilityState::Incomplete
    );
    assert_eq!(
        report.result_completeness(),
        ExecutionCompletenessState::Incomplete
    );
}

#[test]
fn legacy_numeric_zero_without_completeness_evidence_is_not_complete() {
    let result = CoreExecutionResult::new(
        vec![("unique_solution_count".to_owned(), "0".to_owned())],
        Vec::new(),
    );

    let report = resource_report_from_core_result(&result);

    assert!(!report.probability_complete());
    assert_eq!(
        report.execution_availability().state(),
        ExecutionAvailabilityState::Incomplete
    );
    assert_eq!(
        report.result_completeness(),
        ExecutionCompletenessState::Incomplete
    );
}

#[test]
fn complete_non_solution_objective_does_not_require_a_solution_count() {
    let result = CoreExecutionResult::new(
        vec![("objective_complete".to_owned(), "true".to_owned())],
        Vec::new(),
    );

    let report = resource_report_from_core_result(&result);

    assert_eq!(
        report.execution_availability().state(),
        ExecutionAvailabilityState::Available
    );
    assert_eq!(
        report.result_completeness(),
        ExecutionCompletenessState::Complete
    );
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

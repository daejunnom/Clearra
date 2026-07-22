use crate::{
    app_error::{AppError, AppErrorCode},
    render::AppRenderModel,
};
use clearra_core_domain::resource::ResourceReport as CoreResourceReport;
use clearra_core_executor::CoreExecutionResult;
use clearra_host_contract::ResourceReport;
use clearra_validation::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub(crate) fn resource_report_from_render_model(render_model: &AppRenderModel) -> ResourceReport {
    if let Some(result) = render_model.forward_search_result() {
        let mut report = ResourceReport::new(true, "reported");
        report.peak_frontier_states = result.peak_frontier();
        report.probability_complete = result.complete();
        return report;
    }
    let Some(result) = render_model.core_result() else {
        return ResourceReport::new(true, "not-reported");
    };

    resource_report_from_core_result(result)
}

pub(crate) fn resource_report_from_core_domain(source: &CoreResourceReport) -> ResourceReport {
    let mut report = ResourceReport::new(true, "reported");
    report.truncated = source.truncated;
    report.truncation_reason = source
        .truncation_reason
        .map(|reason| reason.as_str().to_owned());
    report.peak_frontier_states = source.peak_frontier_states;
    report.peak_candidate_rows = source.peak_candidate_rows;
    report.peak_hash_buckets = source.peak_hash_buckets;
    report.peak_gpu_bytes = source.peak_gpu_bytes;
    report.peak_cpu_bytes = source.peak_cpu_bytes;
    report.build_worker_backlog_peak = source.build_worker_backlog_peak;
    report.coverage_rows_emitted = source.coverage_rows_emitted;
    report.probability_complete = source.probability_complete && !source.truncated;
    report
}

pub(crate) fn resource_diagnostics_from_render_model(
    render_model: &AppRenderModel,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    let Some(result) = render_model.core_result() else {
        return report;
    };

    if let Some(reason) = resource_truncation_reason(result) {
        report.push(resource_truncation_diagnostic(reason));
    }
    if result.bool_field("trace_retention_truncated") == Some(true) {
        report.push(
            Diagnostic::new(
                DiagnosticCode::WTraceRetentionTruncated,
                "trace retention was truncated; counts may still be complete",
            )
            .with_location(EvidenceLocation::new("app_response.resource_report"))
            .with_evidence(ValidationEvidence::new(
                "trace_retention_reason",
                result.field("trace_retention_reason").unwrap_or("unknown"),
            )),
        );
    }
    if observed_probability_incomplete(result) {
        report.push(
            Diagnostic::new(
                DiagnosticCode::WObservedQueueProbabilityIncomplete,
                "observed queue expansion is incomplete and probability was not renormalized",
            )
            .with_location(EvidenceLocation::new("app_response.resource_report"))
            .with_evidence(ValidationEvidence::new(
                "truncation_reason",
                "observed_universe_truncated",
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Increase the observed queue budget or treat the probability as incomplete.",
            )),
        );
    }

    report
}

pub(crate) fn resource_report_from_failure(
    status: crate::app_response::AppStatus,
    error: &AppError,
) -> ResourceReport {
    let memory_status = if error.code() == AppErrorCode::NativeCoreUnavailable {
        "not-executed-native-core-unavailable"
    } else {
        "not-reported"
    };
    let mut report = ResourceReport::new(
        matches!(status, crate::app_response::AppStatus::ExecutionFailed),
        memory_status,
    );
    if let Some(reason) = failure_truncation_reason(error) {
        report = report.with_truncation(reason);
    }
    report.probability_complete = false;
    report
}

pub(crate) fn resource_diagnostics_from_failure(error: &AppError) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    if error.code() == AppErrorCode::NativeCoreUnavailable {
        report.push(
            Diagnostic::new(
                DiagnosticCode::ENativeCoreUnavailable,
                "the native C core is unavailable; no solver backend was executed",
            )
            .with_location(EvidenceLocation::new("app_response.backend_report"))
            .with_evidence(ValidationEvidence::new("backend_selected", "none"))
            .with_evidence(ValidationEvidence::new("fallback_used", "false"))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Build and link the native C core with the native-c-core feature before executing solver commands.",
            )),
        );
    }
    if let Some(reason) = failure_truncation_reason(error) {
        report.push(resource_truncation_diagnostic(reason));
    }
    report
}

fn resource_report_from_core_result(result: &CoreExecutionResult) -> ResourceReport {
    let mut report = ResourceReport::new(true, result.field("memory_status").unwrap_or("reported"));
    report.peak_frontier_states = result
        .usize_field("resource_peak_frontier_states")
        .unwrap_or(0);
    report.peak_candidate_rows = result
        .usize_field("resource_peak_candidate_rows")
        .unwrap_or(0);
    report.peak_hash_buckets = result
        .usize_field("resource_peak_hash_buckets")
        .unwrap_or(0);
    report.peak_gpu_bytes = result.usize_field("resource_peak_gpu_bytes").unwrap_or(0);
    report.peak_cpu_bytes = result.usize_field("resource_peak_cpu_bytes").unwrap_or(0);
    report.build_worker_backlog_peak = result
        .usize_field("resource_build_worker_backlog_peak")
        .unwrap_or(0);
    report.coverage_rows_emitted = result
        .usize_field("resource_coverage_rows_emitted")
        .unwrap_or(0);

    let count_complete = result.bool_field("count_complete").unwrap_or(true);
    let probability_complete = result
        .bool_field("resource_probability_complete")
        .or_else(|| result.bool_field("probability_complete"))
        .unwrap_or(true);

    if let Some(reason) = resource_truncation_reason(result) {
        report = report.with_truncation(reason);
    }
    report.probability_complete = probability_complete && count_complete && !report.truncated;
    report
}

fn resource_truncation_reason(result: &CoreExecutionResult) -> Option<&str> {
    result
        .field("resource_truncation_reason")
        .filter(|reason| *reason != "none")
        .or_else(|| {
            (result.bool_field("count_complete") == Some(false)).then(|| {
                result
                    .field("count_truncated_reason")
                    .unwrap_or("count_incomplete")
            })
        })
        .or_else(|| {
            observed_probability_incomplete(result).then_some("observed_universe_truncated")
        })
        .or_else(|| {
            (result.bool_field("resource_truncated") == Some(true)).then_some("resource_truncated")
        })
}

fn observed_probability_incomplete(result: &CoreExecutionResult) -> bool {
    result.bool_field("supply_expansion_truncated") == Some(true)
        || result.bool_field("supply_probability_complete") == Some(false)
            && result.field("queue_mode") == Some("observed")
}

fn failure_truncation_reason(error: &AppError) -> Option<&'static str> {
    if error.code() != AppErrorCode::ExecutionFailed {
        return None;
    }

    let message = error.message();
    if message.contains("PackingStatus(6)") || message.contains("CLEARRA_PACKING_CAPACITY_EXCEEDED")
    {
        Some("packing_capacity_exceeded")
    } else if message.contains("CLR_BUILDUP_CAPACITY_EXCEEDED")
        || message.contains("BuildUpCapacityExceeded")
    {
        Some("buildup_capacity_exceeded")
    } else if message.contains("CLR_BUILDUP_ENUMERATION_TRUNCATED")
        || message.contains("EnumerationTruncated")
    {
        Some("buildup_enumeration_truncated")
    } else if message.contains("CLR_COVERAGE_CAPACITY_EXCEEDED")
        || message.contains("CoverageCapacityExceeded")
    {
        Some("coverage_capacity_exceeded")
    } else {
        None
    }
}

fn resource_truncation_diagnostic(reason: &str) -> Diagnostic {
    let code = diagnostic_code_for_resource_reason(reason);
    Diagnostic::new(
        code,
        "resource cap or native truncation made the product result incomplete",
    )
    .with_location(EvidenceLocation::new("app_response.resource_report"))
    .with_evidence(ValidationEvidence::new("truncation_reason", reason))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Increase the relevant resource budget or treat count and probability fields as incomplete.",
    ))
}

fn diagnostic_code_for_resource_reason(reason: &str) -> DiagnosticCode {
    match reason {
        "coverage_rows_exceeded" | "pattern_bits_exceeded" | "coverage_capacity_exceeded" => {
            DiagnosticCode::ECoverageCapacityExceeded
        }
        "buildup_enumeration_truncated" => DiagnosticCode::WBuildUpEnumerationTruncated,
        "buildup_capacity_exceeded" => DiagnosticCode::EBuildUpVariantEnumerationTruncated,
        "observed_universe_truncated" => DiagnosticCode::WObservedQueueProbabilityIncomplete,
        "retained_trace_limit" | "trace_retention_truncated" => {
            DiagnosticCode::WTraceRetentionTruncated
        }
        _ => DiagnosticCode::ECorePackingFailed,
    }
}

#[cfg(test)]
#[path = "resource_contract_tests.rs"]
mod tests;

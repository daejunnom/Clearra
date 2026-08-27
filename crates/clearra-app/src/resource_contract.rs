use crate::{
    app_error::{AppError, AppErrorCode},
    render::AppRenderModel,
};
use clearra_core_domain::resource::ResourceReport as CoreResourceReport;
use clearra_core_executor::CoreExecutionResult;
use clearra_host_contract::{
    ExecutionAvailabilityReason, ExecutionAvailabilityReport, ExecutionAvailabilityState,
    ExecutionCompletenessState, ExecutionSurface, ResourceReport,
};
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
        report.set_result_completeness(if result.complete() {
            ExecutionCompletenessState::Complete
        } else {
            ExecutionCompletenessState::Incomplete
        });
        if !result.complete() {
            report = report.with_execution_availability(ExecutionAvailabilityReport::incomplete(
                ExecutionSurface::current(),
                ExecutionAvailabilityReason::PartialExecution,
            ));
        }
        return report;
    }
    if let Some(result) = render_model.spin_structure_result() {
        let mut report = ResourceReport::new(true, "reported");
        report.set_result_completeness(if result.complete {
            ExecutionCompletenessState::Complete
        } else {
            ExecutionCompletenessState::Incomplete
        });
        if !result.complete {
            report = report.with_execution_availability(ExecutionAvailabilityReport::incomplete(
                ExecutionSurface::current(),
                ExecutionAvailabilityReason::PartialExecution,
            ));
        }
        return report;
    }
    let Some(result) = render_model.core_result() else {
        return ResourceReport::new(false, "not-reported");
    };

    resource_report_from_core_result(result)
}

pub(crate) fn resource_report_from_core_domain(source: &CoreResourceReport) -> ResourceReport {
    let mut report = ResourceReport::new(
        source.execution_started(),
        if source.execution_started() {
            "reported"
        } else {
            "not-executed"
        },
    );
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
    let core_availability = source.execution_availability();
    let mut availability = if core_availability.state()
        == clearra_core_domain::resource::ExecutionAvailabilityState::Available
        && source.truncated
    {
        availability_from_truncation_reason(
            source
                .truncation_reason
                .map(|reason| reason.as_str())
                .unwrap_or("resource_truncated"),
        )
    } else {
        availability_from_core_domain(core_availability)
    };
    if let (Some(descriptor), Some(dense), Some(bytes)) = (
        core_availability.descriptor_pattern_count(),
        core_availability.dense_pattern_count(),
        core_availability.required_dense_bytes(),
    ) {
        availability = availability.with_pattern_evidence(descriptor, dense, bytes);
    }
    if let Some(required_memory_bytes) = core_availability.required_memory_bytes() {
        availability = availability.with_required_memory_bytes(required_memory_bytes);
    }
    report = report.with_execution_availability(availability);
    report.set_result_completeness(if !source.execution_started() {
        ExecutionCompletenessState::NotExecuted
    } else if !source.result_complete() || source.truncated {
        ExecutionCompletenessState::Incomplete
    } else {
        ExecutionCompletenessState::Complete
    });
    report
}

fn availability_from_core_domain(
    source: clearra_core_domain::resource::ExecutionAvailability,
) -> ExecutionAvailabilityReport {
    use clearra_core_domain::resource::{
        ExecutionAvailabilityReason as CoreReason, ExecutionAvailabilityState as CoreState,
    };
    let surface = ExecutionSurface::current();
    let reason = match source.reason() {
        Some(CoreReason::NotExecuted) => ExecutionAvailabilityReason::NotExecuted,
        Some(CoreReason::CapabilityUnavailable) => {
            ExecutionAvailabilityReason::CapabilityUnavailable
        }
        Some(CoreReason::PatternCountAddressSpaceExceeded) => {
            ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded
        }
        Some(CoreReason::DensePatternRepresentationUnavailable) => {
            ExecutionAvailabilityReason::DensePatternRepresentationUnavailable
        }
        Some(CoreReason::ComputeBudgetExceeded) => {
            ExecutionAvailabilityReason::ComputeBudgetExceeded
        }
        Some(CoreReason::MemoryBudgetExceeded) => ExecutionAvailabilityReason::MemoryBudgetExceeded,
        Some(CoreReason::SharedResourceContention) => {
            ExecutionAvailabilityReason::SharedResourceContention
        }
        Some(CoreReason::CancelledByCaller) => ExecutionAvailabilityReason::CancelledByCaller,
        Some(CoreReason::PartialExecution) => ExecutionAvailabilityReason::PartialExecution,
        None => return ExecutionAvailabilityReport::available(surface),
    };
    match source.state() {
        CoreState::Available => ExecutionAvailabilityReport::available(surface),
        CoreState::Unavailable => ExecutionAvailabilityReport::unavailable(surface, reason),
        CoreState::Deferred => ExecutionAvailabilityReport::deferred(surface, reason),
        CoreState::Exhausted => ExecutionAvailabilityReport::exhausted(surface, reason),
        CoreState::Cancelled => ExecutionAvailabilityReport::cancelled(surface),
        CoreState::Incomplete => ExecutionAvailabilityReport::incomplete(surface, reason),
    }
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
    let availability = availability_from_failure(status, error);
    let solver_executed = matches!(status, crate::app_response::AppStatus::ExecutionFailed)
        && matches!(
            availability.state(),
            ExecutionAvailabilityState::Incomplete
                | ExecutionAvailabilityState::Cancelled
                | ExecutionAvailabilityState::Exhausted
        )
        && !failure_prevented_execution(error);
    let memory_status = if error.code() == AppErrorCode::NativeCoreUnavailable {
        "not-executed-native-core-unavailable"
    } else if !solver_executed {
        "not-executed"
    } else {
        "not-reported"
    };
    let mut report = ResourceReport::new(solver_executed, memory_status)
        .with_execution_availability(availability);
    if let Some(reason) = failure_truncation_reason(error) {
        report = report.with_truncation(reason);
    }
    report.probability_complete = false;
    report.set_result_completeness(if solver_executed {
        ExecutionCompletenessState::Incomplete
    } else {
        ExecutionCompletenessState::NotExecuted
    });
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

    let count_complete = result.bool_field("count_complete").unwrap_or(false);
    let semantic_result_complete =
        count_complete || result.bool_field("objective_complete") == Some(true);
    let probability_complete = result
        .bool_field("resource_probability_complete")
        .or_else(|| result.bool_field("probability_complete"))
        .unwrap_or(false);

    if let Some(reason) = resource_truncation_reason(result) {
        report = report.with_truncation(reason);
    }
    report.probability_complete = probability_complete && count_complete && !report.truncated;
    let availability =
        availability_from_core_result(result, report.truncation_reason(), semantic_result_complete);
    report = report.with_execution_availability(availability);
    report.set_result_completeness(if semantic_result_complete && !report.truncated {
        ExecutionCompletenessState::Complete
    } else {
        ExecutionCompletenessState::Incomplete
    });
    report
}

fn availability_from_core_result(
    result: &CoreExecutionResult,
    truncation_reason: Option<&str>,
    semantic_result_complete: bool,
) -> ExecutionAvailabilityReport {
    let mut availability = if let Some(reason) = truncation_reason {
        availability_from_truncation_reason(reason)
    } else {
        match result.field("execution_availability_state") {
            Some("unavailable") => ExecutionAvailabilityReport::unavailable(
                ExecutionSurface::current(),
                availability_reason_from_str(
                    result
                        .field("execution_availability_reason")
                        .unwrap_or("not-executed"),
                ),
            ),
            Some("deferred") => ExecutionAvailabilityReport::deferred(
                ExecutionSurface::current(),
                ExecutionAvailabilityReason::SharedResourceContention,
            ),
            Some("exhausted") => ExecutionAvailabilityReport::exhausted(
                ExecutionSurface::current(),
                availability_reason_from_str(
                    result
                        .field("execution_availability_reason")
                        .unwrap_or("memory-budget-exceeded"),
                ),
            ),
            Some("cancelled") => {
                ExecutionAvailabilityReport::cancelled(ExecutionSurface::current())
            }
            Some("incomplete") => ExecutionAvailabilityReport::incomplete(
                ExecutionSurface::current(),
                ExecutionAvailabilityReason::PartialExecution,
            ),
            Some("available") if semantic_result_complete => {
                ExecutionAvailabilityReport::available(ExecutionSurface::current())
            }
            None if semantic_result_complete => {
                ExecutionAvailabilityReport::available(ExecutionSurface::current())
            }
            _ => ExecutionAvailabilityReport::incomplete(
                ExecutionSurface::current(),
                ExecutionAvailabilityReason::PartialExecution,
            ),
        }
    };
    if let (Some(descriptor), Some(dense), Some(bytes)) = (
        result
            .field("execution_descriptor_pattern_count")
            .and_then(|value| value.parse::<u128>().ok()),
        result
            .field("execution_dense_pattern_count")
            .and_then(|value| value.parse::<u128>().ok()),
        result
            .field("execution_required_dense_bytes")
            .and_then(|value| value.parse::<u128>().ok()),
    ) {
        availability = availability.with_pattern_evidence(descriptor, dense, bytes);
    }
    if let Some(required_memory_bytes) = result
        .field("execution_required_memory_bytes")
        .and_then(|value| value.parse::<u128>().ok())
    {
        availability = availability.with_required_memory_bytes(required_memory_bytes);
    }
    availability
}

fn availability_from_failure(
    status: crate::app_response::AppStatus,
    error: &AppError,
) -> ExecutionAvailabilityReport {
    let surface = ExecutionSurface::current();
    let message = error.message();
    if message.contains("pattern_count_address_space_unavailable")
        || message.contains("PatternCountOverflow")
    {
        ExecutionAvailabilityReport::unavailable(
            surface,
            ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
        )
    } else if message.contains("dense_pattern_representation_unavailable") {
        ExecutionAvailabilityReport::unavailable(
            surface,
            ExecutionAvailabilityReason::DensePatternRepresentationUnavailable,
        )
    } else if message.contains("shared_execution_resource_deferred") {
        ExecutionAvailabilityReport::deferred(
            surface,
            ExecutionAvailabilityReason::SharedResourceContention,
        )
    } else if message.contains("shared_execution_resource_exhausted")
        || message.contains("dense_pattern_memory_budget_exhausted")
    {
        ExecutionAvailabilityReport::exhausted(
            surface,
            ExecutionAvailabilityReason::MemoryBudgetExceeded,
        )
    } else if error.code() == AppErrorCode::NativeCoreUnavailable
        || matches!(status, crate::app_response::AppStatus::Unsupported)
    {
        ExecutionAvailabilityReport::unavailable(
            surface,
            ExecutionAvailabilityReason::CapabilityUnavailable,
        )
    } else if matches!(status, crate::app_response::AppStatus::ExecutionFailed) {
        if let Some(reason) = failure_truncation_reason(error) {
            availability_from_truncation_reason(reason)
        } else {
            ExecutionAvailabilityReport::incomplete(
                surface,
                ExecutionAvailabilityReason::PartialExecution,
            )
        }
    } else {
        ExecutionAvailabilityReport::not_executed(surface)
    }
}

fn failure_prevented_execution(error: &AppError) -> bool {
    let message = error.message();
    error.code() == AppErrorCode::NativeCoreUnavailable
        || message.contains("pattern_count_address_space_unavailable")
        || message.contains("PatternCountOverflow")
        || message.contains("dense_pattern_representation_unavailable")
        || message.contains("shared_execution_resource_deferred")
        || message.contains("shared_execution_resource_exhausted")
        || message.contains("dense_pattern_memory_budget_exhausted")
}

pub(crate) fn availability_from_truncation_reason(reason: &str) -> ExecutionAvailabilityReport {
    let surface = ExecutionSurface::current();
    match reason {
        "frontier_budget_exceeded"
        | "candidate_budget_exceeded"
        | "hash_bucket_budget_exceeded"
        | "gpu_batch_bytes_exceeded"
        | "readback_bytes_exceeded"
        | "build_worker_backlog_exceeded"
        | "coverage_rows_exceeded"
        | "pattern_bits_exceeded"
        | "cpu_time_exceeded"
        | "memory_exceeded"
        | "packing_capacity_exceeded"
        | "buildup_capacity_exceeded"
        | "coverage_capacity_exceeded" => ExecutionAvailabilityReport::exhausted(
            surface,
            if reason.contains("memory") {
                ExecutionAvailabilityReason::MemoryBudgetExceeded
            } else {
                ExecutionAvailabilityReason::ComputeBudgetExceeded
            },
        ),
        _ => ExecutionAvailabilityReport::incomplete(
            surface,
            ExecutionAvailabilityReason::PartialExecution,
        ),
    }
}

pub(crate) fn availability_reason_from_str(value: &str) -> ExecutionAvailabilityReason {
    match value {
        "pattern-count-address-space-exceeded" => {
            ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded
        }
        "dense-pattern-representation-unavailable" => {
            ExecutionAvailabilityReason::DensePatternRepresentationUnavailable
        }
        "compute-budget-exceeded" => ExecutionAvailabilityReason::ComputeBudgetExceeded,
        "memory-budget-exceeded" => ExecutionAvailabilityReason::MemoryBudgetExceeded,
        "shared-resource-contention" => ExecutionAvailabilityReason::SharedResourceContention,
        "cancelled-by-caller" => ExecutionAvailabilityReason::CancelledByCaller,
        "partial-execution" => ExecutionAvailabilityReason::PartialExecution,
        "capability-unavailable" => ExecutionAvailabilityReason::CapabilityUnavailable,
        _ => ExecutionAvailabilityReason::NotExecuted,
    }
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

pub(crate) fn diagnostic_code_for_resource_reason(reason: &str) -> DiagnosticCode {
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

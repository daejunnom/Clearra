use clearra_core_ffi::{
    CBuildVariantViewError, CClrMemStatus, CoreAbiVersion, CoreLeakReport, FfiProblemError,
    CLEARRA_CORE_ABI_VERSION,
};

use crate::{
    diagnostic::{diagnostic_code::DiagnosticCode, diagnostic_severity::DiagnosticSeverity},
    validators::core_security_gate::{CoreExecutionStage, CoreResultBufferKind, CoreSecurityGate},
};

#[test]
fn c_status_maps_to_memory_scope_diagnostic() {
    let diagnostic = CoreSecurityGate::c_status(CClrMemStatus::DoubleRelease, "core.memory")
        .expect("diagnostic");

    assert_eq!(diagnostic.code(), DiagnosticCode::ECMemoryScopeInvalid);
    assert_eq!(diagnostic.evidence()[0].key(), "c_status");
}

#[test]
fn abi_version_mismatch_maps_to_diagnostic() {
    let report = CoreSecurityGate::core_abi_version_mismatch(
        CoreAbiVersion::from_runtime(CLEARRA_CORE_ABI_VERSION + 1),
        "core.abi",
    );

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ECoreAbiVersionMismatch));
}

#[test]
fn ffi_problem_error_maps_to_stage_specific_core_diagnostic() {
    let packing = CoreSecurityGate::ffi_problem_error(
        CoreExecutionStage::Packing,
        FfiProblemError::QueueTooLong { len: 70_000 },
        "core.packing",
    );
    let buildup = CoreSecurityGate::ffi_problem_error(
        CoreExecutionStage::BuildUp,
        FfiProblemError::CandidateOperationCountTooLarge {
            operation_count: 99,
        },
        "core.buildup",
    );

    assert_eq!(packing.code(), DiagnosticCode::ECorePackingFailed);
    assert_eq!(buildup.code(), DiagnosticCode::ECoreBuildUpFailed);
}

#[test]
fn memory_leak_report_maps_to_error() {
    let report = CoreSecurityGate::memory_leak_report(
        CoreLeakReport {
            live_search_scopes: 1,
            live_batch_scopes: 0,
        },
        "core.memory",
    );

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ECMemoryLeakDetected));
}

#[test]
fn backend_security_diagnostics_cover_gpu_and_fallback() {
    let gpu = CoreSecurityGate::gpu_unavailable("gpu_feature_disabled", "backend");
    let fallback = CoreSecurityGate::backend_fallback_used(
        "gpu",
        "cpu-geometry-exact-cover",
        "gpu_feature_disabled",
        "backend",
    );
    let confirm =
        CoreSecurityGate::gpu_result_cpu_confirm_required("hash_exact_confirm_required", "backend");

    assert_eq!(gpu.code(), DiagnosticCode::EBackendGpuUnavailable);
    assert_eq!(fallback.code(), DiagnosticCode::WBackendFallbackUsed);
    assert_eq!(confirm.code(), DiagnosticCode::WGpuResultCpuConfirmRequired);
}

#[test]
fn invalid_c_result_buffer_is_rejected_as_core_buildup_failure() {
    let diagnostic = CoreSecurityGate::invalid_c_result_buffer(
        CoreResultBufferKind::CoverageRowBuffer,
        "word_count_exceeds_input",
        "core.coverage",
    );

    assert_eq!(diagnostic.code(), DiagnosticCode::ECoreBuildUpFailed);
    assert!(diagnostic
        .message()
        .contains("invalid C result buffer was rejected"));
}

#[test]
fn build_variant_view_error_maps_to_ffi_boundary_diagnostic() {
    let bounds = CoreSecurityGate::build_variant_view_error(
        CBuildVariantViewError::KickEvidenceCountExceeded { count: 17, max: 16 },
        "core.ffi",
    );
    let invalid = CoreSecurityGate::build_variant_view_error(
        CBuildVariantViewError::MissingKickEvidencePointer { count: 1 },
        "core.ffi",
    );

    assert_eq!(bounds.code(), DiagnosticCode::ECoreFfiBufferBounds);
    assert_eq!(invalid.code(), DiagnosticCode::ECoreInvalidNativeView);
}

#[test]
fn memory_budget_failures_map_to_specific_diagnostics() {
    let score = CoreSecurityGate::score_matrix_capacity_exceeded(9, 8, "score.matrix");
    let spin = CoreSecurityGate::spin_coverage_capacity_exceeded(10, 8, "spin.coverage");
    let coverage = CoreSecurityGate::coverage_capacity_exceeded(11, 8, "coverage.rows");
    let kick = CoreSecurityGate::kick_evidence_buffer_exhausted(17, 16, "core.buildup");

    assert_eq!(score.code(), DiagnosticCode::EScoreMatrixCapacityExceeded);
    assert_eq!(spin.code(), DiagnosticCode::ESpinCoverageCapacityExceeded);
    assert_eq!(coverage.code(), DiagnosticCode::ECoverageCapacityExceeded);
    assert_eq!(kick.code(), DiagnosticCode::EKickEvidenceBufferExhausted);
}

#[test]
fn score_matrix_capacity_exceeded_diagnostic() {
    let diagnostic = CoreSecurityGate::score_matrix_capacity_exceeded(9, 8, "score.matrix");

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::EScoreMatrixCapacityExceeded
    );
    assert!(diagnostic
        .message()
        .contains("score-cell matrix exceeded its configured memory budget"));
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "row_count" && evidence.value() == "9"));
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "row_limit" && evidence.value() == "8"));
}

#[test]
fn buildup_enumeration_truncation_reports_diagnostic() {
    let diagnostic = CoreSecurityGate::buildup_enumeration_truncated(512, 512, "core.buildup");

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::WBuildUpEnumerationTruncated
    );
    assert!(diagnostic.message().contains("variant enumeration stopped"));
    assert!(diagnostic
        .suggested_next_step()
        .expect("suggested step")
        .text()
        .contains("probability_complete=false"));
}

#[test]
fn coverage_capacity_exceeded_is_error_not_success() {
    let diagnostic = CoreSecurityGate::coverage_capacity_exceeded(1025, 1024, "coverage.rows");

    assert_eq!(diagnostic.code(), DiagnosticCode::ECoverageCapacityExceeded);
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
    assert!(diagnostic
        .suggested_next_step()
        .expect("suggested step")
        .text()
        .contains("do not report an empty or complete coverage set"));
}

#[test]
fn build_up_count_reports_truncation() {
    let diagnostic = CoreSecurityGate::build_up_variant_enumeration_truncated(
        120,
        1,
        "CLR_BUILDUP_ENUMERATION_TRUNCATED",
        "core.buildup",
    );

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::EBuildUpVariantEnumerationTruncated
    );
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "count_complete" && evidence.value() == "false"));
    assert!(diagnostic
        .suggested_next_step()
        .expect("suggested step")
        .text()
        .contains("partial BuildUp result"));
}

#[test]
fn observed_queue_truncation_is_not_renormalized() {
    let diagnostic = CoreSecurityGate::observed_queue_probability_incomplete(
        "0.875",
        "observed_queue_truncated",
        "supply.observed_queue",
    );

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::WObservedQueueProbabilityIncomplete
    );
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Warning);
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "renormalized" && evidence.value() == "false"));
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "probability_complete" && evidence.value() == "false"));
}

#[test]
fn observed_queue_truncation_not_renormalized() {
    let diagnostic = CoreSecurityGate::observed_queue_probability_incomplete(
        "0.875",
        "observed_queue_truncated",
        "supply.observed_queue",
    );

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::WObservedQueueProbabilityIncomplete
    );
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "renormalized" && evidence.value() == "false"));
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "probability_complete" && evidence.value() == "false"));
}

#[test]
fn trace_retention_truncated_warns_without_changing_total_count() {
    let diagnostic =
        CoreSecurityGate::trace_retention_truncated(2, 42, "retained_trace_limit", "pc.trace");

    assert_eq!(diagnostic.code(), DiagnosticCode::WTraceRetentionTruncated);
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Warning);
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "retained_trace_count" && evidence.value() == "2"));
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "total_solution_count" && evidence.value() == "42"));
}

#[test]
fn packing_candidate_as_solution_is_a_security_gate_error() {
    let diagnostic = CoreSecurityGate::packing_candidate_used_as_solution("core.output");

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::EPackingCandidateUsedAsSolution
    );
}

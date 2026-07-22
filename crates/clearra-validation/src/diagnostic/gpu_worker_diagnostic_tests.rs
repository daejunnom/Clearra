use super::*;
use crate::diagnostic::diagnostic_severity::DiagnosticSeverity;

#[test]
fn diagnostic_reports_gpu_worker_fallback_reason() {
    let input = GpuWorkerDiagnosticInput::new("available", "fallback-used")
        .with_fallback_reason("gpu_feature_disabled")
        .with_memory_ticket(42, 3);

    let diagnostic = gpu_backend_fallback_diagnostic(&input);

    assert_eq!(diagnostic.code(), DiagnosticCode::WGpuBackendFallback);
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Warning);
    assert!(diagnostic.evidence().iter().any(|evidence| {
        evidence.key() == "fallback_reason" && evidence.value() == "gpu_feature_disabled"
    }));
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| { evidence.key() == "memory_ticket_id" && evidence.value() == "42" }));
}

#[test]
fn diagnostic_reports_gpu_worker_unavailable_reason() {
    let input = GpuWorkerDiagnosticInput::new("unavailable", "unavailable")
        .with_unavailable_reason("gpu_feature_disabled")
        .with_memory_ticket(42, 3);

    let diagnostic = gpu_device_unavailable_diagnostic(&input);

    assert_eq!(diagnostic.code(), DiagnosticCode::WGpuDeviceUnavailable);
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Warning);
    assert!(diagnostic.evidence().iter().any(|evidence| {
        evidence.key() == "unavailable_reason" && evidence.value() == "gpu_feature_disabled"
    }));
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| { evidence.key() == "fence_epoch" && evidence.value() == "3" }));
}

#[test]
fn gpu_worker_trust_mismatch_is_error() {
    let input = GpuWorkerDiagnosticInput::new("available", "gpu-computed-mismatch");

    let diagnostic = gpu_worker_trust_mismatch_diagnostic(&input);

    assert_eq!(diagnostic.code(), DiagnosticCode::EGpuWorkerTrustMismatch);
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
}

#[test]
fn diagnostic_reports_gpu_worker_missing_memory_ticket() {
    let input = GpuWorkerDiagnosticInput::new("available", "gpu-computed-cpu-confirmed");

    let diagnostic = gpu_worker_memory_ticket_missing_diagnostic(&input);

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::EGpuWorkerMemoryTicketMissing
    );
    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
}

#[test]
fn diagnostic_reports_gpu_buffer_release_before_safe_epoch() {
    let input = GpuWorkerDiagnosticInput::new("available", "gpu-computed-unconfirmed")
        .with_memory_ticket(42, 5)
        .with_scope_epoch(3)
        .with_byte_budget(4096)
        .with_pending_gpu_buffer_releases(1);

    let diagnostic = gpu_buffer_release_deferred_diagnostic(&input);

    assert_eq!(diagnostic.code(), DiagnosticCode::WGpuBufferReleaseDeferred);
    assert!(
        diagnostic
            .evidence()
            .iter()
            .any(|evidence| evidence.key() == "pending_gpu_buffer_releases"
                && evidence.value() == "1")
    );
    assert!(diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "scope_epoch" && evidence.value() == "3"));
}

#[test]
fn diagnostic_reports_pending_release_queue_and_memory_pressure() {
    let input = GpuWorkerDiagnosticInput::new("busy", "gpu-computed-unconfirmed")
        .with_pending_release_queue(2)
        .with_memory_pressure_level("high");

    let pending = pending_release_queue_not_drained_diagnostic(&input);
    let pressure = memory_pressure_high_diagnostic(&input);

    assert_eq!(
        pending.code(),
        DiagnosticCode::WPendingReleaseQueueNotDrained
    );
    assert_eq!(pressure.code(), DiagnosticCode::WMemoryPressureHigh);
    assert!(pressure.evidence().iter().any(|evidence| {
        evidence.key() == "memory_pressure_level" && evidence.value() == "high"
    }));
}

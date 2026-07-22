use clearra_core_ffi::{CBuildVariantViewError, CClrMemStatus};

use crate::{
    diagnostic::{diagnostic_code::DiagnosticCode, diagnostic_severity::DiagnosticSeverity},
    validators::security_diagnostic_gate::SecurityDiagnosticGate,
};

#[test]
fn security_diagnostic_gate_maps_all_s_stage_errors() {
    let diagnostics = [
        SecurityDiagnosticGate::memory_context_double_release("memory.context"),
        SecurityDiagnosticGate::memory_scope_status(CClrMemStatus::InvalidState, "memory.scope"),
        SecurityDiagnosticGate::memory_leak_detected(1, "memory.leak"),
        SecurityDiagnosticGate::ffi_buffer_bounds(
            CBuildVariantViewError::KickEvidenceCountExceeded { count: 17, max: 16 },
            "ffi.view",
        ),
        SecurityDiagnosticGate::ffi_buffer_bounds(
            CBuildVariantViewError::MissingKickEvidencePointer { count: 1 },
            "ffi.view",
        ),
        SecurityDiagnosticGate::kick_evidence_buffer_exhausted("buildup.kick"),
        SecurityDiagnosticGate::gpu_missing_memory_ticket("gpu.result"),
        SecurityDiagnosticGate::gpu_fence_epoch_missing("gpu.result"),
        SecurityDiagnosticGate::gpu_unconfirmed_probability_source("gpu.reduce"),
        SecurityDiagnosticGate::render_runtime_svg_forbidden("render.asset"),
        SecurityDiagnosticGate::render_asset_provenance_missing("render.asset"),
        SecurityDiagnosticGate::gui_subprocess_forbidden("gui.host"),
        SecurityDiagnosticGate::frontend_typed_request_required("frontend.host"),
    ];

    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect::<Vec<_>>();

    for required in [
        DiagnosticCode::ECoreMemoryContextDoubleRelease,
        DiagnosticCode::ECoreMemoryScopeInvalid,
        DiagnosticCode::ECoreMemoryLeakDetected,
        DiagnosticCode::ECoreFfiBufferBounds,
        DiagnosticCode::ECoreInvalidNativeView,
        DiagnosticCode::EKickEvidenceBufferExhausted,
        DiagnosticCode::EGpuWorkerMissingMemoryTicket,
        DiagnosticCode::EGpuFenceEpochMissing,
        DiagnosticCode::EGpuUnconfirmedProbabilitySource,
        DiagnosticCode::ERenderRuntimeSvgForbidden,
        DiagnosticCode::ERenderAssetProvenanceMissing,
        DiagnosticCode::EGuiSubprocessForbidden,
        DiagnosticCode::EFrontendTypedRequestRequired,
    ] {
        assert!(codes.contains(&required), "missing {required:?}");
    }
}

#[test]
fn security_errors_are_not_downgraded_to_warnings() {
    let diagnostics = [
        SecurityDiagnosticGate::memory_context_double_release("memory.context"),
        SecurityDiagnosticGate::gpu_missing_memory_ticket("gpu.result"),
        SecurityDiagnosticGate::gpu_unconfirmed_probability_source("gpu.reduce"),
        SecurityDiagnosticGate::render_runtime_svg_forbidden("render.asset"),
        SecurityDiagnosticGate::gui_subprocess_forbidden("gui.host"),
        SecurityDiagnosticGate::frontend_typed_request_required("frontend.host"),
    ];

    for diagnostic in diagnostics {
        assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
    }
}

#[test]
fn gpu_unconfirmed_probability_source_includes_trust_state_evidence() {
    let diagnostic = SecurityDiagnosticGate::gpu_unconfirmed_probability_source("gpu.reduce");

    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::EGpuUnconfirmedProbabilitySource
    );
    assert!(diagnostic.evidence().iter().any(|evidence| {
        evidence.key() == "gpu_trust_state" && evidence.value() == "gpu-computed-unconfirmed"
    }));
    assert!(diagnostic
        .suggested_next_step()
        .expect("suggested next step")
        .text()
        .contains("CPU exact confirmation"));
}

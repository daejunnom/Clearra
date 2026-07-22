use clearra_core_ffi::{CBuildVariantViewError, CClrMemStatus};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SecurityDiagnosticGate;

impl SecurityDiagnosticGate {
    pub fn memory_context_double_release(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::ECoreMemoryContextDoubleRelease,
            "C memory context release was called after ownership had already been consumed",
        )
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new("c_status", "DoubleRelease"))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Treat the context handle as released and do not dereference the old C pointer.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn memory_scope_status(status: CClrMemStatus, location: &'static str) -> Diagnostic {
        let code = match status {
            CClrMemStatus::DoubleRelease => DiagnosticCode::ECoreMemoryContextDoubleRelease,
            CClrMemStatus::Ok => DiagnosticCode::ECoreMemoryScopeInvalid,
            _ => DiagnosticCode::ECoreMemoryScopeInvalid,
        };

        Diagnostic::new(code, "C memory scope returned a security-sensitive status")
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("c_status", format!("{status:?}")))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Abort the native path and preserve a diagnostic instead of continuing with stale memory views.",
            ))
    }
}
impl SecurityDiagnosticGate {
    pub fn memory_leak_detected(live_scopes: u64, location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::ECoreMemoryLeakDetected,
            "C memory leak report was not clean after execution",
        )
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new(
            "live_scopes",
            live_scopes.to_string(),
        ))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Release or abort all native scopes before accepting product output.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn ffi_buffer_bounds(error: CBuildVariantViewError, location: &'static str) -> Diagnostic {
        match error {
            CBuildVariantViewError::KickEvidenceCountExceeded { count, max } => Diagnostic::new(
                DiagnosticCode::ECoreFfiBufferBounds,
                "native pointer/count view exceeded the Rust FFI boundary",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("count", count.to_string()))
            .with_evidence(ValidationEvidence::new("max", max.to_string()))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Reject the native view before reading through the pointer.",
            )),
            CBuildVariantViewError::MissingKickEvidencePointer { count } => Diagnostic::new(
                DiagnosticCode::ECoreInvalidNativeView,
                "native pointer/count view had a nonzero count with a null pointer",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("count", count.to_string()))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Reject malformed native views instead of fabricating evidence.",
            )),
            CBuildVariantViewError::OperationOrderCountExceeded { count, max }
            | CBuildVariantViewError::TraceStepCountExceeded { count, max } => Diagnostic::new(
                DiagnosticCode::ECoreFfiBufferBounds,
                "native BuildVariant trace count exceeded the Rust FFI boundary",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("count", count.to_string()))
            .with_evidence(ValidationEvidence::new("max", max.to_string()))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Reject the native trace view before reading through either pointer.",
            )),
            CBuildVariantViewError::MissingOperationOrderPointer { count }
            | CBuildVariantViewError::MissingTraceStepsPointer { count } => Diagnostic::new(
                DiagnosticCode::ECoreInvalidNativeView,
                "native BuildVariant trace had a nonzero count with a null pointer",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new("count", count.to_string()))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Reject malformed native trace views before replay materialization.",
            )),
            CBuildVariantViewError::TraceCountMismatch {
                operation_count,
                trace_count,
            } => Diagnostic::new(
                DiagnosticCode::ECoreInvalidNativeView,
                "native BuildVariant operation-order and trace-step counts differed",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "operation_order_count",
                operation_count.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "trace_step_count",
                trace_count.to_string(),
            )),
            CBuildVariantViewError::OperationOrderMismatch { step_index, .. } => Diagnostic::new(
                DiagnosticCode::ECoreInvalidNativeView,
                "native BuildVariant operation order disagreed with its trace steps",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "trace_step_index",
                step_index.to_string(),
            )),
            CBuildVariantViewError::KickEvidenceIndexOutOfRange {
                step_index,
                evidence_index,
                evidence_count,
            } => Diagnostic::new(
                DiagnosticCode::ECoreInvalidNativeView,
                "native BuildVariant trace referenced kick evidence outside its buffer",
            )
            .with_location(EvidenceLocation::new(location))
            .with_evidence(ValidationEvidence::new(
                "trace_step_index",
                step_index.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "kick_evidence_index",
                evidence_index.to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "kick_evidence_count",
                evidence_count.to_string(),
            )),
        }
    }
}
impl SecurityDiagnosticGate {
    pub fn kick_evidence_buffer_exhausted(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::EKickEvidenceBufferExhausted,
            "kick evidence buffer was exhausted before exact evidence was complete",
        )
        .with_location(EvidenceLocation::new(location))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Do not report exact kick-sensitive spin evidence from a truncated buffer.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn gpu_missing_memory_ticket(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::EGpuWorkerMissingMemoryTicket,
            "GPU worker result did not carry a memory ticket",
        )
        .with_location(EvidenceLocation::new(location))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Reject the GPU result until it carries a memory ticket and scope epoch.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn gpu_fence_epoch_missing(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::EGpuFenceEpochMissing,
            "GPU buffer result did not carry a fence epoch",
        )
        .with_location(EvidenceLocation::new(location))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Do not release or trust GPU buffers without a completion fence epoch.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn gpu_unconfirmed_probability_source(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::EGpuUnconfirmedProbabilitySource,
            "unconfirmed GPU result attempted to source exact probability",
        )
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new(
            "gpu_trust_state",
            "gpu-computed-unconfirmed",
        ))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Route GPU candidates through CPU exact confirmation before coverage or probability reduction.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn render_runtime_svg_forbidden(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::ERenderRuntimeSvgForbidden,
            "runtime raw SVG rendering is forbidden",
        )
        .with_location(EvidenceLocation::new(location))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Import SVG assets through the sanitize/rasterize pipeline before runtime use.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn render_asset_provenance_missing(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::ERenderAssetProvenanceMissing,
            "render asset provenance is missing",
        )
        .with_location(EvidenceLocation::new(location))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Add provenance metadata before packaging the built-in skin asset.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn gui_subprocess_forbidden(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::EGuiSubprocessForbidden,
            "GUI attempted to use a CLI subprocess execution shortcut",
        )
        .with_location(EvidenceLocation::new(location))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Use typed clearra-app requests from the GUI host instead of parsing CLI text.",
        ))
    }
}
impl SecurityDiagnosticGate {
    pub fn frontend_typed_request_required(location: &'static str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::EFrontendTypedRequestRequired,
            "frontend request attempted to bypass the typed AppRequest boundary",
        )
        .with_location(EvidenceLocation::new(location))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Build a typed AppRequest and run validation before invoking product execution.",
        ))
    }
}

#[cfg(test)]
#[path = "security_diagnostic_gate_tests.rs"]
mod tests;

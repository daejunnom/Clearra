use clearra_validation::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

use super::*;

#[test]
fn renders_stable_validation_lines() {
    let mut report = DiagnosticReport::new();
    report.push(Diagnostic::new(
        DiagnosticCode::IBuildQueryMvpSupported,
        "build query is supported",
    ));

    assert_eq!(
        DiagnosticPrinter::render(&report),
        "info I_BUILD_QUERY_MVP_SUPPORTED build query is supported"
    );
}

#[test]
fn text_diagnostic_renders_evidence_and_suggested_next_step() {
    let mut report = DiagnosticReport::new();
    report.push(
        Diagnostic::new(
            DiagnosticCode::EGpuWorkerMissingMemoryTicket,
            "GPU worker result did not include a memory ticket",
        )
        .with_location(EvidenceLocation::new("backend.gpu_worker"))
        .with_evidence(ValidationEvidence::new("memory_ticket_id", "0"))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Reject the result and rerun with CPU confirmation evidence.",
        )),
    );

    let rendered = DiagnosticPrinter::render(&report);

    assert!(rendered.contains("error E_GPU_WORKER_MISSING_MEMORY_TICKET"));
    assert!(rendered.contains("location: backend.gpu_worker"));
    assert!(rendered.contains("evidence: memory_ticket_id=0"));
    assert!(rendered.contains("next: Reject the result and rerun with CPU confirmation evidence."));
}

#[test]
fn json_diagnostic_renders_structured_evidence_and_suggested_next_step() {
    let mut report = DiagnosticReport::new();
    report.push(
        Diagnostic::new(
            DiagnosticCode::EFrontendTypedRequestRequired,
            "frontend attempted to bypass typed AppRequest construction",
        )
        .with_evidence(ValidationEvidence::new("frontend_route", "gui-host"))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Build a typed AppRequest before execution.",
        )),
    );

    let rendered = DiagnosticPrinter::render_json(&report);

    assert!(rendered.contains("\"kind\":\"diagnostic\""));
    assert!(rendered.contains("\"diagnostic_count\":1"));
    assert!(rendered.contains("\"code\":\"E_FRONTEND_TYPED_REQUEST_REQUIRED\""));
    assert!(rendered.contains("\"frontend_route\":\"gui-host\""));
    assert!(
        rendered.contains("\"suggested_next_step\":\"Build a typed AppRequest before execution.\"")
    );
}

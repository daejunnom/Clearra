use crate::{
    diagnostic::{
        diagnostic_code::{DiagnosticCode, DiagnosticSeverity},
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    location: Option<EvidenceLocation>,
    evidence: Vec<ValidationEvidence>,
    suggested_next_step: Option<SuggestedNextStep>,
}

impl Diagnostic {
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            severity: code.default_severity(),
            code,
            message: message.into(),
            location: None,
            evidence: Vec::new(),
            suggested_next_step: None,
        }
    }
}
impl Diagnostic {
    pub fn with_location(mut self, location: EvidenceLocation) -> Self {
        self.location = Some(location);
        self
    }
}
impl Diagnostic {
    pub fn with_evidence(mut self, evidence: ValidationEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }
}
impl Diagnostic {
    pub fn with_suggested_next_step(mut self, suggested_next_step: SuggestedNextStep) -> Self {
        self.suggested_next_step = Some(suggested_next_step);
        self
    }
}
impl Diagnostic {
    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }
}
impl Diagnostic {
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }
}
impl Diagnostic {
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl Diagnostic {
    pub fn location(&self) -> Option<&EvidenceLocation> {
        self.location.as_ref()
    }
}
impl Diagnostic {
    pub fn evidence(&self) -> &[ValidationEvidence] {
        &self.evidence
    }
}
impl Diagnostic {
    pub fn suggested_next_step(&self) -> Option<&SuggestedNextStep> {
        self.suggested_next_step.as_ref()
    }
}

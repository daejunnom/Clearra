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

    /// Fallible counterpart for memory-authorized boundaries. The caller can
    /// authorize one additional inline evidence slot before this method and
    /// inspect [`Diagnostic::checked_retained_capacity_bytes`] immediately
    /// afterwards to re-authorize allocator overcapacity.
    pub fn try_with_evidence(
        mut self,
        evidence: ValidationEvidence,
    ) -> Result<Self, std::collections::TryReserveError> {
        self.evidence.try_reserve_exact(1)?;
        self.evidence.push(evidence);
        Ok(self)
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

    /// Returns only heap payload transitively retained by this diagnostic.
    ///
    /// String payloads and the outer evidence buffer are measured by
    /// allocation capacity, and nested location/evidence/suggestion payloads
    /// are included. The inline `Diagnostic` and inline evidence owners are
    /// excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.message.capacity() as u128;
        if let Some(location) = &self.location {
            bytes = bytes.checked_add(location.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_count_bytes(
            self.evidence.capacity() as u128,
            core::mem::size_of::<ValidationEvidence>() as u128,
        )?)?;
        for evidence in &self.evidence {
            bytes = bytes.checked_add(evidence.checked_retained_capacity_bytes()?)?;
        }
        if let Some(suggested_next_step) = &self.suggested_next_step {
            bytes = bytes.checked_add(suggested_next_step.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::{checked_count_bytes, Diagnostic};
    use crate::{
        diagnostic::{diagnostic_code::DiagnosticCode, suggested_next_step::SuggestedNextStep},
        evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    };

    fn allocated_text(capacity: usize, value: &str) -> String {
        let mut text = String::with_capacity(capacity);
        text.push_str(value);
        text
    }

    #[test]
    fn diagnostic_retained_capacity_counts_nested_allocations_fieldwise() {
        let message = allocated_text(128, "request rejected");
        let message_capacity = message.capacity() as u128;
        let path = allocated_text(64, "query.queue");
        let path_capacity = path.capacity() as u128;
        let key = allocated_text(48, "actual");
        let key_capacity = key.capacity() as u128;
        let value = allocated_text(96, "expected");
        let value_capacity = value.capacity() as u128;
        let suggestion = allocated_text(160, "reduce the queue expression");
        let suggestion_capacity = suggestion.capacity() as u128;
        let diagnostic = Diagnostic::new(DiagnosticCode::EPcQueryInvalid, message)
            .with_location(EvidenceLocation::new(path))
            .with_evidence(ValidationEvidence::new(key, value))
            .with_suggested_next_step(SuggestedNextStep::new(suggestion));
        let evidence_outer_bytes = (diagnostic.evidence.capacity() as u128)
            .checked_mul(core::mem::size_of::<ValidationEvidence>() as u128)
            .expect("evidence outer capacity fits u128");
        let expected = message_capacity
            .checked_add(path_capacity)
            .and_then(|bytes| bytes.checked_add(evidence_outer_bytes))
            .and_then(|bytes| bytes.checked_add(key_capacity))
            .and_then(|bytes| bytes.checked_add(value_capacity))
            .and_then(|bytes| bytes.checked_add(suggestion_capacity));

        assert_eq!(diagnostic.checked_retained_capacity_bytes(), expected);
    }

    #[test]
    fn diagnostic_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}

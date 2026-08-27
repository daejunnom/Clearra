use crate::diagnostic::{
    diagnostic::Diagnostic,
    diagnostic_code::{DiagnosticCode, DiagnosticSeverity},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticReport {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty report after fallibly reserving the requested number
    /// of diagnostic slots. Memory-authorized callers must inspect the actual
    /// retained capacity immediately because `try_reserve_exact` may return
    /// more capacity than requested.
    pub fn try_with_capacity(capacity: usize) -> Result<Self, std::collections::TryReserveError> {
        let mut diagnostics = Vec::new();
        diagnostics.try_reserve_exact(capacity)?;
        Ok(Self { diagnostics })
    }
}
impl DiagnosticReport {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}
impl DiagnosticReport {
    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }
}
impl DiagnosticReport {
    pub fn append(&mut self, mut other: DiagnosticReport) {
        self.diagnostics.append(&mut other.diagnostics);
    }

    /// Fallibly reserves additional destination slots without moving any
    /// source report. This lets a caller authorize the old+new+source peak,
    /// reserve, re-authorize actual destination capacity, and only then call
    /// [`DiagnosticReport::append`].
    pub fn try_reserve_exact(
        &mut self,
        additional: usize,
    ) -> Result<(), std::collections::TryReserveError> {
        self.diagnostics.try_reserve_exact(additional)
    }

    pub fn capacity(&self) -> usize {
        self.diagnostics.capacity()
    }
}
impl DiagnosticReport {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
impl DiagnosticReport {
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}
impl DiagnosticReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error)
    }
}
impl DiagnosticReport {
    pub fn contains_code(&self, code: DiagnosticCode) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == code)
    }

    /// Returns only heap payload transitively retained by the report.
    ///
    /// The outer diagnostic buffer is measured by allocation capacity and
    /// includes its inline `Diagnostic` slots. Every nested diagnostic heap
    /// payload is then included by capacity. The inline `DiagnosticReport` is
    /// excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = checked_count_bytes(
            self.diagnostics.capacity() as u128,
            core::mem::size_of::<Diagnostic>() as u128,
        )?;
        for diagnostic in &self.diagnostics {
            bytes = bytes.checked_add(diagnostic.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::{checked_count_bytes, DiagnosticReport};
    use crate::{
        diagnostic::{
            diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
            suggested_next_step::SuggestedNextStep,
        },
        evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    };

    #[test]
    fn report_retained_capacity_counts_outer_slots_and_nested_diagnostics() {
        let first = Diagnostic::new(DiagnosticCode::EPcQueryInvalid, "invalid queue")
            .with_location(EvidenceLocation::with_index("query.queue", 2))
            .with_evidence(ValidationEvidence::new("actual", "bag-aligned"))
            .with_suggested_next_step(SuggestedNextStep::new("use fixed or pattern"));
        let second = Diagnostic::new(DiagnosticCode::EGuiFormInvalid, "missing output policy");
        let mut report = DiagnosticReport::new();
        report.push(first);
        report.push(second);
        let outer_bytes = (report.diagnostics.capacity() as u128)
            .checked_mul(core::mem::size_of::<Diagnostic>() as u128)
            .expect("diagnostic outer capacity fits u128");
        let nested_bytes = report
            .diagnostics
            .iter()
            .try_fold(0_u128, |bytes, diagnostic| {
                bytes.checked_add(diagnostic.checked_retained_capacity_bytes()?)
            });
        let expected = nested_bytes.and_then(|bytes| bytes.checked_add(outer_bytes));

        assert_eq!(report.checked_retained_capacity_bytes(), expected);
    }

    #[test]
    fn report_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}

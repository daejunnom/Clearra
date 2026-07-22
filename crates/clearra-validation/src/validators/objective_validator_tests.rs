use clearra_coverage::{cover::CoverSelection, pattern::pattern_bitset::PatternBitSet};

use crate::diagnostic::diagnostic_code::DiagnosticSeverity;

use super::*;

#[test]
fn mvp_objectives_are_supported() {
    for kind in [
        ObjectiveKind::All,
        ObjectiveKind::Unique,
        ObjectiveKind::MinimumCover,
    ] {
        let report = validate_objective_kind(kind);

        assert!(!report.has_errors());
        assert!(report.contains_code(DiagnosticCode::IObjectiveMvpSupported));
    }
}

#[test]
fn greedy_fallback_cover_selection_becomes_warning_diagnostic() {
    let selection = CoverSelection::greedy_fallback(vec![0], PatternBitSet::new(1), true, 21, 20);

    let diagnostics = ObjectiveValidator::cover_selection_diagnostics(&selection);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].code(),
        DiagnosticCode::WMinimumCoverGreedyFallback
    );
    assert_eq!(diagnostics[0].severity(), DiagnosticSeverity::Warning);
    assert!(diagnostics[0].message().contains("greedy fallback"));
    assert!(diagnostics[0].message().contains("may not be minimal"));
}

#[test]
fn exact_cover_selection_has_no_fallback_diagnostic() {
    let selection = CoverSelection::exact_minimum(vec![0], PatternBitSet::new(1));

    let diagnostics = ObjectiveValidator::cover_selection_diagnostics(&selection);

    assert!(diagnostics.is_empty());
}

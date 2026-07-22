use clearra_setup_search::query::PieceBudget;

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    piece_set_diagnostic_builder::oversized_piece_budget_diagnostic,
    piece_set_standard_validator::validate_pieces_impl,
};

pub(super) fn validate_budget_impl(budget: &PieceBudget) -> DiagnosticReport {
    let mut report = validate_pieces_impl(budget.allowed_pieces(), "setup.piece_budget");
    if budget.max_piece_count() > 7 {
        report.push(oversized_piece_budget_diagnostic(
            budget.max_piece_count().into(),
        ));
    }
    report
}

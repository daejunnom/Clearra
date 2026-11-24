use clearra_setup_search::query::PieceBudget;

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::piece_set_standard_validator::validate_pieces_impl;

pub(super) fn validate_budget_impl(budget: &PieceBudget) -> DiagnosticReport {
    // The set describes the allowed tetromino kinds, not a one-bag queue
    // horizon. The query's board and supply model validate their real limits.
    validate_pieces_impl(budget.allowed_pieces(), "setup.piece_budget")
}

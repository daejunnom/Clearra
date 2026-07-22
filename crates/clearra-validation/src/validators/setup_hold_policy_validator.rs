use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_setup_search::query::SetupHoldPolicy;

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::validation_evidence::ValidationEvidence,
};

use super::setup_query_diagnostic_builder::invalid_setup_query;

pub(super) fn validate_hold_policy(hold_policy: SetupHoldPolicy, report: &mut DiagnosticReport) {
    match hold_policy {
        SetupHoldPolicy::Disabled => report.push(invalid_setup_query(
            "setup.hold_policy",
            "setup search MVP1 requires hold-aware evaluation",
            "hold_disabled",
        )),
        SetupHoldPolicy::EnabledEmpty => {}
        SetupHoldPolicy::EnabledWithPiece(piece) => {
            if !PieceKind::STANDARD_TETROMINOES.contains(&piece) {
                report.push(
                    invalid_setup_query(
                        "setup.hold_policy",
                        "initial hold piece must be a standard tetromino in MVP1",
                        "unsupported_hold_piece",
                    )
                    .with_evidence(ValidationEvidence::new(
                        "piece",
                        piece.as_ascii().to_string(),
                    )),
                );
            }
        }
    }
}

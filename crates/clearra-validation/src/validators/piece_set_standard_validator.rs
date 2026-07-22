use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::piece_set_diagnostic_builder::{
    standard_piece_set_supported_diagnostic, standard_piece_set_unsupported_diagnostic,
};

pub(super) fn validate_pieces_impl(
    pieces: &[PieceKind],
    location: &'static str,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    let duplicate = pieces
        .iter()
        .enumerate()
        .find_map(|(index, piece)| pieces[..index].contains(piece).then_some(*piece));
    let missing = PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .find(|piece| !pieces.contains(piece));

    if pieces.len() == PieceKind::STANDARD_TETROMINOES.len()
        && duplicate.is_none()
        && missing.is_none()
    {
        report.push(standard_piece_set_supported_diagnostic(
            location,
            pieces.len(),
        ));
        return report;
    }

    let reason = if pieces.is_empty() {
        "empty_piece_set".to_owned()
    } else if let Some(piece) = duplicate {
        format!("duplicate_piece_{}", piece.as_ascii())
    } else if let Some(piece) = missing {
        format!("missing_piece_{}", piece.as_ascii())
    } else {
        "non_standard_piece_count".to_owned()
    };

    report.push(standard_piece_set_unsupported_diagnostic(
        location, pieces, reason,
    ));
    report
}

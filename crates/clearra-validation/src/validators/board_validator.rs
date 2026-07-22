use clearra_core_domain::board::board_size::BoardSize;
use clearra_geometry::board::{board_backend_capability_for_size, BoardRuntimeUnsupportedReason};
use clearra_geometry::layout::board_backend::{backend_kind_for_size, BoardBackendKind};
use clearra_profiles::board::{board_profile::BoardProfile, standard10::STANDARD_10_WIDTH};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

const BOARD_WIDTH_OUT_OF_SCOPE_REASON: &str = "board_width_out_of_scope";
const BOARD_BACKEND_NOT_CONNECTED_REASON: &str = "board_backend_not_connected";
const WIDE_BOARD_RUNTIME_NOT_CONNECTED_REASON: &str = "wide_board_runtime_not_connected";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoardValidator;

impl BoardValidator {
    pub fn validate_size(size: BoardSize) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        if size.width() == STANDARD_10_WIDTH {
            report.push(
                Diagnostic::new(
                    DiagnosticCode::IBoardMvpSupported,
                    "board width is supported by MVP1 standard 10-wide search",
                )
                .with_location(EvidenceLocation::new("board"))
                .with_evidence(ValidationEvidence::new("width", size.width().to_string()))
                .with_evidence(ValidationEvidence::new("height", size.height().to_string())),
            );
        } else {
            report.push(
                Diagnostic::new(
                    DiagnosticCode::EBoardUnsupportedMvp,
                    "only standard 10-wide boards are supported in MVP1",
                )
                .with_location(EvidenceLocation::new("board"))
                .with_evidence(ValidationEvidence::new("width", size.width().to_string()))
                .with_evidence(ValidationEvidence::new(
                    "reason",
                    BOARD_WIDTH_OUT_OF_SCOPE_REASON,
                ))
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Use the standard 10-wide board profile.",
                )),
            );
            report.push(
                Diagnostic::new(
                    DiagnosticCode::EBoardWidthOutOfScope,
                    "board_width_out_of_scope: this board width is not connected to MVP1 search",
                )
                .with_location(EvidenceLocation::new("board.width"))
                .with_evidence(ValidationEvidence::new("width", size.width().to_string()))
                .with_evidence(ValidationEvidence::new(
                    "reason",
                    BOARD_WIDTH_OUT_OF_SCOPE_REASON,
                )),
            );
        }
        report
    }
}
impl BoardValidator {
    pub fn validate_profile(profile: BoardProfile) -> DiagnosticReport {
        if profile.is_standard_10() {
            Self::validate_size(profile.size())
        } else {
            let mut report = DiagnosticReport::new();
            report.push(
                Diagnostic::new(
                    DiagnosticCode::EBoardUnsupportedMvp,
                    "only the standard 10 board profile is supported in MVP1",
                )
                .with_location(EvidenceLocation::new("board.profile"))
                .with_evidence(ValidationEvidence::new(
                    "profile",
                    format!("{:?}", profile.id()),
                )),
            );
            report
        }
    }
}
impl BoardValidator {
    pub fn validate_backend_mvp3_guard(size: BoardSize) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        let backend_kind = backend_kind_for_size(size);
        let capability = board_backend_capability_for_size(size);
        if backend_kind == BoardBackendKind::Board64 && size.width() == STANDARD_10_WIDTH {
            report.push(
                Diagnostic::new(
                    DiagnosticCode::IBoardMvpSupported,
                    "Board64 fast path is supported by the current MVP search runtime",
                )
                .with_location(EvidenceLocation::new("board.backend"))
                .with_evidence(ValidationEvidence::new("backend", "board64"))
                .with_evidence(ValidationEvidence::new("area", size.area().to_string())),
            );
            return report;
        }

        let unsupported_reason = if backend_kind == BoardBackendKind::Board64 {
            BoardRuntimeUnsupportedReason::BoardWidthOutOfScope
        } else {
            capability.unsupported_reason()
        };
        report.push(
            Diagnostic::new(
                DiagnosticCode::ECustomBoardUnsupportedMvp,
                "Board128, Board256, and Wide board descriptors are unsupported by the search runtime",
            )
            .with_location(EvidenceLocation::new("board.backend"))
            .with_evidence(ValidationEvidence::new(
                "backend",
                backend_kind_name(backend_kind),
            ))
            .with_evidence(ValidationEvidence::new("width", size.width().to_string()))
            .with_evidence(ValidationEvidence::new("height", size.height().to_string()))
            .with_evidence(ValidationEvidence::new("area", size.area().to_string()))
            .with_evidence(ValidationEvidence::new(
                "descriptor_supported",
                capability.descriptor_supported().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "basic_ops_supported",
                capability.basic_ops_supported().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "operation_mask_supported",
                capability.operation_mask_supported().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "packing_supported",
                capability.packing_supported().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "reason",
                unsupported_reason_marker(unsupported_reason),
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Keep custom board sizes behind validation until PC/search placement tables are generated per board backend.",
            )),
        );
        report.push(
            Diagnostic::new(
                diagnostic_code_for_reason(unsupported_reason),
                format!(
                    "{}: {} cannot execute through the current packing runtime",
                    unsupported_reason_marker(unsupported_reason),
                    backend_kind_name(backend_kind)
                ),
            )
            .with_location(EvidenceLocation::new("board.backend"))
            .with_evidence(ValidationEvidence::new(
                "backend",
                backend_kind_name(backend_kind),
            ))
            .with_evidence(ValidationEvidence::new(
                "reason",
                unsupported_reason_marker(unsupported_reason),
            )),
        );
        report
    }
}

pub fn validate_board_size(size: BoardSize) -> DiagnosticReport {
    BoardValidator::validate_size(size)
}

pub fn validate_board_profile(profile: BoardProfile) -> DiagnosticReport {
    BoardValidator::validate_profile(profile)
}

pub fn validate_board_backend_mvp3_guard(size: BoardSize) -> DiagnosticReport {
    BoardValidator::validate_backend_mvp3_guard(size)
}

fn backend_kind_name(kind: BoardBackendKind) -> &'static str {
    match kind {
        BoardBackendKind::Board64 => "board64",
        BoardBackendKind::Board128 => "board128",
        BoardBackendKind::Board256 => "board256",
        BoardBackendKind::Wide => "wide",
    }
}

fn diagnostic_code_for_reason(reason: BoardRuntimeUnsupportedReason) -> DiagnosticCode {
    match reason {
        BoardRuntimeUnsupportedReason::None => DiagnosticCode::IBoardMvpSupported,
        BoardRuntimeUnsupportedReason::BoardWidthOutOfScope => {
            DiagnosticCode::EBoardWidthOutOfScope
        }
        BoardRuntimeUnsupportedReason::BoardBackendNotConnected => {
            DiagnosticCode::EBoardBackendNotConnected
        }
        BoardRuntimeUnsupportedReason::WideBoardRuntimeNotConnected => {
            DiagnosticCode::EWideBoardRuntimeNotConnected
        }
    }
}

fn unsupported_reason_marker(reason: BoardRuntimeUnsupportedReason) -> &'static str {
    match reason {
        BoardRuntimeUnsupportedReason::None => "none",
        BoardRuntimeUnsupportedReason::BoardWidthOutOfScope => BOARD_WIDTH_OUT_OF_SCOPE_REASON,
        BoardRuntimeUnsupportedReason::BoardBackendNotConnected => {
            BOARD_BACKEND_NOT_CONNECTED_REASON
        }
        BoardRuntimeUnsupportedReason::WideBoardRuntimeNotConnected => {
            WIDE_BOARD_RUNTIME_NOT_CONNECTED_REASON
        }
    }
}

#[cfg(test)]
#[path = "board_validator_tests.rs"]
mod tests;

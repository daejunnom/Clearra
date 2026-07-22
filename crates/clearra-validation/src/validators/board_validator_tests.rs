use clearra_profiles::board::standard10::standard_10_board_profile;

use crate::diagnostic::diagnostic_code::{DiagnosticCode, DiagnosticSeverity};

use super::*;

#[test]
fn standard_10_board_is_supported() {
    let report = validate_board_profile(standard_10_board_profile());

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IBoardMvpSupported));
}

#[test]
fn non_standard_width_is_rejected() {
    let size = BoardSize::new(12, 20).expect("board");
    let report = validate_board_size(size);

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::EBoardUnsupportedMvp
            && diagnostic.severity() == DiagnosticSeverity::Error
    }));
}

#[test]
fn board128_backend_is_guarded_until_search_runtime_is_generic() {
    let size = BoardSize::new(10, 12).expect("board128");
    let report = validate_board_backend_mvp3_guard(size);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ECustomBoardUnsupportedMvp));
    assert!(report.contains_code(DiagnosticCode::EBoardBackendNotConnected));
}

#[test]
fn wide_backend_is_guarded_until_search_runtime_is_generic() {
    let size = BoardSize::new(16, 20).expect("wide");
    let report = validate_board_backend_mvp3_guard(size);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ECustomBoardUnsupportedMvp));
    assert!(report.contains_code(DiagnosticCode::EWideBoardRuntimeNotConnected));
}

#[test]
fn unsupported_board_width_reports_reason_without_silent_fallback() {
    let size = BoardSize::new(12, 4).expect("board64 custom width");
    let report = validate_board_size(size);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::EBoardWidthOutOfScope));
}

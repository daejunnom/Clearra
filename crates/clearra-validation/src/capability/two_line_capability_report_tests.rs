use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_profiles::{
    board::standard10::standard_10_board_profile,
    pieces::standard_tetrominoes::standard_tetromino_piece_set_profile,
};
use clearra_rules::profile::builtin_rules::srs;
use clearra_two_line::capability::two_line_capability::{
    TwoLineCapability, TwoLineCapabilityInput,
};

use crate::diagnostic::{
    diagnostic_code::{DiagnosticCode, DiagnosticSeverity},
    diagnostic_report::DiagnosticReport,
};

use super::*;

#[test]
fn enabled_fast_path_becomes_info() {
    let capability = TwoLineCapability::evaluate(TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::two_lines(),
        srs(),
        true,
        true,
    ));

    let report = two_line_capability_report(capability);

    assert_report_contains(
        &report,
        DiagnosticCode::IFastPathTwoLineEnabled,
        DiagnosticSeverity::Info,
    );
}

#[test]
fn disabled_fast_path_becomes_warning() {
    let capability = TwoLineCapability::evaluate(TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::six_lines(),
        srs(),
        true,
        true,
    ));

    let report = two_line_capability_report(capability);

    assert_report_contains(
        &report,
        DiagnosticCode::WFastPathTwoLineDisabled,
        DiagnosticSeverity::Warning,
    );
    assert!(!report.has_errors());
}

fn assert_report_contains(
    report: &DiagnosticReport,
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
) {
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code() == code && diagnostic.severity() == severity));
}

use clearra_app::{
    encode_ctk3_compact, Ctk3Document, Ctk3Operation, Ctk3Page, Ctk3PageFlags, Ctk3Piece,
    Ctk3Rotation,
};

use super::*;
use crate::exit::ExitCode;

#[test]
fn cli_sequence_dependencies_executes_the_exact_document_contract() {
    let mut page = Ctk3Page::new(0, Vec::new());
    page.flags = Ctk3PageFlags::default();
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::O,
        rotation: Ctk3Rotation::Spawn,
        x: 0,
        y: 0,
    });
    let document =
        encode_ctk3_compact(&Ctk3Document::new(10, vec![page])).expect("one-operation CTK3");
    let output = run_with_args([
        "clearra",
        "utility",
        "sequence-dependencies",
        "--document",
        document.as_str(),
        "--rule-profile",
        "srs-plus",
        "--kick-profile",
        "srs-plus",
        "--timeout-seconds",
        "900",
    ]);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output
        .stdout()
        .contains("contract_id: operation-dependency-report.v1"));
    assert!(output.stdout().contains("exact_order_count: 1"));
}

#[test]
fn cli_sequence_losslessly_normalizes_and_replays_the_document_trace() {
    let mut page = Ctk3Page::new(0, Vec::new());
    page.flags = Ctk3PageFlags::default();
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::O,
        rotation: Ctk3Rotation::Spawn,
        x: 0,
        y: 0,
    });
    let document =
        encode_ctk3_compact(&Ctk3Document::new(10, vec![page])).expect("one-operation CTK3");
    let output = run_with_args([
        "clearra",
        "utility",
        "sequence",
        "--document",
        document.as_str(),
        "--rule-profile",
        "srs-plus",
        "--kick-profile",
        "srs-plus",
        "--timeout-seconds",
        "900",
    ]);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output
        .stdout()
        .contains("contract_id: operation-sequence.v1"));
    assert!(output.stdout().contains("normalized_trace: 0:O:0:0:0"));
    assert!(output.stdout().contains("operation_count: 1"));

    let tie_output = run_with_args([
        "clearra",
        "utility",
        "sequence",
        "--document",
        document.as_str(),
        "--ties",
    ]);
    assert_eq!(tie_output.exit_code(), ExitCode::ValidationFailed);
}

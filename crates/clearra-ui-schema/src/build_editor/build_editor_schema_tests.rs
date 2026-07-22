use super::*;

#[test]
fn build_slots_use_canonical_standard_piece_profile() {
    let schema = BuildEditorSchema::mvp_template_slots(1);
    let expected = standard_tetromino_piece_set_profile()
        .pieces()
        .iter()
        .map(|piece| piece.as_ascii())
        .collect::<Vec<_>>();

    assert_eq!(schema.slots()[0].allowed_pieces(), expected.as_slice());
}

#[test]
fn build_editor_schema_exposes_template_and_slot_field_schema() {
    let schema = BuildEditorSchema::mvp_template_slots(1);

    assert!(schema.custom_domains_enabled());
    assert_eq!(schema.template_id(), "mvp-build-template");
    assert_eq!(schema.board_width(), 10);
    assert_eq!(schema.board_height(), 20);
    assert!(schema
        .fields()
        .iter()
        .any(|field| field.id() == "template_id"));
    assert!(schema.slots()[0]
        .fields()
        .iter()
        .any(|field| field.id() == "allowed_pieces"));
    assert!(schema.slots()[0]
        .fields()
        .iter()
        .any(|field| field.id() == "cells"));
    assert_eq!(schema.preview_board().width(), 10);
    assert_eq!(schema.preview_board().occupied_cells().len(), 1);
    assert_eq!(schema.coverage_summary().probability(), 0.0);
    assert!(schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "packing_candidate_count"));
    assert!(schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "build_variant_count"));
}

#[test]
fn build_editor_schema_accepts_validation_diagnostics_and_coverage_summary() {
    use clearra_validation::{
        diagnostic::{diagnostic::Diagnostic, diagnostic_code::DiagnosticCode},
        validators::build_query_validator::validate_build_coverage_query,
    };

    let report = {
        let mut report = DiagnosticReport::new();
        report.push(Diagnostic::new(
            DiagnosticCode::EBuildQueryInvalid,
            "template diagnostic",
        ));
        report
    };
    let schema = BuildEditorSchema::mvp_template_slots(0).with_validation_report(&report);

    assert_eq!(schema.validation_diagnostics().len(), 1);
    assert_eq!(
        schema.validation_diagnostics()[0].code(),
        DiagnosticCode::EBuildQueryInvalid.as_str()
    );

    let _validator_marker = validate_build_coverage_query;
}

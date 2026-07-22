use clearra_core_domain::{board::cell::CellCoord, piece::piece_kind::PieceKind};

use super::*;
use crate::template::{
    build_slot::{
        BuildSlot, BuildSlotId, SlotCanonicalization, SlotHoldConstraint, SlotOrderConstraint,
        SlotSymmetry,
    },
    build_template::{TemplateCanonicalization, TemplateSymmetry},
    NATIVE_TEMPLATE_SCHEMA_VERSION,
};

#[test]
fn typed_template_import_export_contract_carries_interpreted_template_only() {
    let template = BuildTemplate::new(
        "typed-template",
        vec![BuildSlot::new(
            BuildSlotId::new(1),
            vec![CellCoord::new_unchecked(0, 0)],
        )],
    );
    let import = TemplateImport::new("convert-adapter", TemplateImportFormat::Adapter, template);

    assert_eq!(import.source_name(), "convert-adapter");
    assert_eq!(import.format(), TemplateImportFormat::Adapter);
    assert!(!import.format().accepts_raw_text());

    let export = TemplateExport::new(
        "editor-save",
        TemplateExportFormat::Json,
        import.into_template(),
    );

    assert_eq!(export.target_name(), "editor-save");
    assert_eq!(export.template().id(), "typed-template");
}

#[test]
fn native_json_template_import_export_roundtrips_editor_contract() {
    let template = BuildTemplate::new(
        "native-json-template",
        vec![
            BuildSlot::new(
                BuildSlotId::new(1),
                vec![
                    CellCoord::new_unchecked(0, 0),
                    CellCoord::new_unchecked(1, 0),
                ],
            )
            .with_label("left slot")
            .with_allowed_pieces(vec![PieceKind::I, PieceKind::O])
            .with_required_piece(PieceKind::I)
            .with_hold_constraint(SlotHoldConstraint::RequiresHold)
            .with_order_constraint(SlotOrderConstraint::Before(BuildSlotId::new(2)))
            .with_symmetry(SlotSymmetry::MirrorX)
            .with_canonicalization(SlotCanonicalization::CanonicalBySymmetry),
            BuildSlot::new(BuildSlotId::new(2), vec![CellCoord::new_unchecked(2, 0)])
                .with_allowed_pieces(vec![PieceKind::T]),
        ],
    )
    .with_label("editor template")
    .with_board_size(clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"))
    .with_symmetry(TemplateSymmetry::MirrorX)
    .with_canonicalization(TemplateCanonicalization::CanonicalByGeometry);

    let export = TemplateExport::new("editor-save", TemplateExportFormat::Json, template.clone());
    let json = export.to_json().expect("json export");
    let import = TemplateImport::from_json("cli", &json).expect("json import");

    assert_eq!(import.format(), TemplateImportFormat::Json);
    assert_eq!(import.template(), &template);
    assert!(json.contains(&format!(
        "\"schema_version\": {NATIVE_TEMPLATE_SCHEMA_VERSION}"
    )));
    assert!(json.contains("\"order_constraint\""));
}

#[test]
fn native_json_template_import_works() {
    let json = r#"{
        "schema_version": 2,
        "id": "native-json-template",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 0, "y": 0 }],
            "allowed_pieces": ["I", "O"]
        }]
    }"#;

    let import = TemplateImport::from_json("fixture", json).expect("native JSON import");

    assert_eq!(import.format(), TemplateImportFormat::Json);
    assert_eq!(import.template().id(), "native-json-template");
    assert_eq!(import.template().slots().len(), 1);
}

#[test]
fn native_json_template_import_rejects_raw_external_text() {
    let error = TemplateImport::from_json("external", "not-json").expect_err("invalid json");

    assert_eq!(error, TemplateJsonError::InvalidJson);
}

#[test]
fn native_json_template_import_rejects_unknown_fields() {
    let json = r#"{
        "schema_version": 2,
        "id": "bad",
        "board": { "width": 10, "height": 4 },
        "slots": [],
        "raw_external": "not-json"
    }"#;

    let error = TemplateImport::from_json("bad", json).expect_err("unknown field");

    assert_eq!(
        error,
        TemplateJsonError::UnknownField {
            context: "template",
            field: "raw_external".to_owned()
        }
    );
}

#[test]
fn native_json_template_import_rejects_out_of_bounds_cell() {
    let json = r#"{
        "schema_version": 2,
        "id": "bad-cell",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 10, "y": 0 }],
            "allowed_pieces": ["I"]
        }]
    }"#;

    let error = TemplateImport::from_json("bad-cell", json).expect_err("out of bounds");

    assert!(matches!(
        error,
        TemplateJsonError::InvalidField {
            context: "template.slots[].cells[]",
            field: "x/y",
            ..
        }
    ));
}

#[test]
fn native_json_template_import_rejects_duplicate_slot_cells() {
    let json = r#"{
        "schema_version": 2,
        "id": "duplicate-cell",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 0, "y": 0 }, { "x": 0, "y": 0 }],
            "allowed_pieces": ["I"]
        }]
    }"#;

    let error = TemplateImport::from_json("duplicate-cell", json).expect_err("duplicate cell");

    assert_eq!(
        error,
        TemplateJsonError::InvalidField {
            context: "template.slots[]",
            field: "cells",
            reason: "duplicate cell at slot index 0 and cell index 1".to_owned()
        }
    );
}

#[test]
fn native_json_template_import_rejects_duplicate_allowed_pieces() {
    let json = r#"{
        "schema_version": 2,
        "id": "duplicate-allowed-piece",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 0, "y": 0 }],
            "allowed_pieces": ["I", "O", "I"]
        }]
    }"#;

    let error = TemplateImport::from_json("duplicate-allowed-piece", json).expect_err("duplicate");

    assert_eq!(
        error,
        TemplateJsonError::InvalidField {
            context: "template.slots[]",
            field: "allowed_pieces",
            reason: "duplicate allowed piece I at slot index 0 and allowed_pieces index 2"
                .to_owned()
        }
    );
}

#[test]
fn native_json_template_import_rejects_required_piece_outside_allowed_pieces() {
    let json = r#"{
        "schema_version": 2,
        "id": "bad-required-piece",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 0, "y": 0 }],
            "allowed_pieces": ["I"],
            "required_piece": "O"
        }]
    }"#;

    let error = TemplateImport::from_json("bad-required-piece", json).expect_err("required piece");

    assert!(matches!(
        error,
        TemplateJsonError::InvalidField {
            context: "template.slots[]",
            field: "required_piece",
            ..
        }
    ));
}

use serde_json::{Map, Value};

use super::{
    template_json_error::TemplateJsonError,
    template_json_fields::{reject_unknown_fields, required_u64},
};

pub const NATIVE_TEMPLATE_SCHEMA_VERSION: u64 = 2;

pub(crate) fn validate_template_schema(
    object: &Map<String, Value>,
) -> Result<(), TemplateJsonError> {
    reject_unknown_fields(
        object,
        &[
            "schema_version",
            "id",
            "label",
            "board",
            "symmetry",
            "canonicalization",
            "slots",
        ],
        "template",
    )?;

    let schema_version = required_u64(object, "template", "schema_version")?;
    if schema_version != NATIVE_TEMPLATE_SCHEMA_VERSION {
        return Err(TemplateJsonError::UnsupportedSchemaVersion {
            version: schema_version,
        });
    }

    Ok(())
}

pub(crate) fn validate_board_fields(object: &Map<String, Value>) -> Result<(), TemplateJsonError> {
    reject_unknown_fields(object, &["width", "height"], "template.board")
}

pub(crate) fn validate_slot_fields(object: &Map<String, Value>) -> Result<(), TemplateJsonError> {
    reject_unknown_fields(
        object,
        &[
            "id",
            "label",
            "cells",
            "allowed_pieces",
            "required_piece",
            "hold_constraint",
            "order_constraint",
            "symmetry",
            "canonicalization",
        ],
        "template.slots[]",
    )
}

pub(crate) fn validate_order_constraint_fields(
    object: &Map<String, Value>,
) -> Result<(), TemplateJsonError> {
    reject_unknown_fields(
        object,
        &["kind", "slot_id"],
        "template.slots[].order_constraint",
    )
}

pub(crate) fn validate_cell_fields(object: &Map<String, Value>) -> Result<(), TemplateJsonError> {
    reject_unknown_fields(object, &["x", "y"], "template.slots[].cells[]")
}

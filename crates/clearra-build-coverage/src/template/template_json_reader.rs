use std::collections::BTreeSet;

use clearra_core_domain::{
    board::{board_size::BoardSize, cell::CellCoord},
    piece::piece_kind::PieceKind,
};
use serde_json::Value;

use super::{
    build_slot::{BuildSlot, BuildSlotId},
    build_template::BuildTemplate,
    template_json_enums::{
        optional_piece, parse_piece_array, parse_slot_canonicalization, parse_slot_hold_constraint,
        parse_slot_order_constraint, parse_slot_symmetry, parse_template_canonicalization,
        parse_template_symmetry,
    },
    template_json_error::TemplateJsonError,
    template_json_fields::{
        invalid_field, object, optional_field, optional_string, required_array, required_field,
        required_string, required_u16, required_u32,
    },
    template_json_schema::{
        validate_board_fields, validate_cell_fields, validate_slot_fields, validate_template_schema,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TemplateJsonReader;

impl TemplateJsonReader {
    pub(crate) fn from_value(value: &Value) -> Result<BuildTemplate, TemplateJsonError> {
        parse_template(value)
    }
}

fn parse_template(value: &Value) -> Result<BuildTemplate, TemplateJsonError> {
    let object = object(value, "template")?;
    validate_template_schema(object)?;

    let board_size = parse_board(required_field(object, "template", "board")?)?;
    let slots = required_array(object, "template", "slots")?
        .iter()
        .enumerate()
        .map(|(index, slot)| parse_slot(slot, board_size, index))
        .collect::<Result<Vec<_>, _>>()?;

    let mut template = BuildTemplate::new(required_string(object, "template", "id")?, slots)
        .with_board_size(board_size)
        .with_symmetry(parse_template_symmetry(optional_string(
            object, "template", "symmetry",
        )?)?)
        .with_canonicalization(parse_template_canonicalization(optional_string(
            object,
            "template",
            "canonicalization",
        )?)?);

    if let Some(label) = optional_string(object, "template", "label")? {
        template = template.with_label(label);
    }

    Ok(template)
}

fn parse_board(value: &Value) -> Result<BoardSize, TemplateJsonError> {
    let object = object(value, "template.board")?;
    validate_board_fields(object)?;

    let width = required_u16(object, "template.board", "width")?;
    let height = required_u16(object, "template.board", "height")?;
    BoardSize::new(width, height).map_err(|error| TemplateJsonError::InvalidField {
        context: "template.board",
        field: "width/height",
        reason: format!("{error:?}"),
    })
}

fn parse_slot(
    value: &Value,
    board_size: BoardSize,
    slot_index: usize,
) -> Result<BuildSlot, TemplateJsonError> {
    let context = "template.slots[]";
    let object = object(value, context)?;
    validate_slot_fields(object)?;

    let slot_id = BuildSlotId::new(required_u32(object, context, "id")?);
    let cells = required_array(object, context, "cells")?
        .iter()
        .enumerate()
        .map(|(cell_index, cell)| parse_cell(cell, board_size, slot_index, cell_index))
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_cells(&cells, slot_index)?;

    let allowed_pieces = parse_piece_array(object, context, "allowed_pieces")?;
    validate_unique_allowed_pieces(&allowed_pieces, slot_index)?;
    let required_piece = optional_piece(object, context, "required_piece")?;
    validate_required_piece_allowed(required_piece, &allowed_pieces, slot_index)?;

    let mut slot = BuildSlot::new(slot_id, cells)
        .with_allowed_pieces(allowed_pieces)
        .with_hold_constraint(parse_slot_hold_constraint(optional_string(
            object,
            context,
            "hold_constraint",
        )?)?)
        .with_order_constraint(parse_slot_order_constraint(optional_field(
            object,
            "order_constraint",
        ))?)
        .with_symmetry(parse_slot_symmetry(optional_string(
            object, context, "symmetry",
        )?)?)
        .with_canonicalization(parse_slot_canonicalization(optional_string(
            object,
            context,
            "canonicalization",
        )?)?);

    if let Some(label) = optional_string(object, context, "label")? {
        slot = slot.with_label(label);
    }
    if let Some(piece) = required_piece {
        slot = slot.with_required_piece(piece);
    }

    Ok(slot)
}

fn parse_cell(
    value: &Value,
    board_size: BoardSize,
    slot_index: usize,
    cell_index: usize,
) -> Result<CellCoord, TemplateJsonError> {
    let context = "template.slots[].cells[]";
    let object = object(value, context)?;
    validate_cell_fields(object)?;

    let x = required_u16(object, context, "x")?;
    let y = required_u16(object, context, "y")?;
    CellCoord::new(x, y, board_size).map_err(|error| {
        invalid_field(
            context,
            "x/y",
            format!(
                "cell at slot index {slot_index} and cell index {cell_index} is outside board: {error:?}"
            ),
        )
    })
}

fn validate_unique_cells(cells: &[CellCoord], slot_index: usize) -> Result<(), TemplateJsonError> {
    let mut seen = BTreeSet::new();
    for (cell_index, cell) in cells.iter().copied().enumerate() {
        if !seen.insert(cell) {
            return Err(invalid_field(
                "template.slots[]",
                "cells",
                format!("duplicate cell at slot index {slot_index} and cell index {cell_index}"),
            ));
        }
    }
    Ok(())
}

fn validate_unique_allowed_pieces(
    allowed_pieces: &[PieceKind],
    slot_index: usize,
) -> Result<(), TemplateJsonError> {
    let mut seen = BTreeSet::new();
    for (piece_index, piece) in allowed_pieces.iter().copied().enumerate() {
        if !seen.insert(piece) {
            return Err(invalid_field(
                "template.slots[]",
                "allowed_pieces",
                format!(
                    "duplicate allowed piece {} at slot index {slot_index} and allowed_pieces index {piece_index}",
                    piece.as_ascii()
                ),
            ));
        }
    }
    Ok(())
}

fn validate_required_piece_allowed(
    required_piece: Option<PieceKind>,
    allowed_pieces: &[PieceKind],
    slot_index: usize,
) -> Result<(), TemplateJsonError> {
    let Some(required_piece) = required_piece else {
        return Ok(());
    };
    if allowed_pieces.contains(&required_piece) {
        return Ok(());
    }

    Err(invalid_field(
        "template.slots[]",
        "required_piece",
        format!(
            "required piece {} is not in allowed_pieces for slot index {slot_index}",
            required_piece.as_ascii()
        ),
    ))
}

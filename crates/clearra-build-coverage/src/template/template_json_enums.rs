use clearra_core_domain::piece::piece_kind::PieceKind;
use serde_json::{Map, Value};

use super::{
    build_slot::{
        BuildSlotId, SlotCanonicalization, SlotHoldConstraint, SlotOrderConstraint, SlotSymmetry,
    },
    build_template::{TemplateCanonicalization, TemplateSymmetry},
    template_json_error::TemplateJsonError,
    template_json_fields::{
        invalid_field, object, optional_field, optional_string, required_array, required_string,
    },
    template_json_schema::validate_order_constraint_fields,
};

pub(crate) fn parse_piece_array(
    object: &Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<Vec<PieceKind>, TemplateJsonError> {
    required_array(object, context, field)?
        .iter()
        .map(|value| {
            let Some(piece) = value.as_str() else {
                return Err(invalid_field(context, field, "expected piece string"));
            };
            parse_piece_string(context, field, piece)
        })
        .collect()
}

pub(crate) fn optional_piece(
    object: &Map<String, Value>,
    context: &'static str,
    field: &'static str,
) -> Result<Option<PieceKind>, TemplateJsonError> {
    match optional_string(object, context, field)? {
        Some(value) => parse_piece_string(context, field, &value).map(Some),
        None => Ok(None),
    }
}

fn parse_piece_string(
    context: &'static str,
    field: &'static str,
    value: &str,
) -> Result<PieceKind, TemplateJsonError> {
    let mut chars = value.chars();
    let Some(piece) = chars.next() else {
        return Err(invalid_field(context, field, "empty piece string"));
    };
    if chars.next().is_some() {
        return Err(invalid_field(context, field, "piece must be one character"));
    }

    PieceKind::from_ascii(piece).map_err(|_| invalid_field(context, field, "unknown piece kind"))
}

pub(crate) fn parse_template_symmetry(
    value: Option<String>,
) -> Result<TemplateSymmetry, TemplateJsonError> {
    match value.as_deref().unwrap_or("none") {
        "none" => Ok(TemplateSymmetry::None),
        "mirror-x" => Ok(TemplateSymmetry::MirrorX),
        "mirror-y" => Ok(TemplateSymmetry::MirrorY),
        "rotate-180" => Ok(TemplateSymmetry::Rotate180),
        _ => Err(invalid_field(
            "template",
            "symmetry",
            "unknown template symmetry",
        )),
    }
}

pub(crate) fn parse_template_canonicalization(
    value: Option<String>,
) -> Result<TemplateCanonicalization, TemplateJsonError> {
    match value.as_deref().unwrap_or("none") {
        "none" => Ok(TemplateCanonicalization::None),
        "preserve-input" => Ok(TemplateCanonicalization::PreserveInput),
        "canonical-by-slot-id" => Ok(TemplateCanonicalization::CanonicalBySlotId),
        "canonical-by-geometry" => Ok(TemplateCanonicalization::CanonicalByGeometry),
        "canonical-by-symmetry" => Ok(TemplateCanonicalization::CanonicalBySymmetry),
        _ => Err(invalid_field(
            "template",
            "canonicalization",
            "unknown template canonicalization",
        )),
    }
}

pub(crate) fn parse_slot_hold_constraint(
    value: Option<String>,
) -> Result<SlotHoldConstraint, TemplateJsonError> {
    match value.as_deref().unwrap_or("any") {
        "any" => Ok(SlotHoldConstraint::Any),
        "requires-hold" => Ok(SlotHoldConstraint::RequiresHold),
        "forbids-hold" => Ok(SlotHoldConstraint::ForbidsHold),
        _ => Err(invalid_field(
            "template.slots[]",
            "hold_constraint",
            "unknown hold constraint",
        )),
    }
}

pub(crate) fn parse_slot_order_constraint(
    value: Option<&Value>,
) -> Result<SlotOrderConstraint, TemplateJsonError> {
    let Some(value) = value else {
        return Ok(SlotOrderConstraint::Any);
    };
    if let Some(kind) = value.as_str() {
        return parse_slot_order_kind(kind, None);
    }

    let object = object(value, "template.slots[].order_constraint")?;
    validate_order_constraint_fields(object)?;

    parse_slot_order_kind(
        &required_string(object, "template.slots[].order_constraint", "kind")?,
        optional_field(object, "slot_id")
            .map(|value| {
                value.as_u64().ok_or_else(|| {
                    invalid_field(
                        "template.slots[].order_constraint",
                        "slot_id",
                        "expected unsigned integer",
                    )
                })
            })
            .transpose()?
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    invalid_field(
                        "template.slots[].order_constraint",
                        "slot_id",
                        "value exceeds u32",
                    )
                })
            })
            .transpose()?,
    )
}

fn parse_slot_order_kind(
    kind: &str,
    referenced_slot: Option<u32>,
) -> Result<SlotOrderConstraint, TemplateJsonError> {
    match kind {
        "any" => Ok(SlotOrderConstraint::Any),
        "before" => referenced_slot
            .map(|slot_id| SlotOrderConstraint::Before(BuildSlotId::new(slot_id)))
            .ok_or_else(|| {
                invalid_field(
                    "template.slots[].order_constraint",
                    "slot_id",
                    "before order constraint requires slot_id",
                )
            }),
        "after" => referenced_slot
            .map(|slot_id| SlotOrderConstraint::After(BuildSlotId::new(slot_id)))
            .ok_or_else(|| {
                invalid_field(
                    "template.slots[].order_constraint",
                    "slot_id",
                    "after order constraint requires slot_id",
                )
            }),
        _ => Err(invalid_field(
            "template.slots[].order_constraint",
            "kind",
            "unknown order constraint",
        )),
    }
}

pub(crate) fn parse_slot_symmetry(
    value: Option<String>,
) -> Result<SlotSymmetry, TemplateJsonError> {
    match value.as_deref().unwrap_or("none") {
        "none" => Ok(SlotSymmetry::None),
        "mirror-x" => Ok(SlotSymmetry::MirrorX),
        "mirror-y" => Ok(SlotSymmetry::MirrorY),
        "rotate-180" => Ok(SlotSymmetry::Rotate180),
        _ => Err(invalid_field(
            "template.slots[]",
            "symmetry",
            "unknown slot symmetry",
        )),
    }
}

pub(crate) fn parse_slot_canonicalization(
    value: Option<String>,
) -> Result<SlotCanonicalization, TemplateJsonError> {
    match value.as_deref().unwrap_or("none") {
        "none" => Ok(SlotCanonicalization::None),
        "preserve-input" => Ok(SlotCanonicalization::PreserveInput),
        "canonical-by-cells" => Ok(SlotCanonicalization::CanonicalByCells),
        "canonical-by-symmetry" => Ok(SlotCanonicalization::CanonicalBySymmetry),
        _ => Err(invalid_field(
            "template.slots[]",
            "canonicalization",
            "unknown slot canonicalization",
        )),
    }
}

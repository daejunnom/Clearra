use serde_json::{json, Value};

use super::{
    build_slot::{BuildSlot, SlotOrderConstraint},
    build_template::BuildTemplate,
    template_json_schema::NATIVE_TEMPLATE_SCHEMA_VERSION,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TemplateJsonWriter;

impl TemplateJsonWriter {
    pub(crate) fn to_value(template: &BuildTemplate) -> Value {
        template_to_json(template)
    }
}

fn template_to_json(template: &BuildTemplate) -> Value {
    json!({
        "schema_version": NATIVE_TEMPLATE_SCHEMA_VERSION,
        "id": template.id(),
        "label": template.label(),
        "board": {
            "width": template.board_size().width(),
            "height": template.board_size().height(),
        },
        "symmetry": template.symmetry().as_str(),
        "canonicalization": template.canonicalization().as_str(),
        "slots": template.slots().iter().map(slot_to_json).collect::<Vec<_>>(),
    })
}

fn slot_to_json(slot: &BuildSlot) -> Value {
    json!({
        "id": slot.id().get(),
        "label": slot.label(),
        "cells": slot.cells().iter().map(|cell| {
            json!({
                "x": cell.x(),
                "y": cell.y(),
            })
        }).collect::<Vec<_>>(),
        "allowed_pieces": slot.allowed_pieces().iter().map(|piece| {
            piece.as_ascii().to_string()
        }).collect::<Vec<_>>(),
        "required_piece": slot.required_piece().map(|piece| piece.as_ascii().to_string()),
        "hold_constraint": slot.hold_constraint().as_str(),
        "order_constraint": order_constraint_to_json(slot.order_constraint()),
        "symmetry": slot.symmetry().as_str(),
        "canonicalization": slot.canonicalization().as_str(),
    })
}

fn order_constraint_to_json(order_constraint: SlotOrderConstraint) -> Value {
    match order_constraint {
        SlotOrderConstraint::Any => json!({ "kind": "any" }),
        SlotOrderConstraint::Before(slot_id) => {
            json!({ "kind": "before", "slot_id": slot_id.get() })
        }
        SlotOrderConstraint::After(slot_id) => {
            json!({ "kind": "after", "slot_id": slot_id.get() })
        }
    }
}

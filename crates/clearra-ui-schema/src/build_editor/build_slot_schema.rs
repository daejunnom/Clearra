use clearra_build_coverage::template::{
    BuildSlot, SlotCanonicalization, SlotHoldConstraint, SlotOrderConstraint, SlotSymmetry,
};

use super::{
    build_cell_schema::BuildCellSchema,
    build_field_schema::{BuildFieldSchema, BuildFieldType},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildSlotSchema {
    id: String,
    label: String,
    required: bool,
    cells: Vec<BuildCellSchema>,
    allowed_pieces: Vec<char>,
    required_piece: Option<char>,
    hold_constraint: String,
    order_constraint: String,
    order_reference: Option<String>,
    symmetry: String,
    canonicalization: String,
    fields: Vec<BuildFieldSchema>,
}

impl BuildSlotSchema {
    pub fn new(id: impl Into<String>, label: impl Into<String>, allowed_pieces: Vec<char>) -> Self {
        let allowed_piece_options = allowed_pieces
            .iter()
            .map(char::to_string)
            .collect::<Vec<_>>();
        Self {
            id: id.into(),
            label: label.into(),
            required: true,
            cells: Vec::new(),
            required_piece: None,
            hold_constraint: SlotHoldConstraint::Any.as_str().to_owned(),
            order_constraint: SlotOrderConstraint::Any.as_str().to_owned(),
            order_reference: None,
            symmetry: SlotSymmetry::None.as_str().to_owned(),
            canonicalization: SlotCanonicalization::None.as_str().to_owned(),
            fields: slot_fields(allowed_piece_options),
            allowed_pieces,
        }
    }
}
impl BuildSlotSchema {
    pub fn from_build_slot(slot: &BuildSlot) -> Self {
        let label = slot
            .label()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Slot {}", slot.id().get()));
        let allowed_pieces = slot
            .allowed_pieces()
            .iter()
            .map(|piece| piece.as_ascii())
            .collect::<Vec<_>>();
        let allowed_piece_options = allowed_pieces
            .iter()
            .map(char::to_string)
            .collect::<Vec<_>>();

        Self {
            id: format!("slot-{}", slot.id().get()),
            label,
            required: true,
            cells: slot
                .cells()
                .iter()
                .copied()
                .map(BuildCellSchema::from_coord)
                .collect(),
            required_piece: slot.required_piece().map(|piece| piece.as_ascii()),
            hold_constraint: slot.hold_constraint().as_str().to_owned(),
            order_constraint: slot.order_constraint().as_str().to_owned(),
            order_reference: slot
                .order_constraint()
                .referenced_slot()
                .map(|slot_id| format!("slot-{}", slot_id.get())),
            symmetry: slot.symmetry().as_str().to_owned(),
            canonicalization: slot.canonicalization().as_str().to_owned(),
            fields: slot_fields(allowed_piece_options),
            allowed_pieces,
        }
    }
}
impl BuildSlotSchema {
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}
impl BuildSlotSchema {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl BuildSlotSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl BuildSlotSchema {
    pub fn is_required(&self) -> bool {
        self.required
    }
}
impl BuildSlotSchema {
    pub fn allowed_pieces(&self) -> &[char] {
        &self.allowed_pieces
    }
}
impl BuildSlotSchema {
    pub fn cells(&self) -> &[BuildCellSchema] {
        &self.cells
    }
}
impl BuildSlotSchema {
    pub fn required_piece(&self) -> Option<char> {
        self.required_piece
    }
}
impl BuildSlotSchema {
    pub fn hold_constraint(&self) -> &str {
        &self.hold_constraint
    }
}
impl BuildSlotSchema {
    pub fn order_constraint(&self) -> &str {
        &self.order_constraint
    }
}
impl BuildSlotSchema {
    pub fn order_reference(&self) -> Option<&str> {
        self.order_reference.as_deref()
    }
}
impl BuildSlotSchema {
    pub fn symmetry(&self) -> &str {
        &self.symmetry
    }
}
impl BuildSlotSchema {
    pub fn canonicalization(&self) -> &str {
        &self.canonicalization
    }
}
impl BuildSlotSchema {
    pub fn fields(&self) -> &[BuildFieldSchema] {
        &self.fields
    }
}

fn slot_fields(allowed_piece_options: Vec<String>) -> Vec<BuildFieldSchema> {
    vec![
        BuildFieldSchema::new("label", "Label", BuildFieldType::Text, false, Vec::new()),
        BuildFieldSchema::new("cells", "Cells", BuildFieldType::CellList, true, Vec::new()),
        BuildFieldSchema::new(
            "allowed_pieces",
            "Allowed pieces",
            BuildFieldType::PieceMultiSelect,
            true,
            allowed_piece_options.clone(),
        ),
        BuildFieldSchema::new(
            "required_piece",
            "Required piece",
            BuildFieldType::PieceSelect,
            false,
            allowed_piece_options,
        ),
        BuildFieldSchema::new(
            "hold_constraint",
            "Hold constraint",
            BuildFieldType::Select,
            false,
            vec![
                SlotHoldConstraint::Any.as_str().to_owned(),
                SlotHoldConstraint::RequiresHold.as_str().to_owned(),
                SlotHoldConstraint::ForbidsHold.as_str().to_owned(),
            ],
        ),
        BuildFieldSchema::new(
            "order_constraint",
            "Order constraint",
            BuildFieldType::Select,
            false,
            vec![
                SlotOrderConstraint::Any.as_str().to_owned(),
                "before".to_owned(),
                "after".to_owned(),
            ],
        ),
        BuildFieldSchema::new(
            "symmetry",
            "Symmetry",
            BuildFieldType::Select,
            false,
            vec![
                SlotSymmetry::None.as_str().to_owned(),
                SlotSymmetry::MirrorX.as_str().to_owned(),
                SlotSymmetry::MirrorY.as_str().to_owned(),
                SlotSymmetry::Rotate180.as_str().to_owned(),
            ],
        ),
        BuildFieldSchema::new(
            "canonicalization",
            "Canonicalization",
            BuildFieldType::Select,
            false,
            vec![
                SlotCanonicalization::None.as_str().to_owned(),
                SlotCanonicalization::PreserveInput.as_str().to_owned(),
                SlotCanonicalization::CanonicalByCells.as_str().to_owned(),
                SlotCanonicalization::CanonicalBySymmetry
                    .as_str()
                    .to_owned(),
            ],
        ),
    ]
}

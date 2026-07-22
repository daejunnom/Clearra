use clearra_core_domain::{board::cell::CellCoord, piece::piece_kind::PieceKind};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuildSlotId(u32);

impl BuildSlotId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl BuildSlotId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildSlot {
    id: BuildSlotId,
    cells: Vec<CellCoord>,
    label: Option<String>,
    allowed_pieces: Vec<PieceKind>,
    required_piece: Option<PieceKind>,
    hold_constraint: SlotHoldConstraint,
    order_constraint: SlotOrderConstraint,
    symmetry: SlotSymmetry,
    canonicalization: SlotCanonicalization,
}

impl BuildSlot {
    pub fn new(id: BuildSlotId, cells: Vec<CellCoord>) -> Self {
        Self {
            id,
            cells,
            label: None,
            allowed_pieces: PieceKind::STANDARD_TETROMINOES.to_vec(),
            required_piece: None,
            hold_constraint: SlotHoldConstraint::Any,
            order_constraint: SlotOrderConstraint::Any,
            symmetry: SlotSymmetry::None,
            canonicalization: SlotCanonicalization::None,
        }
    }
}
impl BuildSlot {
    pub fn id(&self) -> BuildSlotId {
        self.id
    }
}
impl BuildSlot {
    pub fn cells(&self) -> &[CellCoord] {
        &self.cells
    }
}
impl BuildSlot {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}
impl BuildSlot {
    pub fn allowed_pieces(&self) -> &[PieceKind] {
        &self.allowed_pieces
    }
}
impl BuildSlot {
    pub fn required_piece(&self) -> Option<PieceKind> {
        self.required_piece
    }
}
impl BuildSlot {
    pub fn hold_constraint(&self) -> SlotHoldConstraint {
        self.hold_constraint
    }
}
impl BuildSlot {
    pub fn order_constraint(&self) -> SlotOrderConstraint {
        self.order_constraint
    }
}
impl BuildSlot {
    pub fn symmetry(&self) -> SlotSymmetry {
        self.symmetry
    }
}
impl BuildSlot {
    pub fn canonicalization(&self) -> SlotCanonicalization {
        self.canonicalization
    }
}
impl BuildSlot {
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}
impl BuildSlot {
    pub fn with_allowed_pieces(mut self, allowed_pieces: Vec<PieceKind>) -> Self {
        self.allowed_pieces = allowed_pieces;
        self
    }
}
impl BuildSlot {
    pub fn with_required_piece(mut self, required_piece: PieceKind) -> Self {
        self.required_piece = Some(required_piece);
        self
    }
}
impl BuildSlot {
    pub fn with_hold_constraint(mut self, hold_constraint: SlotHoldConstraint) -> Self {
        self.hold_constraint = hold_constraint;
        self
    }
}
impl BuildSlot {
    pub fn with_order_constraint(mut self, order_constraint: SlotOrderConstraint) -> Self {
        self.order_constraint = order_constraint;
        self
    }
}
impl BuildSlot {
    pub fn with_symmetry(mut self, symmetry: SlotSymmetry) -> Self {
        self.symmetry = symmetry;
        self
    }
}
impl BuildSlot {
    pub fn with_canonicalization(mut self, canonicalization: SlotCanonicalization) -> Self {
        self.canonicalization = canonicalization;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SlotHoldConstraint {
    #[default]
    Any,
    RequiresHold,
    ForbidsHold,
}

impl SlotHoldConstraint {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::RequiresHold => "requires-hold",
            Self::ForbidsHold => "forbids-hold",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SlotOrderConstraint {
    #[default]
    Any,
    Before(BuildSlotId),
    After(BuildSlotId),
}

impl SlotOrderConstraint {
    pub const fn referenced_slot(self) -> Option<BuildSlotId> {
        match self {
            Self::Any => None,
            Self::Before(slot_id) | Self::After(slot_id) => Some(slot_id),
        }
    }
}
impl SlotOrderConstraint {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Before(_) => "before",
            Self::After(_) => "after",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SlotSymmetry {
    #[default]
    None,
    MirrorX,
    MirrorY,
    Rotate180,
}

impl SlotSymmetry {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MirrorX => "mirror-x",
            Self::MirrorY => "mirror-y",
            Self::Rotate180 => "rotate-180",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SlotCanonicalization {
    #[default]
    None,
    PreserveInput,
    CanonicalByCells,
    CanonicalBySymmetry,
}

impl SlotCanonicalization {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PreserveInput => "preserve-input",
            Self::CanonicalByCells => "canonical-by-cells",
            Self::CanonicalBySymmetry => "canonical-by-symmetry",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_slot_carries_editor_metadata_without_replacing_geometry() {
        let slot = BuildSlot::new(BuildSlotId::new(1), vec![CellCoord::new_unchecked(0, 0)])
            .with_label("left well")
            .with_allowed_pieces(vec![PieceKind::I, PieceKind::O])
            .with_required_piece(PieceKind::I)
            .with_hold_constraint(SlotHoldConstraint::RequiresHold)
            .with_order_constraint(SlotOrderConstraint::Before(BuildSlotId::new(2)))
            .with_symmetry(SlotSymmetry::MirrorX)
            .with_canonicalization(SlotCanonicalization::CanonicalBySymmetry);

        assert_eq!(slot.label(), Some("left well"));
        assert_eq!(slot.allowed_pieces(), &[PieceKind::I, PieceKind::O]);
        assert_eq!(slot.required_piece(), Some(PieceKind::I));
        assert_eq!(slot.hold_constraint(), SlotHoldConstraint::RequiresHold);
        assert_eq!(
            slot.order_constraint().referenced_slot(),
            Some(BuildSlotId::new(2))
        );
        assert_eq!(slot.symmetry().as_str(), "mirror-x");
        assert_eq!(slot.canonicalization().as_str(), "canonical-by-symmetry");
    }
}

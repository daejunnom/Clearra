use clearra_core_domain::board::board_size::BoardSize;

use super::build_slot::{BuildSlot, BuildSlotId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTemplate {
    id: String,
    label: Option<String>,
    board_size: BoardSize,
    slots: Vec<BuildSlot>,
    symmetry: TemplateSymmetry,
    canonicalization: TemplateCanonicalization,
}

impl BuildTemplate {
    pub fn new(id: impl Into<String>, slots: Vec<BuildSlot>) -> Self {
        Self {
            id: id.into(),
            label: None,
            board_size: BoardSize::standard_10x20(),
            slots,
            symmetry: TemplateSymmetry::None,
            canonicalization: TemplateCanonicalization::None,
        }
    }
}
impl BuildTemplate {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl BuildTemplate {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}
impl BuildTemplate {
    pub fn board_size(&self) -> BoardSize {
        self.board_size
    }
}
impl BuildTemplate {
    pub fn slots(&self) -> &[BuildSlot] {
        &self.slots
    }
}
impl BuildTemplate {
    pub fn slot(&self, id: BuildSlotId) -> Option<&BuildSlot> {
        self.slots.iter().find(|slot| slot.id() == id)
    }
}
impl BuildTemplate {
    pub fn symmetry(&self) -> TemplateSymmetry {
        self.symmetry
    }
}
impl BuildTemplate {
    pub fn canonicalization(&self) -> TemplateCanonicalization {
        self.canonicalization
    }
}
impl BuildTemplate {
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}
impl BuildTemplate {
    pub fn with_board_size(mut self, board_size: BoardSize) -> Self {
        self.board_size = board_size;
        self
    }
}
impl BuildTemplate {
    pub fn with_symmetry(mut self, symmetry: TemplateSymmetry) -> Self {
        self.symmetry = symmetry;
        self
    }
}
impl BuildTemplate {
    pub fn with_canonicalization(mut self, canonicalization: TemplateCanonicalization) -> Self {
        self.canonicalization = canonicalization;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TemplateSymmetry {
    #[default]
    None,
    MirrorX,
    MirrorY,
    Rotate180,
}

impl TemplateSymmetry {
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
pub enum TemplateCanonicalization {
    #[default]
    None,
    PreserveInput,
    CanonicalBySlotId,
    CanonicalByGeometry,
    CanonicalBySymmetry,
}

impl TemplateCanonicalization {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PreserveInput => "preserve-input",
            Self::CanonicalBySlotId => "canonical-by-slot-id",
            Self::CanonicalByGeometry => "canonical-by-geometry",
            Self::CanonicalBySymmetry => "canonical-by-symmetry",
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::board::cell::CellCoord;

    use super::*;

    #[test]
    fn build_template_carries_editor_import_export_metadata() {
        let template = BuildTemplate::new(
            "template-a",
            vec![BuildSlot::new(
                BuildSlotId::new(1),
                vec![CellCoord::new_unchecked(0, 0)],
            )],
        )
        .with_label("T-spin shell")
        .with_board_size(BoardSize::new(10, 4).expect("board"))
        .with_symmetry(TemplateSymmetry::MirrorX)
        .with_canonicalization(TemplateCanonicalization::CanonicalByGeometry);

        assert_eq!(template.id(), "template-a");
        assert_eq!(template.label(), Some("T-spin shell"));
        assert_eq!(template.board_size().height(), 4);
        assert_eq!(template.symmetry().as_str(), "mirror-x");
        assert_eq!(
            template.canonicalization().as_str(),
            "canonical-by-geometry"
        );
    }
}

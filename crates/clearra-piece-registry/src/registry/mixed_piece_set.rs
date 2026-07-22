use clearra_core_domain::{
    ids::piece_id::PieceDefinitionId,
    piece::{piece_kind::PieceKind, standard_tetromino_piece::StandardTetrominoPiece},
};

use crate::custom::CustomPieceDefinition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MixedPieceSet {
    id: String,
    label: String,
    entries: Vec<MixedPieceSetEntry>,
}

impl MixedPieceSet {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        entries: Vec<MixedPieceSetEntry>,
    ) -> Result<Self, MixedPieceSetError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(MixedPieceSetError::EmptyPieceSetId);
        }
        let label = label.into();
        if label.trim().is_empty() {
            return Err(MixedPieceSetError::EmptyPieceSetLabel);
        }
        if entries.is_empty() {
            return Err(MixedPieceSetError::EmptyPieceSet);
        }

        let mut stable_ids = Vec::new();
        for entry in &entries {
            let stable_id = entry.stable_id();
            if stable_ids.contains(&stable_id) {
                return Err(MixedPieceSetError::DuplicateStablePieceId {
                    id: stable_id.as_str().to_owned(),
                });
            }
            stable_ids.push(stable_id);
        }

        Ok(Self { id, label, entries })
    }
}
impl MixedPieceSet {
    pub fn standard_plus_custom(
        id: impl Into<String>,
        label: impl Into<String>,
        custom_pieces: Vec<CustomPieceDefinition>,
    ) -> Result<Self, MixedPieceSetError> {
        let mut entries = PieceKind::STANDARD_TETROMINOES
            .iter()
            .copied()
            .map(MixedPieceSetEntry::Standard)
            .collect::<Vec<_>>();
        entries.extend(custom_pieces.into_iter().map(MixedPieceSetEntry::Custom));
        Self::new(id, label, entries)
    }
}
impl MixedPieceSet {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl MixedPieceSet {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl MixedPieceSet {
    pub fn entries(&self) -> &[MixedPieceSetEntry] {
        &self.entries
    }
}
impl MixedPieceSet {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
impl MixedPieceSet {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
impl MixedPieceSet {
    pub fn contains_custom(&self) -> bool {
        self.entries.iter().any(MixedPieceSetEntry::is_custom)
    }
}
impl MixedPieceSet {
    pub fn custom_piece_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.is_custom())
            .count()
    }
}
impl MixedPieceSet {
    pub fn standard_fast_path_compatible(&self) -> bool {
        !self.contains_custom()
    }
}
impl MixedPieceSet {
    pub fn mixed_area_multiset(&self) -> Vec<usize> {
        self.entries.iter().map(MixedPieceSetEntry::area).collect()
    }
}
impl MixedPieceSet {
    pub fn stable_piece_ids(&self) -> Vec<PieceDefinitionId> {
        self.entries
            .iter()
            .map(MixedPieceSetEntry::stable_id)
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MixedPieceSetEntry {
    Standard(PieceKind),
    Custom(CustomPieceDefinition),
}

impl MixedPieceSetEntry {
    pub fn stable_id(&self) -> PieceDefinitionId {
        match self {
            Self::Standard(piece) => standard_piece_definition_id(*piece),
            Self::Custom(definition) => definition.id().clone(),
        }
    }
}
impl MixedPieceSetEntry {
    pub fn label(&self) -> String {
        match self {
            Self::Standard(piece) => piece.as_ascii().to_string(),
            Self::Custom(definition) => definition.label().to_owned(),
        }
    }
}
impl MixedPieceSetEntry {
    pub fn area(&self) -> usize {
        match self {
            Self::Standard(_) => 4,
            Self::Custom(definition) => definition.area(),
        }
    }
}
impl MixedPieceSetEntry {
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MixedPieceSetError {
    EmptyPieceSetId,
    EmptyPieceSetLabel,
    EmptyPieceSet,
    DuplicateStablePieceId { id: String },
}

pub fn standard_piece_definition_id(piece: PieceKind) -> PieceDefinitionId {
    StandardTetrominoPiece::new(piece).piece_definition_id()
}

#[cfg(test)]
#[path = "mixed_piece_set_tests.rs"]
mod tests;

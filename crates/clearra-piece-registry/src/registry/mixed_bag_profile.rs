use std::collections::BTreeSet;

use clearra_core_domain::ids::piece_id::PieceDefinitionId;

use super::mixed_piece_set::MixedPieceSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MixedBagProfile {
    id: String,
    piece_set_id: String,
    entries: Vec<MixedBagEntry>,
    boundary_models: BagBoundaryModels,
}

impl MixedBagProfile {
    pub fn new(
        id: impl Into<String>,
        piece_set: &MixedPieceSet,
        entries: Vec<MixedBagEntry>,
        boundary_models: BagBoundaryModels,
    ) -> Result<Self, MixedBagProfileError> {
        if entries.is_empty() {
            return Err(MixedBagProfileError::EmptyBag);
        }

        let stable_ids = piece_set
            .stable_piece_ids()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();

        for entry in &entries {
            if !stable_ids.contains(entry.piece_id()) {
                return Err(MixedBagProfileError::UnknownPieceId {
                    piece_id: entry.piece_id().clone(),
                });
            }
            if entry.multiplicity() == 0 {
                return Err(MixedBagProfileError::ZeroMultiplicity {
                    piece_id: entry.piece_id().clone(),
                });
            }
            if entry.weight() == 0 {
                return Err(MixedBagProfileError::ZeroWeight {
                    piece_id: entry.piece_id().clone(),
                });
            }
            if !seen.insert(entry.piece_id().clone()) {
                return Err(MixedBagProfileError::DuplicatePieceId {
                    piece_id: entry.piece_id().clone(),
                });
            }
        }

        Ok(Self {
            id: id.into(),
            piece_set_id: piece_set.id().to_owned(),
            entries,
            boundary_models,
        })
    }
}
impl MixedBagProfile {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl MixedBagProfile {
    pub fn piece_set_id(&self) -> &str {
        &self.piece_set_id
    }
}
impl MixedBagProfile {
    pub fn entries(&self) -> &[MixedBagEntry] {
        &self.entries
    }
}
impl MixedBagProfile {
    pub fn bag_size(&self) -> usize {
        self.entries.iter().map(MixedBagEntry::multiplicity).sum()
    }
}
impl MixedBagProfile {
    pub fn total_weight(&self) -> u32 {
        self.entries.iter().map(MixedBagEntry::weight).sum()
    }
}
impl MixedBagProfile {
    pub fn mixed_bag_schema_validates(&self) -> bool {
        !self.entries.is_empty() && self.bag_size() > 0 && self.total_weight() > 0
    }
}
impl MixedBagProfile {
    pub fn boundary_models(&self) -> BagBoundaryModels {
        self.boundary_models
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MixedBagEntry {
    piece_id: PieceDefinitionId,
    multiplicity: usize,
    weight: u32,
}

impl MixedBagEntry {
    pub fn new(piece_id: PieceDefinitionId, multiplicity: usize, weight: u32) -> Self {
        Self {
            piece_id,
            multiplicity,
            weight,
        }
    }
}
impl MixedBagEntry {
    pub fn piece_id(&self) -> &PieceDefinitionId {
        &self.piece_id
    }
}
impl MixedBagEntry {
    pub fn multiplicity(&self) -> usize {
        self.multiplicity
    }
}
impl MixedBagEntry {
    pub fn weight(&self) -> u32 {
        self.weight
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagBoundaryModels {
    fixed_sequence: bool,
    observed_window: bool,
    bag_aligned_pattern: bool,
}

impl BagBoundaryModels {
    pub const fn new(
        fixed_sequence: bool,
        observed_window: bool,
        bag_aligned_pattern: bool,
    ) -> Self {
        Self {
            fixed_sequence,
            observed_window,
            bag_aligned_pattern,
        }
    }
}
impl BagBoundaryModels {
    pub const fn all_mvp3_models() -> Self {
        Self::new(true, true, true)
    }
}
impl BagBoundaryModels {
    pub const fn fixed_sequence(self) -> bool {
        self.fixed_sequence
    }
}
impl BagBoundaryModels {
    pub const fn observed_window(self) -> bool {
        self.observed_window
    }
}
impl BagBoundaryModels {
    pub const fn bag_aligned_pattern(self) -> bool {
        self.bag_aligned_pattern
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MixedBagProfileError {
    EmptyBag,
    UnknownPieceId { piece_id: PieceDefinitionId },
    DuplicatePieceId { piece_id: PieceDefinitionId },
    ZeroMultiplicity { piece_id: PieceDefinitionId },
    ZeroWeight { piece_id: PieceDefinitionId },
}

#[cfg(test)]
#[path = "mixed_bag_profile_tests.rs"]
mod tests;

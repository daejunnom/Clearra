use std::collections::BTreeSet;

use clearra_core_domain::ids::piece_id::PieceDefinitionId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomBagProfile {
    bag_profile_id: String,
    piece_set_id: String,
    entries: Vec<CustomBagEntry>,
}

impl CustomBagProfile {
    pub fn new(
        bag_profile_id: impl Into<String>,
        piece_set_id: impl Into<String>,
        entries: Vec<CustomBagEntry>,
    ) -> Result<Self, CustomBagProfileError> {
        let bag_profile_id = bag_profile_id.into();
        if bag_profile_id.trim().is_empty() {
            return Err(CustomBagProfileError::EmptyBagProfileId);
        }
        let piece_set_id = piece_set_id.into();
        if piece_set_id.trim().is_empty() {
            return Err(CustomBagProfileError::EmptyPieceSetId);
        }
        if entries.is_empty() {
            return Err(CustomBagProfileError::EmptyEntries);
        }

        let mut seen = BTreeSet::new();
        for entry in &entries {
            if entry.multiplicity() == 0 {
                return Err(CustomBagProfileError::ZeroMultiplicity {
                    piece_definition_id: entry.piece_definition_id().clone(),
                });
            }
            if entry.weight() == 0 {
                return Err(CustomBagProfileError::ZeroWeight {
                    piece_definition_id: entry.piece_definition_id().clone(),
                });
            }
            if !seen.insert(entry.piece_definition_id().clone()) {
                return Err(CustomBagProfileError::DuplicatePieceDefinitionId {
                    piece_definition_id: entry.piece_definition_id().clone(),
                });
            }
        }

        Ok(Self {
            bag_profile_id,
            piece_set_id,
            entries,
        })
    }
}
impl CustomBagProfile {
    pub fn bag_profile_id(&self) -> &str {
        &self.bag_profile_id
    }
}
impl CustomBagProfile {
    pub fn piece_set_id(&self) -> &str {
        &self.piece_set_id
    }
}
impl CustomBagProfile {
    pub fn entries(&self) -> &[CustomBagEntry] {
        &self.entries
    }
}
impl CustomBagProfile {
    pub fn bag_size(&self) -> usize {
        self.entries.iter().map(CustomBagEntry::multiplicity).sum()
    }
}
impl CustomBagProfile {
    pub fn total_weight(&self) -> u32 {
        self.entries.iter().map(CustomBagEntry::weight).sum()
    }
}
impl CustomBagProfile {
    pub fn custom_bag_schema_valid(&self) -> bool {
        !self.entries.is_empty() && self.bag_size() > 0 && self.total_weight() > 0
    }
}
impl CustomBagProfile {
    pub fn runtime_guard(&self) -> MixedBagSchemaRuntimeGuard {
        MixedBagSchemaRuntimeGuard
    }
}
impl CustomBagProfile {
    pub fn runtime_guard_reason(&self) -> &'static str {
        "custom_bag_runtime_not_connected"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomBagEntry {
    piece_definition_id: PieceDefinitionId,
    multiplicity: usize,
    weight: u32,
}

impl CustomBagEntry {
    pub fn new(piece_definition_id: PieceDefinitionId, multiplicity: usize, weight: u32) -> Self {
        Self {
            piece_definition_id,
            multiplicity,
            weight,
        }
    }
}
impl CustomBagEntry {
    pub fn piece_definition_id(&self) -> &PieceDefinitionId {
        &self.piece_definition_id
    }
}
impl CustomBagEntry {
    pub fn multiplicity(&self) -> usize {
        self.multiplicity
    }
}
impl CustomBagEntry {
    pub fn weight(&self) -> u32 {
        self.weight
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomBagProfileError {
    EmptyBagProfileId,
    EmptyPieceSetId,
    EmptyEntries,
    DuplicatePieceDefinitionId {
        piece_definition_id: PieceDefinitionId,
    },
    ZeroMultiplicity {
        piece_definition_id: PieceDefinitionId,
    },
    ZeroWeight {
        piece_definition_id: PieceDefinitionId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MixedBagSchemaRuntimeGuard;

impl MixedBagSchemaRuntimeGuard {
    pub const fn reason(self) -> &'static str {
        "custom_bag_runtime_not_connected"
    }
}

#[cfg(test)]
#[path = "custom_bag_profile_tests.rs"]
mod tests;

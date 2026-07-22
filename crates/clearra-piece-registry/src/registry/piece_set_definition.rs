use clearra_core_domain::ids::piece_id::PieceDefinitionId;

use super::{
    piece_registry_bridge::{piece_area_multiset_fingerprint, piece_definition_id_fingerprint},
    MixedPieceSet, MixedPieceSetEntry,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PieceSetDefinition {
    piece_set_id: String,
    pieces: Vec<MixedPieceSetEntry>,
    standard_fast_path_compatible: bool,
    mixed_area_multiset: Vec<usize>,
    piece_definition_id_fingerprint: u64,
    piece_area_multiset_fingerprint: u64,
    piece_set_profile_id: u64,
}

impl PieceSetDefinition {
    pub fn from_mixed_piece_set(piece_set: &MixedPieceSet) -> Self {
        let pieces = piece_set.entries().to_vec();
        let mixed_area_multiset = pieces
            .iter()
            .map(MixedPieceSetEntry::area)
            .collect::<Vec<_>>();
        let stable_piece_ids = pieces
            .iter()
            .map(MixedPieceSetEntry::stable_id)
            .collect::<Vec<PieceDefinitionId>>();

        Self {
            piece_set_id: piece_set.id().to_owned(),
            standard_fast_path_compatible: !piece_set.contains_custom(),
            piece_definition_id_fingerprint: piece_definition_id_fingerprint(&stable_piece_ids),
            piece_area_multiset_fingerprint: piece_area_multiset_fingerprint(&mixed_area_multiset),
            piece_set_profile_id: piece_set_profile_id(piece_set.id()),
            pieces,
            mixed_area_multiset,
        }
    }
}
impl PieceSetDefinition {
    pub fn piece_set_id(&self) -> &str {
        &self.piece_set_id
    }
}
impl PieceSetDefinition {
    pub fn pieces(&self) -> &[MixedPieceSetEntry] {
        &self.pieces
    }
}
impl PieceSetDefinition {
    pub fn standard_fast_path_compatible(&self) -> bool {
        self.standard_fast_path_compatible
    }
}
impl PieceSetDefinition {
    pub fn mixed_area_multiset(&self) -> &[usize] {
        &self.mixed_area_multiset
    }
}
impl PieceSetDefinition {
    pub fn piece_definition_id_fingerprint(&self) -> u64 {
        self.piece_definition_id_fingerprint
    }
}
impl PieceSetDefinition {
    pub fn piece_area_multiset_fingerprint(&self) -> u64 {
        self.piece_area_multiset_fingerprint
    }
}
impl PieceSetDefinition {
    pub fn piece_set_profile_id(&self) -> u64 {
        self.piece_set_profile_id
    }
}

pub fn piece_set_profile_id(piece_set_id: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in b"piece-set-profile:v1" {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for byte in piece_set_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

#[cfg(test)]
#[path = "piece_set_definition_tests.rs"]
mod tests;

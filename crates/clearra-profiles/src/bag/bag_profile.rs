use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::pieces::piece_set_profile::PieceSetProfileId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BagProfileId {
    Standard7Bag,
}

impl BagProfileId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard7Bag => "standard-7-bag",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagProfile {
    id: BagProfileId,
    piece_set_id: PieceSetProfileId,
    pieces_per_bag: &'static [PieceKind],
    entries: &'static [BagProfileEntry],
}

impl BagProfile {
    pub const fn new(
        id: BagProfileId,
        piece_set_id: PieceSetProfileId,
        pieces_per_bag: &'static [PieceKind],
        entries: &'static [BagProfileEntry],
    ) -> Self {
        Self {
            id,
            piece_set_id,
            pieces_per_bag,
            entries,
        }
    }
}
impl BagProfile {
    pub fn id(self) -> BagProfileId {
        self.id
    }
}
impl BagProfile {
    pub fn piece_set_id(self) -> PieceSetProfileId {
        self.piece_set_id
    }
}
impl BagProfile {
    pub fn pieces_per_bag(self) -> &'static [PieceKind] {
        self.pieces_per_bag
    }
}
impl BagProfile {
    pub fn entries(self) -> &'static [BagProfileEntry] {
        self.entries
    }
}
impl BagProfile {
    pub fn bag_size(self) -> usize {
        self.entries.iter().map(|entry| entry.multiplicity()).sum()
    }
}
impl BagProfile {
    pub fn multiplicity_for(self, piece: PieceKind) -> usize {
        self.entries
            .iter()
            .find(|entry| entry.piece() == piece)
            .map(|entry| entry.multiplicity())
            .unwrap_or(0)
    }
}
impl BagProfile {
    pub fn total_weight(self) -> u32 {
        self.entries.iter().map(|entry| entry.weight()).sum()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagProfileEntry {
    piece: PieceKind,
    multiplicity: usize,
    weight: u32,
}

impl BagProfileEntry {
    pub const fn new(piece: PieceKind, multiplicity: usize, weight: u32) -> Self {
        Self {
            piece,
            multiplicity,
            weight,
        }
    }
}
impl BagProfileEntry {
    pub const fn piece(self) -> PieceKind {
        self.piece
    }
}
impl BagProfileEntry {
    pub const fn multiplicity(self) -> usize {
        self.multiplicity
    }
}
impl BagProfileEntry {
    pub const fn weight(self) -> u32 {
        self.weight
    }
}

#[cfg(test)]
#[path = "bag_profile_tests.rs"]
mod tests;

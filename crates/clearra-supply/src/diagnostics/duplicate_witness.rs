use std::collections::BTreeMap;

use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::bag::bag_profile::BagProfile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DuplicateWitness {
    piece: PieceKind,
    first_index: usize,
    duplicate_index: usize,
    bag_start_index: usize,
    initial_offset: usize,
}

impl DuplicateWitness {
    pub fn new(
        piece: PieceKind,
        first_index: usize,
        duplicate_index: usize,
        bag_start_index: usize,
        initial_offset: usize,
    ) -> Self {
        Self {
            piece,
            first_index,
            duplicate_index,
            bag_start_index,
            initial_offset,
        }
    }
}
impl DuplicateWitness {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}
impl DuplicateWitness {
    pub fn first_index(self) -> usize {
        self.first_index
    }
}
impl DuplicateWitness {
    pub fn duplicate_index(self) -> usize {
        self.duplicate_index
    }
}
impl DuplicateWitness {
    pub fn bag_start_index(self) -> usize {
        self.bag_start_index
    }
}
impl DuplicateWitness {
    pub fn initial_offset(self) -> usize {
        self.initial_offset
    }
}

pub fn duplicate_for_boundary_offset(
    pieces: &[PieceKind],
    bag_size: usize,
    initial_offset: usize,
) -> Option<DuplicateWitness> {
    if bag_size == 7 {
        return duplicate_for_boundary_offset_with_profile(
            pieces,
            &BagProfile::standard_7(),
            initial_offset,
        );
    }

    if bag_size == 0 || initial_offset >= bag_size {
        return None;
    }

    let mut seen: BTreeMap<PieceKind, usize> = BTreeMap::new();
    let mut offset = initial_offset;
    let mut bag_start_index = 0;

    for (index, piece) in pieces.iter().copied().enumerate() {
        if offset == bag_size {
            offset = 0;
            seen.clear();
            bag_start_index = index;
        }

        if let Some(first_index) = seen.insert(piece, index) {
            return Some(DuplicateWitness::new(
                piece,
                first_index,
                index,
                bag_start_index,
                initial_offset,
            ));
        }

        offset += 1;
    }

    None
}

pub fn first_duplicate_across_offsets(
    pieces: &[PieceKind],
    bag_size: usize,
) -> Option<DuplicateWitness> {
    (0..bag_size)
        .filter_map(|offset| duplicate_for_boundary_offset(pieces, bag_size, offset))
        .min_by_key(|witness| (witness.duplicate_index(), witness.initial_offset()))
}

pub fn duplicate_for_boundary_offset_with_profile(
    pieces: &[PieceKind],
    bag_profile: &BagProfile,
    initial_offset: usize,
) -> Option<DuplicateWitness> {
    let bag_size = bag_profile.bag_size();
    if bag_size == 0 || initial_offset >= bag_size {
        return None;
    }

    let mut seen: BTreeMap<PieceKind, (usize, usize)> = BTreeMap::new();
    let mut offset = initial_offset;
    let mut bag_start_index = 0;

    for (index, piece) in pieces.iter().copied().enumerate() {
        if offset == bag_size {
            offset = 0;
            seen.clear();
            bag_start_index = index;
        }

        let allowed_multiplicity = bag_profile.multiplicity_for(piece);
        if allowed_multiplicity == 0 {
            return Some(DuplicateWitness::new(
                piece,
                index,
                index,
                bag_start_index,
                initial_offset,
            ));
        }

        let entry = seen.entry(piece).or_insert((index, 0));
        entry.1 += 1;
        if entry.1 > allowed_multiplicity {
            return Some(DuplicateWitness::new(
                piece,
                entry.0,
                index,
                bag_start_index,
                initial_offset,
            ));
        }

        offset += 1;
    }

    None
}

pub fn first_duplicate_across_offsets_with_profile(
    pieces: &[PieceKind],
    bag_profile: &BagProfile,
) -> Option<DuplicateWitness> {
    (0..bag_profile.bag_size())
        .filter_map(|offset| {
            duplicate_for_boundary_offset_with_profile(pieces, bag_profile, offset)
        })
        .min_by_key(|witness| (witness.duplicate_index(), witness.initial_offset()))
}

#[cfg(test)]
#[path = "duplicate_witness_tests.rs"]
mod tests;

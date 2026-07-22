use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    bag::bag_profile::BagProfile,
    diagnostics::duplicate_witness::{
        duplicate_for_boundary_offset, duplicate_for_boundary_offset_with_profile,
        first_duplicate_across_offsets, first_duplicate_across_offsets_with_profile,
        DuplicateWitness,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagBoundaryCandidate {
    initial_offset: usize,
}

impl BagBoundaryCandidate {
    pub fn new(initial_offset: usize) -> Self {
        Self { initial_offset }
    }
}
impl BagBoundaryCandidate {
    pub fn initial_offset(self) -> usize {
        self.initial_offset
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BagBoundaryReport {
    bag_size: usize,
    candidates: Vec<BagBoundaryCandidate>,
    duplicate_witness: Option<DuplicateWitness>,
}

impl BagBoundaryReport {
    pub fn analyze_observed_window(pieces: &[PieceKind], bag_size: usize) -> Self {
        if bag_size == 0 {
            return Self {
                bag_size,
                candidates: Vec::new(),
                duplicate_witness: None,
            };
        }

        let candidates = (0..bag_size)
            .filter(|offset| duplicate_for_boundary_offset(pieces, bag_size, *offset).is_none())
            .map(BagBoundaryCandidate::new)
            .collect::<Vec<_>>();

        let duplicate_witness = if candidates.is_empty() {
            first_duplicate_across_offsets(pieces, bag_size)
        } else {
            None
        };

        Self {
            bag_size,
            candidates,
            duplicate_witness,
        }
    }
}
impl BagBoundaryReport {
    pub fn analyze_observed_window_with_profile(
        pieces: &[PieceKind],
        bag_profile: &BagProfile,
    ) -> Self {
        let bag_size = bag_profile.bag_size();
        if bag_size == 0 {
            return Self {
                bag_size,
                candidates: Vec::new(),
                duplicate_witness: None,
            };
        }

        let candidates = (0..bag_size)
            .filter(|offset| {
                duplicate_for_boundary_offset_with_profile(pieces, bag_profile, *offset).is_none()
            })
            .map(BagBoundaryCandidate::new)
            .collect::<Vec<_>>();

        let duplicate_witness = if candidates.is_empty() {
            first_duplicate_across_offsets_with_profile(pieces, bag_profile)
        } else {
            None
        };

        Self {
            bag_size,
            candidates,
            duplicate_witness,
        }
    }
}
impl BagBoundaryReport {
    pub fn analyze_fixed_queue(pieces: &[PieceKind], bag_size: usize) -> Self {
        if bag_size == 0 {
            return Self {
                bag_size,
                candidates: Vec::new(),
                duplicate_witness: None,
            };
        }

        let duplicate_witness = duplicate_for_boundary_offset(pieces, bag_size, 0);
        let candidates = if duplicate_witness.is_none() {
            vec![BagBoundaryCandidate::new(0)]
        } else {
            Vec::new()
        };

        Self {
            bag_size,
            candidates,
            duplicate_witness,
        }
    }
}
impl BagBoundaryReport {
    pub fn analyze_fixed_queue_with_profile(
        pieces: &[PieceKind],
        bag_profile: &BagProfile,
    ) -> Self {
        let bag_size = bag_profile.bag_size();
        if bag_size == 0 {
            return Self {
                bag_size,
                candidates: Vec::new(),
                duplicate_witness: None,
            };
        }

        let duplicate_witness = duplicate_for_boundary_offset_with_profile(pieces, bag_profile, 0);
        let candidates = if duplicate_witness.is_none() {
            vec![BagBoundaryCandidate::new(0)]
        } else {
            Vec::new()
        };

        Self {
            bag_size,
            candidates,
            duplicate_witness,
        }
    }
}
impl BagBoundaryReport {
    pub fn bag_size(&self) -> usize {
        self.bag_size
    }
}
impl BagBoundaryReport {
    pub fn candidates(&self) -> &[BagBoundaryCandidate] {
        &self.candidates
    }
}
impl BagBoundaryReport {
    pub fn duplicate_witness(&self) -> Option<DuplicateWitness> {
        self.duplicate_witness
    }
}
impl BagBoundaryReport {
    pub fn is_compatible(&self) -> bool {
        !self.candidates.is_empty()
    }
}
impl BagBoundaryReport {
    pub fn is_ambiguous(&self) -> bool {
        self.candidates.len() != 1
    }
}

pub fn standard_7_bag_observed_boundary_report(pieces: &[PieceKind]) -> BagBoundaryReport {
    BagBoundaryReport::analyze_observed_window_with_profile(pieces, &BagProfile::standard_7())
}

pub fn standard_7_bag_fixed_boundary_report(pieces: &[PieceKind]) -> BagBoundaryReport {
    BagBoundaryReport::analyze_fixed_queue_with_profile(pieces, &BagProfile::standard_7())
}

#[cfg(test)]
#[path = "bag_boundary_tests.rs"]
mod tests;

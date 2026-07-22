use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::pieces::{
    piece_set_profile::PieceSetProfileId, standard_tetrominoes::STANDARD_TETROMINOES,
};

use super::bag_profile::{BagProfile, BagProfileEntry, BagProfileId};

const STANDARD_7_BAG_ENTRIES: [BagProfileEntry; 7] = [
    BagProfileEntry::new(PieceKind::I, 1, 1),
    BagProfileEntry::new(PieceKind::O, 1, 1),
    BagProfileEntry::new(PieceKind::T, 1, 1),
    BagProfileEntry::new(PieceKind::S, 1, 1),
    BagProfileEntry::new(PieceKind::Z, 1, 1),
    BagProfileEntry::new(PieceKind::J, 1, 1),
    BagProfileEntry::new(PieceKind::L, 1, 1),
];

pub fn standard_7_bag_profile() -> BagProfile {
    BagProfile::new(
        BagProfileId::Standard7Bag,
        PieceSetProfileId::StandardTetrominoes,
        &STANDARD_TETROMINOES,
        &STANDARD_7_BAG_ENTRIES,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_bag_has_one_of_each_standard_piece() {
        let profile = standard_7_bag_profile();

        assert_eq!(profile.bag_size(), 7);
        assert_eq!(profile.entries(), &STANDARD_7_BAG_ENTRIES);
        assert_eq!(profile.multiplicity_for(PieceKind::I), 1);
        assert_eq!(profile.total_weight(), 7);
        assert_eq!(
            profile.piece_set_id(),
            PieceSetProfileId::StandardTetrominoes
        );
        assert_eq!(profile.pieces_per_bag(), &STANDARD_TETROMINOES);
    }
}

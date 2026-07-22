use clearra_core_domain::piece::piece_kind::PieceKind;

use super::bag_profile::BagProfile;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BagState {
    epoch: u16,
    generated_count: u16,
    remainder_counts: [u8; 7],
}

impl BagState {
    pub fn fresh(profile: &BagProfile) -> Result<Self, BagStateError> {
        Self::from_remainder(profile, 0, 0, profile_counts(profile)?)
    }

    pub fn fresh_standard_7_bag() -> Self {
        Self::fresh(&BagProfile::standard_7()).expect("standard 7-bag state is valid")
    }

    pub fn from_remainder(
        profile: &BagProfile,
        epoch: u16,
        generated_count: u16,
        remainder_counts: [u8; 7],
    ) -> Result<Self, BagStateError> {
        let full = profile_counts(profile)?;
        for (index, remaining) in remainder_counts.into_iter().enumerate() {
            if remaining > full[index] {
                return Err(BagStateError::RemainderExceedsProfile {
                    piece: PieceKind::STANDARD_TETROMINOES[index],
                    remaining,
                    profile_count: full[index],
                });
            }
        }
        Ok(Self {
            epoch,
            generated_count,
            remainder_counts,
        })
    }

    pub const fn epoch(self) -> u16 {
        self.epoch
    }

    pub const fn generated_count(self) -> u16 {
        self.generated_count
    }

    pub const fn remainder_counts(self) -> [u8; 7] {
        self.remainder_counts
    }

    pub fn bag_size(self) -> usize {
        self.remainder_counts
            .iter()
            .map(|count| usize::from(*count))
            .sum()
    }

    pub fn offset(self) -> usize {
        usize::from(self.generated_count % 7)
    }

    pub fn packed_remainder_key(self) -> u64 {
        self.remainder_counts
            .into_iter()
            .enumerate()
            .fold(0_u64, |key, (index, count)| {
                let piece_code = (index + 1) as u64;
                key | (u64::from(count) << (piece_code * 4))
            })
    }
}

fn profile_counts(profile: &BagProfile) -> Result<[u8; 7], BagStateError> {
    let mut counts = [0_u8; 7];
    for (index, piece) in PieceKind::STANDARD_TETROMINOES.iter().copied().enumerate() {
        let multiplicity = profile.multiplicity_for(piece);
        if multiplicity > 15 {
            return Err(BagStateError::MultiplicityTooLarge {
                piece,
                multiplicity,
            });
        }
        counts[index] = multiplicity as u8;
    }
    if counts.iter().all(|count| *count == 0) {
        return Err(BagStateError::NoStandardPieces);
    }
    Ok(counts)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BagStateError {
    NoStandardPieces,
    MultiplicityTooLarge {
        piece: PieceKind,
        multiplicity: usize,
    },
    RemainderExceedsProfile {
        piece: PieceKind,
        remaining: u8,
        profile_count: u8,
    },
}

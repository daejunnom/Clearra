use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BagProfile {
    id: String,
    entries: Vec<BagProfileEntry>,
}

impl BagProfile {
    pub fn new(
        id: impl Into<String>,
        entries: Vec<BagProfileEntry>,
    ) -> Result<Self, BagProfileError> {
        if entries.is_empty() {
            return Err(BagProfileError::EmptyBag);
        }

        for (index, entry) in entries.iter().enumerate() {
            if entry.multiplicity() == 0 {
                return Err(BagProfileError::ZeroMultiplicity {
                    piece: entry.piece(),
                });
            }
            if entry.weight() == 0 {
                return Err(BagProfileError::ZeroWeight {
                    piece: entry.piece(),
                });
            }
            if entries[..index]
                .iter()
                .any(|candidate| candidate.piece() == entry.piece())
            {
                return Err(BagProfileError::DuplicatePiece {
                    piece: entry.piece(),
                });
            }
        }

        Ok(Self {
            id: id.into(),
            entries,
        })
    }
}
impl BagProfile {
    pub fn standard_7() -> Self {
        Self::new(
            "standard-7-bag",
            PieceKind::STANDARD_TETROMINOES
                .iter()
                .copied()
                .map(|piece| BagProfileEntry::new(piece, 1, 1))
                .collect(),
        )
        .expect("standard 7-bag profile is valid")
    }
}
impl BagProfile {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl BagProfile {
    pub fn entries(&self) -> &[BagProfileEntry] {
        &self.entries
    }
}
impl BagProfile {
    pub fn pieces(&self) -> impl Iterator<Item = PieceKind> + '_ {
        self.entries.iter().map(|entry| entry.piece())
    }
}
impl BagProfile {
    pub fn bag_size(&self) -> usize {
        self.entries.iter().map(|entry| entry.multiplicity()).sum()
    }
}
impl BagProfile {
    pub fn multiplicity_for(&self, piece: PieceKind) -> usize {
        self.entries
            .iter()
            .find(|entry| entry.piece() == piece)
            .map(|entry| entry.multiplicity())
            .unwrap_or(0)
    }
}
impl BagProfile {
    pub fn entry_index(&self, piece: PieceKind) -> Option<usize> {
        self.entries.iter().position(|entry| entry.piece() == piece)
    }
}
impl BagProfile {
    pub fn total_weight(&self) -> u32 {
        self.entries.iter().map(|entry| entry.weight()).sum()
    }
}
impl BagProfile {
    pub fn pattern_universe_hint(&self, target_len: usize) -> PatternUniverseHint {
        let bag_size = self.bag_size().max(1);
        let visible_depth = target_len.min(bag_size);
        let branch_count = (self.entries.len().max(1) as u128).saturating_pow(visible_depth as u32);
        let bag_span = (target_len / bag_size).saturating_add(1) as u128;
        let lower_bound = branch_count.saturating_mul(bag_span);

        if lower_bound > 4096 {
            PatternUniverseHint::SparseRecommended
        } else {
            PatternUniverseHint::DenseMaterializedAcceptable
        }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BagProfileError {
    EmptyBag,
    ZeroMultiplicity { piece: PieceKind },
    ZeroWeight { piece: PieceKind },
    DuplicatePiece { piece: PieceKind },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternUniverseHint {
    DenseMaterializedAcceptable,
    SparseRecommended,
}

#[cfg(test)]
#[path = "bag_profile_tests.rs"]
mod tests;

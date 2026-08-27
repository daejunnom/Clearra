use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FixedSequence {
    pieces: Vec<PieceKind>,
}

impl FixedSequence {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }
}
impl FixedSequence {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl FixedSequence {
    pub fn into_pieces(self) -> Vec<PieceKind> {
        self.pieces
    }
}
impl FixedSequence {
    pub fn len(&self) -> usize {
        self.pieces.len()
    }
}
impl FixedSequence {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// Returns heap bytes retained by the backing piece vector, measured from
    /// allocation capacity. The inline `FixedSequence` owner is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_count_bytes(
            self.pieces.capacity() as u128,
            core::mem::size_of::<PieceKind>() as u128,
        )
    }
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::{checked_count_bytes, FixedSequence};

    #[test]
    fn retained_capacity_counts_allocated_piece_slots_instead_of_length() {
        let mut pieces = Vec::with_capacity(128);
        pieces.push(PieceKind::I);
        let retained_capacity = pieces.capacity();
        let sequence = FixedSequence::new(pieces);

        assert_eq!(
            sequence.checked_retained_capacity_bytes(),
            Some(
                (retained_capacity as u128)
                    .checked_mul(core::mem::size_of::<PieceKind>() as u128)
                    .expect("test capacity fits u128")
            )
        );
        assert!(retained_capacity > sequence.len());
    }

    #[test]
    fn retained_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 1), Some(u128::MAX));
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}

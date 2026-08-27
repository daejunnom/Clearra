use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::finite_allocation::{FiniteSupplyAllocationError, FiniteSupplyAllocationTransaction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FlatPatternSequences {
    offsets: Vec<usize>,
    pieces: Vec<PieceKind>,
}

impl FlatPatternSequences {
    pub(super) fn from_nested(sequences: Vec<Vec<PieceKind>>) -> Option<Self> {
        let piece_count = sequences
            .iter()
            .try_fold(0usize, |total, sequence| total.checked_add(sequence.len()))?;
        let mut offsets = Vec::with_capacity(sequences.len() + 1);
        let mut pieces = Vec::with_capacity(piece_count);
        offsets.push(0);
        for sequence in sequences {
            pieces.extend(sequence);
            offsets.push(pieces.len());
        }
        Some(Self { offsets, pieces })
    }

    pub(super) fn from_single_slice_finite(
        sequence: &[PieceKind],
        transaction: &mut FiniteSupplyAllocationTransaction<'_>,
    ) -> Result<Self, FiniteSupplyAllocationError> {
        let mut offsets = transaction.try_vec_with_capacity::<usize>(2)?;
        let mut pieces = transaction.try_vec_with_capacity::<PieceKind>(sequence.len())?;
        offsets.push(0);
        pieces.extend_from_slice(sequence);
        offsets.push(pieces.len());
        Ok(Self { offsets, pieces })
    }

    pub(super) fn from_nested_prefix_finite(
        sequences: &[Vec<PieceKind>],
        prefix_len: usize,
        transaction: &mut FiniteSupplyAllocationTransaction<'_>,
    ) -> Result<Self, FiniteSupplyAllocationError> {
        let offset_count = sequences
            .len()
            .checked_add(1)
            .ok_or(FiniteSupplyAllocationError::ProjectionOverflow)?;
        let piece_count = sequences.iter().try_fold(0usize, |total, sequence| {
            total.checked_add(prefix_len.min(sequence.len()))
        });
        let piece_count = piece_count.ok_or(FiniteSupplyAllocationError::ProjectionOverflow)?;

        let mut offsets = transaction.try_vec_with_capacity::<usize>(offset_count)?;
        let mut pieces = transaction.try_vec_with_capacity::<PieceKind>(piece_count)?;
        offsets.push(0);
        for sequence in sequences {
            let retained_len = prefix_len.min(sequence.len());
            pieces.extend_from_slice(&sequence[..retained_len]);
            offsets.push(pieces.len());
        }
        Ok(Self { offsets, pieces })
    }

    pub(super) fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub(super) fn get(&self, index: usize) -> &[PieceKind] {
        let start = self.offsets[index];
        let end = self.offsets[index + 1];
        &self.pieces[start..end]
    }

    /// Returns only the heap payload retained by the two backing vectors.
    ///
    /// The inline `FlatPatternSequences` owner is deliberately excluded. Both
    /// buffers are measured by allocation capacity rather than logical length.
    pub(super) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_count_bytes(
            self.offsets.capacity() as u128,
            core::mem::size_of::<usize>() as u128,
        )?
        .checked_add(checked_count_bytes(
            self.pieces.capacity() as u128,
            core::mem::size_of::<PieceKind>() as u128,
        )?)
    }
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::{checked_count_bytes, FlatPatternSequences};

    #[test]
    fn retained_capacity_counts_flat_offsets_and_piece_payloads() {
        let sequences = vec![
            vec![PieceKind::I, PieceKind::O],
            vec![PieceKind::T, PieceKind::S],
        ];
        let flat = FlatPatternSequences::from_nested(sequences).expect("flat sequence storage");
        let expected = (flat.offsets.capacity() as u128)
            .checked_mul(core::mem::size_of::<usize>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (flat.pieces.capacity() as u128)
                        .checked_mul(core::mem::size_of::<PieceKind>() as u128)?,
                )
            })
            .expect("test storage fits u128");

        assert_eq!(flat.checked_retained_capacity_bytes(), Some(expected));
    }

    #[test]
    fn retained_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 1), Some(u128::MAX));
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}

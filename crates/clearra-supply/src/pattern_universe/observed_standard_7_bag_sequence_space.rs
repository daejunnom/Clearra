use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::bag::bag_boundary::BagBoundaryReport;

const BAG_SIZE: usize = 7;
const FULL_BAG_PERMUTATIONS: u128 = 5_040;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CandidateState {
    initial_offset: u8,
    offset: u8,
    used_mask: u8,
    suffix_count: u128,
}

/// Lazy rank/unrank storage for the exact observed standard-7-bag language.
///
/// Ordering is identical to `ObservedQueueExpansion`: boundary candidates are
/// visited by ascending initial offset and suffix pieces use the canonical
/// tetromino order at every depth. Only the observed prefix and at most seven
/// compact candidate states are retained, regardless of pattern count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ObservedStandard7BagSequenceSpace {
    observed: Vec<PieceKind>,
    sequence_len: u16,
    candidates: Vec<CandidateState>,
    pattern_count: usize,
    total_pattern_count: u128,
}

impl ObservedStandard7BagSequenceSpace {
    pub(super) fn new(
        observed: &[PieceKind],
        sequence_len: u16,
    ) -> Result<Self, ObservedStandard7BagSequenceSpaceError> {
        if usize::from(sequence_len) < observed.len() {
            return Err(ObservedStandard7BagSequenceSpaceError::SequenceTooShort);
        }
        let report = BagBoundaryReport::analyze_observed_window(observed, BAG_SIZE);
        if !report.is_compatible() {
            return Err(ObservedStandard7BagSequenceSpaceError::IncompatibleBoundary);
        }
        let remaining = usize::from(sequence_len) - observed.len();
        let mut candidates = Vec::with_capacity(report.candidates().len());
        let mut total_pattern_count = 0_u128;
        for candidate in report.candidates().iter().copied() {
            let initial_offset = u8::try_from(candidate.initial_offset())
                .map_err(|_| ObservedStandard7BagSequenceSpaceError::InvalidBoundaryOffset)?;
            let (offset, used_mask) = prefix_state(observed, initial_offset)?;
            let suffix_count = count_from_state(remaining, offset, used_mask)
                .ok_or(ObservedStandard7BagSequenceSpaceError::PatternCountOverflow)?;
            if suffix_count == 0 {
                continue;
            }
            total_pattern_count = total_pattern_count
                .checked_add(suffix_count)
                .ok_or(ObservedStandard7BagSequenceSpaceError::PatternCountOverflow)?;
            candidates.push(CandidateState {
                initial_offset,
                offset,
                used_mask,
                suffix_count,
            });
        }
        if candidates.is_empty() || total_pattern_count == 0 {
            return Err(ObservedStandard7BagSequenceSpaceError::NoPatterns);
        }
        let pattern_count = checked_pattern_count(total_pattern_count)?;
        Ok(Self {
            observed: observed.to_vec(),
            sequence_len,
            candidates,
            pattern_count,
            total_pattern_count,
        })
    }

    pub(super) const fn len(&self) -> usize {
        self.pattern_count
    }

    pub(super) const fn sequence_len(&self) -> usize {
        self.sequence_len as usize
    }

    pub(super) fn observed_len(&self) -> usize {
        self.observed.len()
    }

    pub(super) const fn total_pattern_count(&self) -> u128 {
        self.total_pattern_count
    }

    pub(super) fn boundary_candidate_count(&self) -> u8 {
        self.candidates.len() as u8
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.observed.capacity() * core::mem::size_of::<PieceKind>()
            + self.candidates.capacity() * core::mem::size_of::<CandidateState>()
    }

    /// Returns only the heap payload retained by the observed-prefix and
    /// boundary-candidate buffers, measured by allocation capacity.
    ///
    /// The inline lazy-space owner is deliberately excluded.
    pub(super) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_count_bytes(
            self.observed.capacity() as u128,
            core::mem::size_of::<PieceKind>() as u128,
        )?
        .checked_add(checked_count_bytes(
            self.candidates.capacity() as u128,
            core::mem::size_of::<CandidateState>() as u128,
        )?)
    }

    pub(super) fn sequence(
        &self,
        index: usize,
    ) -> Result<Vec<PieceKind>, ObservedStandard7BagSequenceSpaceError> {
        let mut sequence = Vec::with_capacity(self.sequence_len());
        self.write_sequence(index, &mut sequence)?;
        Ok(sequence)
    }

    pub(super) fn write_sequence(
        &self,
        index: usize,
        output: &mut Vec<PieceKind>,
    ) -> Result<(), ObservedStandard7BagSequenceSpaceError> {
        output.clear();
        if index >= self.pattern_count {
            return Err(ObservedStandard7BagSequenceSpaceError::PatternIndexOutOfRange);
        }
        let mut rank = index as u128;
        let mut selected_candidate = None;
        for candidate in self.candidates.iter().copied() {
            if rank < candidate.suffix_count {
                selected_candidate = Some(candidate);
                break;
            }
            rank -= candidate.suffix_count;
        }
        let candidate = selected_candidate
            .ok_or(ObservedStandard7BagSequenceSpaceError::PatternIndexOutOfRange)?;
        output.reserve(self.sequence_len());
        output.extend_from_slice(&self.observed);

        let mut offset = candidate.offset;
        let mut used_mask = candidate.used_mask;
        let mut remaining = self.sequence_len() - self.observed.len();
        while remaining != 0 {
            (offset, used_mask) = normalized_state(offset, used_mask)?;
            let mut selected = None;
            for (piece_index, piece) in PieceKind::STANDARD_TETROMINOES.iter().copied().enumerate()
            {
                let bit = 1_u8 << piece_index;
                if used_mask & bit != 0 {
                    continue;
                }
                let next_offset = offset + 1;
                let next_mask = used_mask | bit;
                let branch_count = count_from_state(remaining - 1, next_offset, next_mask)
                    .ok_or(ObservedStandard7BagSequenceSpaceError::PatternCountOverflow)?;
                if rank < branch_count {
                    selected = Some((piece, next_offset, next_mask));
                    break;
                }
                rank -= branch_count;
            }
            let (piece, next_offset, next_mask) =
                selected.ok_or(ObservedStandard7BagSequenceSpaceError::RankInvariantViolated)?;
            output.push(piece);
            offset = next_offset;
            used_mask = next_mask;
            remaining -= 1;
        }
        Ok(())
    }
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

fn prefix_state(
    observed: &[PieceKind],
    initial_offset: u8,
) -> Result<(u8, u8), ObservedStandard7BagSequenceSpaceError> {
    let mut offset = initial_offset;
    let mut used_mask = 0_u8;
    for piece in observed.iter().copied() {
        (offset, used_mask) = normalized_state(offset, used_mask)?;
        let bit = piece_bit(piece);
        if used_mask & bit != 0 {
            return Err(ObservedStandard7BagSequenceSpaceError::IncompatibleBoundary);
        }
        used_mask |= bit;
        offset += 1;
    }
    Ok((offset, used_mask))
}

fn normalized_state(
    offset: u8,
    used_mask: u8,
) -> Result<(u8, u8), ObservedStandard7BagSequenceSpaceError> {
    match usize::from(offset) {
        BAG_SIZE => Ok((0, 0)),
        0..=6 => Ok((offset, used_mask)),
        _ => Err(ObservedStandard7BagSequenceSpaceError::InvalidBoundaryOffset),
    }
}

fn count_from_state(remaining: usize, offset: u8, used_mask: u8) -> Option<u128> {
    if remaining == 0 {
        return Some(1);
    }
    let (offset, used_mask) = normalized_state(offset, used_mask).ok()?;
    let slots_before_boundary = BAG_SIZE.checked_sub(usize::from(offset))?;
    let draws_before_boundary = remaining.min(slots_before_boundary);
    let available_pieces = BAG_SIZE.checked_sub(used_mask.count_ones() as usize)?;
    let prefix_count = checked_falling_factorial(available_pieces, draws_before_boundary)?;
    let tail_len = remaining - draws_before_boundary;
    prefix_count.checked_mul(aligned_suffix_count(tail_len)?)
}

fn aligned_suffix_count(remaining: usize) -> Option<u128> {
    let full_bags = remaining / BAG_SIZE;
    let tail = remaining % BAG_SIZE;
    let mut count = 1_u128;
    for _ in 0..full_bags {
        count = count.checked_mul(FULL_BAG_PERMUTATIONS)?;
    }
    count.checked_mul(checked_falling_factorial(BAG_SIZE, tail)?)
}

fn checked_falling_factorial(value: usize, count: usize) -> Option<u128> {
    if count > value {
        return Some(0);
    }
    let mut product = 1_u128;
    for index in 0..count {
        product = product.checked_mul((value - index) as u128)?;
    }
    Some(product)
}

fn checked_pattern_count(
    total_pattern_count: u128,
) -> Result<usize, ObservedStandard7BagSequenceSpaceError> {
    usize::try_from(total_pattern_count)
        .map_err(|_| ObservedStandard7BagSequenceSpaceError::PatternCountOverflow)
}

const fn piece_bit(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 1 << 0,
        PieceKind::O => 1 << 1,
        PieceKind::T => 1 << 2,
        PieceKind::S => 1 << 3,
        PieceKind::Z => 1 << 4,
        PieceKind::J => 1 << 5,
        PieceKind::L => 1 << 6,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ObservedStandard7BagSequenceSpaceError {
    SequenceTooShort,
    IncompatibleBoundary,
    InvalidBoundaryOffset,
    NoPatterns,
    PatternCountOverflow,
    PatternIndexOutOfRange,
    RankInvariantViolated,
}

#[cfg(test)]
mod tests {
    use crate::{
        normalize::observed_queue_expansion::ObservedQueueExpansion,
        queue::observed_queue::ObservedQueue,
    };

    use super::*;

    fn eager_sequences(observed: &[PieceKind], sequence_len: usize) -> Vec<Vec<PieceKind>> {
        ObservedQueueExpansion::expand(
            &ObservedQueue::new(observed.to_vec()),
            sequence_len,
            usize::MAX,
        )
        .expect("small eager reference")
        .patterns()
        .iter()
        .map(|pattern| pattern.queue_pattern().pieces().to_vec())
        .collect()
    }

    #[test]
    fn lazy_rank_order_matches_eager_single_bag_fieldwise() {
        let observed = [PieceKind::I, PieceKind::O];
        let expected = eager_sequences(&observed, 4);
        let space =
            ObservedStandard7BagSequenceSpace::new(&observed, 4).expect("lazy observed space");

        assert_eq!(space.len(), expected.len());
        for (index, expected) in expected.into_iter().enumerate() {
            assert_eq!(space.sequence(index).expect("ranked sequence"), expected);
        }
    }

    #[test]
    fn lazy_rank_order_matches_eager_across_bag_boundaries_fieldwise() {
        let observed = [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
        ];
        let expected = eager_sequences(&observed, 8);
        let space =
            ObservedStandard7BagSequenceSpace::new(&observed, 8).expect("lazy observed space");

        assert_eq!(space.len(), expected.len());
        for (index, expected) in expected.into_iter().enumerate() {
            assert_eq!(space.sequence(index).expect("ranked sequence"), expected);
        }
    }

    #[test]
    fn empty_observed_four_line_space_is_large_but_retains_constant_memory() {
        let space =
            ObservedStandard7BagSequenceSpace::new(&[], 11).expect("four-line observed space");

        assert!(space.total_pattern_count() > 4_233_600);
        assert_eq!(space.len() as u128, space.total_pattern_count());
        assert_eq!(space.boundary_candidate_count(), 7);
        assert!(space.retained_bytes() <= 512);
        let expected = (space.observed.capacity() as u128)
            .checked_mul(core::mem::size_of::<PieceKind>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (space.candidates.capacity() as u128)
                        .checked_mul(core::mem::size_of::<CandidateState>() as u128)?,
                )
            })
            .expect("test storage fits u128");
        assert_eq!(space.checked_retained_capacity_bytes(), Some(expected));
        assert_eq!(space.sequence(0).expect("first").len(), 11);
        assert_eq!(space.sequence(space.len() - 1).expect("last").len(), 11);
    }

    #[test]
    fn out_of_range_rank_and_count_overflow_fail_closed() {
        let space = ObservedStandard7BagSequenceSpace::new(&[], 4).expect("space");
        let mut output = vec![PieceKind::I];
        assert_eq!(
            space.write_sequence(space.len(), &mut output),
            Err(ObservedStandard7BagSequenceSpaceError::PatternIndexOutOfRange)
        );
        assert!(output.is_empty());
        assert_eq!(aligned_suffix_count(u16::MAX as usize), None);
        assert_eq!(
            checked_pattern_count(u128::MAX),
            Err(ObservedStandard7BagSequenceSpaceError::PatternCountOverflow)
        );
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}

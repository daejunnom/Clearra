//! The canonical original-queue ordinal axis for a fresh standard 7-bag.
//! Each bag is a permutation in `STANDARD_TETROMINOES` order; the final bag
//! may be a prefix. No queue list or per-row heap allocation is required.

use super::piece_kind::PieceKind;

const WIDTH: usize = PieceKind::STANDARD_TETROMINOES.len();
const FULL_BAG: u8 = (1 << WIDTH) - 1;
const MAX_COMPACT_SEQUENCE: usize = 11;

const CARDINALITIES: [usize; MAX_COMPACT_SEQUENCE + 1] = {
    let mut counts = [1; MAX_COMPACT_SEQUENCE + 1];
    let mut depth = 1;
    while depth <= MAX_COMPACT_SEQUENCE {
        counts[depth] = counts[depth - 1] * (WIDTH - (depth - 1) % WIDTH);
        depth += 1;
    }
    counts
};

/// Nonempty, complete standard-bag sequence space with at most 11 draws.
/// This is an ordinal codec, not proof that a producer used this supply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardBagOrdinals {
    sequence_len: u8,
}

impl StandardBagOrdinals {
    pub const fn new(sequence_len: usize) -> Option<Self> {
        if sequence_len == 0 || sequence_len > MAX_COMPACT_SEQUENCE {
            return None;
        }
        Some(Self {
            sequence_len: sequence_len as u8,
        })
    }

    pub const fn len(self) -> usize {
        CARDINALITIES[self.sequence_len()]
    }

    pub const fn is_empty(self) -> bool {
        false
    }

    pub const fn sequence_len(self) -> usize {
        self.sequence_len as usize
    }

    /// Only the first returned `length` elements are part of the queue.
    pub fn sequence_at(self, index: usize) -> Option<([PieceKind; MAX_COMPACT_SEQUENCE], usize)> {
        let mut row = [PieceKind::I; MAX_COMPACT_SEQUENCE];
        let mut used = 0;
        let valid = self.write_ordinal(index, |piece| {
            row[used] = piece;
            used += 1;
        });
        valid.then_some((row, used))
    }

    fn write_ordinal(self, mut rank: usize, mut write: impl FnMut(PieceKind)) -> bool {
        let mut block = self.len();
        if rank >= block {
            return false;
        }
        let mut available = FULL_BAG;
        for _ in 0..self.sequence_len() {
            if available == 0 {
                available = FULL_BAG;
            }
            // Each of the currently available kinds heads exactly the same
            // number of suffixes. At the refill the factor is seven again.
            // No recomputation or wide-integer division is needed here.
            block /= available.count_ones() as usize;
            let digit = rank / block;
            rank %= block;
            write(select_piece(&mut available, digit));
        }
        true
    }
}

/// Decode an ordinal through the same kernel, including legacy rows longer
/// than the compact backing admits. Invalid requests emit no pieces.
pub fn write_standard_bag_ordinal(
    sequence_len: u16,
    index: usize,
    write: impl FnMut(PieceKind),
) -> bool {
    let length = usize::from(sequence_len);
    if let Some(space) = StandardBagOrdinals::new(length) {
        return space.write_ordinal(index, write);
    }
    if length == 0 || index as u128 >= suffix_count(length, WIDTH) {
        return false;
    }
    decode_large_ordinal(length, index, write);
    true
}

fn decode_large_ordinal(length: usize, index: usize, mut write: impl FnMut(PieceKind)) {
    let mut rank = index as u128;
    let mut available = FULL_BAG;
    for remaining in (0..length).rev() {
        if available == 0 {
            available = FULL_BAG;
        }
        // Removing any one available piece leaves an equal number of
        // suffixes. Rank selects a bit among the available bits, not among
        // all seven kinds.
        let block = suffix_count(remaining, available.count_ones() as usize - 1);
        let digit = (rank / block) as usize;
        rank %= block;
        write(select_piece(&mut available, digit));
    }
}

fn select_piece(available: &mut u8, digit: usize) -> PieceKind {
    let mut choices = *available;
    for _ in 0..digit {
        choices &= choices - 1;
    }
    let selected = choices.trailing_zeros() as usize;
    *available ^= 1 << selected;
    PieceKind::STANDARD_TETROMINOES[selected]
}

fn suffix_count(mut remaining: usize, mut available: usize) -> u128 {
    let mut count = 1_u128;
    while remaining != 0 {
        if available == 0 {
            available = WIDTH;
        }
        let take = remaining.min(available);
        for factor in available - take + 1..=available {
            count = count.saturating_mul(factor as u128);
        }
        if count == u128::MAX {
            return count;
        }
        remaining -= take;
        available = WIDTH;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_row(space: StandardBagOrdinals, index: usize, expected: &str) {
        let (row, length) = space.sequence_at(index).unwrap();
        assert_eq!(length, space.sequence_len());
        assert!(row[..length]
            .iter()
            .map(|kind| kind.as_ascii())
            .eq(expected.chars()));
    }

    #[test]
    fn compact_standard_bag_ordinal_boundaries_preserve_order() {
        assert_eq!(StandardBagOrdinals::new(0), None);
        assert_eq!(StandardBagOrdinals::new(12), None);
        assert_eq!(StandardBagOrdinals::new(usize::MAX), None);
        let one = StandardBagOrdinals::new(1).unwrap();
        assert_eq!(one.len(), 7);
        assert_row(one, 0, "I");
        assert_row(one, 6, "L");
        let seven = StandardBagOrdinals::new(7).unwrap();
        assert_eq!(seven.len(), 5_040);
        assert_row(seven, 0, "IOTSZJL");
        assert_row(seven, 5_039, "LJZSTOI");
        let eleven = StandardBagOrdinals::new(11).unwrap();
        assert!(!eleven.is_empty());
        assert_eq!(eleven.len(), 4_233_600);
        assert_row(eleven, 0, "IOTSZJLIOTS");
        assert_row(eleven, 839, "IOTSZJLLJZS");
        assert_row(eleven, 840, "IOTSZLJIOTS");
        assert_row(eleven, eleven.len() - 1, "LJZSTOILJZS");
        assert_eq!(eleven.sequence_at(eleven.len()), None);
    }

    #[test]
    fn short_ordinal_space_matches_independent_ordered_draws() {
        let space = StandardBagOrdinals::new(3).unwrap();
        let mut index = 0;
        for a in PieceKind::STANDARD_TETROMINOES {
            for b in PieceKind::STANDARD_TETROMINOES {
                for c in PieceKind::STANDARD_TETROMINOES {
                    if a == b || a == c || b == c {
                        continue;
                    }
                    let (row, length) = space.sequence_at(index).unwrap();
                    assert_eq!(length, 3);
                    assert_eq!(row[..length], [a, b, c]);
                    index += 1;
                }
            }
        }
        assert_eq!(index, 210);
        assert_eq!(space.sequence_at(index), None);
    }

    #[test]
    fn shared_writer_keeps_large_existing_axes_and_rejects_before_writing() {
        let mut row = Vec::new();
        assert!(write_standard_bag_ordinal(12, 2_520, |piece| row.push(piece)));
        assert!(row
            .iter()
            .map(|piece| piece.as_ascii())
            .eq("IOTSZLJIOTSZ".chars()));
        assert!(!write_standard_bag_ordinal(12, 12_700_800, |_| panic!(
            "invalid rank"
        )));
        assert!(!write_standard_bag_ordinal(0, 0, |_| panic!(
            "invalid sequence length"
        )));
    }
}

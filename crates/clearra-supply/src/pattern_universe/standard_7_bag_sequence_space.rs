use clearra_core_domain::piece::piece_kind::PieceKind;

const FULL_STANDARD_BAG: u8 = 0x7f;

/// Lexicographic standard 7-bag sequence space. A pattern is reconstructed
/// from its rank, so large complete universes do not retain every sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Standard7BagSequenceSpace {
    sequence_len: u16,
    pattern_count: usize,
}

impl Standard7BagSequenceSpace {
    pub(super) const fn new(sequence_len: u16, pattern_count: usize) -> Self {
        Self {
            sequence_len,
            pattern_count,
        }
    }

    pub(super) const fn len(&self) -> usize {
        self.pattern_count
    }

    pub(super) const fn sequence_len(&self) -> usize {
        self.sequence_len as usize
    }

    pub(super) fn sequence(&self, index: usize) -> Vec<PieceKind> {
        let mut sequence = Vec::with_capacity(self.sequence_len());
        self.write_sequence(index, &mut sequence);
        sequence
    }

    pub(super) fn write_sequence(&self, index: usize, output: &mut Vec<PieceKind>) {
        assert!(
            index < self.pattern_count,
            "pattern index belongs to universe"
        );
        output.clear();
        output.reserve(self.sequence_len());
        let mut rank = index as u128;
        let mut available = FULL_STANDARD_BAG;
        for depth in 0..usize::from(self.sequence_len) {
            if available == 0 {
                available = FULL_STANDARD_BAG;
            }
            let remaining = usize::from(self.sequence_len) - depth - 1;
            let mut selected = None;
            for (piece_index, piece) in PieceKind::STANDARD_TETROMINOES.iter().copied().enumerate()
            {
                let bit = 1_u8 << piece_index;
                if available & bit == 0 {
                    continue;
                }
                let branch_size = suffix_count(remaining, available & !bit);
                if rank < branch_size {
                    selected = Some((piece, bit));
                    break;
                }
                rank -= branch_size;
            }
            let (piece, bit) = selected.expect("rank has one lexicographic branch");
            output.push(piece);
            available &= !bit;
        }
    }
}

fn suffix_count(remaining: usize, available: u8) -> u128 {
    if remaining == 0 {
        return 1;
    }
    let available_count = available.count_ones() as usize;
    if remaining <= available_count {
        return falling_factorial(available_count, remaining);
    }
    falling_factorial(available_count, available_count)
        .saturating_mul(suffix_count(remaining - available_count, FULL_STANDARD_BAG))
}

const fn falling_factorial(value: usize, count: usize) -> u128 {
    let mut product = 1_u128;
    let mut index = 0;
    while index < count {
        product *= (value - index) as u128;
        index += 1;
    }
    product
}

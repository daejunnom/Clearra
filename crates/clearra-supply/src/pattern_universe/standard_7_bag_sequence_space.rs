use clearra_core_domain::piece::{
    piece_kind::PieceKind, standard_bag_ordinals::write_standard_bag_ordinal,
};

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
        assert!(
            write_standard_bag_ordinal(self.sequence_len, index, |piece| output.push(piece)),
            "rank has one lexicographic branch"
        );
    }
}

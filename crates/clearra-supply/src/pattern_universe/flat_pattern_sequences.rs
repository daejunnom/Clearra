use clearra_core_domain::piece::piece_kind::PieceKind;

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

    pub(super) fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub(super) fn get(&self, index: usize) -> &[PieceKind] {
        let start = self.offsets[index];
        let end = self.offsets[index + 1];
        &self.pieces[start..end]
    }
}

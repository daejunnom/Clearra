use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::queue::{
    fixed_sequence::FixedSequence,
    queue_parser::{parse_fixed_sequence, QueueParseError},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PieceSequenceAssemblyError {
    UnknownPiece { index: usize, value: char },
}

impl PieceSequenceAssemblyError {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownPiece { index, value } => {
                format!("unknown piece '{value}' at queue index {index}")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PieceSequenceAssembler;

impl PieceSequenceAssembler {
    pub fn parse_fixed_sequence(queue: &str) -> Result<FixedSequence, PieceSequenceAssemblyError> {
        parse_fixed_sequence(queue).map_err(|error| match error {
            QueueParseError::UnknownPiece { index, value } => {
                PieceSequenceAssemblyError::UnknownPiece { index, value }
            }
        })
    }
}
impl PieceSequenceAssembler {
    pub fn parse_piece(piece: char) -> Result<PieceKind, PieceSequenceAssemblyError> {
        PieceKind::from_ascii(piece).map_err(|_| PieceSequenceAssemblyError::UnknownPiece {
            index: 0,
            value: piece,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_piece_sequences_with_opening_style_separators() {
        let sequence = PieceSequenceAssembler::parse_fixed_sequence("I, O T").expect("sequence");

        assert_eq!(sequence.len(), 3);
    }
}
